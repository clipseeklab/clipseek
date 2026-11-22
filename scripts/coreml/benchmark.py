#!/usr/bin/env python3
"""Measure isolated native processes and enforce the existing vector-parity gate."""
import argparse
import ctypes
import json
import math
import os
from pathlib import Path
import signal
import struct
import subprocess
import sys
import tempfile
import time

DIMENSION = 768
COSINE_THRESHOLD = 0.9999
MEMORY_LIMIT = 16 * 1024**3


def vector(path):
    data = Path(path).read_bytes()
    if len(data) != DIMENSION * 4:
        raise ValueError("Expected exactly 768 little-endian float32 values")
    values = struct.unpack("<768f", data)
    if not all(math.isfinite(value) for value in values):
        raise ValueError("Vector contains non-finite values")
    norm = math.sqrt(math.fsum(value * value for value in values))
    if norm <= 1e-12:
        raise ValueError("Vector has zero norm")
    return [value / norm for value in values]


def compare(candidate, reference):
    a, b = vector(candidate), vector(reference)
    cosine = min(1.0, max(-1.0, math.fsum(x * y for x, y in zip(a, b))))
    return {"cosine": cosine, "threshold": COSINE_THRESHOLD,
            "passed": cosine > COSINE_THRESHOLD, "dimension": DIMENSION}


class Usage(ctypes.Structure):
    # Darwin rusage_info_v0. Separate CoreML services and device caches are excluded.
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in
        ("user", "system", "idle", "interrupts", "pageins", "wired", "rss",
         "footprint", "start", "exit")
    ]


def memory_reader():
    if sys.platform != "darwin":
        raise ValueError("Native memory measurement requires macOS")
    library = ctypes.CDLL("/usr/lib/libproc.dylib")
    library.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    library.proc_pid_rusage.restype = ctypes.c_int

    def read(pid):
        usage = Usage()
        if library.proc_pid_rusage(pid, 0, ctypes.byref(usage)) != 0:
            return None
        return {"rss": usage.rss, "footprint": usage.footprint}

    return read


def write_json(path, result):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as file:
        temporary = Path(file.name)
        json.dump(result, file, indent=2)
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def measure(output, command):
    if not command:
        raise ValueError("Expected a command after --")
    read = memory_reader()
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    peak = {"rss": 0, "footprint": 0}
    rejected = False
    start = time.monotonic()
    # Each encoder/reference runs separately; no shell and no combined model residency.
    with output.with_suffix(".stdout.log").open("wb") as stdout, \
            output.with_suffix(".stderr.log").open("wb") as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr,
                                   start_new_session=True)
        try:
            while process.poll() is None:
                current = read(process.pid)
                if current:
                    peak = {key: max(peak[key], current[key]) for key in peak}
                    if max(current.values()) > MEMORY_LIMIT:
                        rejected = True
                        os.killpg(process.pid, signal.SIGTERM)
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                        break
                time.sleep(0.01)
            code = process.wait()
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    result = {"exitCode": code, "resourceRejected": rejected,
              "memoryLimitBytes": MEMORY_LIMIT, "peakClientMemory": peak,
              "seconds": time.monotonic() - start,
              "samplingIntervalSeconds": 0.01}
    write_json(output, result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)
    check = subparsers.add_parser("compare")
    check.add_argument("candidate")
    check.add_argument("reference")
    run = subparsers.add_parser("measure")
    run.add_argument("output")
    run.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        if args.action == "compare":
            result = compare(args.candidate, args.reference)
            code = 0 if result["passed"] else 1
        else:
            command = args.command[1:] if args.command[:1] == ["--"] else args.command
            result = measure(args.output, command)
            code = 125 if result["resourceRejected"] else result["exitCode"]
            if code < 0:
                code = 128 - code
        print(json.dumps(result), flush=True)
        return code
    except (OSError, ValueError) as error:
        print(json.dumps({"error": str(error), "passed": False}), flush=True)
        return 2


if __name__ == "__main__":
    sys.exit(main())

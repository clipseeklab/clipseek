#!/usr/bin/env python3
"""Regenerate model-only developer artifacts; never run during application startup."""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
BUCKETS = [32, 128, 256, 512, 1024, 2048, 2560, 4096, 4608, 8192]
SUFFIX = {"q8": "_quantized", "q4": "_q4", "fp32": ""}


def verify(models, precision, graph_only=False):
    catalogue = {item["path"]: item for item in json.loads((ROOT / "src-tauri/model-files.json").read_text())}
    names = ["onnx/model" + SUFFIX[precision] + ".onnx"]
    if not graph_only:
        names += [names[0] + "_data"]
    for name in names:
        expected = catalogue[name]
        path = models / precision / name
        if path.stat().st_size != expected["size"]:
            raise ValueError("Source model size mismatch: " + name)
        digest = hashlib.sha256()
        with path.open("rb") as file:
            for data in iter(lambda: file.read(1024 * 1024), b""):
                digest.update(data)
        if digest.hexdigest() != expected["sha256"]:
            raise ValueError("Source model checksum mismatch: " + name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--models", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--precision", choices=list(SUFFIX), default="q8")
    parser.add_argument("--bucket", type=int, choices=BUCKETS, default=512)
    parser.add_argument("--templates", action="store_true", help="Emit data-free runtime templates")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if not args.templates:
        verify(args.models, args.precision)
        from _source import load_model
        from _converter import export_model
        export_model(load_model(args.models, args.precision), str(args.output), "text", args.bucket)
        return
    for precision in SUFFIX:
        verify(args.models, precision, graph_only=True)
    verify(args.models, "q8")
    revision = re.search(r'pub const MODEL_REVISION: &str = "([a-f0-9]+)"',
                         (ROOT / "src-tauri/src/embedding/domain.rs").read_text()).group(1)
    # Fresh workers keep exporter memory bounded; source arrays are not retained across buckets.
    with tempfile.TemporaryDirectory(prefix="clipstudio-coreml-export-") as directory:
        stage = Path(directory)
        def worker(bucket):
            subprocess.run([sys.executable, __file__, "--models", str(args.models), "--output",
                            str(stage / ("q8-select-" + str(bucket))), "--bucket", str(bucket)],
                           check=True)
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            list(pool.map(worker, BUCKETS))
        from _templates import collect
        collect(args.models, stage, args.output, ROOT / "src-tauri/native/coreml", revision)


if __name__ == "__main__":
    main()

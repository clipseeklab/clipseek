import importlib.util
import json
import math
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("coreml_benchmark", ROOT / "scripts/coreml/benchmark.py")
benchmark = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(benchmark)


class BenchmarkTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def save(self, name, values):
        path = self.root / name
        path.write_bytes(struct.pack("<" + str(len(values)) + "f", *values))
        return path

    def test_comparison_normalizes_native_pooled_vectors(self):
        a = self.save("a.bin", [2.0] + [0.0] * 767)
        b = self.save("b.bin", [1.0] + [0.0] * 767)
        self.assertTrue(benchmark.compare(a, b)["passed"])

    def test_comparison_preserves_strict_cosine_gate(self):
        reference = self.save("reference.bin", [1.0] + [0.0] * 767)
        for cosine, expected in [(0.99995, True), (0.9998, False), (-1.0, False)]:
            with self.subTest(cosine=cosine):
                candidate = self.save("candidate.bin", [cosine, math.sqrt(1 - cosine**2)] + [0.0] * 766)
                self.assertEqual(benchmark.compare(candidate, reference)["passed"], expected)

    def test_comparison_rejects_wrong_size_nonfinite_and_zero_vectors(self):
        for values in [[1.0] * 767, [0.0] * 768, [float("nan")] * 768, [float("inf")] * 768]:
            with self.subTest(first=values[0], count=len(values)):
                with self.assertRaises(ValueError):
                    benchmark.vector(self.save("invalid.bin", values))

    def test_cli_exits_nonzero_on_mismatched_vectors(self):
        a = self.save("a.bin", [1.0] + [0.0] * 767)
        b = self.save("b.bin", [0.0, 1.0] + [0.0] * 766)
        result = subprocess.run([sys.executable, str(ROOT / "scripts/coreml/benchmark.py"),
                                 "compare", str(a), str(b)], capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(json.loads(result.stdout)["passed"])

    def test_measurement_preserves_child_failure(self):
        output = self.root / "failure.json"
        with patch.object(benchmark, "memory_reader", return_value=lambda _: None):
            result = benchmark.measure(output, [sys.executable, "-c", "raise SystemExit(7)"])
        self.assertEqual(result["exitCode"], 7)
        self.assertFalse(result["resourceRejected"])
        self.assertEqual(json.loads(output.read_text())["exitCode"], 7)

    def test_resource_guard_terminates_only_its_own_process_group(self):
        output = self.root / "guard.json"
        read = lambda _: {"rss": benchmark.MEMORY_LIMIT + 1, "footprint": 123}
        with patch.object(benchmark, "memory_reader", return_value=read):
            result = benchmark.measure(output, [sys.executable, "-c", "import time; time.sleep(10)"])
        self.assertTrue(result["resourceRejected"])
        self.assertNotEqual(result["exitCode"], 0)
        self.assertEqual(result["peakClientMemory"]["footprint"], 123)

    def test_guard_also_rejects_large_footprint_with_low_resident_memory(self):
        output = self.root / "footprint.json"
        read = lambda _: {"rss": 1024, "footprint": benchmark.MEMORY_LIMIT + 1}
        with patch.object(benchmark, "memory_reader", return_value=read):
            result = benchmark.measure(output, [sys.executable, "-c", "import time; time.sleep(10)"])
        self.assertTrue(result["resourceRejected"])
        self.assertNotEqual(result["exitCode"], 0)


if __name__ == "__main__":
    unittest.main()

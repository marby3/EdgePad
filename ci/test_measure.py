"""Check for Issue #9: the measurement script reports every figure.

Run from the repo root: python -m unittest ci/test_measure.py
"""

import json
import pathlib
import subprocess
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIGURES = (
    "exe_bytes",
    "startup_ms",
    "private_bytes",
    "working_set_bytes",
    "idle_seconds",
    "idle_cpu_ms",
)


class MeasureTest(unittest.TestCase):
    # AC1
    def test_measure_reports_every_figure_for_the_release_exe(self):
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True)
        out = subprocess.run(
            [sys.executable, "ci/measure.py", "--idle", "2", "--json"],
            cwd=ROOT, capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(out.returncode, 0, out.stderr)
        result = json.loads(out.stdout)
        for name in FIGURES:
            self.assertIn(name, result)
            self.assertIsInstance(result[name], (int, float), name)
            self.assertGreaterEqual(result[name], 0, name)
        self.assertGreater(result["exe_bytes"], 0)
        self.assertGreater(result["working_set_bytes"], 0)
        self.assertEqual(result["idle_seconds"], 2)

        table = subprocess.run(
            [sys.executable, "ci/measure.py", "--idle", "1"],
            cwd=ROOT, capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(table.returncode, 0, table.stderr)
        self.assertIn("| exe", table.stdout, "markdown table expected without --json")


if __name__ == "__main__":
    unittest.main()

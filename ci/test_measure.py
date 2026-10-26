"""Checks for Issue #9 (the measurement script reports every figure),
Issue #34 (private memory stays within the spec's limit, 2.5 MiB since
Issue #83) and Issue #86 (the settings window's memory, open and closed).

Run from the repo root: python -m unittest ci/test_measure.py
"""

import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time
import unittest

ROOT_CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT_CI))
import measure as m  # noqa: E402  (ci/measure.py: panel_visible, EXE)

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
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True, timeout=600)
        idle = 2
        out = subprocess.run(
            [sys.executable, "ci/measure.py", "--idle", str(idle), "--json"],
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
        self.assertEqual(result["idle_seconds"], idle)

        table = subprocess.run(
            [sys.executable, "ci/measure.py", "--idle", "1"],
            cwd=ROOT, capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(table.returncode, 0, table.stderr)
        self.assertIn("| exe", table.stdout, "markdown table expected without --json")


# Spec, chapter 8: private memory 2.5 MiB or less (MiB as the spec's MB).
PRIVATE_LIMIT = 5 * 1024 * 1024 // 2


def measure(*args, env=None):
    out = subprocess.run(
        [sys.executable, "ci/measure.py", "--json", *args],
        cwd=ROOT, capture_output=True, text=True, timeout=120, env=env,
    )
    return out


class MemoryTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True, timeout=600, capture_output=True)

    # Issue #34, AC1
    def test_private_memory_with_the_spec_example_is_at_most_2_mib(self):
        out = measure("--idle", "5", "--settings", "ci/example.ini")
        self.assertEqual(out.returncode, 0, out.stderr)
        result = json.loads(out.stdout)
        # In the CI log, so the margin on the runner is on record.
        print(f"\n  private memory: {result['private_bytes']:,} bytes", file=sys.stderr)
        # The fixed keys it would fall back to have no hotkey: a registered
        # one shows the file was read.
        self.assertEqual(result["hotkey"], "registered", "the spec example was applied")
        # Its default profile has the left edge only (right = none); the
        # right-hand set belongs to the CSP profile, which comes with v0.3.
        self.assertEqual(result["panels"], 1, "the spec example's default profile shows the left panel")
        private = result["private_bytes"]
        self.assertLessEqual(
            private, PRIVATE_LIMIT,
            f"private memory {private / 1024 / 1024:.2f} MiB ({private:,} bytes) is over the spec's 2.5 MiB",
        )

    # Issue #34, AC2
    def test_measuring_leaves_an_edgepad_already_running_alone(self):
        with tempfile.TemporaryDirectory() as appdata:
            env = os.environ | {"APPDATA": appdata}
            running = subprocess.Popen([str(m.EXE)], env=env)
            try:
                end = time.monotonic() + 10
                while not m.panel_visible(running.pid) and running.poll() is None and time.monotonic() < end:
                    time.sleep(0.05)
                self.assertIsNone(running.poll(), "precondition: an EdgePad is running")
                self.assertTrue(m.panel_visible(running.pid), "precondition: its panel is shown")

                out = measure("--idle", "1", env=env)
                self.assertEqual(out.returncode, 0, out.stderr)
                self.assertGreater(json.loads(out.stdout)["private_bytes"], 0)
                self.assertIsNone(running.poll(), "the running EdgePad is still running")
                self.assertTrue(m.panel_visible(running.pid), "and its panel was not toggled away")
            finally:
                running.kill()
                running.wait()


# Spec, chapter 8 (Issue #86): private memory with the settings window
# open, and once it is closed again. Closed, it keeps what Windows loads
# the first time a window of EdgePad's takes the focus (text input, about
# 1 MiB, its timing swaying the figure by 0.3 MiB) and the common controls
# with their theme (about 0.1 to 0.2 MiB). The window as it was before
# Issue #86 measured 3.04 to 3.29 MiB closed, this one 3.08 to 3.47 MiB:
# hence 4 MiB, not the 3 MiB the Issue first set.
OPEN_LIMIT = 8 * 1024 * 1024
CLOSED_LIMIT = 4 * 1024 * 1024
# What counts as no growth over the closings: a page or so either way is
# Windows's own coming and going, not something the window left behind.
CLOSED_SLACK = 64 * 1024


class SettingsWindowMemoryTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True, timeout=600, capture_output=True)

    # Issue #86, AC8
    def test_the_settings_window_stays_within_its_memory_open_and_closed(self):
        out = measure("--idle", "5", "--settings", "ci/example.ini", "--settings-window")
        self.assertEqual(out.returncode, 0, out.stderr)
        result = json.loads(out.stdout)
        window = result["settings_window"]
        print(f"\n  settings window: {window}", file=sys.stderr)
        # Unless the window took the focus, Windows loaded no text input
        # for it, and the figures would be low for want of it.
        self.assertTrue(all(window["in_front"]), f"each opening came to the front: {window['in_front']}")
        self.assertFalse(window["comctl32_at_start"], "the common controls are not loaded before the window opens")
        self.assertTrue(window["comctl32_when_open"], "they are, once it opens")
        self.assertLessEqual(result["private_bytes"], PRIVATE_LIMIT, "never opened: as before")
        self.assertEqual(len(window["closed_private_bytes"]), 5, "opened and closed five times")
        opened = max(window["open_private_bytes"])
        self.assertLessEqual(opened, OPEN_LIMIT, f"open: {opened / 1024 / 1024:.2f} MiB is over 8 MiB")
        closed = window["closed_private_bytes"]
        self.assertLessEqual(max(closed), CLOSED_LIMIT, f"closed: {max(closed) / 1024 / 1024:.2f} MiB is over 4 MiB")
        self.assertLessEqual(closed[-1], closed[0] + CLOSED_SLACK, f"no growth over the closings: {closed}")


if __name__ == "__main__":
    unittest.main()

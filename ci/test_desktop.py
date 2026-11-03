"""Checks for Issue #153: the tests that drive the desktop (launch
edgepad.exe, move the mouse, press keys, bring windows to the front) run
in CI only, behind the `desktop-tests` feature, so a local `cargo test
--workspace` leaves the mouse and keyboard to whoever is at the machine.

Run from the repo root: python -m unittest ci/test_desktop.py
"""

import json
import pathlib
import re
import subprocess
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import retry  # noqa: E402  (ci/retry.py)

ROOT = pathlib.Path(__file__).resolve().parent.parent
TESTS = ROOT / "crates" / "edgepad" / "tests"
FEATURE = "desktop-tests"

# What in a test file means it drives the desktop: the shared helpers
# launch the exe and send input; the rest is doing it directly.
DRIVES_DESKTOP = re.compile(r"\bmod common\b|\bSendInput\b|\bSetCursorPos\b|\bCommand::new\b")


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True,
    ).stdout
    return {p["name"]: p for p in json.loads(out)["packages"]}


def test_targets(package):
    """Test target name → its required features."""
    return {t["name"]: t.get("required-features", []) for t in package["targets"] if "test" in t["kind"]}


class DesktopTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.packages = metadata()

    def test_local_run_leaves_out_only_the_desktop_tests(self):
        """AC1: without features, every edgepad test but version.rs needs
        `desktop-tests`; the core's tests and the unit tests need nothing."""
        edgepad = self.packages["edgepad"]
        for name, needs in test_targets(edgepad).items():
            with self.subTest(target=name):
                self.assertEqual(needs, [] if name == "version" else [FEATURE])
        core = self.packages["edgepad-core"]
        for target in [t for t in edgepad["targets"] if "test" not in t["kind"]] + core["targets"]:
            with self.subTest(target=target["name"]):
                self.assertEqual(target.get("required-features", []), [])

    def test_every_file_that_drives_the_desktop_is_gated(self):
        """AC2: a test file that launches the exe or sends input is gated."""
        gated = {n for n, needs in test_targets(self.packages["edgepad"]).items() if FEATURE in needs}
        for path in sorted(TESTS.glob("*.rs")):
            if DRIVES_DESKTOP.search(path.read_text(encoding="utf-8")):
                with self.subTest(file=path.name):
                    self.assertIn(path.stem, gated, f"{path.name} drives the desktop: give it required-features = [\"{FEATURE}\"]")

    def test_ci_runs_the_desktop_tests(self):
        """AC3: CI's run turns the feature on, so it still runs them all."""
        self.assertIn(FEATURE, self.packages["edgepad"]["features"])
        self.assertIn("--all-features", retry.CARGO)
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        self.assertIn("run: python ci/retry.py", workflow)

    def test_ci_can_be_started_by_hand(self):
        """AC4: a work branch can run CI before it has a PR."""
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        on = re.search(r"^on:\n((?:[ \t]+.*\n|\n)*)", workflow, re.M)
        self.assertIsNotNone(on, "ci.yml has an on: block")
        self.assertRegex(on.group(1), r"(?m)^  workflow_dispatch:")

    def test_claude_md_says_how_to_test(self):
        """AC5: CLAUDE.md tells what runs locally and how the desktop tests,
        RED included, are checked in CI."""
        path = ROOT / "CLAUDE.md"
        self.assertTrue(path.exists(), "CLAUDE.md at the repo root")
        text = path.read_text(encoding="utf-8")
        for needed in ["cargo test --workspace", FEATURE, "gh workflow run", "RED"]:
            with self.subTest(needed=needed):
                self.assertIn(needed, text)


if __name__ == "__main__":
    unittest.main()

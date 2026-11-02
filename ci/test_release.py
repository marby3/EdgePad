"""Release checks for Issue #79: the licence and the zip a tag publishes.
The exe goes out unsigned (Issue #147).

Run from the repo root: python -m unittest ci/test_release.py
The exe's version resource is checked by crates/edgepad/tests/version.rs.
"""

import pathlib
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
POLICY = ROOT / "docs" / "code-signing-policy.md"
SIGNING = re.compile(r"sign\s*path", re.I)


def version():
    with open(ROOT / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["workspace"]["package"]["version"]


def package(*args):
    return subprocess.run(
        [sys.executable, "ci/package.py", *args], cwd=ROOT, capture_output=True, text=True, timeout=60
    )


def step(name):
    """The text of the release workflow's step named `name`, up to the next."""
    text = WORKFLOW.read_text(encoding="utf-8")
    at = text.find(f"- name: {name}")
    if at < 0:
        raise AssertionError(f"no step {name!r} in {WORKFLOW.name}")
    after = text.find("\n      - ", at + 1)
    return text[at:] if after < 0 else text[at:after]


class ReleaseTest(unittest.TestCase):
    # AC1
    def test_the_licence_is_mit_in_the_repo_and_the_manifest(self):
        licence = (ROOT / "LICENSE").read_text(encoding="utf-8")
        self.assertTrue(licence.startswith("MIT License"), licence[:40])
        self.assertIn("Permission is hereby granted, free of charge", licence)
        with open(ROOT / "Cargo.toml", "rb") as f:
            self.assertEqual(tomllib.load(f)["workspace"]["package"].get("license"), "MIT")

    # AC3
    def test_a_version_tag_builds_the_zip_of_the_listed_files_and_attaches_it(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertRegex(text, r"on:\s*\n\s*push:\s*\n\s*tags:\s*\[\"v\*\.\*\.\*\"\]")
        self.assertIn("runs-on: windows-latest", text)
        self.assertIn("python ci/package.py", step("Zip"))
        self.assertIn("gh release upload", step("Attach to the Release"))
        with tempfile.TemporaryDirectory() as out:
            exe = pathlib.Path(out) / "edgepad.exe"
            exe.write_bytes(b"MZ not really")
            done = package("--tag", f"v{version()}", "--exe", str(exe), "--out", out)
            self.assertEqual(done.returncode, 0, done.stderr)
            zipped = pathlib.Path(out) / f"EdgePad-v{version()}-windows-x64.zip"
            with zipfile.ZipFile(zipped) as z:
                # Issue #127, AC7: the shipped settings and keyboards too.
                presets = ["edgepad.ini", "kb-alice64.json", "kb-corne-v4.json", "kb-split42.json", "kb-us.json"]
                # Issue #128, AC4: the READMEs too.
                readmes = ["README.md", "README.ja.md"]
                self.assertEqual(sorted(z.namelist()), sorted(["LICENSE", "edgepad.exe", *presets, *readmes]))
                self.assertEqual(z.read("edgepad.ini"), (ROOT / "presets" / "edgepad.ini").read_bytes())
                self.assertEqual(z.read("edgepad.exe"), b"MZ not really", "the exe given, signed or not")

    # AC4
    def test_a_tag_not_the_manifests_version_publishes_nothing(self):
        self.assertIn("python ci/package.py --check", step("Check the tag"))
        with tempfile.TemporaryDirectory() as out:
            done = package("--tag", "v99.0.0", "--exe", sys.executable, "--out", out)
            self.assertNotEqual(done.returncode, 0)
            self.assertIn(version(), done.stderr, "says which version the manifest has")
            self.assertEqual(list(pathlib.Path(out).iterdir()), [], "no zip")
        self.assertNotEqual(package("--check", "v99.0.0").returncode, 0)
        self.assertEqual(package("--check", f"v{version()}").returncode, 0)

    # Issue #147, AC1
    def test_the_exe_built_goes_in_the_zip_with_no_signing_step(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIsNone(SIGNING.search(text), "no signing step or variable")
        self.assertIn("--exe target/release/edgepad.exe", step("Zip"))
        self.assertNotIn("gh release edit", step("Attach to the Release"), "the notes are left as made")

    # Issue #147, AC2
    def test_the_signing_policy_and_every_mention_of_signing_are_gone(self):
        self.assertFalse(POLICY.exists())
        here = pathlib.Path(__file__).resolve()
        places = [*ROOT.glob("README*.md"), *(ROOT / "docs").rglob("*.md"), *(ROOT / ".github").rglob("*"), *(ROOT / "ci").rglob("*.py")]
        for path in places:
            if path.is_file() and path.name != "roadmap.md" and path.resolve() != here:
                self.assertIsNone(SIGNING.search(path.read_text(encoding="utf-8")), path.relative_to(ROOT))
        # What the policy said of the network, kept in the READMEs.
        self.assertRegex((ROOT / "README.md").read_text(encoding="utf-8"), r"(?i)network")
        self.assertIn("ネットワーク", (ROOT / "README.ja.md").read_text(encoding="utf-8"))

if __name__ == "__main__":
    unittest.main()

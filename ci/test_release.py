"""Release checks for Issue #79: the licence, the zip a tag publishes, and
the signing the release workflow does once SignPath is set up.

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
                self.assertEqual(sorted(z.namelist()), ["LICENSE", "edgepad.exe"])
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

    # AC5
    def test_the_exe_is_signed_through_signpath_when_set_up_and_noted_unsigned_when_not(self):
        signed = "vars.SIGNPATH_ORGANIZATION_ID != ''"
        sign = step("Sign with SignPath")
        self.assertIn(f"if: {signed}", sign)
        self.assertIn("uses: signpath/github-action-submit-signing-request@v1", sign)
        self.assertIn("organization-id: ${{ vars.SIGNPATH_ORGANIZATION_ID }}", sign)
        self.assertIn("wait-for-completion: true", sign)
        self.assertIn(f"if: {signed}", step("Unsigned exe for SignPath"))
        unsigned = step("Note an unsigned release")
        self.assertIn("if: vars.SIGNPATH_ORGANIZATION_ID == ''", unsigned)
        self.assertIn("::notice", unsigned)
        # The zip takes the signed exe when there is one.
        self.assertIn("signed/edgepad.exe", step("Zip"))

    # AC6
    def test_the_code_signing_policy_is_written_and_linked_from_the_release(self):
        policy = POLICY.read_text(encoding="utf-8")
        self.assertIn("Free code signing provided by SignPath.io, certificate by SignPath Foundation", policy)
        for role in ("Authors", "Reviewers", "Approvers"):
            self.assertRegex(policy, rf"{role}[^\n]*\[[^\]]+\]\(https://github\.com/[^)]+\)", role)
        self.assertRegex(policy, r"(?i)privacy")
        self.assertRegex(policy, r"(?i)network")
        self.assertIn("[Code signing policy](", step("Attach to the Release"))
        self.assertIn("docs/code-signing-policy.md", step("Attach to the Release"))


if __name__ == "__main__":
    unittest.main()

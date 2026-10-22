"""Build checks for Issue #1: workspace layout and the release exe.

Run from the repo root: python -m unittest ci/test_build.py
setUpClass builds the release exe, so the tests always look at a fresh one.
"""

import json
import pathlib
import struct
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "release" / "edgepad.exe"
MAX_EXE_BYTES = 500 * 1024
FORBIDDEN_DLL_PREFIXES = ("vcruntime", "ucrtbase", "api-ms-win-crt-")
IMAGE_SUBSYSTEM_WINDOWS_GUI = 2


def read_pe(path):
    """Return (subsystem, [imported dll names]) of a PE32+ image."""
    data = path.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("not a PE image")
    sections_count = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("not PE32+")
    subsystem = struct.unpack_from("<H", data, optional + 68)[0]
    import_rva = struct.unpack_from("<I", data, optional + 112 + 8)[0]

    sections = []
    table = optional + optional_size
    for i in range(sections_count):
        s = table + i * 40
        vsize, vaddr, raw_size, raw_ptr = struct.unpack_from("<IIII", data, s + 8)
        sections.append((vaddr, max(vsize, raw_size), raw_ptr))

    def offset(rva):
        for vaddr, size, raw_ptr in sections:
            if vaddr <= rva < vaddr + size:
                return rva - vaddr + raw_ptr
        raise ValueError(f"rva {rva:#x} outside sections")

    dlls = []
    if import_rva:
        desc = offset(import_rva)
        while True:
            fields = struct.unpack_from("<IIIII", data, desc)
            if not any(fields):
                break
            name = offset(fields[3])
            dlls.append(data[name:data.index(b"\0", name)].decode("ascii"))
            desc += 20
    return subsystem, dlls


class BuildTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Failures here surface in the individual tests, one per criterion.
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT)
        meta = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=ROOT, capture_output=True, text=True,
        )
        packages = json.loads(meta.stdout)["packages"] if meta.returncode == 0 else []
        cls.packages = {p["name"]: p for p in packages}

    # AC1
    def test_core_has_no_dependencies_and_bin_only_core_and_windows_sys(self):
        self.assertEqual(set(self.packages), {"edgepad-core", "edgepad"})
        self.assertEqual(self.packages["edgepad-core"]["dependencies"], [])
        bin_deps = {d["name"] for d in self.packages["edgepad"]["dependencies"]}
        self.assertEqual(bin_deps, {"edgepad-core", "windows-sys"})

    # AC2
    def test_release_exe_is_at_most_500kb(self):
        self.assertTrue(EXE.is_file(), f"{EXE} was not built")
        self.assertLessEqual(EXE.stat().st_size, MAX_EXE_BYTES)

    # AC3
    def test_release_exe_links_crt_statically(self):
        self.assertTrue(EXE.is_file(), f"{EXE} was not built")
        _, dlls = read_pe(EXE)
        bad = [d for d in dlls if d.lower().startswith(FORBIDDEN_DLL_PREFIXES)]
        self.assertEqual(bad, [], f"imports: {dlls}")

    # AC4
    def test_release_exe_is_gui_subsystem_and_exits_zero(self):
        self.assertTrue(EXE.is_file(), f"{EXE} was not built")
        subsystem, _ = read_pe(EXE)
        self.assertEqual(subsystem, IMAGE_SUBSYSTEM_WINDOWS_GUI)
        result = subprocess.run([str(EXE)], timeout=10)
        self.assertEqual(result.returncode, 0)

    # AC5
    def test_ci_runs_tests_and_these_checks_on_windows(self):
        path = ROOT / ".github" / "workflows" / "ci.yml"
        self.assertTrue(path.is_file(), f"{path} is missing")
        workflow = path.read_text(encoding="utf-8")
        for needle in ("pull_request", "push", "main", "windows-latest",
                       "cargo test", "ci/test_build.py"):
            self.assertIn(needle, workflow)


if __name__ == "__main__":
    unittest.main()

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
MAX_EXE_BYTES = 500 * 1024  # 500 KiB
IMAGE_FILE_MACHINE_AMD64 = 0x8664
FORBIDDEN_DLL_PREFIXES = ("vcruntime", "ucrtbase", "api-ms-win-crt-")
IMAGE_SUBSYSTEM_WINDOWS_GUI = 2


def read_pe(path):
    """Return (machine, subsystem, [imported dll names]) of a PE32+ image.

    Both the import and the delay-import directories are read. A truncated
    or malformed image raises ValueError.
    """
    try:
        return _read_pe(path.read_bytes())
    except (struct.error, IndexError, UnicodeDecodeError) as e:
        raise ValueError(f"malformed PE image: {e}") from e


def _read_pe(data):
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("not a PE image")
    machine, sections_count = struct.unpack_from("<HH", data, pe + 4)
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("not PE32+")
    subsystem = struct.unpack_from("<H", data, optional + 68)[0]
    directories = optional + 112
    import_rva = struct.unpack_from("<I", data, directories + 1 * 8)[0]
    delay_import_rva = struct.unpack_from("<I", data, directories + 13 * 8)[0]

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

    def dll_names(directory_rva, descriptor_format, name_field):
        names = []
        if directory_rva:
            desc = offset(directory_rva)
            while True:
                fields = struct.unpack_from(descriptor_format, data, desc)
                if not any(fields):
                    break
                name = offset(fields[name_field])
                names.append(data[name:data.index(b"\0", name)].decode("ascii"))
                desc += struct.calcsize(descriptor_format)
        return names

    # IMAGE_IMPORT_DESCRIPTOR: 5 dwords, Name is the 4th.
    # IMAGE_DELAYLOAD_DESCRIPTOR: 8 dwords, DllNameRVA is the 2nd.
    dlls = dll_names(import_rva, "<IIIII", 3) + dll_names(delay_import_rva, "<IIIIIIII", 1)
    return machine, subsystem, dlls


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
        machine, _, _ = read_pe(EXE)
        self.assertEqual(machine, IMAGE_FILE_MACHINE_AMD64)
        self.assertLessEqual(EXE.stat().st_size, MAX_EXE_BYTES)

    # AC3
    def test_release_exe_links_crt_statically(self):
        self.assertTrue(EXE.is_file(), f"{EXE} was not built")
        _, _, dlls = read_pe(EXE)
        bad = [d for d in dlls if d.lower().startswith(FORBIDDEN_DLL_PREFIXES)]
        self.assertEqual(bad, [], f"imports: {dlls}")

    # AC4 (the exe stays resident since #3, so only the subsystem is checked)
    def test_release_exe_is_gui_subsystem(self):
        self.assertTrue(EXE.is_file(), f"{EXE} was not built")
        _, subsystem, _ = read_pe(EXE)
        self.assertEqual(subsystem, IMAGE_SUBSYSTEM_WINDOWS_GUI)

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

"""Measure the release exe against the lightness targets (spec, chapter 8).

Launches a copy of target/release/edgepad.exe from a scratch folder with its
own %APPDATA%, so an EdgePad already running is neither met as the running
instance nor toggled. Times how long the panel takes to appear, waits idle,
then reads memory and CPU time and stops the process.
The startup time runs from just before the launch call, so it includes
process creation and errs on the slow side. KB and MB mean KiB and MiB.
Needs Windows 10 20H2 or later (the spec's floor is 21H2).

    python ci/measure.py              # markdown table, 60 s idle
    python ci/measure.py --idle 10    # shorter idle wait
    python ci/measure.py --json       # machine-readable
    python ci/measure.py --settings ci/example.ini   # with a settings file

Build first: cargo build --release
"""

import argparse
import ctypes
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import winreg
from ctypes import wintypes

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "release" / "edgepad.exe"
# Same class as crates/edgepad/src/panel.rs registers.
PANEL_CLASS = "EdgePadPanel"

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)

user32.FindWindowExW.restype = wintypes.HWND
user32.FindWindowExW.argtypes = [wintypes.HWND, wintypes.HWND, wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.IsWindowVisible.argtypes = [wintypes.HWND]
kernel32.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]

PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
PROCESS_VM_READ = 0x0010


class PROCESS_MEMORY_COUNTERS_EX(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("PageFaultCount", wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
        ("PrivateUsage", ctypes.c_size_t),
    ]


psapi.GetProcessMemoryInfo.argtypes = [
    wintypes.HANDLE, ctypes.POINTER(PROCESS_MEMORY_COUNTERS_EX), wintypes.DWORD,
]


def panels_visible(pid):
    """How many panels of process `pid` are on screen."""
    count, hwnd = 0, None
    while True:
        hwnd = user32.FindWindowExW(None, hwnd, PANEL_CLASS, None)
        if not hwnd:
            return count
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            count += 1


def panel_visible(pid):
    return panels_visible(pid) > 0


def cpu_ms(handle):
    times = [wintypes.FILETIME() for _ in range(4)]
    if not kernel32.GetProcessTimes(handle, *map(ctypes.byref, times)):
        raise ctypes.WinError(ctypes.get_last_error())
    as_int = lambda ft: ft.dwHighDateTime << 32 | ft.dwLowDateTime
    return (as_int(times[2]) + as_int(times[3])) / 10_000


def memory(handle):
    counters = PROCESS_MEMORY_COUNTERS_EX()
    counters.cb = ctypes.sizeof(counters)
    if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
        raise ctypes.WinError(ctypes.get_last_error())
    return counters.PrivateUsage, counters.WorkingSetSize


def environment():
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as key:
        cpu = winreg.QueryValueEx(key, "ProcessorNameString")[0].strip()
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") as key:
        display = winreg.QueryValueEx(key, "DisplayVersion")[0]
        build = winreg.QueryValueEx(key, "CurrentBuildNumber")[0]
    # Physical pixels of the primary monitor at the system DPI. Without the
    # awareness the figures would be virtualised.
    if not user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-2)):
        raise ctypes.WinError(ctypes.get_last_error())
    width, height = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
    dpi = user32.GetDpiForSystem()
    return {
        # The kernel still says 10 on Windows 11; build 22000 is where 11 begins.
        "windows": f"Windows {11 if int(build) >= 22000 else 10} {display} (build {build})",
        "cpu": cpu,
        "screen": f"{width}x{height}, {dpi} DPI ({dpi * 100 // 96}%)",
    }


def measure(idle, settings=None):
    if not EXE.is_file():
        sys.exit(f"{EXE} not found: run cargo build --release first")
    with tempfile.TemporaryDirectory(prefix="edgepad-measure-") as scratch:
        scratch = pathlib.Path(scratch)
        (scratch / "bin").mkdir()
        (scratch / "appdata").mkdir()
        exe = scratch / "bin" / "edgepad.exe"
        shutil.copy2(EXE, exe)
        if settings:
            shutil.copy2(settings, scratch / "bin" / "edgepad.ini")
        env = os.environ | {"APPDATA": str(scratch / "appdata")}
        return run(exe, env, idle)


def run(exe, env, idle):
    started = time.perf_counter()
    app = subprocess.Popen([str(exe)], env=env)
    try:
        while not panel_visible(app.pid):
            if app.poll() is not None:
                sys.exit(f"edgepad.exe exited with {app.returncode} before showing its panel")
            if time.perf_counter() - started > 10:
                sys.exit("no panel within 10 s")
            time.sleep(0.002)
        startup_ms = (time.perf_counter() - started) * 1000
        handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, False, app.pid)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            before = cpu_ms(handle)
            time.sleep(idle)
            idle_cpu = cpu_ms(handle) - before
            private, working_set = memory(handle)
            panels = panels_visible(app.pid)
        finally:
            kernel32.CloseHandle(handle)
    finally:
        app.kill()
        app.wait()
    return {
        "exe_bytes": EXE.stat().st_size,
        "startup_ms": round(startup_ms, 1),
        "private_bytes": private,
        "working_set_bytes": working_set,
        "idle_seconds": idle,
        "idle_cpu_ms": round(idle_cpu, 1),
        "panels": panels,
    }


def table(r, env):
    kib = lambda b: f"{b / 1024:,.0f} KiB"
    mib = lambda b: f"{b / 1024 / 1024:.2f} MiB"
    rows = [
        ("exe size", kib(r["exe_bytes"]), "500 KiB or less"),
        ("startup (launch to panel shown)", f'{r["startup_ms"]} ms', "100 ms or less"),
        ("private memory", mib(r["private_bytes"]), "2 MiB or less"),
        ("working set", mib(r["working_set_bytes"]), "5 MiB or less"),
        (f'idle CPU time over {r["idle_seconds"]} s', f'{r["idle_cpu_ms"]} ms', "0% (no timers, no polling)"),
    ]
    lines = [f"{k}: {v}" for k, v in env.items()]
    lines += ["", "| exe measurement | value | target |", "|---|---|---|"]
    lines += [f"| {a} | {b} | {c} |" for a, b, c in rows]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--idle", type=int, default=60, help="seconds to wait idle (default 60)")
    parser.add_argument("--json", action="store_true", help="print JSON instead of a table")
    parser.add_argument("--settings", type=pathlib.Path, help="settings file to put beside the exe")
    args = parser.parse_args()
    if args.settings and not args.settings.is_file():
        parser.error(f"--settings {args.settings}: no such file")
    if args.idle < 1:
        parser.error("--idle must be at least 1 second")
    # Environment first: a failure here should not throw away a long run.
    env = environment()
    result = measure(args.idle, args.settings)
    if args.json:
        print(json.dumps(result | {"environment": env}))
    else:
        print(table(result, env))


if __name__ == "__main__":
    main()

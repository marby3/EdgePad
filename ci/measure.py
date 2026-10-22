"""Measure the release exe against the lightness targets (spec, chapter 8).

Launches target/release/edgepad.exe, times how long the panel takes to
appear, waits idle, then reads memory and CPU time and stops the process.

    python ci/measure.py              # markdown table, 60 s idle
    python ci/measure.py --idle 10    # shorter idle wait
    python ci/measure.py --json       # machine-readable

Build first: cargo build --release
"""

import argparse
import ctypes
import json
import pathlib
import platform
import subprocess
import sys
import time
import winreg
from ctypes import wintypes

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "release" / "edgepad.exe"
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


def panel_visible(pid):
    hwnd = None
    while True:
        hwnd = user32.FindWindowExW(None, hwnd, PANEL_CLASS, None)
        if not hwnd:
            return False
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            return True


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
    # Physical pixels, as EdgePad (DPI aware) sees them.
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-2))
    width, height = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
    dpi = user32.GetDpiForSystem()
    return {
        "windows": f"{platform.system()} {platform.release()} {display} (build {build})",
        "cpu": cpu,
        "screen": f"{width}x{height}, {dpi} DPI ({dpi * 100 // 96}%)",
    }


def measure(idle):
    if not EXE.is_file():
        sys.exit(f"{EXE} not found: run cargo build --release first")
    started = time.perf_counter()
    app = subprocess.Popen([str(EXE)])
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
    }


def table(r, env):
    kib = lambda b: f"{b / 1024:,.0f} KiB"
    mib = lambda b: f"{b / 1024 / 1024:.2f} MiB"
    rows = [
        ("exe size", kib(r["exe_bytes"]), "500 KB or less"),
        ("startup (launch to panel shown)", f'{r["startup_ms"]} ms', "100 ms or less"),
        ("private memory", mib(r["private_bytes"]), "2 MB or less"),
        ("working set", mib(r["working_set_bytes"]), "5 MB or less"),
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
    args = parser.parse_args()
    result = measure(args.idle)
    if args.json:
        print(json.dumps(result | {"environment": environment()}))
    else:
        print(table(result, environment()))


if __name__ == "__main__":
    main()

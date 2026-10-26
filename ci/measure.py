"""Measure the release exe against the lightness targets (spec, chapter 8).

Launches target/release/edgepad.exe with a scratch folder as its %APPDATA%,
so an EdgePad already running is neither met as the running instance nor
toggled. The exe runs where it is: a fresh copy would be virus-scanned on
its first start and inflate the startup time.

Times how long the panel takes to appear, waits idle, then reads memory and
CPU time and stops the process. The startup time runs from just before the
launch call, so it includes process creation and errs on the slow side.

With --settings-window, the settings window is then opened from the tray and
closed again five times, and the private memory read while it is open and
after each closing; and whether the common controls (comctl32 v6) were
loaded before it first opened.

With --settings, the hotkey the file names is checked as well: free before
the launch, held once EdgePad is up. That shows the settings were applied
(a rejected file falls back to the fixed keys, which have no hotkey). If
another app already holds that hotkey, the run says so instead.

KB and MB mean KiB and MiB. Needs Windows 10 20H2 or later (the spec's floor
is 21H2).

    python ci/measure.py              # markdown table, 60 s idle
    python ci/measure.py --idle 10    # shorter idle wait
    python ci/measure.py --json       # machine-readable
    python ci/measure.py --settings ci/example.ini   # with a settings file
    python ci/measure.py --settings-window           # and the settings window

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
# Same classes as crates/edgepad/src/panel.rs, tray.rs and editor.rs register.
PANEL_CLASS = "EdgePadPanel"
TRAY_CLASS = "EdgePadTray"
EDITOR_CLASS = "EdgePadSettings"
# The tray icon's callback message (tray.rs), and the item that opens the
# settings window.
TRAY_CALLBACK = 0x8000 + 1
SETTINGS_ITEM = "設定…"
# How often the settings window is opened and closed, and how long it is
# given to settle each time: Windows loads its text input in its own time.
OPENINGS = 5
SETTLE = 1.5

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

PROCESS_QUERY_INFORMATION = 0x0400
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


def windows_of(pid, class_name):
    """The top-level windows of `class_name` that process `pid` owns."""
    found, hwnd = [], None
    while True:
        hwnd = user32.FindWindowExW(None, hwnd, class_name, None)
        if not hwnd:
            return found
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid:
            found.append(hwnd)


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


psapi.EnumProcessModulesEx.argtypes = [
    wintypes.HANDLE, ctypes.POINTER(wintypes.HMODULE), wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), wintypes.DWORD,
]
psapi.GetModuleFileNameExW.argtypes = [wintypes.HANDLE, wintypes.HMODULE, wintypes.LPWSTR, wintypes.DWORD]
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.SendMessageW.restype = wintypes.HMENU
user32.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.GetMenuItemCount.argtypes = [wintypes.HMENU]
user32.GetMenuItemID.restype = wintypes.UINT
user32.GetMenuItemID.argtypes = [wintypes.HMENU, ctypes.c_int]
user32.GetMenuStringW.argtypes = [wintypes.HMENU, wintypes.UINT, wintypes.LPWSTR, ctypes.c_int, wintypes.UINT]
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.IsWindow.argtypes = [wintypes.HWND]

WM_CLOSE = 0x0010
WM_COMMAND = 0x0111
WM_KEYDOWN = 0x0100
WM_RBUTTONUP = 0x0205
MN_GETHMENU = 0x01E1
MF_BYPOSITION = 0x400
VK_ESCAPE = 0x1B
VK_MENU = 0x12
KEYEVENTF_KEYUP = 0x2
MOUSEEVENTF_LEFTDOWN = 0x2
MOUSEEVENTF_LEFTUP = 0x4
LIST_MODULES_ALL = 0x3


def comctl32_v6(handle):
    """Whether the process has comctl32 version 6 loaded: the one in WinSxS."""
    modules = (wintypes.HMODULE * 1024)()
    needed = wintypes.DWORD()
    if not psapi.EnumProcessModulesEx(handle, modules, ctypes.sizeof(modules), ctypes.byref(needed), LIST_MODULES_ALL):
        raise ctypes.WinError(ctypes.get_last_error())
    name = ctypes.create_unicode_buffer(520)
    for module in modules[: needed.value // ctypes.sizeof(wintypes.HMODULE)]:
        if not psapi.GetModuleFileNameExW(handle, module, name, 520):
            raise ctypes.WinError(ctypes.get_last_error())
        path = name.value.lower()
        if "microsoft.windows.common-controls" in path and path.endswith("comctl32.dll"):
            return True
    return False


def wait_for(found, seconds, what):
    """The first of what `found` returns, waiting up to `seconds` for it."""
    end = time.perf_counter() + seconds
    while True:
        got = found()
        if got:
            return got[0]
        if time.perf_counter() > end:
            sys.exit(f"{what}: not within {seconds} s")
        time.sleep(0.02)


def shown(pid, class_name):
    return [w for w in windows_of(pid, class_name) if user32.IsWindowVisible(w)]


def settings_command(pid):
    """The tray window of process `pid`, and the command id of 設定… in its
    menu: the menu opened, read, and closed again."""
    tray = wait_for(lambda: windows_of(pid, TRAY_CLASS), 5, "the tray window")
    user32.PostMessageW(tray, TRAY_CALLBACK, 0, WM_RBUTTONUP)
    window = wait_for(lambda: shown(pid, "#32768"), 5, "the tray menu opens")
    menu = user32.SendMessageW(window, MN_GETHMENU, 0, 0)
    command = None
    text = ctypes.create_unicode_buffer(64)
    for i in range(user32.GetMenuItemCount(menu)):
        user32.GetMenuStringW(menu, i, text, 64, MF_BYPOSITION)
        if SETTINGS_ITEM in text.value:
            command = user32.GetMenuItemID(menu, i)
    user32.PostMessageW(window, WM_KEYDOWN, VK_ESCAPE, 0)
    time.sleep(0.2)
    if command is None:
        sys.exit(f"no {SETTINGS_ITEM} in the tray menu")
    return tray, command


def to_front(hwnd):
    """Bring `hwnd` to the front, as a user's click would: a window takes
    the focus, and Windows loads its text input. A tap of Alt first makes
    this process the one with the last input, which may set the front;
    failing that, a click in the middle of the window, on no part of it.
    Whether it came to the front."""
    for _ in range(3):
        user32.keybd_event(VK_MENU, 0, 0, 0)
        user32.keybd_event(VK_MENU, 0, KEYEVENTF_KEYUP, 0)
        user32.SetForegroundWindow(hwnd)
        time.sleep(0.2)
        if user32.GetForegroundWindow() == hwnd:
            return True
    r = wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(r))
    user32.SetCursorPos((r.left + r.right) // 2, (r.top + r.bottom) // 2)
    user32.mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
    user32.mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)
    time.sleep(0.3)
    return user32.GetForegroundWindow() == hwnd


def settings_window(pid, handle):
    """Open the settings window and close it OPENINGS times, reading the
    private memory each time it is open and once it is closed."""
    at_start = comctl32_v6(handle)
    tray, command = settings_command(pid)
    opened, closed, in_front, when_open = [], [], [], False
    for _ in range(OPENINGS):
        user32.PostMessageW(tray, WM_COMMAND, command, 0)
        window = wait_for(lambda: shown(pid, EDITOR_CLASS), 5, "the settings window opens")
        in_front.append(to_front(window))
        time.sleep(SETTLE)
        opened.append(memory(handle)[0])
        when_open = when_open or comctl32_v6(handle)
        user32.PostMessageW(window, WM_CLOSE, 0, 0)
        wait_for(lambda: [True] if not user32.IsWindow(window) else [], 5, "the settings window closes")
        time.sleep(SETTLE)
        closed.append(memory(handle)[0])
    return {
        "comctl32_at_start": at_start,
        "comctl32_when_open": when_open,
        "in_front": in_front,
        "open_private_bytes": opened,
        "closed_private_bytes": closed,
    }


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


user32.RegisterHotKey.argtypes = [wintypes.HWND, ctypes.c_int, wintypes.UINT, wintypes.UINT]
user32.UnregisterHotKey.argtypes = [wintypes.HWND, ctypes.c_int]
MODS = {"ctrl": 0x2, "shift": 0x4, "alt": 0x1, "win": 0x8}
MOD_NOREPEAT = 0x4000
PROBE_ID = 0x4550  # "EP"


def hotkey_of(settings):
    """The (modifiers, virtual key) of `[general] hotkey` in the settings
    file, for a letter, digit or F-key, or None."""
    section = None
    for line in pathlib.Path(settings).read_text(encoding="utf-8-sig").splitlines():
        line = line.split(";")[0].strip()
        if line.startswith("["):
            section = line.strip("[]").strip().lower()
        elif section == "general" and "=" in line:
            name, value = (p.strip() for p in line.split("=", 1))
            if name.lower() != "hotkey" or value.lower() == "none":
                continue
            *mods, key = [p.strip() for p in value.split("+")]
            vk = None
            if len(key) == 1 and key.isalnum():
                vk = ord(key.upper())
            elif key[:1].upper() == "F" and key[1:].isdigit() and 1 <= int(key[1:]) <= 24:
                vk = 0x6F + int(key[1:])
            if vk is None or any(m.lower() not in MODS for m in mods):
                return None
            return sum(MODS[m.lower()] for m in mods), vk
    return None


def hotkey_free(hotkey):
    """Whether no one holds `hotkey`: this process can take it (and gives it back)."""
    mods, vk = hotkey
    if user32.RegisterHotKey(None, PROBE_ID, mods | MOD_NOREPEAT, vk):
        user32.UnregisterHotKey(None, PROBE_ID)
        return True
    return False


def measure(idle, settings=None, window=False):
    if not EXE.is_file():
        sys.exit(f"{EXE} not found: run cargo build --release first")
    if settings and not pathlib.Path(settings).is_file():
        sys.exit(f"--settings {settings}: no such file")
    # A settings file beside the exe would win over the one given here.
    beside = EXE.with_name("edgepad.ini")
    if beside.exists():
        sys.exit(f"{beside} would be read instead: move it away to measure")
    with tempfile.TemporaryDirectory(prefix="edgepad-measure-") as appdata:
        if settings:
            folder = pathlib.Path(appdata) / "EdgePad"
            folder.mkdir()
            shutil.copy2(settings, folder / "edgepad.ini")
        hotkey = hotkey_of(settings) if settings else None
        free_before = hotkey_free(hotkey) if hotkey else None
        env = os.environ | {"APPDATA": appdata}
        result = run(env, idle, hotkey, window)
    if hotkey is None:
        result["hotkey"] = "none"
    elif not free_before:
        result["hotkey"] = "taken by another app"
    else:
        result["hotkey"] = "registered" if result.pop("hotkey_held") else "not registered"
    result.pop("hotkey_held", None)
    return result


def run(env, idle, hotkey, window=False):
    started = time.perf_counter()
    app = subprocess.Popen([str(EXE)], env=env)
    try:
        while not panel_visible(app.pid):
            if app.poll() is not None:
                sys.exit(f"edgepad.exe exited with {app.returncode} before showing its panel")
            if time.perf_counter() - started > 10:
                sys.exit("no panel within 10 s")
            time.sleep(0.002)
        startup_ms = (time.perf_counter() - started) * 1000
        handle = kernel32.OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, False, app.pid)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            before = cpu_ms(handle)
            time.sleep(idle)
            idle_cpu = cpu_ms(handle) - before
            private, working_set = memory(handle)
            panels = panels_visible(app.pid)
            held = hotkey is not None and not hotkey_free(hotkey)
            # Last: it loads what stays in the process once the window closes.
            opened = settings_window(app.pid, handle) if window else None
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
        "hotkey_held": held,
    } | ({"settings_window": opened} if opened else {})


def table(r, env):
    kib = lambda b: f"{b / 1024:,.0f} KiB"
    mib = lambda b: f"{b / 1024 / 1024:.2f} MiB"
    rows = [
        ("exe size", kib(r["exe_bytes"]), "1,024 KiB (1 MiB) or less"),
        ("startup (launch to panel shown)", f'{r["startup_ms"]} ms', "100 ms or less"),
        ("private memory", mib(r["private_bytes"]), "2.5 MiB or less"),
        ("working set", mib(r["working_set_bytes"]), "5 MiB or less"),
        (f'idle CPU time over {r["idle_seconds"]} s', f'{r["idle_cpu_ms"]} ms', "0% (no timers, no polling)"),
        ("panels shown", str(r["panels"]), "as the settings say"),
        ("hotkey of the settings", r["hotkey"], "registered, when there is one"),
    ]
    window = r.get("settings_window")
    if window:
        rows += [
            ("comctl32 v6 before the settings window opens", str(window["comctl32_at_start"]), "False"),
            ("private memory, settings window open (most)", mib(max(window["open_private_bytes"])), "8 MiB or less"),
            (
                "private memory, settings window closed (each time)",
                ", ".join(mib(b) for b in window["closed_private_bytes"]),
                "4 MiB or less, not growing",
            ),
        ]
    lines = [f"{k}: {v}" for k, v in env.items()]
    lines += ["", "| exe measurement | value | target |", "|---|---|---|"]
    lines += [f"| {a} | {b} | {c} |" for a, b, c in rows]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--idle", type=int, default=60, help="seconds to wait idle (default 60)")
    parser.add_argument("--json", action="store_true", help="print JSON instead of a table")
    parser.add_argument("--settings", type=pathlib.Path, help="settings file to measure with")
    parser.add_argument("--settings-window", action="store_true", help="open and close the settings window too")
    args = parser.parse_args()
    if args.settings and not args.settings.is_file():
        parser.error(f"--settings {args.settings}: no such file")
    if args.idle < 1:
        parser.error("--idle must be at least 1 second")
    # Environment first: a failure here should not throw away a long run.
    env = environment()
    result = measure(args.idle, args.settings, args.settings_window)
    if args.json:
        print(json.dumps(result | {"environment": env}))
    else:
        print(table(result, env))


if __name__ == "__main__":
    main()

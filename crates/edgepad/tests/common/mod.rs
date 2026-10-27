//! Helpers shared by the acceptance tests: launch the real exe, find its
//! panel, pump messages, read the screen. Tests share the desktop, so every
//! test holds `desktop()` for its whole run.

#![allow(dead_code)]

use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::System::Threading::{
    WaitForInputIdle,
    AttachThreadInput, GetCurrentThreadId, GetProcessTimes,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Controls::{
    CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, POINTER_FEEDBACK_NONE,
    POINTER_TYPE_INFO,
};
use windows_sys::Win32::UI::Input::Pointer::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub mod editor;

pub const PANEL_CLASS: &str = "EdgePadPanel";
pub const KEY_LOGICAL_PX: i32 = 56;
pub const KEY_COUNT: i32 = 5;

pub static DESKTOP: Mutex<()> = Mutex::new(());
pub static DPI: Once = Once::new();

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Serialise tests and make this process see physical pixels like the exe
/// does: Per-Monitor V2, so no coordinate is ever scaled for it.
pub fn desktop() -> MutexGuard<'static, ()> {
    DPI.call_once(|| {
        let ok = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        assert!(ok != 0, "SetProcessDpiAwarenessContext failed: coordinates would be virtualised");
    });
    DESKTOP.lock().unwrap_or_else(|e| e.into_inner())
}

/// A running edgepad.exe, and the scratch folder it was given, if any.
/// Dropping it ends the process and removes the folder.
pub struct Running(pub Child, pub Option<PathBuf>);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        if let Some(dir) = &self.1 {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// A fresh, empty folder for one launch.
fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("edgepad-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch folder");
    dir
}

/// Start `exe` with %APPDATA% pointing at `appdata`, and wait until its UI
/// thread first sits idle in its message loop. EdgePad creates the panel and
/// the tray before entering that loop.
fn start(exe: &Path, appdata: &Path, dir: Option<PathBuf>, args: &[&str]) -> Running {
    let child = Command::new(exe).args(args).env("APPDATA", appdata).spawn().expect("launch edgepad.exe");
    let app = Running(child, dir);
    let idle = unsafe { WaitForInputIdle(app.0.as_raw_handle(), 10_000) };
    assert_eq!(idle, 0, "edgepad.exe did not become input-idle (timeout or exit)");
    app
}

/// Launch the exe with no settings file. It runs from a scratch copy, so a
/// settings file beside the built exe or in this machine's %APPDATA% cannot
/// leak in.
pub fn launch() -> Running {
    launch_with(None, None)
}

/// Launch a copy of the exe from a scratch folder, with `beside_exe` written
/// as edgepad.ini next to it and `in_appdata` as %APPDATA%\EdgePad\edgepad.ini.
pub fn launch_with(beside_exe: Option<&str>, in_appdata: Option<&str>) -> Running {
    launch_with_args(beside_exe, in_appdata, &[])
}

/// As `launch_with`, passing `args` on the command line.
pub fn launch_with_args(beside_exe: Option<&str>, in_appdata: Option<&str>, args: &[&str]) -> Running {
    let dir = scratch_dir();
    let (bin, appdata) = (dir.join("bin"), dir.join("appdata"));
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(appdata.join("EdgePad")).unwrap();
    let exe = bin.join("edgepad.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_edgepad"), &exe).expect("copy edgepad.exe");
    if let Some(text) = beside_exe {
        std::fs::write(bin.join("edgepad.ini"), text).unwrap();
    }
    if let Some(text) = in_appdata {
        std::fs::write(appdata.join("EdgePad").join("edgepad.ini"), text).unwrap();
    }
    start(&exe, &appdata, Some(dir), args)
}

/// The folder holding the copy of the exe that `app` runs.
pub fn exe_dir(app: &Running) -> PathBuf {
    app.1.as_ref().expect("launched from a scratch folder").join("bin")
}

/// The folder `app` was given as %APPDATA%.
pub fn appdata_dir(app: &Running) -> PathBuf {
    app.1.as_ref().expect("launched from a scratch folder").join("appdata")
}

/// Start the same exe as `app` again, with the same %APPDATA%, and wait for
/// that second process to end.
pub fn launch_again(app: &Running) -> std::process::ExitStatus {
    let mut child = Command::new(exe_dir(app).join("edgepad.exe"))
        .env("APPDATA", appdata_dir(app))
        .spawn()
        .expect("launch edgepad.exe again");
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if let Some(status) = child.try_wait().expect("try_wait") {
            pump(200);
            return status;
        }
        pump(50);
    }
    let _ = child.kill();
    panic!("the second edgepad.exe was still running after 5s");
}

/// What the test build of `app` recorded: one line per tray notification
/// ("notice: ...") and per file it would have opened ("open: <path>").
pub fn test_events(app: &Running) -> Vec<String> {
    let text = std::fs::read_to_string(appdata_dir(app).join("edgepad-test-events.txt")).unwrap_or_default();
    text.lines().map(String::from).collect()
}

/// One item of the tray menu: its text, command id, checked, greyed.
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub text: String,
    pub id: u32,
    pub checked: bool,
    pub grayed: bool,
}

pub const TRAY_CLASS: &str = "EdgePadTray";
pub const TRAY_CALLBACK: u32 = WM_APP + 1;

/// Open the tray menu of `app`, read its items, and close it again.
pub fn tray_menu(app: &Running) -> Vec<MenuItem> {
    let tray = find_window(app, TRAY_CLASS, false);
    unsafe { PostMessageW(tray, TRAY_CALLBACK, 0, WM_RBUTTONUP as isize) };
    let menu_window = find_window(app, "#32768", true);
    let menu = unsafe { SendMessageW(menu_window, MN_GETHMENU, 0, 0) } as HMENU;
    let mut items = Vec::new();
    for i in 0..unsafe { GetMenuItemCount(menu) } {
        let state = unsafe { GetMenuState(menu, i as u32, MF_BYPOSITION) };
        if state & MF_SEPARATOR != 0 {
            continue;
        }
        let mut text = [0u16; 64];
        let n = unsafe { GetMenuStringW(menu, i as u32, text.as_mut_ptr(), 64, MF_BYPOSITION) };
        items.push(MenuItem {
            text: String::from_utf16_lossy(&text[..n.max(0) as usize]),
            id: unsafe { GetMenuItemID(menu, i) },
            checked: state & MF_CHECKED != 0,
            grayed: state & MF_GRAYED != 0,
        });
    }
    unsafe { PostMessageW(menu_window, WM_KEYDOWN, VK_ESCAPE as usize, 0) };
    pump(200);
    items
}

/// The tray menu item of `app` whose text contains `text`.
pub fn menu_item(app: &Running, text: &str) -> MenuItem {
    let items = tray_menu(app);
    items.iter().find(|i| i.text.contains(text)).cloned().unwrap_or_else(|| panic!("no {text} in the tray menu: {items:?}"))
}

/// Choose the tray menu item of `app` whose text contains `text`.
pub fn choose(app: &Running, text: &str) {
    let id = menu_item(app, text).id;
    let tray = find_window(app, TRAY_CLASS, false);
    unsafe { PostMessageW(tray, WM_COMMAND, id as usize, 0) };
    pump(300);
}

/// Pump this thread's messages for `ms` so test windows stay painted.
pub fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        unsafe {
            let mut msg: MSG = zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for a top-level window of class `class` owned by `app` to appear.
/// With `visible`, only a visible one counts.
pub fn find_window(app: &Running, class: &str, visible: bool) -> HWND {
    let name = wide(class);
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        let mut hwnd = null_mut();
        loop {
            hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
            if hwnd.is_null() {
                break;
            }
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            if pid == app.0.id() && (!visible || unsafe { IsWindowVisible(hwnd) } != 0) {
                return hwnd;
            }
        }
        pump(50);
    }
    panic!("no {class} window from edgepad.exe within 5s");
}

/// Wait for the panel of `app` to appear and return its window.
///
/// A touch that lands in the first moments after the panel appears can miss
/// it: the system's touch hit-testing learns of a new window only once the
/// desktop compositor has drawn it, and until then the contact is turned into
/// mouse input instead. Waiting for the next composition pass closes that
/// gap (measured: 4 of 10 first touches lost without it, 0 of 12 with it).
pub fn find_panel(app: &Running) -> HWND {
    let panel = find_window(app, PANEL_CLASS, true);
    assert_eq!(unsafe { DwmFlush() }, 0, "DwmFlush failed");
    panel
}

pub fn window_rect(hwnd: HWND) -> RECT {
    let mut r: RECT = unsafe { zeroed() };
    unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

/// The client area of `hwnd`, on the screen.
pub fn client_rect(hwnd: HWND) -> RECT {
    let mut r: RECT = unsafe { zeroed() };
    unsafe {
        GetClientRect(hwnd, &mut r);
        MapWindowPoints(hwnd, null_mut(), &mut r as *mut RECT as *mut POINT, 2);
    }
    r
}

pub fn primary_monitor() -> RECT {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        info.rcMonitor
    }
}

pub fn key_px() -> i32 {
    px(KEY_LOGICAL_PX)
}

/// Copy a screen rectangle, layered windows included, as 0x00RRGGBB pixels.
pub fn capture(r: RECT) -> Vec<u32> {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        let mut bits = null_mut();
        let dib = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let old = SelectObject(mem, dib);
        BitBlt(mem, 0, 0, w, h, screen, r.left, r.top, SRCCOPY | CAPTUREBLT);
        let pixels = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize)
            .iter()
            .map(|p| p & 0x00FF_FFFF)
            .collect();
        SelectObject(mem, old);
        DeleteObject(dib);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        pixels
    }
}

pub fn channels(p: u32) -> [i32; 3] {
    [(p >> 16 & 0xFF) as i32, (p >> 8 & 0xFF) as i32, (p & 0xFF) as i32]
}

pub static BACKDROP_COLOR: AtomicU32 = AtomicU32::new(0);

pub unsafe extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_PAINT {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let brush = CreateSolidBrush(BACKDROP_COLOR.load(Ordering::Relaxed));
            FillRect(dc, &ps.rcPaint, brush);
            DeleteObject(brush);
            EndPaint(hwnd, &ps);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

/// A test window, destroyed on drop even when an assertion fails.
pub struct TestWindow(pub HWND);

impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

/// A plain test window of a single colour.
pub fn test_window(class: &str, ex_style: u32, r: RECT) -> TestWindow {
    unsafe {
        let class = wide(class);
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(backdrop_proc);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            ex_style,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        assert!(!hwnd.is_null(), "create test window");
        UpdateWindow(hwnd);
        pump(100);
        TestWindow(hwnd)
    }
}

pub fn set_backdrop(hwnd: HWND, color: u32) {
    BACKDROP_COLOR.store(color, Ordering::Relaxed);
    unsafe {
        InvalidateRect(hwnd, null(), 1);
        UpdateWindow(hwnd);
    }
    pump(300);
}

pub fn cpu_time_100ns(app: &Running) -> u64 {
    let ft = |f: FILETIME| (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64;
    unsafe {
        let (mut c, mut e, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) =
            (zeroed(), zeroed(), zeroed(), zeroed());
        let ok = GetProcessTimes(app.0.as_raw_handle(), &mut c, &mut e, &mut k, &mut u);
        assert!(ok != 0, "GetProcessTimes");
        ft(k) + ft(u)
    }
}

/// Move the mouse to `p` and apply `buttons` in one injected event, so a
/// real mouse moving at the same time cannot put the click somewhere else.
pub fn mouse_at(p: POINT, buttons: MOUSE_EVENT_FLAGS) {
    unsafe {
        let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let mut input: INPUT = zeroed();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.dx = (p.x * 65535 + (w - 1) / 2) / (w - 1);
        input.Anonymous.mi.dy = (p.y * 65535 + (h - 1) / 2) / (h - 1);
        input.Anonymous.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | buttons;
        SendInput(1, &input, size_of::<INPUT>() as i32);
    }
}

pub fn click(p: POINT) {
    mouse_at(p, MOUSEEVENTF_LEFTDOWN);
    mouse_at(p, MOUSEEVENTF_LEFTUP);
}

/// Bring `hwnd` to the foreground even though this process is in the background.
pub fn force_foreground(hwnd: HWND) {
    unsafe {
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, null_mut());
        let me = GetCurrentThreadId();
        AttachThreadInput(me, fg_thread, 1);
        SetForegroundWindow(hwnd);
        AttachThreadInput(me, fg_thread, 0);
    }
    pump(200);
}

/// Bring `hwnd`, of this process or another, to the front, and fail with
/// what is in front instead if it cannot be done within 2 s.
///
/// Windows lets only the process that last had input set the foreground,
/// and the CI machines differ in what that is: on some, a window of another
/// process keeps the front whatever this process does. So this tries in
/// turn: SetForegroundWindow (`force_foreground`); a tap of Alt, which makes
/// this process the one with the last input, then SetForegroundWindow
/// again; a click on the window.
pub fn wait_in_front(hwnd: HWND) {
    let in_front = |ms: u64| {
        let end = Instant::now() + Duration::from_millis(ms);
        while unsafe { GetForegroundWindow() } != hwnd && Instant::now() < end {
            pump(50);
        }
        let front = unsafe { GetForegroundWindow() };
        front == hwnd
    };
    if in_front(300) {
        return;
    }
    force_foreground(hwnd);
    if in_front(300) {
        return;
    }
    keyboard(VK_MENU, true);
    keyboard(VK_MENU, false);
    unsafe { SetForegroundWindow(hwnd) };
    if in_front(300) {
        return;
    }
    let r = window_rect(hwnd);
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    click(POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 });
    let done = in_front(1000);
    unsafe { SetCursorPos(saved.x, saved.y) };
    if !done {
        let front = unsafe { GetForegroundWindow() };
        let mut class = [0u16; 128];
        let n = unsafe { GetClassNameW(front, class.as_mut_ptr(), 128) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(front, &mut pid) };
        let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        panic!("the window did not come to the front: {front:?} of class {class:?}, process {pid}, is in front");
    }
}

// ---- Key output and pointer input ----

pub const VK: [u16; 5] = [VK_ESCAPE, VK_LCONTROL, VK_LSHIFT, VK_LMENU, VK_SPACE];
pub const ESC: usize = 0;
pub const CTRL: usize = 1;
pub const SHIFT: usize = 2;
pub const ALT: usize = 3;
pub const SPACE: usize = 4;

/// (virtual key, is key-up, dwExtraInfo) of one injected key event.
pub type KeyEvent = (u16, bool, usize);

pub static EVENTS: Mutex<Vec<KeyEvent>> = Mutex::new(Vec::new());

/// (virtual key, scan code, is extended) of each recorded key event.
pub static SCANS: Mutex<Vec<(u16, u32, bool)>> = Mutex::new(Vec::new());

/// dwExtraInfo of key events a test sends in place of the physical keyboard.
/// The hook lets them through unrecorded, so Windows sees the key as down.
pub const KEYBOARD_TAG: usize = 0x5445_5354;

pub unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let k = &*(lp as *const KBDLLHOOKSTRUCT);
            if k.flags & LLKHF_INJECTED != 0 && k.dwExtraInfo != KEYBOARD_TAG {
                EVENTS.lock().unwrap().push((k.vkCode as u16, k.flags & LLKHF_UP != 0, k.dwExtraInfo));
                SCANS.lock().unwrap().push((k.vkCode as u16, k.scanCode, k.flags & LLKHF_EXTENDED != 0));
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, wp, lp)
    }
}

/// Press or release `vk` as if on the physical keyboard (see `KEYBOARD_TAG`).
pub fn keyboard(vk: u16, down: bool) {
    unsafe {
        let mut input: INPUT = zeroed();
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.dwFlags = if down { 0 } else { KEYEVENTF_KEYUP };
        input.Anonymous.ki.dwExtraInfo = KEYBOARD_TAG;
        assert_eq!(SendInput(1, &input, size_of::<INPUT>() as i32), 1, "SendInput");
    }
    pump(100);
}

/// Records and swallows injected key events while alive.
pub struct KeyLog(pub HHOOK);

impl KeyLog {
    pub fn start() -> KeyLog {
        EVENTS.lock().unwrap().clear();
        SCANS.lock().unwrap().clear();
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), null_mut(), 0) };
        assert!(!hook.is_null(), "install keyboard hook");
        KeyLog(hook)
    }

    /// Key events so far as (virtual key, is key-up).
    pub fn keys(&self) -> Vec<(u16, bool)> {
        EVENTS.lock().unwrap().iter().map(|&(vk, up, _)| (vk, up)).collect()
    }

    pub fn all(&self) -> Vec<KeyEvent> {
        EVENTS.lock().unwrap().clone()
    }

    /// Key events so far as (virtual key, scan code, is extended).
    pub fn scans(&self) -> Vec<(u16, u32, bool)> {
        SCANS.lock().unwrap().clone()
    }

    /// Pump until `n` key events have been recorded, or 2 s have passed.
    /// Windows hands this thread each event in turn and waits for it, so a
    /// long burst from one SendInput takes a while to come through.
    pub fn wait_for(&self, n: usize) {
        let end = Instant::now() + Duration::from_secs(2);
        while EVENTS.lock().unwrap().len() < n && Instant::now() < end {
            pump(20);
        }
    }
}

impl Drop for KeyLog {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

pub fn down(key: usize) -> (u16, bool) {
    (VK[key], false)
}

pub fn up(key: usize) -> (u16, bool) {
    (VK[key], true)
}

/// Centre of key `i` of the panel.
pub fn key_centre(panel: RECT, i: usize) -> POINT {
    let size = panel.right - panel.left;
    POINT { x: panel.left + size / 2, y: panel.top + size * i as i32 + size / 2 }
}

/// A point well outside the panel, level with key `i`.
pub fn outside(panel: RECT, i: usize) -> POINT {
    let p = key_centre(panel, i);
    POINT { x: panel.right + 300, y: p.y }
}

pub fn mouse_down_at(p: POINT) {
    mouse_at(p, MOUSEEVENTF_LEFTDOWN);
    pump(100);
}

pub fn mouse_move_to(p: POINT) {
    mouse_at(p, 0);
    pump(100);
}

pub fn mouse_up_at(p: POINT) {
    mouse_at(p, MOUSEEVENTF_LEFTUP);
    pump(150);
}

/// Injected touch contacts. Every frame carries every active contact.
///
/// Windows cancels an injected contact that goes without an update for a few
/// hundred milliseconds, which the panel rightly treats as the finger lifting.
/// A keeper thread therefore re-sends the active contacts every 30 ms, so a
/// contact stays down however the test waits, until `up` lifts it.
pub struct Touch {
    contacts: Arc<Mutex<Vec<(u32, POINT)>>>,
    /// The first error the keeper hit; surfaced by the next touch call.
    failure: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    keeper: Option<std::thread::JoinHandle<()>>,
}

static TOUCH_INIT: Once = Once::new();

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn touch_info(id: u32, p: POINT, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO {
    let mut t: POINTER_TOUCH_INFO = unsafe { zeroed() };
    t.pointerInfo.pointerType = PT_TOUCH;
    t.pointerInfo.pointerId = id;
    t.pointerInfo.ptPixelLocation = p;
    t.pointerInfo.pointerFlags = flags;
    t.touchMask = TOUCH_MASK_NONE;
    t
}

/// Update frames for every active contact.
fn moving(contacts: &[(u32, POINT)]) -> Vec<POINTER_TOUCH_INFO> {
    let flags = POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT;
    contacts.iter().map(|&(id, p)| touch_info(id, p, flags)).collect()
}

fn inject(frame: &[POINTER_TOUCH_INFO]) -> std::io::Result<()> {
    match unsafe { InjectTouchInput(frame.len() as u32, frame.as_ptr()) } {
        0 => Err(std::io::Error::last_os_error()),
        _ => Ok(()),
    }
}

impl Touch {
    pub fn new() -> Touch {
        TOUCH_INIT.call_once(|| {
            let ok = unsafe { InitializeTouchInjection(10, TOUCH_FEEDBACK_NONE) };
            assert!(ok != 0, "InitializeTouchInjection");
        });
        let contacts: Arc<Mutex<Vec<(u32, POINT)>>> = Arc::default();
        let failure: Arc<Mutex<Option<String>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let keeper = {
            let (contacts, failure, stop) = (contacts.clone(), failure.clone(), stop.clone());
            // Never panics: an error is recorded for the test thread and the
            // keeper keeps going, so the real cause is what the test reports.
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    {
                        let active = lock(&contacts);
                        if !active.is_empty() {
                            if let Err(e) = inject(&moving(&active)) {
                                lock(&failure).get_or_insert(format!("keeping touch contacts alive: {e}"));
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(30));
                }
            })
        };
        Touch { contacts, failure, stop, keeper: Some(keeper) }
    }

    /// Panic with the keeper's error, if it hit one.
    fn check(&self) {
        if let Some(e) = lock(&self.failure).take() {
            panic!("{e}");
        }
    }

    pub fn down(&mut self, id: u32, p: POINT) {
        {
            let mut active = lock(&self.contacts);
            let mut frame = moving(&active);
            frame.push(touch_info(id, p, POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT));
            inject(&frame).expect("InjectTouchInput (down)");
            active.push((id, p));
        }
        pump(100);
        self.check();
    }

    pub fn move_to(&mut self, id: u32, p: POINT) {
        {
            let mut active = lock(&self.contacts);
            for c in active.iter_mut().filter(|c| c.0 == id) {
                c.1 = p;
            }
            inject(&moving(&active)).expect("InjectTouchInput (move)");
        }
        pump(100);
        self.check();
    }

    pub fn up(&mut self, id: u32) {
        self.check();
        self.lift(id).expect("InjectTouchInput (up)");
        pump(150);
        self.check();
    }

    fn lift(&self, id: u32) -> std::io::Result<()> {
        let mut active = lock(&self.contacts);
        let mut frame = moving(&active);
        for f in frame.iter_mut().filter(|f| f.pointerInfo.pointerId == id) {
            f.pointerInfo.pointerFlags = POINTER_FLAG_UP;
        }
        // Forget the contact only once it is really lifted.
        inject(&frame)?;
        active.retain(|c| c.0 != id);
        Ok(())
    }

    /// Wait in place. Contacts stay down on their own (the keeper refreshes
    /// them); this only pumps this thread's messages.
    pub fn hold(&self, ms: u64) {
        pump(ms);
        self.check();
    }
}

impl Drop for Touch {
    fn drop(&mut self) {
        // Lift whatever is still down. Best effort: the test may already be
        // failing, and a second panic here would abort the whole run.
        let ids: Vec<u32> = lock(&self.contacts).iter().map(|&(id, _)| id).collect();
        for id in ids {
            let _ = self.lift(id);
        }
        self.stop.store(true, Ordering::Relaxed);
        if let Some(keeper) = self.keeper.take() {
            let _ = keeper.join();
        }
    }
}

pub fn pen_frame(p: POINT, flags: POINTER_FLAGS) -> POINTER_TYPE_INFO {
    let mut info: POINTER_TYPE_INFO = unsafe { zeroed() };
    info.r#type = PT_PEN;
    let mut pen: POINTER_PEN_INFO = unsafe { zeroed() };
    pen.pointerInfo.pointerType = PT_PEN;
    pen.pointerInfo.ptPixelLocation = p;
    pen.pointerInfo.pointerFlags = flags;
    pen.penMask = PEN_MASK_NONE;
    info.Anonymous.penInfo = pen;
    info
}

/// Press a key with an injected pen: hover in, touch down, hold, lift.
pub fn pen_tap(p: POINT) {
    unsafe {
        let pen = CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_NONE);
        assert!(!pen.is_null(), "CreateSyntheticPointerDevice");
        let steps = [
            POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE,
            POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
            POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
            POINTER_FLAG_UP | POINTER_FLAG_INRANGE,
            POINTER_FLAG_UPDATE,
        ];
        for flags in steps {
            let frame = pen_frame(p, flags);
            let ok = InjectSyntheticPointerInput(pen, &frame, 1);
            assert!(ok != 0, "InjectSyntheticPointerInput: {}", std::io::Error::last_os_error());
            pump(100);
        }
        DestroySyntheticPointerDevice(pen);
    }
}

pub fn panel_rect(app: &Running) -> (HWND, RECT) {
    let hwnd = find_panel(app);
    let mut r: RECT = unsafe { zeroed() };
    unsafe { GetWindowRect(hwnd, &mut r) };
    (hwnd, r)
}

/// Every visible panel of `app`, once the first has appeared. EdgePad makes
/// all its panels before it first goes idle, so none is still to come.
pub fn panels(app: &Running) -> Vec<RECT> {
    find_panel(app);
    panels_now(app)
}

/// The visible panels of `app` at this moment, without waiting for any.
pub fn panels_now(app: &Running) -> Vec<RECT> {
    let name = wide(PANEL_CLASS);
    let mut found = Vec::new();
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
        if hwnd.is_null() {
            return found;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 {
            found.push(window_rect(hwnd));
        }
    }
}

/// The visible handle against the left edge of the primary monitor, once it
/// can be touched (see `find_panel`).
pub fn left_handle(app: &Running) -> RECT {
    let m = primary_monitor();
    let name = wide("EdgePadHandle");
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        let mut hwnd = null_mut();
        loop {
            hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
            if hwnd.is_null() {
                break;
            }
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            let r = window_rect(hwnd);
            if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 && r.left == m.left {
                assert_eq!(unsafe { DwmFlush() }, 0, "DwmFlush failed");
                return r;
            }
        }
        assert!(Instant::now() < end, "no handle on the left edge");
        pump(20);
    }
}

/// The panel against the left edge of the primary monitor.
pub fn left_panel(app: &Running) -> RECT {
    let m = primary_monitor();
    *panels(app).iter().find(|r| r.left == m.left).expect("no panel on the left edge")
}

/// `logical` pixels at the primary monitor's DPI, rounded as EdgePad rounds them.
pub fn px(logical: i32) -> i32 {
    // The primary monitor's DPI now, which EdgePad follows; the system DPI
    // is fixed at sign-in and goes stale when the scale changes.
    let (mut dpi, mut y) = (0, 0);
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    assert_eq!(unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut y) }, 0, "GetDpiForMonitor");
    let dpi = dpi as i32;
    (logical * dpi + 48) / 96
}

/// Centre of the cell in column `col`, row `row` of a grid of `key`-sized
/// cells starting at the panel's top-left corner.
pub fn cell(panel: RECT, key: i32, col: i32, row: i32) -> POINT {
    POINT { x: panel.left + col * key + key / 2, y: panel.top + row * key + key / 2 }
}

//! Helpers shared by the acceptance tests: launch the real exe, find its
//! panel, pump messages, read the screen. Tests share the desktop, so every
//! test holds `desktop()` for its whole run.

#![allow(dead_code)]

use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, GetProcessTimes,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_SYSTEM_AWARE, GetDpiForSystem, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Controls::{
    CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, POINTER_FEEDBACK_NONE,
    POINTER_TYPE_INFO,
};
use windows_sys::Win32::UI::Input::Pointer::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const PANEL_CLASS: &str = "EdgePadPanel";
pub const KEY_LOGICAL_PX: i32 = 56;
pub const KEY_COUNT: i32 = 5;

pub static DESKTOP: Mutex<()> = Mutex::new(());
pub static DPI: Once = Once::new();

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Serialise tests and make this process see physical pixels like the exe does.
pub fn desktop() -> MutexGuard<'static, ()> {
    DPI.call_once(|| {
        let ok = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE) };
        assert!(ok != 0, "SetProcessDpiAwarenessContext failed: coordinates would be virtualised");
    });
    DESKTOP.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Running(pub Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn launch() -> Running {
    Running(Command::new(env!("CARGO_BIN_EXE_edgepad")).spawn().expect("launch edgepad.exe"))
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
pub fn find_panel(app: &Running) -> HWND {
    find_window(app, PANEL_CLASS, true)
}

pub fn window_rect(hwnd: HWND) -> RECT {
    let mut r: RECT = unsafe { zeroed() };
    unsafe { GetWindowRect(hwnd, &mut r) };
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
    let dpi = unsafe { GetDpiForSystem() } as i32;
    (KEY_LOGICAL_PX * dpi + 48) / 96
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

pub unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let k = &*(lp as *const KBDLLHOOKSTRUCT);
            if k.flags & LLKHF_INJECTED != 0 {
                EVENTS.lock().unwrap().push((k.vkCode as u16, k.flags & LLKHF_UP != 0, k.dwExtraInfo));
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, wp, lp)
    }
}

/// Records and swallows injected key events while alive.
pub struct KeyLog(pub HHOOK);

impl KeyLog {
    pub fn start() -> KeyLog {
        EVENTS.lock().unwrap().clear();
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

/// Injected touch contacts. Every frame carries every active contact, and
/// `hold` keeps them alive with update frames.
pub struct Touch {
    pub contacts: Vec<(u32, POINT)>,
}

pub static TOUCH_INIT: Once = Once::new();

impl Touch {
    pub fn new() -> Touch {
        TOUCH_INIT.call_once(|| {
            let ok = unsafe { InitializeTouchInjection(10, TOUCH_FEEDBACK_NONE) };
            assert!(ok != 0, "InitializeTouchInjection");
        });
        Touch { contacts: Vec::new() }
    }

    pub fn info(id: u32, p: POINT, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO {
        let mut t: POINTER_TOUCH_INFO = unsafe { zeroed() };
        t.pointerInfo.pointerType = PT_TOUCH;
        t.pointerInfo.pointerId = id;
        t.pointerInfo.ptPixelLocation = p;
        t.pointerInfo.pointerFlags = flags;
        t.touchMask = TOUCH_MASK_NONE;
        t
    }

    pub fn inject(&self, frame: &[POINTER_TOUCH_INFO]) {
        let ok = unsafe { InjectTouchInput(frame.len() as u32, frame.as_ptr()) };
        assert!(ok != 0, "InjectTouchInput: {}", std::io::Error::last_os_error());
    }

    pub fn moving(&self) -> Vec<POINTER_TOUCH_INFO> {
        let flags = POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT;
        self.contacts.iter().map(|&(id, p)| Touch::info(id, p, flags)).collect()
    }

    pub fn down(&mut self, id: u32, p: POINT) {
        let mut frame = self.moving();
        frame.push(Touch::info(id, p, POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT));
        self.inject(&frame);
        self.contacts.push((id, p));
        self.hold(100);
    }

    pub fn move_to(&mut self, id: u32, p: POINT) {
        for c in self.contacts.iter_mut().filter(|c| c.0 == id) {
            c.1 = p;
        }
        self.inject(&self.moving());
        self.hold(100);
    }

    pub fn up(&mut self, id: u32) {
        let mut frame = self.moving();
        for f in frame.iter_mut().filter(|f| f.pointerInfo.pointerId == id) {
            f.pointerInfo.pointerFlags = POINTER_FLAG_UP;
        }
        self.inject(&frame);
        self.contacts.retain(|c| c.0 != id);
        self.hold(150);
    }

    pub fn hold(&self, ms: u64) {
        let mut left = ms;
        while left > 0 {
            if !self.contacts.is_empty() {
                self.inject(&self.moving());
            }
            let step = left.min(30);
            pump(step);
            left -= step;
        }
    }
}

impl Drop for Touch {
    fn drop(&mut self) {
        while let Some(&(id, _)) = self.contacts.first() {
            self.up(id);
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

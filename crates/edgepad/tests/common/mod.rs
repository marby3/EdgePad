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

/// Wait for the panel of `app` to appear and return its window.
pub fn find_panel(app: &Running) -> HWND {
    let class = wide(PANEL_CLASS);
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        let mut hwnd = null_mut();
        loop {
            hwnd = unsafe { FindWindowExW(null_mut(), hwnd, class.as_ptr(), null()) };
            if hwnd.is_null() {
                break;
            }
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 {
                return hwnd;
            }
        }
        pump(50);
    }
    panic!("no visible {PANEL_CLASS} window from edgepad.exe within 5s");
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

pub fn click(x: i32, y: i32) {
    unsafe {
        SetCursorPos(x, y);
        let mut inputs: [INPUT; 2] = zeroed();
        inputs[0].r#type = INPUT_MOUSE;
        inputs[0].Anonymous.mi.dwFlags = MOUSEEVENTF_LEFTDOWN;
        inputs[1].r#type = INPUT_MOUSE;
        inputs[1].Anonymous.mi.dwFlags = MOUSEEVENTF_LEFTUP;
        SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
    }
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

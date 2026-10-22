//! Acceptance tests for Issue #3: the fixed five-key panel on the left edge.
//!
//! Each test launches the real exe and inspects its panel from outside, the way
//! a user sees it: window styles, position, the pixels on screen, CPU time.
//! Tests share the desktop, so they run one at a time.

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

const PANEL_CLASS: &str = "EdgePadPanel";
const KEY_LOGICAL_PX: i32 = 56;
const KEY_COUNT: i32 = 5;

static DESKTOP: Mutex<()> = Mutex::new(());
static DPI: Once = Once::new();

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Serialise tests and make this process see physical pixels like the exe does.
fn desktop() -> MutexGuard<'static, ()> {
    DPI.call_once(|| unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
    });
    DESKTOP.lock().unwrap_or_else(|e| e.into_inner())
}

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn launch() -> Running {
    Running(Command::new(env!("CARGO_BIN_EXE_edgepad")).spawn().expect("launch edgepad.exe"))
}

/// Pump this thread's messages for `ms` so test windows stay painted.
fn pump(ms: u64) {
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
fn find_panel(app: &Running) -> HWND {
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

fn window_rect(hwnd: HWND) -> RECT {
    let mut r: RECT = unsafe { zeroed() };
    unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

fn primary_monitor() -> RECT {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        info.rcMonitor
    }
}

fn key_px() -> i32 {
    let dpi = unsafe { GetDpiForSystem() } as i32;
    (KEY_LOGICAL_PX * dpi + 48) / 96
}

/// Copy a screen rectangle, layered windows included, as 0x00RRGGBB pixels.
fn capture(r: RECT) -> Vec<u32> {
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

fn channels(p: u32) -> [i32; 3] {
    [(p >> 16 & 0xFF) as i32, (p >> 8 & 0xFF) as i32, (p & 0xFF) as i32]
}

static BACKDROP_COLOR: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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

/// A plain test window of a single colour.
fn test_window(class: &str, ex_style: u32, r: RECT) -> HWND {
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
        hwnd
    }
}

fn set_backdrop(hwnd: HWND, color: u32) {
    BACKDROP_COLOR.store(color, Ordering::Relaxed);
    unsafe {
        InvalidateRect(hwnd, null(), 1);
        UpdateWindow(hwnd);
    }
    pump(300);
}

fn cpu_time_100ns(app: &Running) -> u64 {
    let ft = |f: FILETIME| (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64;
    unsafe {
        let (mut c, mut e, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) =
            (zeroed(), zeroed(), zeroed(), zeroed());
        let ok = GetProcessTimes(app.0.as_raw_handle(), &mut c, &mut e, &mut k, &mut u);
        assert!(ok != 0, "GetProcessTimes");
        ft(k) + ft(u)
    }
}

fn click(x: i32, y: i32) {
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
fn force_foreground(hwnd: HWND) {
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

// AC1
#[test]
fn panel_shows_five_fixed_keys_in_a_column_on_the_left_edge_of_the_primary_monitor() {
    let _desktop = desktop();
    assert_eq!(edgepad_core::keys::M0_LABELS, ["Esc", "Ctrl", "Shift", "Alt", "Space"]);

    let app = launch();
    let r = window_rect(find_panel(&app));
    let monitor = primary_monitor();
    let key = key_px();
    let height = key * KEY_COUNT;
    assert_eq!(r.left, monitor.left, "flush with the left edge");
    assert_eq!(r.right - r.left, key, "one key wide");
    assert_eq!(r.bottom - r.top, height, "five keys tall");
    let centred = monitor.top + (monitor.bottom - monitor.top - height) / 2;
    assert!((r.top - centred).abs() <= 1, "vertically centred: top {} vs {centred}", r.top);
}

// AC2
#[test]
fn panel_window_never_activates_stays_on_top_and_stays_off_taskbar_and_alt_tab() {
    let _desktop = desktop();
    let app = launch();
    let panel = find_panel(&app);
    let ex = unsafe { GetWindowLongPtrW(panel, GWL_EXSTYLE) } as u32;
    for (flag, name) in [
        (WS_EX_NOACTIVATE, "WS_EX_NOACTIVATE"),
        (WS_EX_TOPMOST, "WS_EX_TOPMOST"),
        (WS_EX_TOOLWINDOW, "WS_EX_TOOLWINDOW"),
        (WS_EX_LAYERED, "WS_EX_LAYERED"),
    ] {
        assert!(ex & flag != 0, "{name} missing");
    }
    assert!(ex & WS_EX_APPWINDOW == 0, "WS_EX_APPWINDOW would put it on the taskbar");
}

// AC3
#[test]
fn clicking_the_panel_keeps_focus_on_the_previous_foreground_window() {
    let _desktop = desktop();
    let app = launch();
    let panel = window_rect(find_panel(&app));

    let monitor = primary_monitor();
    let area = RECT {
        left: monitor.left + 300,
        top: monitor.top + 100,
        right: monitor.left + 700,
        bottom: monitor.top + 400,
    };
    let foreground = test_window("EdgePadTestForeground", 0, area);
    force_foreground(foreground);
    assert_eq!(unsafe { GetForegroundWindow() }, foreground, "precondition: test window in front");

    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    click((panel.left + panel.right) / 2, (panel.top + panel.bottom) / 2);
    pump(300);
    unsafe { SetCursorPos(saved.x, saved.y) };

    assert_eq!(unsafe { GetForegroundWindow() }, foreground, "focus moved off the test window");
    unsafe { DestroyWindow(foreground) };
}

// AC4
#[test]
fn panel_is_drawn_with_update_layered_window_at_90_percent_with_a_label_on_every_key() {
    let _desktop = desktop();
    // A topmost backdrop made before the panel sits right under it.
    let monitor = primary_monitor();
    let key = key_px();
    let under = RECT {
        left: monitor.left,
        top: monitor.top,
        right: monitor.left + key * 2,
        bottom: monitor.bottom,
    };
    let backdrop = test_window("EdgePadTestBackdrop", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, under);

    let app = launch();
    let hwnd = find_panel(&app);
    let panel = window_rect(hwnd);

    let (mut key_color, mut alpha, mut flags) = (0, 0, 0);
    let attrs = unsafe { GetLayeredWindowAttributes(hwnd, &mut key_color, &mut alpha, &mut flags) };
    assert_eq!(attrs, 0, "GetLayeredWindowAttributes works only without UpdateLayeredWindow");

    // 90% opaque: 10% of the backdrop shows through, so white vs black differs by ~25.
    let corner = RECT { left: panel.left + 1, top: panel.top + 1, right: panel.left + 2, bottom: panel.top + 2 };
    set_backdrop(backdrop, 0x00FF_FFFF);
    let on_white = channels(capture(corner)[0]);
    set_backdrop(backdrop, 0x0000_0000);
    let on_black = channels(capture(corner)[0]);
    for c in 0..3 {
        let diff = on_white[c] - on_black[c];
        assert!((20..=31).contains(&diff), "backdrop shows through by {diff}/255, want ~25");
    }

    // Each key's middle holds label strokes that stand out from the key face.
    let size = panel.right - panel.left;
    for i in 0..KEY_COUNT {
        let top = panel.top + i * size;
        let middle = RECT {
            left: panel.left + size / 5,
            top: top + size / 5,
            right: panel.right - size / 5,
            bottom: top + size - size / 5,
        };
        let pixels = capture(middle);
        let face = channels(pixels[0]);
        let ink = pixels
            .iter()
            .filter(|&&p| channels(p).iter().zip(face).any(|(a, b)| (a - b).abs() > 60))
            .count();
        assert!(ink >= 20, "key {i} shows no label ({ink} ink pixels)");
    }
    unsafe { DestroyWindow(backdrop) };
}

// AC5
#[test]
fn idle_panel_uses_under_10ms_cpu_in_2_seconds() {
    let _desktop = desktop();
    let app = launch();
    find_panel(&app);
    let before = cpu_time_100ns(&app);
    std::thread::sleep(Duration::from_secs(2));
    let used_ms = (cpu_time_100ns(&app) - before) as f64 / 10_000.0;
    assert!(used_ms < 10.0, "idle CPU {used_ms:.1}ms in 2s");
}

//! The tray icon, its menu and its notifications, owned by a hidden
//! top-level window. A hidden window rather than a message-only one, because
//! only top-level windows hear the TaskbarCreated broadcast sent when
//! Explorer restarts.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use edgepad_core::config::Edge;
use edgepad_core::keys::{Combo, Mods};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::log::debug_log;
use crate::{autostart, panel, profile, send, settings, watch};

// The class name, TRAY_ID, TRAY_CALLBACK, the menu texts and the test
// messages are the contract the tests drive the tray through: change them
// together.
const TRAY_ID: u32 = 1;
const TRAY_CALLBACK: u32 = WM_APP + 1;
/// Posted by `post_send_failed`.
const SEND_FAILED: u32 = WM_APP + 2;
/// Posted by `post_foreground`, with the window in wParam.
const FOREGROUND: u32 = WM_APP + 3;
/// Posted by `post_out_of_reach`.
const OUT_OF_REACH: u32 = WM_APP + 4;

const CMD_EXIT: usize = 1001;
const CMD_OPEN: usize = 1002;
const CMD_RELOAD: usize = 1003;
const CMD_PAUSE: usize = 1004;
const CMD_RELEASE: usize = 1005;
const CMD_AUTOSTART: usize = 1006;
const CMD_PIN: usize = 1007;
/// The one hotkey registered with the tray window.
const HOTKEY_ID: i32 = 1;
/// One per edge, CMD_EDGE + Edge as usize.
const CMD_EDGE: usize = 1010;

const EDGES: [(Edge, *const u16); 4] = [
    (Edge::Left, w!("左辺を表示(&L)")),
    (Edge::Right, w!("右辺を表示(&R)")),
    (Edge::Top, w!("上辺を表示(&T)")),
    (Edge::Bottom, w!("下辺を表示(&B)")),
];

static TRAY: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());
static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0);
/// Whether HOTKEY_ID is registered now.
static HOTKEY_ON: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "fault-injection")]
static TEST_PANIC: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "fault-injection")]
static TEST_SEND_FAIL: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "fault-injection")]
static TEST_ELEVATED: AtomicU32 = AtomicU32::new(0);

/// Create the tray window and show the icon. Returns null on failure. A
/// missing taskbar is not a failure: the icon appears once it is created.
pub unsafe fn create() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = instance;
        wc.lpszClassName = w!("EdgePadTray");
        RegisterClassW(&wc);

        TASKBAR_CREATED_MSG.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        #[cfg(feature = "fault-injection")]
        TEST_PANIC.store(RegisterWindowMessageW(w!("EdgePadTestPanic")), Ordering::Relaxed);
        #[cfg(feature = "fault-injection")]
        TEST_SEND_FAIL.store(RegisterWindowMessageW(w!("EdgePadTestSendFail")), Ordering::Relaxed);
        #[cfg(feature = "fault-injection")]
        TEST_ELEVATED.store(RegisterWindowMessageW(w!("EdgePadTestElevated")), Ordering::Relaxed);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            wc.lpszClassName,
            w!("EdgePad"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if !hwnd.is_null() {
            TRAY.store(hwnd, Ordering::Relaxed);
            add_icon(hwnd);
            // Lock and user switch arrive as WM_WTSSESSION_CHANGE; sleep
            // (WM_POWERBROADCAST) reaches every top-level window unasked.
            if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
                notify("画面ロックの知らせを受け取れません。ロックの前に、押しているキーを離してください");
            }
        }
        hwnd
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    data
}

/// Copy `text` into a fixed UTF-16 field, cut to fit with its terminator,
/// never between the halves of a surrogate pair.
fn fill(dst: &mut [u16], text: &str) {
    let mut room = dst.len() - 1;
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() > room && (0xD800..0xDC00).contains(&units[room - 1]) {
        room -= 1;
    }
    for (d, s) in dst[..room].iter_mut().zip(units) {
        *d = s;
    }
}

unsafe fn add_icon(hwnd: HWND) {
    unsafe {
        let mut data = icon_data(hwnd);
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY_CALLBACK;
        // Placeholder artwork until the real icon ships (v1.0).
        data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        fill(&mut data.szTip, "EdgePad");
        Shell_NotifyIconW(NIM_ADD, &data);
    }
}

/// Tell the user something went wrong, in a tray balloon.
pub fn notify(text: &str) {
    test_event(&format!("notice: {text}"));
    let hwnd = TRAY.load(Ordering::Relaxed);
    if hwnd.is_null() {
        return;
    }
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_WARNING;
    fill(&mut data.szInfoTitle, "EdgePad");
    fill(&mut data.szInfo, text);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
}

/// Make `hotkey` the one that toggles every edge, replacing any before it;
/// `None` leaves none. A hotkey another app holds is reported.
pub unsafe fn set_hotkey(hotkey: Option<Combo>) {
    let hwnd = TRAY.load(Ordering::Relaxed);
    if hwnd.is_null() {
        return;
    }
    if HOTKEY_ON.swap(false, Ordering::Relaxed) && unsafe { UnregisterHotKey(hwnd, HOTKEY_ID) } == 0 {
        // Still registered: a new one under the same id would fail too.
        HOTKEY_ON.store(true, Ordering::Relaxed);
        debug_log!("ホットキー: 前のものを解除できない（新しいものは登録しない）");
        notify("ホットキーを解除できませんでした");
        return;
    }
    let Some(combo) = hotkey else { return };
    // The parser admits a hotkey only with a key besides the modifiers.
    let Some(key) = combo.key else { return };
    let flags: [(Mods, HOT_KEY_MODIFIERS); 4] =
        [(Mods::CTRL, MOD_CONTROL), (Mods::SHIFT, MOD_SHIFT), (Mods::ALT, MOD_ALT), (Mods::WIN, MOD_WIN)];
    // Held down, it toggles once, not over and over.
    let mods = flags.iter().filter(|(m, _)| combo.mods.contains(*m)).fold(MOD_NOREPEAT, |all, (_, f)| all | f);
    if unsafe { RegisterHotKey(hwnd, HOTKEY_ID, mods, send::vk(key) as u32) } != 0 {
        HOTKEY_ON.store(true, Ordering::Relaxed);
        debug_log!("ホットキー {combo}: 登録した");
    } else {
        debug_log!("ホットキー {combo}: 登録できない");
        notify(&format!("ホットキー {combo} を登録できませんでした。ほかのアプリが使っているかもしれません"));
    }
}

/// Have the tray report a failed send. Only posts: safe from the panic hook.
pub unsafe fn post_send_failed() {
    let hwnd = TRAY.load(Ordering::Relaxed);
    if !hwnd.is_null() {
        unsafe { PostMessageW(hwnd, SEND_FAILED, 0, 0) };
    }
}

/// Have the tray follow `hwnd`, now in front, to its profile. Only posts:
/// the WinEvent hook calls this whatever the UI thread is doing.
pub unsafe fn post_foreground(hwnd: HWND) {
    let tray = TRAY.load(Ordering::Relaxed);
    if !tray.is_null() {
        unsafe { PostMessageW(tray, FOREGROUND, hwnd as WPARAM, 0) };
    }
}

/// Have the tray say that keys do not reach the app in front. Only posts:
/// safe from the panic hook.
pub unsafe fn post_out_of_reach() {
    let hwnd = TRAY.load(Ordering::Relaxed);
    if !hwnd.is_null() {
        unsafe { PostMessageW(hwnd, OUT_OF_REACH, 0, 0) };
    }
}

/// Test builds record what a person would see or get opened, one line each
/// in %APPDATA%\edgepad-test-events.txt.
pub fn test_event(line: &str) {
    #[cfg(feature = "fault-injection")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        use std::io::Write;
        let path = std::path::Path::new(&appdata).join("edgepad-test-events.txt");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
    #[cfg(not(feature = "fault-injection"))]
    let _ = line;
}

unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        let check = |on: bool| if on { MF_CHECKED } else { 0 };
        let edges = panel::edges();
        for (edge, text) in EDGES {
            let flags = match edges[edge as usize] {
                None => MF_GRAYED,
                Some(shown) => check(shown),
            };
            AppendMenuW(menu, MF_STRING | flags, CMD_EDGE + edge as usize, text);
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        let (pin, flags) = match profile::shown() {
            Some((name, pinned)) => (format!("プロファイル「{name}」を固定(&F)"), check(pinned)),
            None => ("プロファイルを固定(&F)".into(), MF_GRAYED),
        };
        let pin: Vec<u16> = pin.encode_utf16().chain([0]).collect();
        AppendMenuW(menu, MF_STRING | flags, CMD_PIN, pin.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_OPEN, w!("設定ファイルを開く(&O)"));
        AppendMenuW(menu, MF_STRING, CMD_RELOAD, w!("設定を再読込(&E)"));
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING | check(panel::is_paused()), CMD_PAUSE, w!("一時停止(&P)"));
        AppendMenuW(menu, MF_STRING, CMD_RELEASE, w!("全キー解放(&K)"));
        AppendMenuW(menu, MF_STRING | check(autostart::enabled()), CMD_AUTOSTART, w!("Windows 起動時に自動起動(&S)"));
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_EXIT, w!("終了(&X)"));
        let mut at = POINT { x: 0, y: 0 };
        GetCursorPos(&mut at);
        // Without this the menu would not close when clicking elsewhere.
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, at.x, at.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}

/// Open the settings file in its default app, making one first if needed.
unsafe fn open_settings(hwnd: HWND) {
    let Some(path) = settings::path_or_template() else {
        notify("設定ファイルを作れませんでした");
        return;
    };
    test_event(&format!("open: {}", path.display()));
    // Test builds stop short of starting an editor.
    #[cfg(not(feature = "fault-injection"))]
    unsafe {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let file: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        ShellExecuteW(hwnd, w!("open"), file.as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
    #[cfg(feature = "fault-injection")]
    let _ = hwnd;
}

/// Leave: let go of every held key first, then take the icon down and quit.
unsafe fn exit(hwnd: HWND) {
    unsafe {
        debug_log!("全キー解放: 終了");
        send::release_all();
        WTSUnRegisterSessionNotification(hwnd);
        Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
        PostQuitMessage(0);
        debug_log!("終了");
    }
}

unsafe fn command(hwnd: HWND, id: usize) {
    unsafe {
        match id {
            CMD_EXIT => exit(hwnd),
            CMD_OPEN => open_settings(hwnd),
            CMD_RELOAD => watch::reload(true),
            CMD_PAUSE => panel::toggle_pause(),
            CMD_PIN => profile::toggle_pin(),
            CMD_RELEASE => {
                debug_log!("全キー解放: メニュー");
                panel::release_all();
            }
            CMD_AUTOSTART => {
                if !autostart::toggle() {
                    notify("自動起動の設定を変えられませんでした");
                }
            }
            _ => {
                if let Some(&(edge, _)) = EDGES.iter().find(|(e, _)| CMD_EDGE + *e as usize == id) {
                    panel::toggle_edge(edge);
                }
            }
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            TRAY_CALLBACK => {
                if matches!(lp as u32, WM_RBUTTONUP | WM_CONTEXTMENU) {
                    show_menu(hwnd);
                }
                0
            }
            WM_COMMAND => {
                command(hwnd, wp & 0xFFFF);
                0
            }
            WM_HOTKEY => {
                if wp as i32 == HOTKEY_ID {
                    panel::toggle_all();
                }
                0
            }
            FOREGROUND => {
                profile::foreground_changed(wp as HWND);
                0
            }
            OUT_OF_REACH => {
                let app = profile::front().unwrap_or_else(|| "前面のアプリ".into());
                notify(&format!("{app} は管理者として動いているため、キーが届きません。EdgePad も管理者として起動してください"));
                0
            }
            SEND_FAILED => {
                notify("キーを送れませんでした。管理者として動いているアプリへは、EdgePad も管理者として起動しないと送れません。");
                0
            }
            // Locked, switched away from or going to sleep: EdgePad will not
            // hear of a finger lifting from here on, so let go of every key now.
            WM_WTSSESSION_CHANGE => {
                let why = match wp as u32 {
                    WTS_SESSION_LOCK => Some("画面ロック"),
                    WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => Some("ユーザー切り替え"),
                    _ => None,
                };
                if let Some(why) = why {
                    debug_log!("全キー解放: {why}");
                    panel::release_all();
                }
                0
            }
            WM_POWERBROADCAST if wp as u32 == PBT_APMSUSPEND => {
                debug_log!("全キー解放: スリープ");
                panel::release_all();
                1
            }
            // Windows' theme or accent colour may have changed.
            WM_SETTINGCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED => {
                if !panel::restyle() {
                    PostMessageW(hwnd, msg, 0, 0);
                }
                0
            }
            WM_QUERYENDSESSION => 1,
            WM_ENDSESSION => {
                if wp != 0 {
                    exit(hwnd);
                }
                0
            }
            m if m == TASKBAR_CREATED_MSG.load(Ordering::Relaxed) => {
                add_icon(hwnd);
                0
            }
            #[cfg(feature = "fault-injection")]
            m if m == TEST_PANIC.load(Ordering::Relaxed) => panic!("fault injection"),
            #[cfg(feature = "fault-injection")]
            m if m == TEST_ELEVATED.load(Ordering::Relaxed) => {
                profile::TEST_ELEVATED.store(wp != 0, Ordering::Relaxed);
                0
            }
            #[cfg(feature = "fault-injection")]
            m if m == TEST_SEND_FAIL.load(Ordering::Relaxed) => {
                send::FAIL.store(wp != 0, Ordering::Relaxed);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

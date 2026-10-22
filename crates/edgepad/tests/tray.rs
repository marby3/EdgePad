//! Acceptance tests for Issue #7: the minimal tray icon, and releasing every
//! held key on exit and on panic.
//!
//! The tray window is reached by its class; the icon is probed with
//! Shell_NotifyIcon(NIM_MODIFY), which succeeds only while the icon exists.

mod common;

use std::mem::{size_of, zeroed};
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::Shell::{NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;
use common::*;

const TRAY_CLASS: &str = "EdgePadTray";
/// The icon's uID and callback message, EdgePad's side of the tray contract.
const TRAY_ID: u32 = 1;
const TRAY_CALLBACK: u32 = WM_APP + 1;

fn find_tray(app: &Running) -> HWND {
    find_window(app, TRAY_CLASS, false)
}

fn icon_data(tray: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = tray;
    data.uID = TRAY_ID;
    data
}

fn icon_exists(tray: HWND) -> bool {
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &icon_data(tray)) != 0 }
}

fn wait_exit(app: &mut Running) -> ExitStatus {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if let Some(status) = app.0.try_wait().expect("try_wait") {
            return status;
        }
        pump(50);
    }
    panic!("edgepad.exe still running 5s after being told to exit");
}

/// Open the tray menu, return the id of its 終了 item, and close the menu.
fn exit_command(app: &Running, tray: HWND) -> u32 {
    unsafe { PostMessageW(tray, TRAY_CALLBACK, 0, WM_RBUTTONUP as isize) };
    let menu_window = find_window(app, "#32768", true);
    let menu = unsafe { SendMessageW(menu_window, MN_GETHMENU, 0, 0) } as HMENU;
    let mut found = None;
    for i in 0..unsafe { GetMenuItemCount(menu) } {
        let mut text = [0u16; 64];
        let n = unsafe { GetMenuStringW(menu, i as u32, text.as_mut_ptr(), 64, MF_BYPOSITION) };
        if String::from_utf16_lossy(&text[..n.max(0) as usize]).contains("終了") {
            found = Some(unsafe { GetMenuItemID(menu, i) });
        }
    }
    unsafe { PostMessageW(menu_window, WM_KEYDOWN, VK_ESCAPE as usize, 0) };
    pump(200);
    found.expect("tray menu has no 終了 item")
}

fn choose_exit(app: &Running, tray: HWND) {
    let id = exit_command(app, tray);
    unsafe { PostMessageW(tray, WM_COMMAND, id as usize, 0) };
}

fn panel(app: &Running) -> RECT {
    let hwnd = find_panel(app);
    let mut r: RECT = unsafe { zeroed() };
    unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

// AC1
#[test]
fn a_tray_icon_appears_on_start() {
    let _desktop = desktop();
    let app = launch();
    let tray = find_tray(&app);
    assert!(icon_exists(tray), "no tray icon for EdgePad's tray window");
}

// AC2
#[test]
fn choosing_exit_from_the_tray_menu_removes_icon_and_panel_and_exits_zero() {
    let _desktop = desktop();
    let mut app = launch();
    let tray = find_tray(&app);
    let panel = find_panel(&app);

    choose_exit(&app, tray);
    let status = wait_exit(&mut app);
    assert_eq!(status.code(), Some(0), "exit status {status:?}");
    assert!(!icon_exists(tray), "tray icon left behind");
    assert_eq!(unsafe { IsWindow(panel) }, 0, "panel left behind");
}

// AC3
#[test]
fn exiting_releases_held_keys_from_the_menu_and_on_logoff() {
    let _desktop = desktop();

    // Exit from the tray menu while a finger holds Ctrl.
    let mut app = launch();
    let tray = find_tray(&app);
    let r = panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(r, CTRL));
    choose_exit(&app, tray);
    wait_exit(&mut app);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)]);
    drop(touch);

    // Logoff while a finger holds Shift.
    let app = launch();
    let tray = find_tray(&app);
    let r = panel(&app);
    let before = log.keys().len();
    let mut touch = Touch::new();
    touch.down(1, key_centre(r, SHIFT));
    unsafe {
        SendMessageW(tray, WM_QUERYENDSESSION, 0, ENDSESSION_LOGOFF as isize);
        SendMessageW(tray, WM_ENDSESSION, 1, ENDSESSION_LOGOFF as isize);
    }
    pump(200);
    assert_eq!(log.keys()[before..], [down(SHIFT), up(SHIFT)]);
}

// AC4
#[test]
#[cfg_attr(not(feature = "fault-injection"), ignore = "needs --features fault-injection")]
fn a_panic_releases_held_keys_before_the_process_ends() {
    let _desktop = desktop();
    let mut app = launch();
    let tray = find_tray(&app);
    let r = panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(r, ALT));

    let panic_message = unsafe { RegisterWindowMessageW(w!("EdgePadTestPanic")) };
    unsafe { PostMessageW(tray, panic_message, 0, 0) };
    let status = wait_exit(&mut app);
    assert!(!status.success(), "a panic should not look like a clean exit: {status:?}");
    assert_eq!(log.keys(), [down(ALT), up(ALT)]);
}

// AC5
#[test]
fn the_icon_comes_back_when_the_taskbar_is_recreated() {
    let _desktop = desktop();
    let app = launch();
    let tray = find_tray(&app);
    assert!(icon_exists(tray), "precondition: icon shown");

    // What a crashed Explorer leaves: the icon gone, then TaskbarCreated.
    unsafe { Shell_NotifyIconW(NIM_DELETE, &icon_data(tray)) };
    assert!(!icon_exists(tray), "precondition: icon removed");
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    unsafe { SendMessageW(tray, taskbar_created, 0, 0) };
    pump(200);
    assert!(icon_exists(tray), "icon not re-added after TaskbarCreated");
}

//! Acceptance tests for Issue #32: locking the screen, switching users and
//! going to sleep let go of every key EdgePad holds.
//!
//! Really locking or suspending would stop the machine running the tests, so
//! the tray window is sent the message Windows would send (as tests/tray.rs
//! does for logoff).

mod common;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, SendMessageW, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
    WTS_CONSOLE_DISCONNECT, WTS_REMOTE_DISCONNECT, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};
use common::*;

/// With a finger holding Ctrl, send the tray `msg`: Ctrl comes up, lifting
/// the finger afterwards sends nothing, and the panel stays shown.
fn releases_on(msg: u32, wp: u32) {
    let app = launch();
    let tray: HWND = find_window(&app, TRAY_CLASS, false);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    touch.hold(150);
    assert_eq!(log.keys(), [down(CTRL)]);

    unsafe { SendMessageW(tray, msg, wp as usize, 0) };
    pump(200);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "Ctrl is released");
    touch.up(1);
    pump(200);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "lifting the finger afterwards sends nothing");
    let now: Vec<_> = panels_now(&app).iter().map(|r| (r.left, r.top, r.right, r.bottom)).collect();
    assert_eq!(now, [(panel.left, panel.top, panel.right, panel.bottom)], "the panel stays shown where it was");
}

// AC1
#[test]
fn locking_the_screen_releases_held_keys() {
    let _desktop = desktop();
    releases_on(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK);
}

// AC2
#[test]
fn switching_users_releases_held_keys() {
    let _desktop = desktop();
    releases_on(WM_WTSSESSION_CHANGE, WTS_CONSOLE_DISCONNECT);
    releases_on(WM_WTSSESSION_CHANGE, WTS_REMOTE_DISCONNECT);
}

// AC3
#[test]
fn going_to_sleep_releases_held_keys() {
    let _desktop = desktop();
    releases_on(WM_POWERBROADCAST, PBT_APMSUSPEND);
}

// The notification's code comes from outside (any process can send the
// message): codes not listed, unlocking and resuming among them, change nothing.
#[test]
fn other_session_and_power_codes_leave_held_keys_alone() {
    let _desktop = desktop();
    let app = launch();
    let tray: HWND = find_window(&app, TRAY_CLASS, false);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    touch.hold(150);

    for (msg, wp) in [(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK), (WM_WTSSESSION_CHANGE, 0x55), (WM_POWERBROADCAST, PBT_APMRESUMESUSPEND)] {
        unsafe { SendMessageW(tray, msg, wp as usize, 0) };
    }
    pump(200);
    assert_eq!(log.keys(), [down(CTRL)], "Ctrl stays held");
    touch.up(1);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "and comes up with its finger");
}

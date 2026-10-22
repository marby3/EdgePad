//! Acceptance tests for Issue #32: locking the screen, switching users and
//! going to sleep let go of every key EdgePad holds.
//!
//! Really locking or suspending would stop the machine running the tests, so
//! the tray window is sent the message Windows would send (as tests/tray.rs
//! does for logoff).

mod common;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
use common::*;

const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WTS_CONSOLE_DISCONNECT: usize = 0x2;
const WTS_REMOTE_DISCONNECT: usize = 0x4;
const WTS_SESSION_LOCK: usize = 0x7;
const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMSUSPEND: usize = 0x4;

/// With a finger holding Ctrl, send the tray `msg`: Ctrl comes up, lifting
/// the finger afterwards sends nothing, and the panel stays shown.
fn releases_on(msg: u32, wp: usize) {
    let app = launch();
    let tray: HWND = find_window(&app, TRAY_CLASS, false);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    touch.hold(150);
    assert_eq!(log.keys(), [down(CTRL)]);

    unsafe { SendMessageW(tray, msg, wp, 0) };
    pump(200);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "Ctrl is released");
    touch.up(1);
    pump(200);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "lifting the finger afterwards sends nothing");
    assert_eq!(panels_now(&app).len(), 1, "the panel stays shown");
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

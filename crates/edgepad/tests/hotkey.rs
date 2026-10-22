//! Acceptance tests for Issue #28: the global hotkey of `[general] hotkey`
//! shows and hides every edge.
//!
//! The hotkey is pressed with input not tagged as EdgePad's (`keyboard`), as
//! from a physical keyboard. The keys are F13..F15, which no real keyboard
//! here has, so nothing else answers to them.

mod common;

use std::time::{Duration, Instant};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use common::*;

/// A settings file: `n` hold keys on the left and right, and `hotkey`.
fn settings(n: usize, hotkey: &str) -> String {
    let mut text = format!("[general]\nhotkey = {hotkey}\n[set.pad]\n");
    for c in (b'A'..).take(n) {
        let c = c as char;
        text += &format!("key = {c} | hold | {c}\n");
    }
    text + "[profile.default]\nleft = pad\nright = pad\n"
}

fn save(app: &Running, text: &str) {
    std::fs::write(exe_dir(app).join("edgepad.ini"), text).expect("save edgepad.ini");
}

/// Press and release Ctrl+Alt+`key` as a physical keyboard would.
fn press(key: u16) {
    for (vk, down) in [(VK_LCONTROL, true), (VK_LMENU, true), (key, true), (key, false), (VK_LMENU, false), (VK_LCONTROL, false)] {
        keyboard(vk, down);
    }
    pump(200);
}

/// How many panels of `app` are on screen.
fn shown(app: &Running) -> usize {
    panels_now(app).len()
}

/// Wait up to a second for the panels to be `n` keys long.
fn reloaded(app: &Running, n: i32) -> bool {
    let end = Instant::now() + Duration::from_millis(1000);
    while Instant::now() < end {
        if panels_now(app).iter().any(|r| r.bottom - r.top == n * key_px()) {
            return true;
        }
        pump(20);
    }
    false
}

const A: u16 = b'A' as u16;

// AC1
#[test]
fn the_hotkey_toggles_every_edge_and_releases_held_keys() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings(2, "Ctrl+Alt+F13")), None);
    find_panel(&app);
    assert_eq!(shown(&app), 2);

    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(left_panel(&app), 0));
    touch.hold(150);
    press(VK_F13);
    assert_eq!(shown(&app), 0, "every edge is hidden");
    assert_eq!(log.keys()[..2], [(A, false), (A, true)], "the key held on the panel is released");
    touch.up(1);

    press(VK_F13);
    assert_eq!(shown(&app), 2, "every edge is shown again");
}

// AC2
#[test]
fn the_hotkey_shows_nothing_while_paused() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings(2, "Ctrl+Alt+F13")), None);
    find_panel(&app);
    choose(&app, "一時停止");
    assert_eq!(shown(&app), 0);

    press(VK_F13);
    assert_eq!(shown(&app), 0, "still nothing while paused");

    // The press changed nothing: resuming shows the edges as before, and the
    // hotkey works again.
    choose(&app, "一時停止");
    assert_eq!(shown(&app), 2, "resumed as before the pause");
    press(VK_F13);
    assert_eq!(shown(&app), 0, "the hotkey works once resumed");
}

// AC3
#[test]
fn a_reload_moves_the_hotkey_and_none_removes_it() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings(2, "Ctrl+Alt+F13")), None);
    find_panel(&app);

    save(&app, &settings(3, "Ctrl+Alt+F14"));
    assert!(reloaded(&app, 3), "reloaded");
    press(VK_F13);
    assert_eq!(shown(&app), 2, "the old hotkey does nothing");
    press(VK_F14);
    assert_eq!(shown(&app), 0, "the new hotkey hides every edge");
    press(VK_F14);
    assert_eq!(shown(&app), 2);

    save(&app, &settings(2, "none"));
    assert!(reloaded(&app, 2), "reloaded");
    press(VK_F14);
    assert_eq!(shown(&app), 2, "with none, no hotkey is left");
}

// AC4
#[test]
fn a_hotkey_taken_by_another_app_is_notified_and_edgepad_keeps_running() {
    let _desktop = desktop();
    // This test process takes Ctrl+Alt+F15 first, as another app would.
    let taken = unsafe { RegisterHotKey(std::ptr::null_mut(), 1, MOD_CONTROL | MOD_ALT, VK_F15 as u32) };
    assert!(taken != 0, "precondition: Ctrl+Alt+F15 is free to take");
    let app = launch_with(Some(&settings(2, "Ctrl+Alt+F15")), None);
    find_panel(&app);
    pump(300);

    let events = test_events(&app);
    unsafe { UnregisterHotKey(std::ptr::null_mut(), 1) };
    let notice = events.iter().find(|e| e.starts_with("notice: ")).unwrap_or_else(|| panic!("no notice: {events:?}"));
    assert!(notice.contains("ホットキー") && notice.contains("Ctrl+Alt+F15"), "says which hotkey: {notice}");
    assert_eq!(shown(&app), 2, "EdgePad keeps running with its panels");
}

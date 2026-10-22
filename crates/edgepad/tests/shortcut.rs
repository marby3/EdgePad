//! Acceptance tests for Issue #22: shortcut keys, and extended keys sent with
//! their flag and scan code.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use common::*;

/// The left panel of `app`, launched with `keys` as its only set.
fn launch_keys(keys: &str) -> (Running, RECT) {
    let text = format!("[set.pad]\n{keys}[profile.default]\nleft = pad\n");
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    (app, panel)
}

fn at(panel: RECT, i: usize) -> POINT {
    key_centre(panel, i)
}

// KeyLog records (virtual key, is up): `d` is a key-down, `u` a key-up.
const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

const S: u16 = b'S' as u16;
const Z: u16 = b'Z' as u16;

// AC1
#[test]
fn a_shortcut_sends_nothing_on_press_and_its_whole_combination_once_on_release() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("key = 保存 | shortcut | Ctrl+Shift+S\n");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.hold(300);
    assert_eq!(log.keys(), [], "nothing while the key is held");
    touch.up(1);
    assert_eq!(
        log.keys(),
        [d(VK_LCONTROL), d(VK_LSHIFT), d(S), u(S), u(VK_LSHIFT), u(VK_LCONTROL)],
        "the combination, once, on release"
    );
}

// AC2
#[test]
fn a_modifier_held_on_the_panel_combines_with_a_shortcut_and_is_not_sent_again() {
    let _desktop = desktop();
    let keys = "\
key = Ctrl | modifier | Ctrl
key = Z    | shortcut | Z
key = C-Z  | shortcut | Ctrl+Z
";
    let (_app, panel) = launch_keys(keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.hold(150);
    touch.down(2, at(panel, 1));
    touch.up(2);
    assert_eq!(log.keys(), [d(VK_LCONTROL), d(Z), u(Z)], "Z alone, under the held Ctrl");

    touch.down(2, at(panel, 2));
    touch.up(2);
    assert_eq!(log.keys()[3..], [d(Z), u(Z)], "Ctrl+Z leaves the held Ctrl alone");

    touch.up(1);
    assert_eq!(log.keys()[5..], [u(VK_LCONTROL)], "Ctrl goes up with its finger");
}

// AC3
#[test]
fn a_modifier_held_on_the_physical_keyboard_is_not_touched() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("key = C-Z | shortcut | Ctrl+Z\n");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    keyboard(VK_LCONTROL, true);
    touch.down(1, at(panel, 0));
    touch.up(1);
    keyboard(VK_LCONTROL, false);
    assert_eq!(log.keys(), [d(Z), u(Z)], "only Z: the keyboard's Ctrl is left as it is");
}

// AC4
#[test]
fn extended_keys_carry_the_extended_flag_and_their_scan_code() {
    let _desktop = desktop();
    let keys = "\
key = Left | shortcut | Left
key = Home | shortcut | Ctrl+Home
key = Del  | hold     | Delete
key = A    | hold     | A
";
    let (_app, panel) = launch_keys(keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    for i in 0..4 {
        touch.down(1, at(panel, i));
        touch.up(1);
    }

    let (left, home, delete, a) = ((VK_LEFT, 0x4B, true), (VK_HOME, 0x47, true), (VK_DELETE, 0x53, true), (b'A' as u16, 0x1E, false));
    let ctrl = (VK_LCONTROL, 0x1D, false);
    assert_eq!(log.scans(), [left, left, ctrl, home, home, ctrl, delete, delete, a, a]);
}

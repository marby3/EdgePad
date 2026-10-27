//! Acceptance tests for Issue #104: layer keys, which turn the panel to
//! another key set while held (MO), or from one press to the next (TG).

mod common;

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use common::*;

/// On the left: base, its Raise held for raise, its Tog toggling num; raise
/// holds Lower for lower; num toggles back.
const SETTINGS: &str = "\
[general]
hotkey = Ctrl+Alt+F13
[set.base]
key = A     | shortcut | A
key = Raise | layer    | raise
key = Ctrl  | modifier | Ctrl
key = Tog   | layer    | num | mode=toggle
[set.raise]
key = 1     | shortcut | 1
key = Lower | layer    | lower
key = H     | hold     | H
[set.lower]
key = 9 | shortcut | 9
[set.num]
key = 7    | shortcut | 7
key = 8    | shortcut | 8
key = Back | layer    | num | mode=toggle
[profile.default]
left = base
";

const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

/// How many keys long the left panel is now.
fn length(app: &Running) -> i32 {
    let r = left_panel(app);
    (r.bottom - r.top) / key_px()
}

/// Key `i` of the left panel as it is now.
fn at(app: &Running, i: usize) -> POINT {
    key_centre(left_panel(app), i)
}

fn tap(touch: &mut Touch, id: u32, p: POINT) {
    touch.down(id, p);
    touch.up(id);
}

/// What `log` has after the first `before` keys.
fn since(log: &KeyLog, before: usize) -> Vec<(u16, bool)> {
    log.keys()[before..].to_vec()
}

// AC1
#[test]
fn a_held_layer_key_turns_the_panel_while_held() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    assert_eq!(length(&app), 4, "base");
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, at(&app, 1));
    touch.hold(100);
    assert_eq!(length(&app), 3, "raise while Raise is held");
    let before = log.keys().len();
    tap(&mut touch, 2, at(&app, 0));
    assert_eq!(since(&log, before), [d(b'1' as u16), u(b'1' as u16)], "raise's key, by another finger");
    touch.up(1);
    touch.hold(100);
    assert_eq!(length(&app), 4, "base again once Raise is let go");
    let before = log.keys().len();
    tap(&mut touch, 2, at(&app, 0));
    assert_eq!(since(&log, before), [d(b'A' as u16), u(b'A' as u16)]);
}

// AC2
#[test]
fn a_toggle_layer_key_turns_the_panel_until_its_set_s_key_turns_it_back() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    tap(&mut touch, 1, at(&app, 3));
    touch.hold(100);
    assert_eq!(length(&app), 3, "num, after the press");
    let before = log.keys().len();
    tap(&mut touch, 1, at(&app, 0));
    tap(&mut touch, 1, at(&app, 1));
    assert_eq!(since(&log, before), [d(b'7' as u16), u(b'7' as u16), d(b'8' as u16), u(b'8' as u16)], "num stays");
    tap(&mut touch, 1, at(&app, 2));
    touch.hold(100);
    assert_eq!(length(&app), 4, "num's key toggling num: back to base");
}

// AC3
#[test]
fn a_layer_key_in_a_layer_works_and_the_way_back_is_the_profile_s_set() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let mut touch = Touch::new();
    touch.down(1, at(&app, 1));
    touch.hold(100);
    touch.down(2, at(&app, 1));
    touch.hold(100);
    assert_eq!(length(&app), 1, "lower, from raise");
    touch.up(2);
    touch.hold(100);
    assert_eq!(length(&app), 4, "back to base, the profile's set");
    touch.up(1);
    touch.hold(100);
    assert_eq!(length(&app), 4, "and there it stays");
}

// AC4
#[test]
fn modifiers_carry_over_a_layer_and_held_keys_are_let_go() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    const CTRL: u16 = VK_LCONTROL;
    let one = b'1' as u16;
    // Ctrl latched on base, then a shortcut of raise: sent with Ctrl, and
    // the one-shot latch ends.
    let before = log.keys().len();
    tap(&mut touch, 1, at(&app, 2));
    touch.down(1, at(&app, 1));
    touch.hold(100);
    tap(&mut touch, 2, at(&app, 0));
    touch.up(1);
    touch.hold(100);
    let sent = since(&log, before);
    let ctrl = |k: &(u16, bool)| k.0 == CTRL || k.0 == VK_CONTROL;
    assert!(ctrl(&sent[0]) && !sent[0].1, "Ctrl down first: {sent:?}");
    assert_eq!(sent[1..3], [d(one), u(one)], "then raise's 1: {sent:?}");
    assert!(sent.get(3).is_some_and(|k| ctrl(k) && k.1), "then Ctrl up, the one shot over: {sent:?}");

    // A hold key of raise, held as Raise is let go: up then, not later.
    let h = b'H' as u16;
    touch.down(1, at(&app, 1));
    touch.hold(100);
    let before = log.keys().len();
    touch.down(2, at(&app, 2));
    touch.hold(100);
    touch.up(1);
    touch.hold(100);
    assert_eq!(since(&log, before), [d(h), u(h)], "H up as the layer goes");
    touch.up(2);
    assert_eq!(since(&log, before), [d(h), u(h)], "nothing more when the finger lifts");
}

// AC5
#[test]
fn hiding_the_panel_goes_back_to_the_profile_s_set() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let mut touch = Touch::new();
    tap(&mut touch, 1, at(&app, 3));
    touch.hold(100);
    assert_eq!(length(&app), 3, "num");
    for down in [true, false, true, false] {
        let vk = [VK_LCONTROL, VK_LMENU, VK_F13];
        if down {
            vk.iter().for_each(|&k| keyboard(k, true));
        } else {
            vk.iter().rev().for_each(|&k| keyboard(k, false));
        }
        pump(300);
    }
    assert_eq!(length(&app), 4, "shown again: base");
}

// AC6
#[test]
fn the_panel_takes_the_shape_of_the_set_turned_to() {
    let _desktop = desktop();
    let text = "\
[set.base]
key = A     | shortcut | A
key = Raise | layer    | pad
[set.pad]
layout = free
key = 1 | shortcut | 1 | x=0, y=0
key = 2 | shortcut | 2 | x=1, y=0
key = 3 | shortcut | 3 | x=0, y=1, w=2
[profile.default]
left = base
";
    let app = launch_with(Some(text), None);
    let panel = left_panel(&app);
    assert_eq!((panel.right - panel.left, panel.bottom - panel.top), (key_px(), 2 * key_px()), "base: a column of 2");
    let mut touch = Touch::new();
    touch.down(1, at(&app, 1));
    touch.hold(100);
    let panel = left_panel(&app);
    assert_eq!((panel.right - panel.left, panel.bottom - panel.top), (2 * key_px(), 2 * key_px()), "pad: 2 by 2");
    touch.up(1);
}

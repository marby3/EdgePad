//! Acceptance tests for Issue #5: keys are held down while a finger or the
//! mouse presses them.
//!
//! Touch and pen are injected with InjectTouchInput / InjectSyntheticPointerInput.
//! A low-level keyboard hook records every injected key event and swallows it,
//! so nothing the panel sends reaches other applications during the tests.

mod common;

use windows_sys::Win32::Foundation::RECT;
use common::*;

// AC1
#[test]
fn pressing_a_key_with_touch_or_mouse_holds_its_virtual_key_until_release() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    // Mouse, every key: one key-down while held, one key-up on release.
    for key in [ESC, CTRL, SHIFT, ALT, SPACE] {
        let before = log.keys().len();
        mouse_down_at(key_centre(panel, key));
        pump(400);
        assert_eq!(log.keys()[before..], [down(key)], "mouse held on key {key}");
        mouse_up_at(key_centre(panel, key));
        assert_eq!(log.keys()[before..], [down(key), up(key)], "mouse released key {key}");
    }

    // Touch.
    let before = log.keys().len();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    touch.hold(400);
    assert_eq!(log.keys()[before..], [down(CTRL)], "finger held on Ctrl");
    touch.up(1);
    assert_eq!(log.keys()[before..], [down(CTRL), up(CTRL)], "finger lifted from Ctrl");
}

// AC2
#[test]
fn two_fingers_hold_two_keys_and_lifting_one_releases_only_its_key() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    touch.down(2, key_centre(panel, SHIFT));
    touch.hold(200);
    assert_eq!(log.keys(), [down(CTRL), down(SHIFT)]);

    touch.up(1);
    assert_eq!(log.keys(), [down(CTRL), down(SHIFT), up(CTRL)], "only Ctrl released");

    touch.up(2);
    assert_eq!(log.keys(), [down(CTRL), down(SHIFT), up(CTRL), up(SHIFT)]);
}

// AC3
#[test]
fn sliding_off_the_key_keeps_it_down_until_release() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, ALT));
    touch.move_to(1, outside(panel, ALT));
    touch.hold(300);
    assert_eq!(log.keys(), [down(ALT)], "finger slid off Alt");
    touch.up(1);
    assert_eq!(log.keys(), [down(ALT), up(ALT)], "finger lifted outside the panel");

    let before = log.keys().len();
    mouse_down_at(key_centre(panel, SHIFT));
    let away = outside(panel, SHIFT);
    mouse_move_to(away);
    pump(200);
    assert_eq!(log.keys()[before..], [down(SHIFT)], "cursor dragged off Shift");
    mouse_up_at(away);
    assert_eq!(log.keys()[before..], [down(SHIFT), up(SHIFT)], "button released outside the panel");
}

// AC4
#[test]
fn pen_presses_nothing() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    pen_tap(key_centre(panel, CTRL));
    pump(200);
    assert_eq!(log.keys(), [], "pen must not press keys");
}

// AC5
#[test]
fn a_held_key_changes_colour_and_returns_on_release() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let _log = KeyLog::start();

    // A spot of the Shift key's face, clear of its label.
    let size = panel.right - panel.left;
    let top = panel.top + size * SHIFT as i32;
    let spot = RECT {
        left: panel.left + size / 4,
        top: top + size / 6,
        right: panel.left + size / 4 + 2,
        bottom: top + size / 6 + 2,
    };
    let colour = || channels(capture(spot)[0]);
    let differs = |a: [i32; 3], b: [i32; 3]| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 40);

    let idle = colour();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, SHIFT));
    touch.hold(200);
    let pressed = colour();
    touch.up(1);
    pump(200);
    let released = colour();

    assert!(differs(idle, pressed), "pressed colour {pressed:?} looks like idle {idle:?}");
    assert!(!differs(idle, released), "released colour {released:?} did not return to {idle:?}");
}

// AC6
#[test]
fn every_sent_key_event_carries_the_edgepad_tag() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, SPACE));
    touch.up(1);
    mouse_down_at(key_centre(panel, ESC));
    mouse_up_at(key_centre(panel, ESC));

    let events = log.all();
    assert_eq!(events.len(), 4, "events: {events:?}");
    // One shared, non-zero tag: physical keys carry zero.
    let tag = events[0].2;
    assert_ne!(tag, 0, "EdgePad's key events carry no tag");
    for (vk, is_up, extra) in events {
        assert_eq!(extra, tag, "vk {vk:#x} up={is_up} carried {extra:#x}, others {tag:#x}");
    }
}

// #12: a finger held through a long wait in the test must stay down; the
// test helper, not the test, keeps the injected contact alive.
#[test]
fn a_finger_held_through_a_long_wait_keeps_its_key_down() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    pump(1500);
    assert_eq!(log.keys(), [down(CTRL)], "Ctrl released while the finger was still down");
    touch.up(1);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "lifting the finger released Ctrl");
}

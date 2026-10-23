//! Acceptance tests for Issue #62: a quick flick toward the screen edge
//! hides a one-column panel, which is only a key thick.

mod common;

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};
use common::*;

const A: u16 = b'A' as u16;
const B: u16 = b'B' as u16;

/// A and B down the left, in the one column the edge has by default.
fn launch_pad() -> Running {
    let app = launch_with(Some("[set.pad]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nleft = pad\n"), None);
    find_panel(&app);
    app
}

fn shown(app: &Running) -> bool {
    !panels_now(app).is_empty()
}

// AC1
#[test]
fn a_quick_flick_from_a_keys_centre_to_the_edge_hides_a_one_column_panel() {
    let _desktop = desktop();
    let app = launch_pad();
    let log = KeyLog::start();

    // A finger, from the middle of A to the edge.
    let panel = left_panel(&app);
    let a = key_centre(panel, 0);
    let mut touch = Touch::new();
    touch.down(1, a);
    touch.move_to(1, POINT { x: panel.left + 2, y: a.y });
    pump(200);
    assert!(!shown(&app), "a finger flicked from the middle of the key to the edge hides the panel");
    assert_eq!(log.keys(), [(A, false), (A, true)], "A is pressed, then let go as it hides");
    touch.up(1);
    drop(touch);

    // The mouse, the same way from B.
    choose(&app, "左辺");
    let panel = left_panel(&app);
    let b = key_centre(panel, 1);
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(b);
    mouse_down_at(b);
    mouse_move_to(POINT { x: panel.left, y: b.y });
    mouse_up_at(POINT { x: panel.left, y: b.y });
    unsafe { SetCursorPos(saved.x, saved.y) };
    pump(200);
    assert!(!shown(&app), "the mouse flicked from the middle of the key to the edge hides the panel");
    assert_eq!(log.keys()[2..], [(B, false), (B, true)], "B is pressed, then let go");
}

// AC2
#[test]
fn a_finger_drifting_slowly_to_the_edge_keeps_a_one_column_panel_and_its_key() {
    let _desktop = desktop();
    let app = launch_pad();
    let log = KeyLog::start();

    let panel = left_panel(&app);
    let a = key_centre(panel, 0);
    let mut touch = Touch::new();
    touch.down(1, a);
    touch.hold(400);
    touch.move_to(1, POINT { x: (a.x + panel.left) / 2, y: a.y });
    touch.move_to(1, POINT { x: panel.left + 2, y: a.y });
    pump(200);
    assert!(shown(&app), "a slow drift to the edge leaves the panel shown");
    assert_eq!(log.keys(), [(A, false)], "and A held");
    touch.up(1);
    assert_eq!(log.keys(), [(A, false), (A, true)], "until the finger lifts");
}

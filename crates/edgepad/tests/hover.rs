//! Acceptance tests for Issue #58: showing a panel by resting the mouse on
//! its handle, hiding it once the mouse has left, and hiding it a while
//! after it was last used.
//!
//! Times here run on the wall clock with margins either side: a hover of
//! 400 ms is checked at 250 ms (not yet) and 700 ms (done), an auto-hide of
//! 1500 ms at 1000 ms and 1900 ms.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};
use common::*;

const A: u16 = b'A' as u16;

/// The hover_ms of every test here that hovers.
const HOVER_MS: u64 = 400;

/// A and Ctrl down the left, under `general`.
fn launch_pad(general: &str) -> Running {
    let text = format!("[general]\n{general}\n[set.pad]\nkey = A | hold | A\nkey = Ctrl | modifier | Ctrl\n[profile.default]\nleft = pad\n");
    let app = launch_with(Some(&text), None);
    find_panel(&app);
    app
}

fn shown(app: &Running) -> bool {
    !panels_now(app).is_empty()
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

/// Well away from the left edge: the middle of the primary monitor.
fn away() -> POINT {
    centre(primary_monitor())
}

/// Puts the cursor back where it was when the test ends.
struct Cursor(POINT);

impl Cursor {
    fn save() -> Cursor {
        let mut p = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut p) };
        mouse_move_to(away());
        Cursor(p)
    }
}

impl Drop for Cursor {
    fn drop(&mut self) {
        unsafe { SetCursorPos(self.0.x, self.0.y) };
    }
}

/// Rest the mouse on the left handle long enough to show the panel, as in
/// AC1.
fn hover_show(app: &Running) {
    hover_show_at(app, centre(left_handle(app)));
}

fn hover_show_at(app: &Running, p: POINT) {
    mouse_move_to(p);
    pump(HOVER_MS + 300);
    assert!(shown(app), "precondition: resting on the handle shows the panel");
}

// AC1
#[test]
fn resting_the_mouse_on_the_handle_for_hover_ms_shows_the_panel() {
    let _desktop = desktop();
    let _cursor = Cursor::save();
    let app = launch_pad("hover_ms = 400");
    choose(&app, "左辺");

    // 150 ms on the handle, then away: too short.
    mouse_move_to(centre(left_handle(&app)));
    pump(50);
    mouse_move_to(away());
    pump(600);
    assert!(!shown(&app), "a mouse that left the handle before hover_ms shows nothing");

    mouse_move_to(centre(left_handle(&app)));
    pump(700);
    assert!(shown(&app), "a mouse resting on the handle for hover_ms shows the panel");
    drop(app);

    let app = launch_pad("");
    choose(&app, "左辺");
    mouse_move_to(centre(left_handle(&app)));
    pump(1000);
    assert!(!shown(&app), "with hover_ms off, resting on the handle shows nothing");
}

// AC2
#[test]
fn a_panel_shown_by_hovering_hides_hover_ms_after_the_mouse_leaves_it() {
    let _desktop = desktop();
    let _cursor = Cursor::save();
    let app = launch_pad("hover_ms = 400");
    choose(&app, "左辺");

    // Straight off the panel, which came up under the resting mouse and
    // never saw it move.
    hover_show(&app);
    mouse_move_to(away());
    pump(150);
    assert!(shown(&app), "250 ms after leaving it is still shown");
    pump(600);
    assert!(!shown(&app), "hover_ms after leaving it is hidden");

    // Over the panel first, then off it.
    hover_show(&app);
    mouse_move_to(key_centre(left_panel(&app), 0));
    mouse_move_to(away());
    pump(800);
    assert!(!shown(&app), "hover_ms after leaving it is hidden, having moved over it");

    // From the end of the handle, beyond the panel: resting there keeps it,
    // as on the panel; going hides it.
    let handle = left_handle(&app);
    hover_show_at(&app, POINT { x: centre(handle).x, y: handle.top + 3 });
    let panel = left_panel(&app);
    assert!(handle.top + 3 < panel.top, "precondition: the end of the handle is beyond the panel");
    pump(HOVER_MS + 400);
    assert!(shown(&app), "the mouse resting on the end of the handle keeps the panel");
    mouse_move_to(away());
    pump(150);
    assert!(shown(&app), "250 ms after the mouse went from the end of the handle it is still shown");
    pump(600);
    assert!(!shown(&app), "hover_ms after the mouse went from the end of the handle it is hidden");

    hover_show(&app);
    let panel = left_panel(&app);
    mouse_move_to(key_centre(panel, 0));
    mouse_move_to(away());
    pump(100);
    mouse_move_to(key_centre(panel, 1));
    pump(800);
    assert!(shown(&app), "back on the panel before hover_ms: it stays");

    // Shown from the tray, not by hovering: leaving it hides nothing.
    choose(&app, "左辺");
    assert!(!shown(&app), "precondition: hidden from the tray");
    mouse_move_to(away());
    choose(&app, "左辺");
    let panel = left_panel(&app);
    mouse_move_to(key_centre(panel, 0));
    mouse_move_to(away());
    pump(1000);
    assert!(shown(&app), "a panel shown from the tray stays when the mouse leaves it");
}

// AC3
#[test]
fn auto_hide_ms_after_it_was_shown_or_last_pressed_the_panel_hides() {
    let _desktop = desktop();
    let _cursor = Cursor::save();
    // Swallows the A the tap below sends, so it never reaches the desktop.
    let _log = KeyLog::start();
    // As launched, never touched.
    let app = launch_pad("auto_hide_ms = 1500");
    pump(1900);
    assert!(!shown(&app), "auto_hide_ms after launch, the panel is hidden");
    drop(app);

    let app = launch_pad("auto_hide_ms = 1500");
    pump(1000);
    assert!(shown(&app), "1000 ms after it was shown it is still there");
    let mut touch = Touch::new();
    touch.down(1, key_centre(left_panel(&app), 0));
    touch.up(1);
    pump(1000);
    assert!(shown(&app), "pressing A starts the count again");
    pump(900);
    assert!(!shown(&app), "auto_hide_ms after A was let go it is hidden");

    choose(&app, "左辺");
    assert!(shown(&app), "precondition: shown from the tray");
    pump(1000);
    assert!(shown(&app), "1000 ms after it was shown again it is still there");
    pump(900);
    assert!(!shown(&app), "auto_hide_ms after it was shown it is hidden");
}

// AC4
#[test]
fn neither_counts_while_a_key_is_held_or_a_modifier_latched() {
    let _desktop = desktop();
    let _cursor = Cursor::save();
    let log = KeyLog::start();
    let app = launch_pad("auto_hide_ms = 1500\nhover_ms = 400");
    let mut touch = Touch::new();

    // Auto-hide waits for a held key.
    touch.down(1, key_centre(left_panel(&app), 0));
    touch.hold(2500);
    assert!(shown(&app), "held for longer than auto_hide_ms: still shown");
    touch.up(1);
    assert_eq!(log.keys(), [(A, false), (A, true)], "A was held throughout");
    pump(1000);
    assert!(shown(&app), "counted from the release: still shown");
    pump(900);
    assert!(!shown(&app), "auto_hide_ms after the release: hidden");

    // And for a latched modifier.
    choose(&app, "左辺");
    let ctrl = key_centre(left_panel(&app), 1);
    touch.down(1, ctrl);
    touch.up(1);
    pump(2500);
    assert!(shown(&app), "Ctrl latched for longer than auto_hide_ms: still shown");
    touch.down(1, ctrl);
    touch.up(1);
    pump(1000);
    assert!(shown(&app), "counted from the latch's end: still shown");
    pump(900);
    assert!(!shown(&app), "auto_hide_ms after the latch ended: hidden");
    drop(touch);

    // Leaving a panel shown by hovering waits for a latch too.
    hover_show(&app);
    let ctrl = key_centre(left_panel(&app), 1);
    mouse_move_to(ctrl);
    click(ctrl);
    pump(100);
    mouse_move_to(away());
    pump(2000);
    assert!(shown(&app), "left with Ctrl latched: neither hover_ms nor auto_hide_ms is counted");
    mouse_move_to(ctrl);
    click(ctrl);
    pump(100);
    mouse_move_to(away());
    pump(700);
    assert!(!shown(&app), "left once the latch ended: hidden hover_ms later");
}

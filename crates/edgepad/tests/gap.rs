//! Acceptance tests for Issue #117: the bottom edge's panel and handle
//! kept off the very edge of the screen, so a swipe up from the handle
//! starts where Windows does not take it for its own swipe up from the
//! edge (the Start menu). Each edge's `gap` says how far, in logical
//! pixels; the bottom's is 16 unless written, the others' 0.
//!
//! The handle is 6 logical pixels thick by default. Off the edge, the
//! strip between it and the edge is EdgePad's still, for the mouse alone:
//! a finger there is not on the handle.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SPI_GETWORKAREA, SetCursorPos, SystemParametersInfoW};
use common::editor::within_3s;
use common::*;

/// A and B on the bottom and the top, under `edges`.
fn pads(edges: &str) -> String {
    format!("{edges}\n[set.pad]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nbottom = pad\ntop = pad\n")
}

/// The panel in the lower half of the primary monitor, if one shows.
fn bottom_panel(app: &Running) -> Option<RECT> {
    let m = primary_monitor();
    panels_now(app).into_iter().find(|r| r.top > (m.top + m.bottom) / 2)
}

fn top_panel(app: &Running) -> RECT {
    let m = primary_monitor();
    panels(app).into_iter().find(|r| r.bottom < (m.top + m.bottom) / 2).expect("a panel on the top edge")
}

/// How far up the taskbar reaches from the primary monitor's bottom, in
/// logical pixels; 0 if it is elsewhere or hides. Where it is, at the
/// bottom on CI's runners, a finger or the mouse near the screen's edge is
/// on it, and may open what it shows (Issue #123): the strip a test uses
/// is kept clear of it.
fn taskbar() -> i32 {
    let (m, work) = (primary_monitor(), work_area());
    let px_per_96 = px(96);
    ((m.bottom - work.bottom).max(0) * 96 + px_per_96 - 1) / px_per_96
}

/// The primary monitor's work area: less the taskbar.
fn work_area() -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut _, 0) };
    r
}

/// A finger put down at `p` and lifted where it was.
fn tap(p: POINT) {
    let mut touch = Touch::new();
    touch.down(1, p);
    touch.hold(100);
    touch.up(1);
    pump(500);
}

// AC1
#[test]
fn the_bottom_panel_and_handle_keep_off_the_screens_edge_and_the_others_do_not() {
    let _desktop = desktop();
    let m = primary_monitor();
    let app = launch_with(Some(&pads("")), None);
    let top = top_panel(&app);
    let bottom = bottom_panel(&app).expect("a panel on the bottom edge");
    assert_eq!(bottom.bottom, m.bottom - px(16), "the bottom panel 16 px off the edge: {}", bottom.bottom);
    assert_eq!(top.top, m.top, "the top panel against the edge");

    // Hidden: its handle is off the edge too. A finger between it and the
    // edge is not on it; on it, it shows the panel. Kept off past the
    // taskbar, if it is at the bottom: the strip is clear of it.
    drop(app);
    let gap = taskbar() + 16;
    let app = launch_with(Some(&pads(&format!("[edge.bottom]\ngap = {gap}\n"))), None);
    find_panel(&app);
    choose(&app, "下辺");
    assert!(within_3s(|| bottom_panel(&app).is_none()), "precondition: the bottom panel hidden");
    let x = (m.left + m.right) / 2;
    tap(POINT { x, y: m.bottom - px(gap) + px(8) });
    assert!(bottom_panel(&app).is_none(), "a tap between the handle and the edge is not on it");
    tap(POINT { x, y: m.bottom - px(gap) - px(3) });
    assert!(within_3s(|| bottom_panel(&app).is_some()), "a tap on the handle shows the panel");

    // gap = 0: against the edge, as before.
    drop(app);
    let app = launch_with(Some(&pads("[edge.bottom]\ngap = 0\n")), None);
    find_panel(&app);
    assert!(within_3s(|| bottom_panel(&app).is_some_and(|r| r.bottom == m.bottom)), "gap = 0: against the edge: {:?}", bottom_panel(&app).map(|r| r.bottom));
}

/// Puts the cursor back where it was when the test ends.
struct Cursor(POINT);

impl Drop for Cursor {
    fn drop(&mut self) {
        unsafe { SetCursorPos(self.0.x, self.0.y) };
    }
}

// AC5
#[test]
fn the_mouse_at_the_screens_edge_still_shows_a_panel_kept_off_it() {
    let _desktop = desktop();
    let mut was = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut was) };
    let _cursor = Cursor(was);
    let m = primary_monitor();
    let away = POINT { x: (m.left + m.right) / 2, y: (m.top + m.bottom) / 2 };
    mouse_move_to(away);
    // Off past the taskbar, if it is at the bottom: the mouse rests on the
    // strip at the taskbar's top. With none, at the very edge, where the
    // mouse stops.
    let gap = taskbar() + 16;
    let app = launch_with(Some(&pads(&format!("[general]\nhover_ms = 400\n[edge.bottom]\ngap = {gap}\n"))), None);
    find_panel(&app);
    choose(&app, "下辺");
    assert!(within_3s(|| bottom_panel(&app).is_none()), "precondition: the bottom panel hidden");
    mouse_move_to(POINT { x: away.x, y: work_area().bottom.min(m.bottom) - 1 });
    pump(700);
    assert!(within_3s(|| bottom_panel(&app).is_some()), "resting at the edge shows the panel");
}

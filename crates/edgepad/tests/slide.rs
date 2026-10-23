//! Acceptance tests for Issue #64: panels that slide in and out.
//!
//! A slide of 400 ms is checked about 100 ms in (part of the panel still
//! off the screen) and 600 ms in (done). Commands are posted to the tray
//! without waiting, so the first look comes before the slide is over.

mod common;

use std::time::Duration;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, PostMessageW, SetCursorPos, WM_COMMAND};
use common::*;

const A: u16 = b'A' as u16;

const SLIDE_MS: u64 = 400;

/// A and B down the left, under `general`.
fn launch_pad(general: &str) -> Running {
    let text = format!("[general]\nhotkey = Ctrl+Alt+F17\n{general}\n[set.pad]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nleft = pad\n");
    let app = launch_with(Some(&text), None);
    find_panel(&app);
    app
}

/// The left panel's window as it is now, wherever it is, if it is visible.
fn panel_now(app: &Running) -> Option<RECT> {
    panels_now(app).first().copied()
}

/// A rectangle, or none, for a failure message.
trait Sides {
    fn text(&self) -> String;
}

impl Sides for RECT {
    fn text(&self) -> String {
        format!("({}, {}, {}, {})", self.left, self.top, self.right, self.bottom)
    }
}

impl Sides for Option<RECT> {
    fn text(&self) -> String {
        self.as_ref().map_or("none".into(), Sides::text)
    }
}

fn sides(r: &impl Sides) -> String {
    r.text()
}

/// Whether the panel sits in its place against the left edge.
fn in_place(r: RECT) -> bool {
    r.left == primary_monitor().left
}

/// Whether the panel is part way out of the screen past the left edge.
fn sliding(r: RECT) -> bool {
    r.left < primary_monitor().left && r.right > primary_monitor().left
}

/// The tray's 左辺 command, posted without waiting for it.
struct LeftToggle {
    tray: windows_sys::Win32::Foundation::HWND,
    id: u32,
}

impl LeftToggle {
    fn of(app: &Running) -> LeftToggle {
        let id = menu_item(app, "左辺").id;
        LeftToggle { tray: find_window(app, TRAY_CLASS, false), id }
    }

    fn post(&self) {
        unsafe { PostMessageW(self.tray, WM_COMMAND, self.id as usize, 0) };
    }
}

/// Press and release Ctrl+Alt+F17 as a physical keyboard would, quickly.
fn hotkey() {
    for (vk, down) in [(VK_LCONTROL, true), (VK_LMENU, true), (VK_F17, true), (VK_F17, false), (VK_LMENU, false), (VK_LCONTROL, false)] {
        keyboard(vk, down);
    }
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

/// Look `ms` after the start: the panel as it is then.
fn after(app: &Running, ms: u64) -> Option<RECT> {
    pump(ms);
    panel_now(app)
}

// AC1
#[test]
fn with_slide_ms_off_panels_come_and_go_at_once() {
    let _desktop = desktop();
    let app = launch_pad("");
    let toggle = LeftToggle::of(&app);

    toggle.post();
    assert!(after(&app, 30).is_none(), "hidden at once");
    toggle.post();
    let r = after(&app, 30).expect("shown at once");
    assert!(in_place(r), "and in place at once: {}", sides(&r));
}

// AC2
#[test]
fn with_slide_ms_set_every_way_of_showing_slides_in_and_every_way_of_hiding_slides_out() {
    let _desktop = desktop();
    let app = launch_pad(&format!("slide_ms = {SLIDE_MS}"));
    let toggle = LeftToggle::of(&app);
    let check_in = |how: &str| {
        let r = after(&app, 100).unwrap_or_else(|| panic!("{how}: shown"));
        assert!(sliding(r), "{how}: 100 ms in, the panel is still sliding in: {}", sides(&r));
        let r = after(&app, 500).unwrap_or_else(|| panic!("{how}: still shown"));
        assert!(in_place(r), "{how}: then in place: {}", sides(&r));
    };
    let check_out = |how: &str| {
        let r = after(&app, 100).unwrap_or_else(|| panic!("{how}: 100 ms in, the panel is still on screen"));
        assert!(sliding(r), "{how}: and sliding out: {}", sides(&r));
        assert!(after(&app, 500).is_none(), "{how}: then gone");
    };

    toggle.post();
    check_out("hidden from the tray");
    toggle.post();
    check_in("shown from the tray");

    hotkey();
    check_out("hidden by the hotkey");
    hotkey();
    check_in("shown by the hotkey");

    // A flick toward the edge on the panel, then a tap on the handle.
    let mut touch = Touch::new();
    let a = key_centre(panel_now(&app).expect("shown"), 0);
    touch.down(1, a);
    touch.move_to(1, POINT { x: primary_monitor().left + 2, y: a.y });
    check_out("hidden by a flick");
    touch.up(1);
    pump(200);
    let handle = left_handle(&app);
    touch.down(1, centre(handle));
    touch.up(1);
    check_in("shown by a tap on the handle");
    drop(touch);

    // A short swipe from the handle shows it dimmed: it slides in too.
    toggle.post();
    pump(SLIDE_MS + 200);
    let start = centre(left_handle(&app));
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(start);
    mouse_down_at(start);
    mouse_at(POINT { x: start.x + key_px() * 3 / 4, y: start.y }, 0);
    let r = after(&app, 100).expect("dimmed: shown");
    mouse_up_at(POINT { x: start.x + key_px() * 3 / 4, y: start.y });
    unsafe { SetCursorPos(saved.x, saved.y) };
    assert!(sliding(r), "shown dimmed: 100 ms in, still sliding in: {}", sides(&r));
    let r = after(&app, 500).expect("dimmed: still shown");
    assert!(in_place(r), "shown dimmed: then in place: {}", sides(&r));
}

// AC3
#[test]
fn keys_let_go_as_the_slide_out_starts_and_a_sliding_panel_presses_nothing() {
    let _desktop = desktop();
    let app = launch_pad(&format!("slide_ms = {SLIDE_MS}"));
    let toggle = LeftToggle::of(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, key_centre(panel_now(&app).expect("shown"), 0));
    touch.hold(100);
    assert_eq!(log.keys(), [(A, false)], "precondition: A held");
    toggle.post();
    let r = after(&app, 60).expect("still on screen");
    assert!(sliding(r) || in_place(r), "about to slide or sliding out: {}", sides(&r));
    assert_eq!(log.keys(), [(A, false), (A, true)], "A is let go as the slide out starts");
    touch.up(1);
    pump(SLIDE_MS + 200);
    assert!(panel_now(&app).is_none(), "precondition: hidden");

    // A touch on the panel as it slides in, where it is at that moment.
    toggle.post();
    let r = after(&app, 100).expect("sliding in");
    assert!(sliding(r), "precondition: sliding in: {}", sides(&r));
    let on_it = POINT { x: (r.left.max(primary_monitor().left) + r.right) / 2, y: key_centre(r, 0).y };
    touch.down(1, on_it);
    touch.up(1);
    pump(SLIDE_MS);
    assert_eq!(log.keys().len(), 2, "a touch on the sliding panel presses nothing: {:?}", log.keys());

    let r = panel_now(&app).expect("in place");
    assert!(in_place(r), "precondition: in place: {}", sides(&r));
    touch.down(1, key_centre(r, 0));
    touch.up(1);
    assert_eq!(log.keys()[2..], [(A, false), (A, true)], "once in place, keys press again");
}

// AC4
#[test]
fn the_timer_runs_only_while_sliding_and_an_idle_panel_after_it_uses_no_cpu() {
    let _desktop = desktop();
    let app = launch_pad(&format!("slide_ms = {SLIDE_MS}"));
    let toggle = LeftToggle::of(&app);
    toggle.post();
    let r = after(&app, 100);
    assert!(r.is_some_and(sliding), "precondition: it slides out: {}", sides(&r));
    pump(SLIDE_MS + 100);
    toggle.post();
    pump(SLIDE_MS + 600);
    assert!(panel_now(&app).is_some_and(in_place), "precondition: slid out and in again");

    let before = cpu_time_100ns(&app);
    std::thread::sleep(Duration::from_secs(2));
    let used_ms = (cpu_time_100ns(&app) - before) as f64 / 10_000.0;
    assert!(used_ms < 10.0, "idle CPU after sliding: {used_ms:.1}ms in 2s");
}

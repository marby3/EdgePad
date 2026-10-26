//! Acceptance tests for Issue #64: panels that slide in and out.
//!
//! A slide of 400 ms is checked about 100 ms in (only part of the panel in
//! the screen) and 600 ms in (done). Commands are posted to the tray without
//! waiting, so the first look comes before the slide is over. A sliding
//! panel never reaches past its edge: only the part already in is shown.

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
fn sides(r: &Option<RECT>) -> String {
    r.map_or("none".into(), |r| format!("({}, {}, {}, {})", r.left, r.top, r.right, r.bottom))
}

/// Whether the panel sits whole in its place against the left edge: a key
/// thick, one column.
fn in_place(r: RECT) -> bool {
    r.left == primary_monitor().left && r.right - r.left == key_px()
}

/// Whether only part of the panel is in, against the left edge, and none of
/// it beyond the edge.
fn sliding(r: RECT) -> bool {
    r.left == primary_monitor().left && r.right - r.left < key_px()
}

/// Watch the left panel closely after a command, until it is no longer as
/// it was (`was`: shown, or not): the first new state seen. A slide shows a
/// part before the whole; coming and going at once shows the end at once.
/// Panics if nothing changes within a second.
fn first_change(app: &Running, was: bool) -> Option<RECT> {
    let end = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        let now = panel_now(app);
        if now.is_some() != was {
            return now;
        }
        assert!(std::time::Instant::now() < end, "nothing changed within a second");
        pump(5);
    }
}

/// Watch the left panel closely after a command to hide it, until it is
/// gone: whether it stayed whole and in place until then, as one that goes
/// at once does. A slide out shows a part on its way.
fn gone_whole(app: &Running) -> bool {
    let end = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        match panel_now(app) {
            None => return true,
            Some(r) if !in_place(r) => return false,
            Some(_) => {}
        }
        assert!(std::time::Instant::now() < end, "not gone within a second");
        pump(5);
    }
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

/// Press and release Ctrl+Alt+F17 as a physical keyboard would, in one
/// SendInput: `keyboard` waits after each key, which would outlast a slide.
fn hotkey() {
    let keys = [(VK_LCONTROL, true), (VK_LMENU, true), (VK_F17, true), (VK_F17, false), (VK_LMENU, false), (VK_LCONTROL, false)];
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&(vk, down)| {
            let mut input: INPUT = unsafe { std::mem::zeroed() };
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = vk;
            input.Anonymous.ki.dwFlags = if down { 0 } else { KEYEVENTF_KEYUP };
            input.Anonymous.ki.dwExtraInfo = KEYBOARD_TAG;
            input
        })
        .collect();
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    assert_eq!(sent as usize, inputs.len(), "SendInput");
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
    assert!(gone_whole(&app), "hidden at once, whole until gone");
    toggle.post();
    let r = first_change(&app, false).expect("shown");
    assert!(in_place(r), "whole and in place from the first look: {}", sides(&Some(r)));
}

// AC2: the tray, the hotkey, a flick on the panel, a tap on the handle and
// a short swipe from it. Hovering and auto-hide go the same way as these
// (apply_visibility), and are left to their own tests.
#[test]
fn with_slide_ms_set_every_way_of_showing_slides_in_and_every_way_of_hiding_slides_out() {
    let _desktop = desktop();
    let app = launch_pad(&format!("slide_ms = {SLIDE_MS}"));
    let toggle = LeftToggle::of(&app);
    let check_in = |how: &str| {
        let r = after(&app, 100).unwrap_or_else(|| panic!("{how}: shown"));
        assert!(sliding(r), "{how}: 100 ms in, the panel is still sliding in: {}", sides(&Some(r)));
        let r = after(&app, 500).unwrap_or_else(|| panic!("{how}: still shown"));
        assert!(in_place(r), "{how}: then in place: {}", sides(&Some(r)));
    };
    let check_out = |how: &str| {
        let r = after(&app, 100).unwrap_or_else(|| panic!("{how}: 100 ms in, the panel is still on screen"));
        assert!(sliding(r), "{how}: and sliding out: {}", sides(&Some(r)));
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

    // A short swipe from the handle shows it (dimmed until Issue #85): it
    // slides in too.
    toggle.post();
    pump(SLIDE_MS + 200);
    let start = centre(left_handle(&app));
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(start);
    mouse_down_at(start);
    mouse_at(POINT { x: start.x + key_px() * 3 / 4, y: start.y }, 0);
    let r = after(&app, 100).expect("swiped: shown");
    mouse_up_at(POINT { x: start.x + key_px() * 3 / 4, y: start.y });
    unsafe { SetCursorPos(saved.x, saved.y) };
    assert!(sliding(r), "swiped: 100 ms in, still sliding in: {}", sides(&Some(r)));
    let r = after(&app, 500).expect("swiped: still shown");
    assert!(in_place(r), "swiped: then in place: {}", sides(&Some(r)));
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
    let end = std::time::Instant::now() + Duration::from_secs(1);
    while log.keys().len() < 2 && std::time::Instant::now() < end {
        pump(5);
    }
    assert_eq!(log.keys(), [(A, false), (A, true)], "A is let go as the slide out starts");
    let r = panel_now(&app);
    assert!(r.is_some(), "the panel is still on its way out as A is let go");
    touch.up(1);
    pump(SLIDE_MS + 200);
    assert!(panel_now(&app).is_none(), "precondition: hidden");

    // A touch on the panel as it slides in, where it is at that moment.
    toggle.post();
    let r = after(&app, 100).expect("sliding in");
    assert!(sliding(r), "precondition: sliding in: {}", sides(&Some(r)));
    let on_it = POINT { x: (r.left + r.right) / 2, y: key_centre(RECT { right: r.left + key_px(), ..r }, 0).y };
    touch.down(1, on_it);
    touch.up(1);
    pump(SLIDE_MS);
    assert_eq!(log.keys().len(), 2, "a touch on the sliding panel presses nothing: {:?}", log.keys());

    let r = panel_now(&app).expect("in place");
    assert!(in_place(r), "precondition: in place: {}", sides(&Some(r)));
    touch.down(1, key_centre(r, 0));
    touch.up(1);
    assert_eq!(log.keys()[2..], [(A, false), (A, true)], "once in place, keys press again");
}

// AC4: a slide's timer, if it ran on, would show as CPU time while idle.
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

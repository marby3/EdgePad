//! Acceptance tests for Issue #60: docking an edge as an AppBar, so the
//! work area (what a maximised window fills) gives the panel its room.
//!
//! The work area is the user's own: every test ends EdgePad through its tray
//! menu, the way that gives the room back, even when an assertion fails.

mod common;

use std::mem::{size_of, zeroed};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

/// The work area of the primary monitor.
fn work_area() -> RECT {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        assert!(GetMonitorInfoW(monitor, &mut info) != 0, "GetMonitorInfoW");
        info.rcWork
    }
}

fn rect(r: RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

/// Wait up to two seconds for the work area to be `want`.
fn work_area_becomes(want: RECT) -> bool {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        if rect(work_area()) == rect(want) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        pump(50);
    }
}

/// `base` with its left side moved in by `by`.
fn narrowed(base: RECT, by: i32) -> RECT {
    RECT { left: base.left + by, ..base }
}

/// A running EdgePad, ended from its tray menu when dropped, which then
/// waits for the work area it found at launch to come back.
struct Docked {
    app: Running,
    tray: windows_sys::Win32::Foundation::HWND,
    exit: u32,
    base: RECT,
}

impl Docked {
    fn launch(text: &str) -> Docked {
        let base = work_area();
        let app = launch_with(Some(text), None);
        find_panel(&app);
        let tray = find_window(&app, TRAY_CLASS, false);
        let exit = menu_item(&app, "終了").id;
        Docked { app, tray, exit, base }
    }

    /// End EdgePad from the tray and wait for it to go.
    fn quit(&mut self) -> bool {
        unsafe { PostMessageW(self.tray, WM_COMMAND, self.exit as usize, 0) };
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if matches!(self.app.0.try_wait(), Ok(Some(_))) {
                return true;
            }
            pump(50);
        }
        false
    }
}

impl Drop for Docked {
    fn drop(&mut self) {
        // Best effort, never a panic: the test may be failing already.
        if matches!(self.app.0.try_wait(), Ok(None)) {
            let _ = self.quit();
        }
        // The next test starts from the work area this one found.
        let _ = work_area_becomes(self.base);
    }
}

fn pad(edge: &str) -> String {
    format!("{edge}[set.pad]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nleft = pad\n")
}

fn save(app: &Running, text: &str) {
    std::fs::write(exe_dir(app).join("edgepad.ini"), text).expect("save edgepad.ini");
}

// AC1
#[test]
fn a_docked_edge_takes_the_panels_thickness_from_the_work_area() {
    let _desktop = desktop();
    let base = work_area();
    let key = key_px();

    let docked = Docked::launch(&pad("[edge.left]\ndock = true\n"));
    assert!(work_area_becomes(narrowed(base, key)), "docked: work area {:?}, want the left {key} px taken from {:?}", rect(work_area()), rect(base));
    let panel = left_panel(&docked.app);
    assert_eq!(panel.right - panel.left, key, "the panel stays one key thick");
    drop(docked);
    assert!(work_area_becomes(base), "precondition for what follows: the room came back");

    let _overlay = Docked::launch(&pad(""));
    pump(500);
    assert_eq!(rect(work_area()), rect(base), "not docked: the work area is untouched");
}

// AC2
#[test]
fn hiding_pausing_or_quitting_gives_the_room_back_and_showing_takes_it_again() {
    let _desktop = desktop();
    let base = work_area();
    let key = key_px();
    let mut docked = Docked::launch(&pad("[edge.left]\ndock = true\n"));
    assert!(work_area_becomes(narrowed(base, key)), "precondition: docked");

    choose(&docked.app, "左辺");
    assert!(work_area_becomes(base), "hidden: the room is given back, work area {:?}", rect(work_area()));
    choose(&docked.app, "左辺");
    assert!(work_area_becomes(narrowed(base, key)), "shown again: taken again");

    choose(&docked.app, "一時停止");
    assert!(work_area_becomes(base), "paused: given back");
    choose(&docked.app, "一時停止");
    assert!(work_area_becomes(narrowed(base, key)), "resumed: taken again");

    assert!(docked.quit(), "EdgePad ends from the tray");
    assert!(work_area_becomes(base), "quit: given back, work area {:?}", rect(work_area()));
}

// AC3
#[test]
fn rebuilt_panels_take_the_room_their_new_settings_give() {
    let _desktop = desktop();
    let base = work_area();
    let key = key_px();
    let docked = Docked::launch(&pad("[edge.left]\ndock = true\n"));
    assert!(work_area_becomes(narrowed(base, key)), "precondition: docked one key thick");

    save(&docked.app, &pad("[edge.left]\ndock = true\ncolumns = 2\n"));
    assert!(work_area_becomes(narrowed(base, key * 2)), "two columns: two keys taken, work area {:?}", rect(work_area()));

    save(&docked.app, &pad("[edge.left]\ndock = false\ncolumns = 2\n"));
    assert!(work_area_becomes(base), "dock turned off: all given back, work area {:?}", rect(work_area()));

    save(&docked.app, &pad("[edge.left]\ndock = true\n"));
    assert!(work_area_becomes(narrowed(base, key)), "dock on again: one key taken");
}

// Issue #117, AC4: an edge kept off the screen's edge docks its gap too, so
// a maximised window does not slip under the panel.
#[test]
fn a_docked_edge_kept_off_the_screens_edge_takes_its_gap_too() {
    let _desktop = desktop();
    let base = work_area();
    let key = key_px();
    let text = "[edge.bottom]\ndock = true\n[set.pad]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nbottom = pad\n";
    let docked = Docked::launch(text);
    let want = RECT { bottom: base.bottom - key - px(16), ..base };
    assert!(work_area_becomes(want), "docked: work area {:?}, want {:?}", rect(work_area()), rect(want));
    drop(docked);
}

// Issue #121: the room a docked edge takes runs its whole length; where the
// panel is not, EdgePad's fill covers it, at the panel's background colour
// and opacity, and takes a press there as the panel's background does.

/// EdgePad's fill windows shown now: the class is the contract with
/// crates/edgepad/src/panel.rs.
fn fills(app: &Running) -> Vec<windows_sys::Win32::Foundation::HWND> {
    let name = wide("EdgePadDockFill");
    let mut found = Vec::new();
    let mut hwnd = std::ptr::null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(std::ptr::null_mut(), hwnd, name.as_ptr(), std::ptr::null()) };
        if hwnd.is_null() {
            return found;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 {
            found.push(hwnd);
        }
    }
}

/// A point of the left strip three keys below the left panel, and one
/// above it: past where a pie menu opened from it reaches.
fn beside(app: &Running) -> [POINT; 2] {
    let (panel, key) = (left_panel(app), key_px());
    [POINT { x: panel.left + key / 2, y: panel.bottom + key * 3 }, POINT { x: panel.left + key / 2, y: panel.top - key * 3 }]
}

/// Whether the window on top at `p` is one of `app`'s.
fn ours_at(app: &Running, p: POINT) -> bool {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(WindowFromPoint(p), &mut pid) };
    pid == app.0.id()
}

/// The average colour of the 3 x 3 pixels at `p`.
fn colour_at(p: POINT) -> [i32; 3] {
    let pixels = capture(RECT { left: p.x - 1, top: p.y - 1, right: p.x + 2, bottom: p.y + 2 });
    let sum = pixels.iter().map(|&c| channels(c)).fold([0; 3], |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2]]);
    sum.map(|v| v / 9)
}

fn same(a: [i32; 3], b: [i32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 3)
}

/// The opacity in percent at `p`, from how much of `backdrop` shows through.
fn opacity_at(backdrop: &TestWindow, p: POINT) -> i32 {
    set_backdrop(backdrop.0, 0x00FF_FFFF);
    let on_white = colour_at(p);
    set_backdrop(backdrop.0, 0);
    let on_black = colour_at(p);
    100 - (0..3).map(|c| on_white[c] - on_black[c]).sum::<i32>() / 3 * 100 / 255
}

/// Wait up to 2 s for `done`.
fn within_2s(mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(2);
    while !done() {
        if Instant::now() > end {
            return false;
        }
        pump(50);
    }
    true
}

// Issue #121, AC1
#[test]
fn the_docked_strip_is_covered_beside_the_panel_too() {
    let _desktop = desktop();
    let docked = Docked::launch(&pad("[edge.left]\ndock = true\n"));
    assert!(within_2s(|| work_area().left > docked.base.left), "precondition: docked");
    for p in beside(&docked.app) {
        assert!(within_2s(|| ours_at(&docked.app, p)), "EdgePad covers the strip at ({}, {})", p.x, p.y);
    }
}

// Issue #121, AC2
#[test]
fn the_fill_has_the_panels_colour_and_opacity_and_follows_them() {
    let _desktop = desktop();
    let m = primary_monitor();
    let backdrop = test_window("EdgePadTestDockFill", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, RECT { right: m.left + key_px() * 3, ..m });
    let text = |theme: &str| {
        format!(
            "[general]\nopacity = 70\ndim_opacity = 40\ntheme = {theme}\n[edge.left]\ndock = true\n\
             [set.pad]\nkey = A | hold | A\nkey = 変形 | pie | menu\n[set.menu]\nkey = 一 | shortcut | F1\nkey = 二 | shortcut | F2\n\
             [profile.default]\nleft = pad\n"
        )
    };
    let docked = Docked::launch(&text("light"));
    assert!(within_2s(|| work_area().left > docked.base.left), "precondition: docked");
    let [fill, _] = beside(&docked.app);
    let corner = |app: &Running| {
        let panel = left_panel(app);
        POINT { x: panel.left + 2, y: panel.top + 2 }
    };
    let o = opacity_at(&backdrop, fill);
    assert!((o - 70).abs() <= 4, "at the panel's opacity 70%, not {o}%");
    set_backdrop(backdrop.0, 0x00FF_FFFF);
    let light = colour_at(fill);
    assert!(same(light, colour_at(corner(&docked.app))), "the panel's background colour: {light:?}");

    // Dimmed with the panel while its pie menu is open.
    let _log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(left_panel(&docked.app), 1));
    pump(300);
    let o = opacity_at(&backdrop, fill);
    assert!((o - 40).abs() <= 4, "dimmed to 40% with the pie menu open, not {o}%");
    touch.up(1);
    drop(touch);
    pump(300);

    // The theme changes with the panel's.
    save(&docked.app, &text("dark"));
    set_backdrop(backdrop.0, 0x00FF_FFFF);
    assert!(
        within_2s(|| !same(colour_at(fill), light) && same(colour_at(fill), colour_at(corner(&docked.app)))),
        "dark with the panel: fill {:?}, panel {:?}",
        colour_at(fill),
        colour_at(corner(&docked.app))
    );
}

// Issue #121, AC3
#[test]
fn a_press_on_the_fill_is_a_press_on_the_panels_background() {
    let _desktop = desktop();
    let docked = Docked::launch(&pad("[edge.left]\ndock = true\n"));
    assert!(within_2s(|| work_area().left > docked.base.left), "precondition: docked");
    let [fill, _] = beside(&docked.app);
    assert!(within_2s(|| ours_at(&docked.app, fill)), "precondition: the strip covered");

    // A tap there sends nothing and leaves the panel shown.
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, fill);
    pump(100);
    touch.up(1);
    pump(300);
    assert!(log.keys().is_empty(), "no key: {:?}", log.keys());
    left_panel(&docked.app);

    // A swipe from there toward the edge hides the panel, as from its
    // background.
    touch.down(1, POINT { x: fill.x + key_px() / 3, y: fill.y });
    touch.move_to(1, POINT { x: fill.x - key_px() / 3, y: fill.y });
    touch.move_to(1, POINT { x: primary_monitor().left + 1, y: fill.y });
    touch.up(1);
    let m = primary_monitor();
    assert!(within_2s(|| panels_now(&docked.app).iter().all(|r| r.left != m.left)), "swiped to the edge: hidden");
}

// Issue #121, AC4
#[test]
fn the_fill_goes_with_the_room_and_overlay_edges_have_none() {
    let _desktop = desktop();
    let docked = Docked::launch(&pad("[edge.left]\ndock = true\n"));
    assert!(within_2s(|| !fills(&docked.app).is_empty()), "precondition: filled while docked");

    choose(&docked.app, "左辺");
    assert!(within_2s(|| fills(&docked.app).is_empty()), "hidden: no fill");
    choose(&docked.app, "左辺");
    assert!(within_2s(|| !fills(&docked.app).is_empty()), "shown again: filled");
    choose(&docked.app, "一時停止");
    assert!(within_2s(|| fills(&docked.app).is_empty()), "paused: no fill");
    choose(&docked.app, "一時停止");
    assert!(within_2s(|| !fills(&docked.app).is_empty()), "resumed: filled");
    save(&docked.app, &pad("[edge.left]\ndock = false\n"));
    assert!(within_2s(|| fills(&docked.app).is_empty()), "dock turned off: no fill");
    drop(docked);

    let overlay = Docked::launch(&pad(""));
    pump(500);
    assert!(fills(&overlay.app).is_empty(), "an edge not docked has no fill");
    let [below, _] = beside(&overlay.app);
    assert!(!ours_at(&overlay.app, below), "and nothing of EdgePad's beside its panel");
}

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
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_COMMAND};
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

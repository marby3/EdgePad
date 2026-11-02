//! Acceptance tests for Issue #20, criteria 2 to 4: panels on the chosen
//! monitor, and laid out again on WM_DISPLAYCHANGE.
//!
//! The expected monitor is worked out the way the setting describes, from the
//! monitors this machine has, so the tests hold with one monitor or several.

mod common;

use std::mem::zeroed;

use windows_sys::Win32::Foundation::{LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
    unsafe { (*(out as *mut Vec<HMONITOR>)).push(monitor) };
    1
}

/// Every monitor with its rectangle and GDI device name, in screen order:
/// left to right, then top to bottom.
fn monitors_in_order() -> Vec<(HMONITOR, RECT, String)> {
    let mut found: Vec<HMONITOR> = Vec::new();
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(collect), &mut found as *mut _ as LPARAM) };
    let mut all: Vec<(HMONITOR, RECT, String)> = found
        .into_iter()
        .map(|m| {
            let mut info: MONITORINFOEXW = unsafe { zeroed() };
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            unsafe { GetMonitorInfoW(m, &mut info as *mut _ as *mut MONITORINFO) };
            let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            (m, info.monitorInfo.rcMonitor, String::from_utf16_lossy(&info.szDevice[..len]))
        })
        .collect();
    all.sort_by_key(|(_, r, _)| (r.left, r.top));
    all
}

fn dpi_of(monitor: HMONITOR) -> i32 {
    let (mut x, mut y) = (0, 0);
    assert_eq!(unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }, 0);
    x as i32
}

fn key_on(monitor: HMONITOR) -> i32 {
    (KEY_LOGICAL_PX * dpi_of(monitor) + 48) / 96
}

/// The panel lying on monitor `m` (its centre inside it), against its `left`
/// edge (else its right edge). Stacked monitors share a left edge, so the
/// centre tells their panels apart.
fn panel_against(all: &[RECT], m: RECT, left: bool) -> RECT {
    let on = |r: &RECT| {
        let (x, y) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        x >= m.left && x < m.right && y >= m.top && y < m.bottom
    };
    let found = all.iter().find(|r| on(r) && if left { r.left == m.left } else { r.right == m.right });
    *found.unwrap_or_else(|| panic!("no panel against that edge of {:?}", (m.left, m.top, m.right, m.bottom)))
}

/// Two keys per set, five per left set, so panels are tall.
fn settings(left: &str, right: &str) -> String {
    format!(
        "[edge.left]\nmonitor = {left}\n[edge.right]\nmonitor = {right}\n\
         [set.l]\nkey = Esc | hold | Esc\nkey = Ctrl | modifier | Ctrl\nkey = Shift | modifier | Shift\n\
         key = Alt | modifier | Alt\nkey = Space | hold | Space\n\
         [set.r]\nkey = A | hold | A\nkey = B | hold | B\n\
         [profile.default]\nleft = l\nright = r\n"
    )
}

// AC2
#[test]
fn each_panel_sits_on_its_chosen_monitor_sized_by_that_monitors_dpi() {
    let _desktop = desktop();
    let monitors = monitors_in_order();
    let primary = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let primary_rect = monitors.iter().find(|(h, _, _)| *h == primary).unwrap().1;

    // Issue #149, AC1: monitor 1 is the one Windows calls 1, \\.\DISPLAY1
    // (primary when there is none); a number no monitor has means primary.
    let app = launch_with(Some(&settings("1", "16")), None);
    let all = panels(&app);
    let one = monitors.iter().find(|(_, _, name)| name == r"\\.\DISPLAY1");
    let (first, first_rect) = one.map_or((primary, primary_rect), |&(h, r, _)| (h, r));
    assert!(!monitors.iter().any(|(_, _, name)| name == r"\\.\DISPLAY16"), "no monitor 16 here");
    let left = panel_against(&all, first_rect, true);
    let key = key_on(first);
    assert_eq!((left.right - left.left, left.bottom - left.top), (key, 5 * key), "sized by monitor 1's DPI");
    let centred = first_rect.top + (first_rect.bottom - first_rect.top - 5 * key) / 2;
    assert!((left.top - centred).abs() <= 1, "centred on monitor 1: {} vs {centred}", left.top);
    let right = panel_against(&all, primary_rect, false);
    assert_eq!(right.right - right.left, key_on(primary), "an unknown number falls back to primary");
    drop(app);

    // cursor: the monitor under the mouse when the panel is placed. The
    // cursor is put on the last monitor (the primary one when there is only
    // one) so nothing else decides where it is.
    let (under, under_rect, _) = monitors.last().unwrap().clone();
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    unsafe { SetCursorPos((under_rect.left + under_rect.right) / 2, (under_rect.top + under_rect.bottom) / 2) };
    let app = launch_with(Some(&settings("cursor", "primary")), None);
    let all = panels(&app);
    unsafe { SetCursorPos(saved.x, saved.y) };
    let left = panel_against(&all, under_rect, true);
    assert_eq!(left.right - left.left, key_on(under), "on the monitor under the cursor");
}

// AC3
#[test]
fn on_wm_displaychange_every_panel_is_placed_again() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("primary", "primary")), None);
    let before = panels(&app);
    let name = wide(PANEL_CLASS);

    // Knock every panel out of place, then announce a display change.
    let mut hwnd = std::ptr::null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(std::ptr::null_mut(), hwnd, name.as_ptr(), std::ptr::null()) };
        if hwnd.is_null() {
            break;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() {
            unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), 300, 300, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
            unsafe { SendMessageW(hwnd, WM_DISPLAYCHANGE, 32, 0) };
        }
    }
    pump(200);

    let mut after = panels(&app);
    let mut before = before;
    let key = |r: &RECT| (r.left, r.top, r.right, r.bottom);
    before.sort_by_key(key);
    after.sort_by_key(key);
    assert_eq!(after.iter().map(key).collect::<Vec<_>>(), before.iter().map(key).collect::<Vec<_>>(), "panels back on their edges");
}

// AC4
#[test]
fn a_key_held_through_a_display_change_stays_down_and_goes_up_once() {
    let _desktop = desktop();
    let app = launch();
    let (hwnd, r) = panel_rect(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(r, CTRL));
    touch.hold(200);

    unsafe { SendMessageW(hwnd, WM_DISPLAYCHANGE, 32, 0) };
    touch.hold(200);
    assert_eq!(log.keys(), [down(CTRL)], "Ctrl stays down through the display change");
    touch.up(1);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "one key-up when the finger lifts");
}

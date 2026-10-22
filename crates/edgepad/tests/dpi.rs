//! Acceptance tests for Issue #18: Per-Monitor DPI Aware V2, and laying the
//! panels out again on WM_DPICHANGED.
//!
//! CI cannot change a monitor's scale, so a DPI change is simulated by sending
//! the panel WM_DPICHANGED with the new DPI, as Windows would.

mod common;

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
    GetWindowDpiAwarenessContext, MDT_EFFECTIVE_DPI,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_DPICHANGED};
use common::*;

/// Key side at `dpi`, rounded as EdgePad rounds it.
fn key_at(dpi: i32) -> i32 {
    (KEY_LOGICAL_PX * dpi + 48) / 96
}

/// Tell `panel` its DPI is now `dpi`, as Windows does when the scale changes.
fn change_dpi(panel: HWND, dpi: u32) {
    // EdgePad recomputes the position itself and ignores the suggested
    // rectangle in lParam, so none is sent.
    unsafe { SendMessageW(panel, WM_DPICHANGED, (dpi << 16 | dpi) as usize, 0) };
    pump(200);
}

// AC1
#[test]
fn the_panel_runs_per_monitor_dpi_aware_v2() {
    let _desktop = desktop();
    let app = launch();
    let panel = find_panel(&app);
    let equal = unsafe {
        AreDpiAwarenessContextsEqual(GetWindowDpiAwarenessContext(panel), DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    };
    assert!(equal != 0, "the panel window is not Per-Monitor DPI Aware V2");
}

// AC2
#[test]
fn keys_are_sized_by_the_dpi_of_the_panels_monitor() {
    let _desktop = desktop();
    let (mut x, mut y) = (0, 0);
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    assert_eq!(unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }, 0, "GetDpiForMonitor");

    let app = launch();
    let r = window_rect(find_panel(&app));
    let key = key_at(x as i32);
    assert_eq!((r.right - r.left, r.bottom - r.top), (key, key * KEY_COUNT), "56 px scaled to {x} DPI");
}

// AC3
#[test]
fn on_wm_dpichanged_the_panel_is_laid_out_again_at_the_new_dpi() {
    let _desktop = desktop();
    let app = launch();
    let hwnd = find_panel(&app);
    change_dpi(hwnd, 192);

    let r = window_rect(hwnd);
    let (m, key) = (primary_monitor(), key_at(192));
    assert_eq!((r.right - r.left, r.bottom - r.top), (key, key * KEY_COUNT), "keys are 56 px at 192 DPI");
    assert_eq!(r.left, m.left, "still flush with the left edge");
    let centred = m.top + (m.bottom - m.top - key * KEY_COUNT) / 2;
    assert!((r.top - centred).abs() <= 1, "still centred: top {} vs {centred}", r.top);

    let log = KeyLog::start();
    let mut touch = Touch::new();
    let shift = cell(r, key, 0, SHIFT as i32);
    touch.down(1, shift);
    touch.hold(200);
    touch.up(1);
    assert_eq!(log.keys(), [down(SHIFT), up(SHIFT)], "Shift is pressed at its new place");
}

// AC4
#[test]
fn a_key_held_through_a_dpi_change_stays_down_and_goes_up_once() {
    let _desktop = desktop();
    let app = launch();
    let (hwnd, r) = panel_rect(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(r, CTRL));
    touch.hold(200);

    change_dpi(hwnd, 144);
    touch.hold(200);
    assert_eq!(log.keys(), [down(CTRL)], "Ctrl stays down through the DPI change");
    touch.up(1);
    assert_eq!(log.keys(), [down(CTRL), up(CTRL)], "one key-up when the finger lifts");
}

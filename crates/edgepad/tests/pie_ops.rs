//! Acceptance tests for Issue #72: using a pie menu (spec 4.6). Slid to an
//! item and let go, the item runs as a panel key would; let go in the middle
//! or toward the edge, nothing does. A tap leaves the menu open for an item
//! to be tapped.

mod common;

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const PIE_CLASS: &str = "EdgePadPie";

/// Shift, the pie key, and A on the left; the pie opens one item of each
/// kind it can hold.
const SETTINGS: &str = "\
[set.pad]
key = Shift | modifier | Shift
key = 変形  | pie      | menu
key = A     | shortcut | A
[set.menu]
key = B    | shortcut | B
key = 二回 | macro    | C, D
key = 挨拶 | text     | ab
key = 電卓 | run      | calc.exe
key = 切替 | page     | other
[set.other]
key = X | shortcut | X
key = Y | shortcut | Y
[profile.default]
left = pad
";

const ITEMS: usize = 5;
const SHIFT_KEY: usize = 0;
const PIE_KEY: usize = 1;

const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

const B: u16 = b'B' as u16;
const C: u16 = b'C' as u16;
const D: u16 = b'D' as u16;

/// The pie menu window of `app` now, without waiting for one.
fn pie_window(app: &Running) -> Option<HWND> {
    let name = wide(PIE_CLASS);
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() {
            return Some(hwnd);
        }
    }
}

/// The middle of item `i`'s direction, in degrees: -90 is up on the left
/// edge, +90 down.
fn angle(i: usize) -> f64 {
    -90.0 + (i as f64 + 0.5) * 180.0 / ITEMS as f64
}

/// `d` pixels from `p` toward `degrees` (0 is right, -90 up).
fn toward(p: POINT, degrees: f64, d: f64) -> POINT {
    let a = degrees.to_radians();
    POINT { x: p.x + (d * a.cos()).round() as i32, y: p.y + (d * a.sin()).round() as i32 }
}

/// Well inside item `i`'s sector: two keys out, between the hole (half a
/// key) and the outside (more than two keys for five items).
fn on_item(p: POINT, i: usize) -> POINT {
    toward(p, angle(i), 2.0 * key_px() as f64)
}

fn tap(touch: &mut Touch, id: u32, p: POINT) {
    touch.down(id, p);
    touch.up(id);
}

/// Press the pie key at `p`, slide to `to`, and let go.
fn slide(touch: &mut Touch, p: POINT, to: POINT) {
    touch.down(1, p);
    touch.move_to(1, to);
    touch.up(1);
}

fn launch_pad() -> (Running, POINT) {
    let app = launch_with(Some(SETTINGS), None);
    let p = key_centre(left_panel(&app), PIE_KEY);
    (app, p)
}

// AC1
#[test]
fn sliding_to_an_item_and_letting_go_runs_it_as_a_panel_key_would() {
    let _desktop = desktop();
    let (app, p) = launch_pad();
    let log = KeyLog::start();
    let mut touch = Touch::new();

    slide(&mut touch, p, on_item(p, 0));
    assert_eq!(log.keys(), [d(B), u(B)], "the shortcut B");
    assert!(pie_window(&app).is_none(), "and the menu is closed");

    slide(&mut touch, p, on_item(p, 1));
    log.wait_for(6);
    assert_eq!(log.keys()[2..], [d(C), u(C), d(D), u(D)], "the macro's steps");

    slide(&mut touch, p, on_item(p, 2));
    log.wait_for(10);
    assert!(log.keys()[6..].iter().all(|&(vk, _)| vk == VK_PACKET), "the text, typed: {:?}", log.keys());
    assert_eq!(log.keys().len(), 10, "a and b, down and up");

    slide(&mut touch, p, on_item(p, 3));
    let runs: Vec<String> = test_events(&app).into_iter().filter(|e| e.starts_with("run: ")).collect();
    assert_eq!(runs, ["run: calc.exe | "], "the program");

    slide(&mut touch, p, on_item(p, 4));
    let r = left_panel(&app);
    assert_eq!((r.bottom - r.top) / key_px(), 2, "the panel switched to the two keys of [set.other]");
    assert!(pie_window(&app).is_none());
}

// AC2
#[test]
fn letting_go_in_the_middle_or_toward_the_edge_does_nothing_and_closes() {
    let _desktop = desktop();
    let (app, p) = launch_pad();
    let k = key_px() as f64;
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, p);
    touch.move_to(1, on_item(p, 0));
    touch.move_to(1, toward(p, 0.0, k / 4.0));
    touch.up(1);
    assert!(pie_window(&app).is_none(), "back in the middle: closed");

    slide(&mut touch, p, POINT { x: p.x - (k / 4.0) as i32, y: p.y - k as i32 });
    assert!(pie_window(&app).is_none(), "toward the edge: closed");
    assert_eq!(log.keys(), [], "nothing sent either way");
}

/// A topmost backdrop down the left of the primary monitor, six keys wide,
/// for taps outside the menu to land on. Made before EdgePad starts, so the
/// panel and the menu go above it.
fn backdrop() -> TestWindow {
    let m = primary_monitor();
    let under = RECT { left: m.left, top: m.top, right: m.left + key_px() * 6, bottom: m.bottom };
    test_window("EdgePadTestPieOpsBackdrop", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, under)
}

// AC3
#[test]
fn a_tap_leaves_the_menu_open_for_an_item_to_be_tapped() {
    let _desktop = desktop();
    let backdrop = backdrop();
    let (app, p) = launch_pad();
    let k = key_px() as f64;
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, 1, p);
    assert!(pie_window(&app).is_some(), "a tap leaves it open");
    tap(&mut touch, 2, on_item(p, 0));
    assert_eq!(log.keys(), [d(B), u(B)], "tapping an item runs it");
    assert!(pie_window(&app).is_none(), "and closes the menu");

    tap(&mut touch, 1, p);
    tap(&mut touch, 2, p);
    assert!(pie_window(&app).is_none(), "tapping the middle, on the pie key, closes it");
    assert_eq!(log.keys().len(), 2, "and runs nothing");

    tap(&mut touch, 1, p);
    let pie = pie_window(&app).expect("open again");
    let gap = toward(p, -90.0 + 180.0 / ITEMS as f64, 2.0 * k);
    let outside = toward(p, angle(2), 4.0 * k);
    for (spot, name) in [(gap, "the gap between items"), (outside, "outside the ring")] {
        let under = unsafe { WindowFromPoint(spot) };
        assert_ne!(under, pie, "{name} is not the menu's");
        assert_eq!(under, backdrop.0, "{name} is the window under it");
    }
    tap(&mut touch, 2, outside);
    assert!(pie_window(&app).is_some(), "a tap outside leaves the menu open");
    assert_eq!(log.keys().len(), 2, "and runs nothing");
    tap(&mut touch, 2, p);
}

// AC4
#[test]
fn a_latched_modifier_combines_with_the_item_and_a_one_shot_latch_ends() {
    let _desktop = desktop();
    let (app, p) = launch_pad();
    let shift = key_centre(left_panel(&app), SHIFT_KEY);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, 1, shift);
    slide(&mut touch, p, on_item(p, 0));
    assert_eq!(log.keys(), [d(VK_LSHIFT), d(B), u(B), u(VK_LSHIFT)], "Shift+B, then the latch ends");

    tap(&mut touch, 1, shift);
    slide(&mut touch, p, on_item(p, 2));
    log.wait_for(10);
    let keys = log.keys()[4..].to_vec();
    assert_eq!(keys[..2], [d(VK_LSHIFT), u(VK_LSHIFT)], "Shift is let go before the text: {keys:?}");
    assert!(keys[2..].iter().all(|&(vk, _)| vk == VK_PACKET), "then the text alone: {keys:?}");
}

// AC5
#[test]
fn a_flick_toward_the_edge_on_the_pie_key_neither_hides_the_panel_nor_closes_the_menu() {
    let _desktop = desktop();
    let (app, p) = launch_pad();
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, p);
    // Quick, and far enough that it would hide the panel from any other key.
    touch.move_to(1, POINT { x: p.x - key_px() / 3, y: p.y });
    assert_eq!(panels_now(&app).len(), 1, "the panel stays");
    assert!(pie_window(&app).is_some(), "the menu stays open");
    touch.up(1);
    assert_eq!(log.keys(), [], "let go toward the edge: nothing");
}

// AC6
#[test]
fn a_pen_neither_opens_the_menu_nor_runs_an_item() {
    let _desktop = desktop();
    let (app, p) = launch_pad();
    let log = KeyLog::start();
    let mut touch = Touch::new();

    pen_tap(p);
    assert!(pie_window(&app).is_none(), "a pen does not open it");

    tap(&mut touch, 1, p);
    assert!(pie_window(&app).is_some(), "precondition: open after a finger's tap");
    pen_tap(on_item(p, 0));
    assert_eq!(log.keys(), [], "a pen runs no item");
    assert!(pie_window(&app).is_some(), "and the menu stays open");
    tap(&mut touch, 1, p);
}

//! Acceptance tests for Issue #30: the handle left on a hidden edge, and the
//! swipes and taps that show and hide panels.

mod common;

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const HANDLE_CLASS: &str = "EdgePadHandle";

/// Every visible handle window of `app`, with its rectangle.
fn handles(app: &Running) -> Vec<(HWND, RECT)> {
    let name = wide(HANDLE_CLASS);
    let mut found = Vec::new();
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
        if hwnd.is_null() {
            return found;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 {
            found.push((hwnd, window_rect(hwnd)));
        }
    }
}

/// The visible handle against `edge` ("left" or "right") of the primary monitor.
fn handle_on(app: &Running, edge: &str) -> Option<RECT> {
    let m = primary_monitor();
    handles(app).into_iter().map(|(_, r)| r).find(|r| match edge {
        "left" => r.left == m.left,
        _ => r.right == m.right,
    })
}

/// Wait up to a second for the handle on `edge`: a second launch is handled
/// by the running EdgePad in its own time.
fn wait_handle(app: &Running, edge: &str) -> Option<RECT> {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        let found = handle_on(app, edge);
        if found.is_some() || std::time::Instant::now() >= end {
            return found;
        }
        pump(20);
    }
}

/// Which sides have a visible panel, as "left" / "right".
fn shown(app: &Running) -> Vec<&'static str> {
    let m = primary_monitor();
    let mut edges: Vec<&str> = panels_now(app)
        .iter()
        .map(|r| if r.left == m.left { "left" } else if r.right == m.right { "right" } else { "elsewhere" })
        .collect();
    edges.sort();
    edges
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

const A: u16 = b'A' as u16;

// AC1
#[test]
fn a_hidden_edge_shows_a_handle_sized_by_its_settings() {
    let _desktop = desktop();
    let text = "\
[edge.right]
handle = 8, 20%, 25%
[set.pad]
key = A | hold | A
[profile.default]
left = pad
right = pad
";
    let app = launch_with(Some(text), None);
    find_panel(&app);
    assert!(handles(&app).is_empty(), "no handle while every panel is shown");

    choose(&app, "左辺");
    let m = primary_monitor();
    let h = m.bottom - m.top;
    let left = handle_on(&app, "left").expect("a handle on the hidden left edge");
    assert_eq!(left.right - left.left, px(6), "default width 6");
    assert!(((left.bottom - left.top) - h * 40 / 100).abs() <= 1, "default length 40%");
    assert!(((left.top + left.bottom) / 2 - (m.top + h / 2)).abs() <= 1, "centred by default");
    let (hwnd, _) = handles(&app)[0];
    let (mut alpha, mut flags) = (0u8, 0u32);
    unsafe { GetLayeredWindowAttributes(hwnd, null_mut(), &mut alpha, &mut flags) };
    assert!((76..=77).contains(&alpha), "30% opaque, alpha {alpha}");

    assert_eq!(launch_again(&app).code(), Some(0), "a second launch hides the right edge too");
    let right = wait_handle(&app, "right").expect("a handle on the hidden right edge");
    assert_eq!(right.right - right.left, px(8), "width 8");
    assert!(((right.bottom - right.top) - h * 20 / 100).abs() <= 1, "length 20%");
    assert!(((right.top + right.bottom) / 2 - (m.top + h * 25 / 100)).abs() <= 1, "centred at 25%");
    assert_eq!(handles(&app).len(), 2, "no handle on edges without a panel");

    choose(&app, "一時停止");
    assert!(handles(&app).is_empty(), "no handles while paused");
    choose(&app, "一時停止");
    assert_eq!(handles(&app).len(), 2, "back after the pause");

    choose(&app, "左辺");
    assert_eq!(shown(&app), ["left"], "the left panel is shown");
    assert!(handle_on(&app, "left").is_none(), "and its handle is gone");
}

// AC2
#[test]
fn swiping_inward_from_the_handle_shows_the_panel() {
    let _desktop = desktop();
    let app = launch_with(Some("[set.pad]\nkey = A | hold | A\n[profile.default]\nleft = pad\n"), None);
    find_panel(&app);
    choose(&app, "左辺");
    let handle = handle_on(&app, "left").expect("handle");

    let start = centre(handle);
    let mut touch = Touch::new();
    touch.down(1, start);
    touch.move_to(1, POINT { x: start.x + key_px() / 2, y: start.y });
    touch.move_to(1, POINT { x: start.x + key_px(), y: start.y });
    touch.up(1);
    pump(200);
    assert_eq!(shown(&app), ["left"], "the swipe shows the panel");
    assert!(handle_on(&app, "left").is_none(), "the handle is gone");
}

// AC3
#[test]
fn tapping_or_clicking_the_handle_shows_the_panel() {
    let _desktop = desktop();
    let app = launch_with(Some("[set.pad]\nkey = A | hold | A\n[profile.default]\nleft = pad\n"), None);
    find_panel(&app);

    choose(&app, "左辺");
    let mut touch = Touch::new();
    touch.down(1, centre(handle_on(&app, "left").expect("handle")));
    touch.up(1);
    pump(200);
    assert_eq!(shown(&app), ["left"], "a tap shows the panel");
    drop(touch);

    choose(&app, "左辺");
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    click(centre(handle_on(&app, "left").expect("handle")));
    unsafe { SetCursorPos(saved.x, saved.y) };
    pump(200);
    assert_eq!(shown(&app), ["left"], "a click shows the panel");
}

// AC4
#[test]
fn swiping_toward_the_edge_hides_the_panel_releasing_hold_keys_and_dropping_shortcuts() {
    let _desktop = desktop();
    // Two columns: keys 2 and 3 sit in the inner column, a key away from the
    // screen edge, so a swipe toward the edge stays on screen.
    let text = "\
[edge.left]
columns = 2
[set.pad]
key = X | hold     | X
key = Y | hold     | Y
key = A | hold     | A
key = S | shortcut | Ctrl+S
[profile.default]
left = pad
";
    let app = launch_with(Some(text), None);
    let panel = left_panel(&app);
    let key = key_px();
    let log = KeyLog::start();
    let mut touch = Touch::new();

    let start = cell(panel, key, 1, 0);
    touch.down(1, start);
    touch.move_to(1, POINT { x: start.x - key / 2, y: start.y });
    touch.move_to(1, POINT { x: start.x - key, y: start.y });
    pump(200);
    assert!(shown(&app).is_empty(), "the swipe hides the panel");
    assert!(handle_on(&app, "left").is_some(), "and leaves a handle");
    assert_eq!(log.keys(), [(A, false), (A, true)], "the hold key is pressed, then released as it hides");
    touch.up(1);
    pump(200);
    assert_eq!(log.keys().len(), 2, "lifting afterwards sends nothing");

    choose(&app, "左辺");
    let panel = left_panel(&app);
    let start = cell(panel, key, 1, 1);
    touch.down(1, start);
    touch.move_to(1, POINT { x: start.x - key / 2, y: start.y });
    touch.move_to(1, POINT { x: start.x - key, y: start.y });
    touch.up(1);
    pump(200);
    assert!(shown(&app).is_empty(), "hidden again");
    assert_eq!(log.keys().len(), 2, "the shortcut sends nothing");
}

// AC5
#[test]
fn handle_none_leaves_that_edge_without_a_handle() {
    let _desktop = desktop();
    let text = "\
[edge.left]
handle = none
[set.pad]
key = A | hold | A
[profile.default]
left = pad
right = pad
";
    let app = launch_with(Some(text), None);
    find_panel(&app);
    pump(300);
    assert!(test_events(&app).iter().all(|e| !e.starts_with("notice: ")), "handle = none is accepted: {:?}", test_events(&app));

    assert_eq!(launch_again(&app).code(), Some(0));
    assert!(wait_handle(&app, "right").is_some(), "the right edge still has one");
    assert!(shown(&app).is_empty(), "both edges hidden");
    assert!(handle_on(&app, "left").is_none(), "no handle on the left");
}

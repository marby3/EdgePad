//! Acceptance tests for Issue #54: the dimmed panel a short swipe from the
//! handle shows, and the ways back to the normal panel.
//!
//! Opacity is read off the screen: a topmost backdrop made before EdgePad
//! sits right under the panel, and the panel's corner is captured once over
//! white and once over black. How far the two differ is how much of the
//! backdrop shows through.

mod common;

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const A: u16 = b'A' as u16;
const B: u16 = b'B' as u16;

/// Two keys side by side on the left, at 70% shown and 40% dimmed: neither
/// is a default, so the panel can only be following the settings.
const SETTINGS: &str = "\
[general]
opacity     = 70
dim_opacity = 40
hotkey      = Ctrl+Alt+F16
[edge.left]
columns = 2
[set.pad]
key = A | hold | A
key = B | hold | B
[profile.default]
left = pad
";

/// A topmost backdrop down the left of the primary monitor, three keys wide.
/// Made before EdgePad starts, so the panel and handle go above it.
fn backdrop() -> TestWindow {
    let m = primary_monitor();
    let under = RECT { left: m.left, top: m.top, right: m.left + key_px() * 3, bottom: m.bottom };
    test_window("EdgePadTestDimBackdrop", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, under)
}

/// The panel's opacity in percent, from how much of the backdrop shows
/// through its top-left corner.
fn opacity(backdrop: &TestWindow, panel: RECT) -> i32 {
    let corner = RECT { left: panel.left, top: panel.top, right: panel.left + 3, bottom: panel.top + 3 };
    let average = |pixels: Vec<u32>| {
        let n = pixels.len() as i32;
        let sum = pixels.iter().map(|&p| channels(p)).fold([0; 3], |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2]]);
        sum.map(|v| v / n)
    };
    set_backdrop(backdrop.0, 0x00FF_FFFF);
    let on_white = average(capture(corner));
    set_backdrop(backdrop.0, 0x0000_0000);
    let on_black = average(capture(corner));
    let through = (0..3).map(|c| on_white[c] - on_black[c]).sum::<i32>() / 3;
    100 - through * 100 / 255
}

/// Whether `actual` is `want` percent, give or take rounding on the way.
fn near(actual: i32, want: i32) -> bool {
    (actual - want).abs() <= 4
}

/// The visible handle against the left edge of the primary monitor, once it
/// can be touched (see `find_panel`).
fn left_handle(app: &Running) -> RECT {
    let m = primary_monitor();
    let name = wide("EdgePadHandle");
    let end = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        let mut hwnd: HWND = null_mut();
        loop {
            hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
            if hwnd.is_null() {
                break;
            }
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            let r = window_rect(hwnd);
            if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 && r.left == m.left {
                assert_eq!(unsafe { DwmFlush() }, 0, "DwmFlush failed");
                return r;
            }
        }
        assert!(std::time::Instant::now() < end, "no handle on the left edge");
        pump(20);
    }
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

/// Drag the mouse inward from the left handle, stopping at each distance in
/// turn; `at_each` runs at every stop with the button still down. Dragged
/// with the mouse: on CI a touch moving inward from the screen edge never
/// reaches the handle (see the handle tests).
fn drag_from_handle(app: &Running, distances: &[i32], mut at_each: impl FnMut(i32)) {
    let start = centre(left_handle(app));
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(start);
    mouse_down_at(start);
    let mut last = start;
    for &d in distances {
        last = POINT { x: start.x + d, y: start.y };
        mouse_move_to(last);
        pump(100);
        at_each(d);
    }
    mouse_up_at(last);
    unsafe { SetCursorPos(saved.x, saved.y) };
    pump(200);
}

/// Hide the left panel from the tray, then show it dimmed with a short
/// swipe from the handle.
fn show_dimmed(app: &Running, backdrop: &TestWindow) {
    choose(app, "左辺");
    assert!(panels_now(app).is_empty(), "precondition: the panel is hidden");
    drag_from_handle(app, &[key_px() * 3 / 4], |_| {});
    let o = opacity(backdrop, left_panel(app));
    assert!(near(o, 40), "precondition: a short swipe shows the panel dimmed, at {o}%");
}

// AC1
#[test]
fn a_short_swipe_from_the_handle_shows_the_panel_dimmed() {
    let _desktop = desktop();
    let backdrop = backdrop();
    let app = launch_with(Some(SETTINGS), None);
    find_panel(&app);
    choose(&app, "左辺");

    let mut during = None;
    drag_from_handle(&app, &[key_px() * 3 / 4], |_| {
        let panel = panels_now(&app).first().copied();
        during = panel.map(|p| opacity(&backdrop, p));
    });
    let during = during.expect("half a key in, the panel is shown before the swipe ends");
    assert!(near(during, 40), "shown at dim_opacity 40%, not {during}%");

    let after = opacity(&backdrop, left_panel(&app));
    assert!(near(after, 40), "lifted there, the panel stays dimmed, not {after}%");
}

// AC2
#[test]
fn carrying_the_swipe_on_to_two_keys_makes_the_panel_normal() {
    let _desktop = desktop();
    let backdrop = backdrop();
    let app = launch_with(Some(SETTINGS), None);
    find_panel(&app);
    choose(&app, "左辺");

    let key = key_px();
    let mut seen = Vec::new();
    // Just past two keys: any shorter long swipe would already have turned
    // it normal at the first stop, any longer one not yet at the second.
    drag_from_handle(&app, &[key * 3 / 4, key * 2 + 2], |d| {
        let panel = panels_now(&app).first().copied().expect("the panel is shown during the swipe");
        seen.push((d, opacity(&backdrop, panel)));
    });
    assert!(near(seen[0].1, 40), "half a key in: dimmed, got {seen:?}");
    assert!(near(seen[1].1, 70), "two keys in, before lifting: at opacity 70%, got {seen:?}");

    let after = opacity(&backdrop, left_panel(&app));
    assert!(near(after, 70), "and it stays at 70% once lifted, not {after}%");
}

// AC3
#[test]
fn a_quick_inward_flick_on_the_dimmed_panel_makes_it_normal_and_lets_go_of_the_key() {
    let _desktop = desktop();
    let backdrop = backdrop();
    let app = launch_with(Some(SETTINGS), None);
    find_panel(&app);
    show_dimmed(&app, &backdrop);

    let panel = left_panel(&app);
    let key = key_px();
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let start = cell(panel, key, 0, 0);
    touch.down(1, start);
    touch.move_to(1, POINT { x: start.x + key, y: start.y });
    pump(200);
    assert_eq!(log.keys(), [(A, false), (A, true)], "A is pressed, then let go as the flick lands");
    let o = opacity(&backdrop, left_panel(&app));
    assert!(near(o, 70), "the flick makes the panel normal, at 70%, not {o}%");

    touch.up(1);
    pump(200);
    assert_eq!(log.keys().len(), 2, "lifting afterwards sends nothing");
}

// AC4
#[test]
fn the_dimmed_panel_takes_fingers_ignores_pens_and_hides_with_a_flick_to_the_edge() {
    let _desktop = desktop();
    let backdrop = backdrop();
    let app = launch_with(Some(SETTINGS), None);
    find_panel(&app);
    show_dimmed(&app, &backdrop);

    let panel = left_panel(&app);
    let key = key_px();
    let b = cell(panel, key, 1, 0);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, b);
    touch.hold(100);
    touch.up(1);
    assert_eq!(log.keys(), [(B, false), (B, true)], "a finger presses B");
    let o = opacity(&backdrop, left_panel(&app));
    assert!(near(o, 40), "and the panel is still dimmed, at {o}%");

    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(b);
    mouse_down_at(b);
    mouse_up_at(b);
    unsafe { SetCursorPos(saved.x, saved.y) };
    pump(200);
    assert_eq!(log.keys()[2..], [(B, false), (B, true)], "so does the mouse");

    pen_tap(b);
    pump(200);
    assert_eq!(log.keys().len(), 4, "a pen presses nothing");
    let o = opacity(&backdrop, left_panel(&app));
    assert!(near(o, 40), "and the panel is still dimmed, at {o}%");

    touch.down(1, b);
    touch.move_to(1, POINT { x: b.x - key, y: b.y });
    pump(200);
    assert!(panels_now(&app).is_empty(), "a quick flick toward the edge hides the dimmed panel");
    touch.up(1);
}

// AC5
#[test]
fn shown_again_by_tap_click_hotkey_or_tray_the_panel_is_normal() {
    let _desktop = desktop();
    let backdrop = backdrop();
    let app = launch_with(Some(SETTINGS), None);
    find_panel(&app);

    let hide_from_tray = |app: &Running| {
        choose(app, "左辺");
        assert!(panels_now(app).is_empty(), "precondition: hidden from the tray");
    };
    let ways: [(&str, &dyn Fn(&Running)); 4] = [
        ("a tap on the handle", &|app| {
            let mut touch = Touch::new();
            touch.down(1, centre(left_handle(app)));
            touch.up(1);
            pump(200);
        }),
        ("a click on the handle", &|app| {
            let mut saved = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut saved) };
            click(centre(left_handle(app)));
            unsafe { SetCursorPos(saved.x, saved.y) };
            pump(200);
        }),
        ("the hotkey", &|_| {
            for (vk, down) in [(VK_LCONTROL, true), (VK_LMENU, true), (VK_F16, true), (VK_F16, false), (VK_LMENU, false), (VK_LCONTROL, false)] {
                keyboard(vk, down);
            }
            pump(300);
        }),
        ("the tray", &|app| choose(app, "左辺")),
    ];
    for (way, show) in ways {
        show_dimmed(&app, &backdrop);
        hide_from_tray(&app);
        show(&app);
        let o = opacity(&backdrop, left_panel(&app));
        assert!(near(o, 70), "shown again by {way}, the panel is at 70%, not {o}%");
    }
}

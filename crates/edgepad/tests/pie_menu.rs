//! Acceptance tests for Issue #70: the pie menu on screen while a pie key is
//! held (spec 4.6, 5.3 and 5.6). Its items are rounded buttons on an arc
//! around where the key was pressed, and the one pointed at is lit.
//!
//! Colours and opacity are read off the screen. Which item is pointed at is
//! also in the test events (`pie: point <item>` / `pie: point none`), for
//! items off the screen.

mod common;

use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::System::Threading::{GR_GDIOBJECTS, GetGuiResources};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const PIE_CLASS: &str = "EdgePadPie";

/// A left panel of a pie key over `items` items labelled `label`, then A.
fn settings(general: &str, items: usize, label: &str) -> String {
    let mut text = format!("[general]\n{general}\n[set.pad]\nkey = 変形 | pie | menu\nkey = A | shortcut | A\n[set.menu]\n");
    for i in 0..items {
        text += &format!("key = {label} | shortcut | F{}\n", i + 1);
    }
    text + "[profile.default]\nleft = pad\n"
}

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

/// Item `i` of `n` in the half circle, in degrees: -90 is along the edge
/// toward its start (up on the left edge), +90 toward its end.
fn angle(i: usize, n: usize) -> f64 {
    -90.0 + (i as f64 + 0.5) * 180.0 / n as f64
}

/// The radius of the arc: the least at which no two neighbouring buttons,
/// squares a key across, overlap, and at least one and a half keys.
fn radius(n: usize, key: f64) -> f64 {
    let at = |i| angle(i, n).to_radians();
    let mut r = 1.5 * key;
    for i in 0..n - 1 {
        let (a, b) = (at(i), at(i + 1));
        let apart = (a.cos() - b.cos()).abs().max((a.sin() - b.sin()).abs());
        r = r.max(key / apart);
    }
    r
}

/// The middle of button `i` of `n` on a pie opened at `p` on the left edge.
fn button(p: POINT, i: usize, n: usize) -> (f64, f64) {
    let key = key_px() as f64;
    let (a, r) = (angle(i, n).to_radians(), radius(n, key));
    (p.x as f64 + r * a.cos(), p.y as f64 + r * a.sin())
}

/// A point inside button `i`, above its label.
fn on_button(p: POINT, i: usize, n: usize) -> POINT {
    let (x, y) = button(p, i, n);
    POINT { x: x.round() as i32, y: (y - key_px() as f64 / 3.0).round() as i32 }
}

fn colour_at(p: POINT) -> [i32; 3] {
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

fn near(c: [i32; 3], want: [i32; 3]) -> bool {
    (0..3).all(|i| (c[i] - want[i]).abs() <= 12)
}

/// Dark theme: key faces 0x3A3A3A.
const FACE: [i32; 3] = [0x3A, 0x3A, 0x3A];
const RED: [i32; 3] = [0xFF, 0, 0];

/// Where a finger moved `d` pixels from `p` toward `degrees` (edge frame of
/// the left edge: 0 is right, -90 up) ends.
fn toward(p: POINT, degrees: f64, d: f64) -> POINT {
    let a = degrees.to_radians();
    POINT { x: p.x + (d * a.cos()).round() as i32, y: p.y + (d * a.sin()).round() as i32 }
}

fn points(app: &Running) -> Vec<String> {
    test_events(app).into_iter().filter(|e| e.starts_with("pie: point")).collect()
}

fn gdi_objects(app: &Running) -> u32 {
    unsafe { GetGuiResources(app.0.as_raw_handle(), GR_GDIOBJECTS) }
}

// AC1
#[test]
fn a_pie_key_opens_a_layered_menu_that_takes_no_focus_and_goes_away_on_release() {
    let _desktop = desktop();
    let m = primary_monitor();
    let front = test_window("EdgePadTestPieFront", 0, RECT { left: m.right - 300, top: m.top + 100, right: m.right - 100, bottom: m.top + 300 });
    let app = launch_with(Some(&settings("", 4, "W")), None);
    let panel = left_panel(&app);
    wait_in_front(front.0);
    assert!(pie_window(&app).is_none(), "no menu before the pie key is pressed");
    let gdi_before = gdi_objects(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, key_centre(panel, 0));
    let pie = pie_window(&app).expect("the menu opened on the press");
    assert!(unsafe { IsWindowVisible(pie) } != 0, "and is shown");
    let ex = unsafe { GetWindowLongW(pie, GWL_EXSTYLE) } as u32;
    for (flag, name) in [(WS_EX_NOACTIVATE, "NOACTIVATE"), (WS_EX_TOPMOST, "TOPMOST"), (WS_EX_TOOLWINDOW, "TOOLWINDOW"), (WS_EX_LAYERED, "LAYERED")] {
        assert!(ex & flag != 0, "the menu is {name}");
    }
    assert_eq!(unsafe { GetForegroundWindow() }, front.0, "the app in front keeps the focus");

    touch.up(1);
    assert!(pie_window(&app).is_none(), "released, the menu window is gone");
    assert_eq!(gdi_objects(&app), gdi_before, "and nothing of its drawing is kept");
    assert_eq!(log.keys(), [], "letting go sends nothing yet");
}

// AC2
#[test]
fn the_items_are_buttons_on_an_arc_around_the_press_each_toward_its_own_direction() {
    let _desktop = desktop();
    for n in [2, 4, 8] {
        let app = launch_with(Some(&settings("opacity = 100\ntheme = dark\naccent = #FF0000", n, "W")), None);
        let panel = left_panel(&app);
        let mut touch = Touch::new();
        let p = key_centre(panel, 0);
        touch.down(1, p);
        for i in 0..n {
            let c = colour_at(on_button(p, i, n));
            assert!(near(c, FACE), "{n} items: button {i} toward {}°, {:.0} px out, is a key face, got {c:?}", angle(i, n), radius(n, key_px() as f64));
            // Its label: ink in the middle of the button.
            let (x, y) = button(p, i, n);
            let k = key_px() / 4;
            let (x, y) = (x.round() as i32, y.round() as i32);
            let middle = capture(RECT { left: x - k, top: y - k, right: x + k, bottom: y + k });
            assert!(middle.iter().any(|&px| !near(channels(px), FACE)), "{n} items: button {i} has its label drawn");
        }
        touch.up(1);
    }
}

// AC3
#[test]
fn the_item_pointed_at_is_lit_like_a_held_key_and_follows_the_finger() {
    let _desktop = desktop();
    let n = 4;
    let app = launch_with(Some(&settings("opacity = 100\ntheme = dark\naccent = #FF0000", n, ".")), None);
    let panel = left_panel(&app);
    let mut touch = Touch::new();
    let p = key_centre(panel, 0);
    let k = key_px() as f64;
    let lit = || (0..n).filter(|&i| near(colour_at(on_button(p, i, n)), RED)).collect::<Vec<_>>();

    touch.down(1, p);
    assert_eq!(lit(), [], "nothing lit at first: the finger is in the middle");
    touch.move_to(1, toward(p, angle(2, n), 2.0 * k));
    assert_eq!(lit(), [2], "toward item 2");
    touch.move_to(1, toward(p, angle(0, n), 2.0 * k));
    assert_eq!(lit(), [0], "the light follows to item 0");
    touch.move_to(1, toward(p, angle(1, n), k / 4.0));
    assert_eq!(lit(), [], "back in the middle");
    touch.move_to(1, POINT { x: p.x - (k / 4.0) as i32, y: p.y - k as i32 });
    assert_eq!(lit(), [], "on the edge's side");
    touch.up(1);
}

/// A topmost backdrop down the left of the primary monitor, five keys wide.
/// Made before EdgePad starts, so the panel and the menu go above it.
fn backdrop() -> TestWindow {
    let m = primary_monitor();
    let under = RECT { left: m.left, top: m.top, right: m.left + key_px() * 5, bottom: m.bottom };
    test_window("EdgePadTestPieBackdrop", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, under)
}

/// The opacity in percent around `at`, from how much of the backdrop shows.
fn opacity(backdrop: &TestWindow, at: POINT) -> i32 {
    let spot = RECT { left: at.x - 1, top: at.y - 1, right: at.x + 2, bottom: at.y + 2 };
    let average = |pixels: Vec<u32>| {
        let n = pixels.len() as i32;
        let sum = pixels.iter().map(|&p| channels(p)).fold([0; 3], |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2]]);
        sum.map(|v| v / n)
    };
    set_backdrop(backdrop.0, 0x00FF_FFFF);
    let on_white = average(capture(spot));
    set_backdrop(backdrop.0, 0x0000_0000);
    let on_black = average(capture(spot));
    let through = (0..3).map(|c| on_white[c] - on_black[c]).sum::<i32>() / 3;
    100 - through * 100 / 255
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

/// Hide the left panel from the tray, then show it dimmed with a short mouse
/// swipe from the handle (on CI a touch from the screen edge never reaches
/// the handle; see the handle tests).
fn show_dimmed(app: &Running) {
    choose(app, "左辺");
    assert!(panels_now(app).is_empty(), "precondition: the panel is hidden");
    let start = centre(left_handle(app));
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(start);
    mouse_down_at(start);
    let end = POINT { x: start.x + key_px() * 3 / 4, y: start.y };
    mouse_move_to(end);
    pump(100);
    mouse_up_at(end);
    unsafe { SetCursorPos(saved.x, saved.y) };
    pump(200);
}

// AC4
#[test]
fn the_menu_takes_the_panels_colours_and_its_opacity_dimmed_or_not() {
    let _desktop = desktop();
    let n = 2;
    let k = key_px() as f64;
    {
        let app = launch_with(Some(&settings("opacity = 100\ntheme = light\naccent = #00FF00", n, ".")), None);
        let p = key_centre(left_panel(&app), 0);
        let mut touch = Touch::new();
        touch.down(1, p);
        touch.move_to(1, toward(p, angle(1, n), 2.0 * k));
        let (face, lit) = (colour_at(on_button(p, 0, n)), colour_at(on_button(p, 1, n)));
        assert!(near(face, [0xFA, 0xFA, 0xFA]), "a light key face, got {face:?}");
        assert!(near(lit, [0, 0xFF, 0]), "the accent where it points, got {lit:?}");
        touch.up(1);
    }

    let backdrop = backdrop();
    let app = launch_with(Some(&settings("opacity = 70\ndim_opacity = 40", n, ".")), None);
    let mut touch = Touch::new();
    let p = key_centre(left_panel(&app), 0);
    touch.down(1, p);
    let o = opacity(&backdrop, on_button(p, 0, n));
    assert!((o - 70).abs() <= 4, "at the panel's opacity, 70%, not {o}%");
    touch.up(1);

    show_dimmed(&app);
    let p = key_centre(left_panel(&app), 0);
    touch.down(1, p);
    let o = opacity(&backdrop, on_button(p, 0, n));
    assert!((o - 40).abs() <= 4, "opened from the dimmed panel: at 40%, not {o}%");
    touch.up(1);
}

// AC5
#[test]
fn a_menu_running_off_the_screen_keeps_its_middle_where_it_was_pressed() {
    let _desktop = desktop();
    let n = 2;
    // Enough keys to fill the left edge: the pie key is at the very top.
    let m = primary_monitor();
    let fit = ((m.bottom - m.top) / key_px()) as usize;
    let mut text = settings("opacity = 100\ntheme = dark\naccent = #FF0000", n, ".");
    let more: String = (1..fit - 1).map(|i| format!("key = {i} | shortcut | F{}\n", (i % 12) + 1)).collect();
    text = text.replace("key = A | shortcut | A\n", &format!("key = A | shortcut | A\n{more}"));
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    let p = key_centre(panel, 0);
    assert!(button(p, 0, n).1 < (m.top + key_px() / 2) as f64, "precondition: item 0 runs off the top of the screen");
    let mut touch = Touch::new();

    touch.down(1, p);
    let c = colour_at(on_button(p, 1, n));
    assert!(near(c, FACE), "item 1 is where it would be, not moved down: got {c:?}");
    // Up and in, toward item 0, staying on the screen.
    touch.move_to(1, toward(p, angle(0, n), 0.6 * key_px() as f64));
    assert_eq!(points(&app).last().map(String::as_str), Some("pie: point 0"), "item 0 is pointed at, off the screen as it is");
    touch.up(1);
}

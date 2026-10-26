//! Acceptance tests for Issue #70: the pie menu on screen while a pie key is
//! held (spec 4.6, 5.3 and 5.6): a half ring around where the key was
//! pressed, split into a sector per item, the one pointed at lit.
//!
//! Colours and opacity are read off the screen. Which item is pointed at is
//! also in the test events (`pie: point <item>` / `pie: point none`), for
//! items off the screen.

mod common;

use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
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

/// The ring's hole: the middle, where letting go cancels.
fn inner(key: f64) -> f64 {
    key / 2.0
}

/// The ring's outer radius: two keys, or more for many items, until the
/// sectors are a key wide where their labels go, half way out.
fn outer(n: usize, key: f64) -> f64 {
    // A sector `key` wide along its arc at radius r: r * pi / n >= key.
    let label = key * n as f64 / std::f64::consts::PI;
    (2.0 * key).max(2.0 * label - inner(key))
}

/// The point `r` out from `p` toward the middle of item `i` of `n`.
fn out(p: POINT, i: usize, n: usize, r: f64) -> POINT {
    let a = angle(i, n).to_radians();
    POINT { x: (p.x as f64 + r * a.cos()).round() as i32, y: (p.y as f64 + r * a.sin()).round() as i32 }
}

/// Where item `i`'s label goes: half way between the hole and the outside.
fn label_at(p: POINT, i: usize, n: usize) -> POINT {
    let k = key_px() as f64;
    out(p, i, n, (inner(k) + outer(n, k)) / 2.0)
}

/// A point inside item `i`'s sector, out past its label.
fn on_item(p: POINT, i: usize, n: usize) -> POINT {
    let k = key_px() as f64;
    out(p, i, n, ((inner(k) + outer(n, k)) / 2.0 + outer(n, k)) / 2.0)
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

/// Wait for the menu of `app` to be on screen, composed: a debug build
/// takes a while to work out a big ring pixel by pixel.
fn shown(app: &Running) {
    let end = Instant::now() + Duration::from_secs(3);
    while !pie_window(app).is_some_and(|w| unsafe { IsWindowVisible(w) } != 0) && Instant::now() < end {
        pump(20);
    }
    assert_eq!(unsafe { DwmFlush() }, 0, "DwmFlush failed");
}

/// Wait for the menu of `app` to point at `want` (a test event line) and be
/// redrawn so on screen.
fn pointed(app: &Running, want: &str) {
    let end = Instant::now() + Duration::from_secs(3);
    let at = |app: &Running| points(app).last().cloned().unwrap_or_else(|| "pie: point none".into());
    while at(app) != want && Instant::now() < end {
        pump(20);
    }
    assert_eq!(unsafe { DwmFlush() }, 0, "DwmFlush failed");
}

/// Let go of the pie key pressed at `p` without running anything: out
/// toward the edge first, where letting go cancels and closes the menu. A
/// finger lifted without leaving the middle would be a tap, which leaves it
/// open.
fn cancel(touch: &mut Touch, p: POINT) {
    let k = key_px();
    touch.move_to(1, POINT { x: p.x - k / 4, y: p.y + k });
    touch.up(1);
}

fn gdi_objects(app: &Running) -> u32 {
    unsafe { GetGuiResources(app.0.as_raw_handle(), GR_GDIOBJECTS) }
}

// AC1
#[test]
fn a_pie_key_opens_a_layered_menu_that_takes_no_focus_and_goes_away_when_let_go() {
    let _desktop = desktop();
    let m = primary_monitor();
    let front = test_window("EdgePadTestPieFront", 0, RECT { left: m.right - 300, top: m.top + 100, right: m.right - 100, bottom: m.top + 300 });
    let app = launch_with(Some(&settings("", 4, "W")), None);
    let panel = left_panel(&app);
    wait_in_front(front.0);
    assert!(pie_window(&app).is_none(), "no menu before the pie key is pressed");
    let log = KeyLog::start();
    let mut touch = Touch::new();
    // Windows sets aside one GDI object for good the first time a menu is
    // drawn: count from after that, so what is left of each menu shows.
    touch.down(1, key_centre(panel, 0));
    cancel(&mut touch, key_centre(panel, 0));
    let gdi_before = gdi_objects(&app);

    touch.down(1, key_centre(panel, 0));
    let pie = pie_window(&app).expect("the menu opened on the press");
    assert!(unsafe { IsWindowVisible(pie) } != 0, "and is shown");
    let ex = unsafe { GetWindowLongW(pie, GWL_EXSTYLE) } as u32;
    for (flag, name) in [(WS_EX_NOACTIVATE, "NOACTIVATE"), (WS_EX_TOPMOST, "TOPMOST"), (WS_EX_TOOLWINDOW, "TOOLWINDOW"), (WS_EX_LAYERED, "LAYERED")] {
        assert!(ex & flag != 0, "the menu is {name}");
    }
    assert_eq!(unsafe { GetForegroundWindow() }, front.0, "the app in front keeps the focus");

    cancel(&mut touch, key_centre(panel, 0));
    assert!(pie_window(&app).is_none(), "released, the menu window is gone");
    assert_eq!(gdi_objects(&app), gdi_before, "and nothing of its drawing is kept");
    assert_eq!(log.keys(), [], "let go toward the edge, nothing is sent");
}

// AC2
#[test]
fn the_menu_is_a_half_ring_of_sectors_one_per_item_with_a_hole_in_the_middle() {
    let _desktop = desktop();
    // White under the menu, so a gap or the outside cannot pass for a face.
    let backdrop = backdrop();
    set_backdrop(backdrop.0, 0x00FF_FFFF);
    for n in [2, 4, 8] {
        let app = launch_with(Some(&settings("opacity = 100\ntheme = dark\naccent = #FF0000", n, "W")), None);
        let panel = left_panel(&app);
        let mut touch = Touch::new();
        let p = key_centre(panel, 0);
        let k = key_px() as f64;
        let (hole, rim) = (inner(k), outer(n, k));
        touch.down(1, p);
        shown(&app);
        for i in 0..n {
            let c = colour_at(on_item(p, i, n));
            assert!(near(c, FACE), "{n} items: sector {i} toward {} degrees is a key face, got {c:?}", angle(i, n));
            // Inward, next to the hole, it is still the sector.
            let c = colour_at(out(p, i, n, hole + 4.0));
            assert!(near(c, FACE), "{n} items: sector {i} reaches in to the hole, got {c:?}");
            let l = label_at(p, i, n);
            let q = key_px() / 4;
            let ink = capture(RECT { left: l.x - q, top: l.y - q, right: l.x + q, bottom: l.y + q });
            assert!(ink.iter().any(|&px| !near(channels(px), FACE)), "{n} items: sector {i} has its label half way out");
            let c = colour_at(out(p, i, n, rim + 4.0));
            assert!(!near(c, FACE), "{n} items: nothing past {rim:.0} px out, got {c:?}");
        }
        // Between neighbours, a gap down the boundary.
        for i in 1..n {
            let between = -90.0 + i as f64 * 180.0 / n as f64;
            let c = colour_at(toward(p, between, (hole + rim) / 2.0 + k / 4.0));
            assert!(!near(c, FACE), "{n} items: a gap between sectors {} and {i}, got {c:?}", i - 1);
        }
        let c = colour_at(toward(p, 0.0, hole / 2.0));
        assert!(!near(c, FACE), "{n} items: the middle is a hole, got {c:?}");
        cancel(&mut touch, p);
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
    let lit = || (0..n).filter(|&i| near(colour_at(on_item(p, i, n)), RED)).collect::<Vec<_>>();

    touch.down(1, p);
    shown(&app);
    assert_eq!(lit(), [], "nothing lit at first: the finger is in the middle");
    touch.move_to(1, toward(p, angle(2, n), 2.0 * k));
    pointed(&app, "pie: point 2");
    assert_eq!(lit(), [2], "toward item 2");
    touch.move_to(1, toward(p, angle(0, n), 2.0 * k));
    pointed(&app, "pie: point 0");
    assert_eq!(lit(), [0], "the light follows to item 0");
    touch.move_to(1, toward(p, angle(1, n), k / 4.0));
    pointed(&app, "pie: point none");
    assert_eq!(lit(), [], "back in the middle");
    touch.move_to(1, POINT { x: p.x - (k / 4.0) as i32, y: p.y - k as i32 });
    pointed(&app, "pie: point none");
    assert_eq!(lit(), [], "on the edge's side");
    touch.up(1);
}

/// A topmost backdrop down the left of the primary monitor, wide enough for
/// a menu of eight items. Made before EdgePad starts, so the panel and the
/// menu go above it.
fn backdrop() -> TestWindow {
    let m = primary_monitor();
    let under = RECT { left: m.left, top: m.top, right: m.left + key_px() * 7, bottom: m.bottom };
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

// AC4
#[test]
fn the_menu_takes_the_panels_colours_and_their_opacity() {
    let _desktop = desktop();
    let n = 2;
    let k = key_px() as f64;
    {
        let app = launch_with(Some(&settings("opacity = 100\ntheme = light\naccent = #00FF00", n, ".")), None);
        let p = key_centre(left_panel(&app), 0);
        let log = KeyLog::start();
        let mut touch = Touch::new();
        touch.down(1, p);
        shown(&app);
        touch.move_to(1, toward(p, angle(1, n), 2.0 * k));
        pointed(&app, "pie: point 1");
        let (face, lit) = (colour_at(on_item(p, 0, n)), colour_at(on_item(p, 1, n)));
        assert!(near(face, [0xFA, 0xFA, 0xFA]), "a light key face, got {face:?}");
        assert!(near(lit, [0, 0xFF, 0]), "the accent where it points, got {lit:?}");
        cancel(&mut touch, p);
        assert_eq!(log.keys(), [], "let go toward the edge, nothing is sent");
    }

    let backdrop = backdrop();
    let app = launch_with(Some(&settings("opacity = 70\ndim_opacity = 40", n, ".")), None);
    let mut touch = Touch::new();
    let p = key_centre(left_panel(&app), 0);
    touch.down(1, p);
    shown(&app);
    let o = opacity(&backdrop, on_item(p, 0, n));
    assert!((o - 70).abs() <= 4, "at the panel's opacity, 70%, not {o}%");
    // The gap between the sectors and the outside show what is under.
    let (hole, rim) = (inner(k), outer(n, k));
    let o = opacity(&backdrop, toward(p, 0.0, (hole + rim) / 2.0 + k / 4.0));
    assert!(o <= 4, "the gap between sectors is see-through, not {o}% opaque");
    let o = opacity(&backdrop, out(p, 0, n, rim + 6.0));
    assert!(o <= 4, "outside the ring is see-through, not {o}% opaque");
    cancel(&mut touch, p);
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
    assert!(label_at(p, 0, n).y < m.top, "precondition: item 0 runs off the top of the screen");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, p);
    shown(&app);
    let c = colour_at(on_item(p, 1, n));
    assert!(near(c, FACE), "item 1 is where it would be, not moved down: got {c:?}");
    // The menu runs off the top as far as its ring reaches above the press.
    let pie = pie_window(&app).expect("the menu is open");
    let r = window_rect(pie);
    assert!(r.top <= p.y - outer(n, key_px() as f64) as i32, "not moved onto the screen: {} .. {}", r.top, r.bottom);
    // Up and in, toward item 0, staying on the screen.
    touch.move_to(1, toward(p, angle(0, n), 0.6 * key_px() as f64));
    pointed(&app, "pie: point 0");
    assert_eq!(points(&app).last().map(String::as_str), Some("pie: point 0"), "item 0 is pointed at, off the screen as it is");
    cancel(&mut touch, p);
    assert_eq!(log.keys(), [], "let go toward the edge, nothing is sent");
}

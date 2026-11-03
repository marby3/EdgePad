//! Acceptance tests for Issue #159: a key's flicks (#157) on a panel. A key
//! pressed and slid half a key or more one way does that way's action as it
//! lifts; a small cross around the key shows its ways once the finger has
//! started to slide.
//!
//! Which way the guide points at is also in the test events
//! (`flick: point <way>` / `flick: point none`).

mod common;

use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::UI::Controls::{CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, POINTER_FEEDBACK_NONE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_LCONTROL;
use windows_sys::Win32::UI::Input::Pointer::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const GUIDE_CLASS: &str = "EdgePadFlick";

const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

const fn vk(c: u8) -> u16 {
    c as u16
}

/// The left panel of `app`, launched with `general` in [general] and `keys`
/// as its only set.
fn launch_keys(general: &str, keys: &str) -> (Running, RECT) {
    let text = format!("[general]\n{general}\n[set.pad]\n{keys}[profile.default]\nleft = pad\n");
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    (app, panel)
}

/// `p` moved `dx`, `dy` keys.
fn by(p: POINT, dx: f64, dy: f64) -> POINT {
    let k = key_px() as f64;
    POINT { x: p.x + (dx * k).round() as i32, y: p.y + (dy * k).round() as i32 }
}

/// A finger down at `from`, slid to `to`, lifted.
fn slide(touch: &mut Touch, from: POINT, to: POINT) {
    touch.down(1, from);
    touch.move_to(1, to);
    touch.up(1);
}

/// The guide window of `app` on screen now, if any.
fn guide(app: &Running) -> Option<HWND> {
    let name = wide(GUIDE_CLASS);
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 {
            return Some(hwnd);
        }
    }
}

/// Wait up to a second for the guide to be `on` screen or off it.
fn guide_shown(app: &Running, on: bool) -> bool {
    let end = Instant::now() + Duration::from_secs(1);
    while guide(app).is_some() != on && Instant::now() < end {
        pump(20);
    }
    unsafe { DwmFlush() };
    guide(app).is_some()
}

fn points(app: &Running) -> Vec<String> {
    test_events(app).into_iter().filter(|e| e.starts_with("flick: point")).collect()
}

/// Wait for the guide of `app` to point at `want` (a test event line) and
/// be drawn so on screen.
fn pointed(app: &Running, want: &str) {
    let end = Instant::now() + Duration::from_secs(2);
    while points(app).last().map(String::as_str) != Some(want) && Instant::now() < end {
        pump(20);
    }
    unsafe { DwmFlush() };
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

/// C: its own C; U up, R right, nothing down or left.
const C_KEY: &str = "\
key = C | shortcut | C
flick = up    | U | shortcut | U
flick = right | R | shortcut | R
";

// AC1
#[test]
fn a_key_slid_half_a_key_does_that_ways_action_its_own_untouched_and_nothing_where_it_has_none() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("", C_KEY);
    let c = key_centre(panel, 0);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, c);
    touch.up(1);
    assert_eq!(log.keys(), [d(vk(b'C')), u(vk(b'C'))], "a tap: its own");

    slide(&mut touch, c, by(c, 0.0, -0.75));
    log.wait_for(4);
    assert_eq!(log.keys()[2..], [d(vk(b'U')), u(vk(b'U'))], "up: U, not C");

    slide(&mut touch, c, by(c, 0.75, 0.0));
    log.wait_for(6);
    assert_eq!(log.keys()[4..], [d(vk(b'R')), u(vk(b'R'))], "right: R");

    slide(&mut touch, c, by(c, 0.0, 0.75));
    pump(200);
    assert_eq!(log.keys().len(), 6, "down, where it has nothing: nothing, {:?}", log.keys());

    slide(&mut touch, c, by(c, 0.2, -0.3));
    log.wait_for(8);
    assert_eq!(log.keys()[6..], [d(vk(b'C')), u(vk(b'C'))], "under half a key: its own");
    drop(touch);

    // The mouse, the same way.
    mouse_move_to(c);
    mouse_down_at(c);
    mouse_move_to(by(c, 0.0, -0.75));
    mouse_up_at(by(c, 0.0, -0.75));
    log.wait_for(10);
    assert_eq!(log.keys()[8..], [d(vk(b'U')), u(vk(b'U'))], "the mouse up: U");
}

// AC2
#[test]
fn a_flick_combines_with_a_latched_modifier_and_ends_a_one_shot_latch() {
    let _desktop = desktop();
    let keys = "key = Ctrl | modifier | Ctrl\nkey = Y | shortcut | Y\nflick = up | Z | shortcut | Z\n";
    let (_app, panel) = launch_keys("", keys);
    let (ctrl, y) = (key_centre(panel, 0), key_centre(panel, 1));
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, ctrl);
    touch.up(1);
    slide(&mut touch, y, by(y, 0.0, -0.75));
    log.wait_for(4);
    assert_eq!(log.keys(), [d(VK_LCONTROL), d(vk(b'Z')), u(vk(b'Z')), u(VK_LCONTROL)], "Ctrl+Z, then the latch ends");

    slide(&mut touch, y, by(y, 0.0, -0.75));
    log.wait_for(6);
    assert_eq!(log.keys()[4..], [d(vk(b'Z')), u(vk(b'Z'))], "the next flick goes alone");
}

// AC3
#[test]
fn a_key_with_flicks_slid_toward_the_edge_flicks_and_never_hides_the_panel() {
    let _desktop = desktop();
    let keys = "key = A | shortcut | A\nflick = left | L | shortcut | L\nkey = B | shortcut | B\n";
    let (app, panel) = launch_keys("", keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    // Quickly from the inner side of A to the edge: two pixels in, as an
    // injected contact on the very edge may not land.
    let a = key_centre(panel, 0);
    let from = POINT { x: panel.right - key_px() / 8, y: a.y };
    touch.down(1, from);
    touch.move_to(1, POINT { x: panel.left + 2, y: a.y });
    pump(200);
    assert!(!panels_now(&app).is_empty(), "a key with flicks slid toward the edge leaves the panel up");
    touch.up(1);
    log.wait_for(2);
    assert_eq!(log.keys(), [d(vk(b'L')), u(vk(b'L'))], "it flicked left");

    // B, with none, hides it as before.
    let b = key_centre(panel, 1);
    touch.down(1, b);
    touch.move_to(1, POINT { x: panel.left + 2, y: b.y });
    pump(200);
    assert!(panels_now(&app).is_empty(), "a key without flicks slid toward the edge hides the panel");
    touch.up(1);
    assert_eq!(log.keys().len(), 2, "B is not sent: {:?}", log.keys());
}

// AC4
#[test]
fn a_cross_around_the_key_shows_its_ways_once_it_slides_lights_the_way_it_points_and_goes_as_it_lifts() {
    let _desktop = desktop();
    // C second, so the cross above it is on screen.
    let keys = format!("key = - | none | -\n{C_KEY}");
    let (app, panel) = launch_keys("opacity = 100\ntheme = dark\naccent = #FF0000", &keys);
    let c = key_centre(panel, 1);
    let k = key_px() as f64;
    // Clear of the labels, in the cells above and right of the key.
    let up_cell = by(c, -0.3, -1.0 - 0.3);
    let right_cell = by(c, 1.0 - 0.3, -0.3);
    let mut touch = Touch::new();

    touch.down(1, c);
    assert!(!guide_shown(&app, false), "pressed: no guide");
    touch.move_to(1, by(c, 0.15, 0.0));
    assert!(!guide_shown(&app, false), "under a quarter of a key: no guide");
    touch.up(1);
    assert!(!guide_shown(&app, false), "a tap: no guide");

    touch.down(1, c);
    touch.move_to(1, by(c, 0.3, 0.0));
    assert!(guide_shown(&app, true), "a quarter of a key on: the guide");
    let r = window_rect(guide(&app).unwrap());
    assert!(r.left <= c.x - k as i32 && r.right >= c.x + k as i32, "a key either side across: {}..{} around {}", r.left, r.right, c.x);
    assert!(r.top <= c.y - k as i32 && r.bottom >= c.y + k as i32, "a key either side up and down: {}..{} around {}", r.top, r.bottom, c.y);
    pointed(&app, "flick: point none");
    assert!(near(colour_at(up_cell), FACE), "up, not pointed at: a key face, {:?}", colour_at(up_cell));
    assert!(near(colour_at(right_cell), FACE), "right, not pointed at: a key face, {:?}", colour_at(right_cell));

    touch.move_to(1, by(c, 0.0, -0.75));
    pointed(&app, "flick: point up");
    assert!(near(colour_at(up_cell), RED), "up, pointed at: lit, {:?}", colour_at(up_cell));
    assert!(near(colour_at(right_cell), FACE), "right stays a key face, {:?}", colour_at(right_cell));

    touch.up(1);
    assert!(!guide_shown(&app, false), "lifted: the guide goes");
}

// AC5
#[test]
fn a_flick_cut_short_by_the_panel_hiding_or_the_system_takes_the_guide_away_and_sends_nothing() {
    let _desktop = desktop();
    let (app, panel) = launch_keys("", C_KEY);
    let c = key_centre(panel, 0);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    // Hidden from the tray menu while sliding.
    touch.down(1, c);
    touch.move_to(1, by(c, 0.0, -0.75));
    assert!(guide_shown(&app, true), "sliding: the guide");
    choose(&app, "左エッジ");
    assert!(!guide_shown(&app, false), "the panel hidden: the guide goes");
    touch.up(1);
    pump(200);
    assert_eq!(log.keys(), [], "lifted after: nothing");

    // A contact the system takes away.
    choose(&app, "左エッジ");
    let panel = left_panel(&app);
    let c = key_centre(panel, 0);
    touch.down(1, c);
    touch.move_to(1, by(c, 0.0, -0.75));
    assert!(guide_shown(&app, true), "sliding again: the guide");
    touch.cancel(1);
    assert!(!guide_shown(&app, false), "the contact cancelled: the guide goes");
    pump(200);
    assert_eq!(log.keys(), [], "a cancelled contact sends nothing");
}

// AC6
#[test]
fn a_pen_slid_over_a_key_with_flicks_sends_nothing_and_shows_no_guide() {
    let _desktop = desktop();
    let (app, panel) = launch_keys("", C_KEY);
    let c = key_centre(panel, 0);
    let to = by(c, 0.0, -0.75);
    let log = KeyLog::start();
    let mut shown = false;
    unsafe {
        let pen = CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_NONE);
        assert!(!pen.is_null(), "CreateSyntheticPointerDevice");
        let steps = [
            (c, POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE),
            (c, POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT),
            (by(c, 0.0, -0.4), POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT),
            (to, POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT),
            (to, POINTER_FLAG_UP | POINTER_FLAG_INRANGE),
            (to, POINTER_FLAG_UPDATE),
        ];
        for (p, flags) in steps {
            let frame = pen_frame(p, flags);
            let ok = InjectSyntheticPointerInput(pen, &frame, 1);
            assert!(ok != 0, "InjectSyntheticPointerInput: {}", std::io::Error::last_os_error());
            pump(100);
            shown |= guide(&app).is_some();
        }
        DestroySyntheticPointerDevice(pen);
    }
    pump(200);
    assert!(!shown, "a pen shows no guide");
    assert_eq!(log.keys(), [], "a pen sends nothing");
}

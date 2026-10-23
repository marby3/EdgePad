//! Acceptance tests for the modifier state machine (roadmap v0.3, spec 4.3):
//! a short tap latches a modifier, another tap lets it go, and a shortcut
//! sent under a one-shot latch ends it.
//!
//! A finger tap here is `touch.down` then `touch.up`: about 100 ms, well
//! under the default latch_ms of 250. A press meant to be long holds 400 ms.

mod common;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK};
use common::*;

/// The left panel of `app`, launched with `general` in [general] and `keys`
/// as its only set.
fn launch_keys(general: &str, keys: &str) -> (Running, RECT) {
    let text = format!("[general]\n{general}[set.pad]\n{keys}[profile.default]\nleft = pad\n");
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    (app, panel)
}

const PAD: &str = "\
key = Ctrl  | modifier | Ctrl
key = Shift | modifier | Shift
key = Z     | shortcut | Z
";
const CTRL_KEY: usize = 0;
const SHIFT_KEY: usize = 1;
const Z_KEY: usize = 2;
const Z: u16 = b'Z' as u16;

fn at(panel: RECT, i: usize) -> POINT {
    key_centre(panel, i)
}

// KeyLog records (virtual key, is up): `d` is a key-down, `u` a key-up.
const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

fn tap(touch: &mut Touch, p: POINT) {
    touch.down(1, p);
    touch.up(1);
}

/// The colour of a spot of key `i`'s face, clear of its label.
fn face(panel: RECT, i: usize) -> [i32; 3] {
    let size = panel.right - panel.left;
    let top = panel.top + size * i as i32;
    let (x, y) = (panel.left + size / 4, top + size / 6);
    channels(capture(RECT { left: x, top: y, right: x + 2, bottom: y + 2 })[0])
}

fn differs(a: [i32; 3], b: [i32; 3]) -> bool {
    a.iter().zip(b).any(|(x, y)| (x - y).abs() > 40)
}

// AC1
#[test]
fn a_short_tap_latches_a_modifier_down_and_shows_it_latched() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("", PAD);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let idle = face(panel, CTRL_KEY);

    touch.down(1, at(panel, CTRL_KEY));
    let held = face(panel, CTRL_KEY);
    touch.up(1);
    pump(200);
    assert_eq!(log.keys(), [d(VK_LCONTROL)], "a tap leaves Ctrl down");
    let latched = face(panel, CTRL_KEY);
    assert!(differs(latched, idle), "latched {latched:?} looks like idle {idle:?}");
    assert!(differs(latched, held), "latched {latched:?} looks like held {held:?}");

    // The mouse latches the same way.
    let before = log.keys().len();
    click(at(panel, SHIFT_KEY));
    pump(200);
    assert_eq!(log.keys()[before..], [d(VK_LSHIFT)], "a click leaves Shift down");
}

// AC2
#[test]
fn tapping_a_latched_modifier_again_or_holding_it_lets_it_go() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("", PAD);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let idle = face(panel, CTRL_KEY);

    tap(&mut touch, at(panel, CTRL_KEY));
    tap(&mut touch, at(panel, CTRL_KEY));
    pump(200);
    assert_eq!(log.keys(), [d(VK_LCONTROL), u(VK_LCONTROL)], "the second tap lets Ctrl go");
    let after = face(panel, CTRL_KEY);
    assert!(!differs(after, idle), "Ctrl looks {after:?}, not idle {idle:?}");

    tap(&mut touch, at(panel, CTRL_KEY));
    touch.down(1, at(panel, CTRL_KEY));
    touch.hold(400);
    assert_eq!(log.keys()[2..], [d(VK_LCONTROL)], "pressing a latched key sends nothing more");
    touch.up(1);
    assert_eq!(log.keys()[2..], [d(VK_LCONTROL), u(VK_LCONTROL)], "a long press lets the latch go too");
}

// AC3
#[test]
fn a_long_press_or_one_used_with_another_key_comes_up_on_release() {
    let _desktop = desktop();
    // A latch time long enough that the second case is a short press.
    let (_app, panel) = launch_keys("latch_ms = 2000\n", PAD);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, CTRL_KEY));
    touch.hold(2100);
    touch.up(1);
    assert_eq!(log.keys(), [d(VK_LCONTROL), u(VK_LCONTROL)], "held past latch_ms: up on release");

    touch.down(1, at(panel, CTRL_KEY));
    touch.down(2, at(panel, Z_KEY));
    touch.up(2);
    touch.up(1);
    assert_eq!(
        log.keys()[2..],
        [d(VK_LCONTROL), d(Z), u(Z), u(VK_LCONTROL)],
        "used with Z: up on release, however short"
    );

    touch.down(1, at(panel, CTRL_KEY));
    touch.down(2, at(panel, SHIFT_KEY));
    touch.up(2);
    touch.up(1);
    assert_eq!(
        log.keys()[6..],
        [d(VK_LCONTROL), d(VK_LSHIFT), u(VK_LSHIFT), u(VK_LCONTROL)],
        "another modifier counts as another key"
    );
}

// AC4
#[test]
fn a_shortcut_combines_with_a_one_shot_latch_and_ends_it() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("", PAD);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, at(panel, CTRL_KEY));
    tap(&mut touch, at(panel, SHIFT_KEY));
    tap(&mut touch, at(panel, Z_KEY));
    assert_eq!(
        log.keys(),
        [d(VK_LCONTROL), d(VK_LSHIFT), d(Z), u(Z), u(VK_LCONTROL), u(VK_LSHIFT)],
        "Ctrl+Shift+Z, then both latches end"
    );

    tap(&mut touch, at(panel, Z_KEY));
    assert_eq!(log.keys()[6..], [d(Z), u(Z)], "the next shortcut goes alone");
}

// AC4
#[test]
fn with_latch_lock_a_shortcut_leaves_the_latch_in_place() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("latch = lock\n", PAD);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, at(panel, CTRL_KEY));
    tap(&mut touch, at(panel, Z_KEY));
    tap(&mut touch, at(panel, Z_KEY));
    assert_eq!(log.keys(), [d(VK_LCONTROL), d(Z), u(Z), d(Z), u(Z)], "Ctrl stays latched");

    tap(&mut touch, at(panel, CTRL_KEY));
    assert_eq!(log.keys()[5..], [u(VK_LCONTROL)], "a tap lets it go");
}

// AC5
#[test]
fn every_way_of_letting_go_releases_a_latched_modifier() {
    let _desktop = desktop();
    let release_by: [(&str, fn(&Running)); 3] = [
        ("全キー解放", |app| choose(app, "全キー解放")),
        ("パネルを隠す", |app| choose(app, "左辺")),
        ("画面ロック", |app| {
            let tray: HWND = find_window(app, TRAY_CLASS, false);
            unsafe { SendMessageW(tray, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize, 0) };
            pump(200);
        }),
    ];
    for (how, release) in release_by {
        let (app, panel) = launch_keys("", PAD);
        let log = KeyLog::start();
        let mut touch = Touch::new();
        tap(&mut touch, at(panel, CTRL_KEY));
        assert_eq!(log.keys(), [d(VK_LCONTROL)], "{how}: Ctrl latched");

        release(&app);
        assert_eq!(log.keys(), [d(VK_LCONTROL), u(VK_LCONTROL)], "{how}: Ctrl let go");

        // The latch is forgotten: the next tap latches anew.
        if panels_now(&app).is_empty() {
            choose(&app, "左辺");
        }
        let panel = left_panel(&app);
        tap(&mut touch, at(panel, CTRL_KEY));
        assert_eq!(log.keys()[2..], [d(VK_LCONTROL)], "{how}: the next tap latches again");
    }
}

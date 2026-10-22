//! Acceptance tests for macro and text keys (roadmap v0.3, spec 4.2): a macro
//! sends its steps in order on release, waiting where it says without
//! holding up the panel; a text key types its text as Unicode characters.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use common::*;

/// The left panel of `app`, launched with `general` in [general] and `keys`
/// as its only set.
fn launch_keys(general: &str, keys: &str) -> (Running, RECT) {
    let text = format!("[general]\n{general}[set.pad]\n{keys}[profile.default]\nleft = pad\n");
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    (app, panel)
}

fn at(panel: RECT, i: usize) -> POINT {
    key_centre(panel, i)
}

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

const S: u16 = b'S' as u16;
const A: u16 = b'A' as u16;

const SAVE: &str = "key = 保存 | macro | Ctrl+S, wait 400, Esc\n";

// AC1
#[test]
fn a_macro_sends_its_steps_in_order_on_release_waiting_in_between() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("", SAVE);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.hold(200);
    assert_eq!(log.keys(), [], "nothing while it is held");
    touch.up(1);
    let ctrl_s = [d(VK_LCONTROL), d(S), u(S), u(VK_LCONTROL)];
    assert_eq!(log.keys(), ctrl_s, "the first step at once, then the wait");
    pump(500);
    assert_eq!(log.keys()[4..], [d(VK_ESCAPE), u(VK_ESCAPE)], "Esc after the wait");
}

// AC2
#[test]
fn the_panel_works_while_a_macro_waits_and_hiding_it_drops_the_rest() {
    let _desktop = desktop();
    // The third waits long enough for the tray menu to hide the panel first.
    let keys = format!("{SAVE}key = A | hold | A\nkey = 遅い | macro | Ctrl+S, wait 2000, Esc\n");
    let (app, panel) = launch_keys("", &keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, at(panel, 0));
    tap(&mut touch, at(panel, 1));
    assert_eq!(log.keys()[4..], [d(A), u(A)], "A goes through during the wait");
    pump(400);
    assert_eq!(log.keys()[6..], [d(VK_ESCAPE), u(VK_ESCAPE)], "the macro carries on");

    let before = log.keys().len();
    tap(&mut touch, at(panel, 2));
    choose(&app, "左辺");
    pump(2200);
    assert_eq!(log.keys()[before..].len(), 4, "only Ctrl+S: the rest went with the panel: {:?}", log.keys());
}

// AC3
#[test]
fn a_text_key_types_its_text_as_unicode_characters_on_release() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("", "key = 挨拶 | text | Hi お疲れ😀\n");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.hold(200);
    assert_eq!(log.keys(), [], "nothing while it is held");
    touch.up(1);
    // Each character's units go down, then come up: an emoji is two units.
    let mut expect: Vec<(u16, u32, bool)> = Vec::new();
    let mut ups = Vec::new();
    for c in "Hi お疲れ😀".chars() {
        let mut buf = [0u16; 2];
        let units = c.encode_utf16(&mut buf);
        for up in [false, true] {
            expect.extend(units.iter().map(|&u| (VK_PACKET, u as u32, false)));
            ups.extend(units.iter().map(|_| up));
        }
    }
    assert_eq!(log.scans(), expect, "each character as Unicode key presses");
    let got: Vec<bool> = log.keys().iter().map(|&(_, up)| up).collect();
    assert_eq!(got, ups, "each character's units down, then up");
}

// AC4
#[test]
fn macro_and_text_keys_end_one_shot_latches_before_they_send() {
    let _desktop = desktop();
    let keys = format!("key = Shift | modifier | Shift\n{SAVE}key = 挨拶 | text | ab\n");
    let (_app, panel) = launch_keys("", &keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, at(panel, 0));
    tap(&mut touch, at(panel, 1));
    assert_eq!(log.keys()[..2], [d(VK_LSHIFT), u(VK_LSHIFT)], "Shift is let go before Ctrl+S: {:?}", log.keys());
    pump(500);

    let before = log.keys().len();
    tap(&mut touch, at(panel, 0));
    tap(&mut touch, at(panel, 2));
    let keys = log.keys()[before..].to_vec();
    assert_eq!(keys[..2], [d(VK_LSHIFT), u(VK_LSHIFT)], "Shift is let go before the text: {keys:?}");
    assert_eq!(keys.len(), 6, "then a and b: {keys:?}");
}

// AC4: with latch = lock the latch stays and combines.
#[test]
fn locked_latches_stay_down_through_a_macro() {
    let _desktop = desktop();
    let keys = "key = Shift | modifier | Shift\nkey = 二回 | macro | A, A\n";
    let (_app, panel) = launch_keys("latch = lock\n", keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, at(panel, 0));
    tap(&mut touch, at(panel, 1));
    assert_eq!(log.keys(), [d(VK_LSHIFT), d(A), u(A), d(A), u(A)], "Shift+A twice, Shift still latched");
}

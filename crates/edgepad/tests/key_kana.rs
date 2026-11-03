//! Acceptance tests for Issue #163: kana keys on a panel. `kana | か` sends
//! the kana as its romaji keys, K then A, for the IME to turn into kanji;
//! `kana | ゛゜小` changes the kana sent last, as a phone's key does: a
//! Backspace, then the kana it becomes.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_LSHIFT, VK_OEM_2};
use common::*;

const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

/// A letter's key pressed and let go.
fn letter(c: u8) -> [(u16, bool); 2] {
    [d(c as u16), u(c as u16)]
}

/// Letters' keys, each pressed and let go, in turn.
fn letters(text: &str) -> Vec<(u16, bool)> {
    text.bytes().flat_map(letter).collect()
}

/// The left panel of `app`, launched with `keys` as its only set.
fn launch_keys(keys: &str) -> (Running, RECT) {
    let app = launch_with(Some(&format!("[set.pad]\n{keys}[profile.default]\nleft = pad\n")), None);
    let panel = left_panel(&app);
    (app, panel)
}

fn tap(touch: &mut Touch, p: POINT) {
    touch.down(1, p);
    touch.up(1);
}

// AC1
#[test]
fn a_kana_key_sends_its_kana_as_romaji_keys_and_so_does_its_flick() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("key = か | kana | か\nflick = up | く | kana | く\nkey = ？ | kana | ？\n");
    let (ka, question) = (key_centre(panel, 0), key_centre(panel, 1));
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, ka);
    log.wait_for(4);
    assert_eq!(log.keys(), letters("KA"), "か: K, A");

    touch.down(1, ka);
    touch.move_to(1, POINT { x: ka.x, y: ka.y - key_px() * 3 / 4 });
    touch.up(1);
    log.wait_for(8);
    assert_eq!(log.keys()[4..], letters("KU"), "く, flicked up: K, U");

    tap(&mut touch, question);
    log.wait_for(12);
    assert_eq!(log.keys()[8..], [d(VK_LSHIFT), d(VK_OEM_2), u(VK_OEM_2), u(VK_LSHIFT)], "？: Shift+/");
}

// AC2
#[test]
fn the_mark_key_changes_the_kana_sent_last_in_a_phones_order() {
    let _desktop = desktop();
    let keys = "key = は | kana | は\nkey = な | kana | な\nkey = ゛゜小 | kana | ゛゜小\nkey = X | shortcut | X\n";
    let (_app, panel) = launch_keys(keys);
    let (ha, na, mark, x) = (key_centre(panel, 0), key_centre(panel, 1), key_centre(panel, 2), key_centre(panel, 3));
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let back = [d(VK_BACK), u(VK_BACK)];
    let changed = |to: &str| back.iter().copied().chain(letters(to)).collect::<Vec<_>>();

    // は, then ば, ぱ and は again.
    tap(&mut touch, ha);
    log.wait_for(4);
    assert_eq!(log.keys(), letters("HA"));
    let mut seen = 4;
    for to in ["BA", "PA", "HA"] {
        tap(&mut touch, mark);
        log.wait_for(seen + 6);
        assert_eq!(log.keys()[seen..], changed(to), "は changed to {to}");
        seen += 6;
    }

    // な has no other: nothing.
    tap(&mut touch, na);
    log.wait_for(seen + 4);
    tap(&mut touch, mark);
    pump(300);
    assert_eq!(log.keys()[seen..], letters("NA"), "な, and the mark does nothing");
    seen += 4;

    // Another key between: nothing.
    tap(&mut touch, ha);
    tap(&mut touch, x);
    log.wait_for(seen + 6);
    tap(&mut touch, mark);
    pump(300);
    assert_eq!(log.keys()[seen..], [letters("HA"), letter(b'X').to_vec()].concat(), "X between, and the mark does nothing");
}

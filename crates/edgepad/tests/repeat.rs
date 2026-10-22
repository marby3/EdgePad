//! Acceptance tests for the repeat key (roadmap v0.3, spec 4.2): sent once
//! as it is pressed and, held past the repeat delay, every interval until it
//! is released.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use common::*;

/// The left panel of `app`, launched with `keys` as its only set.
fn launch_keys(keys: &str) -> (Running, RECT) {
    let text = format!("[set.pad]\n{keys}[profile.default]\nleft = pad\n");
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

const Z: u16 = b'Z' as u16;
const BRACKET: u16 = VK_OEM_4;

/// How many times `log` holds the press-and-release of `vk`, checking that
/// nothing else is in it.
fn taps(log: &KeyLog, vk: u16) -> usize {
    let keys = log.keys();
    assert!(keys.chunks(2).all(|pair| pair == [d(vk), u(vk)]), "only taps of {vk:#x}: {keys:?}");
    keys.len() / 2
}

// AC1
#[test]
fn a_repeat_key_sends_once_as_it_is_pressed_and_a_short_tap_sends_only_that() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("key = 縮小 | repeat | [ | interval=50\n");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    assert_eq!(log.keys(), [d(BRACKET), u(BRACKET)], "sent as soon as it is pressed");
    touch.up(1);
    pump(400);
    assert_eq!(taps(&log, BRACKET), 1, "a short tap sends it once");
}

// AC2
#[test]
fn held_past_the_delay_it_repeats_every_interval_until_released() {
    let _desktop = desktop();
    let (_app, panel) = launch_keys("key = 縮小 | repeat | [ | interval=100\n");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.hold(1200);
    // 1 on press, then one per 100 ms from 300 ms on: about 10 by 1300 ms.
    // Timers on a busy machine run late, never early.
    let held = taps(&log, BRACKET);
    assert!((5..=12).contains(&held), "{held} sends while held 1.3 s");
    touch.up(1);
    let released = taps(&log, BRACKET);
    pump(500);
    assert_eq!(taps(&log, BRACKET), released, "nothing more after release");
}

// AC3
#[test]
fn each_repeat_sends_the_whole_combination_and_two_repeat_keys_run_at_once() {
    let _desktop = desktop();
    let keys = "\
key = 戻す | repeat | Ctrl+Z | interval=100
key = 縮小 | repeat | [      | interval=100
";
    let (_app, panel) = launch_keys(keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.hold(500);
    touch.up(1);
    let keys = log.keys();
    let undo = [d(VK_LCONTROL), d(Z), u(Z), u(VK_LCONTROL)];
    assert!(keys.len() >= 8 && keys.chunks(4).all(|c| c == undo), "whole Ctrl+Z each time: {keys:?}");

    let before = log.keys().len();
    touch.down(1, at(panel, 0));
    touch.down(2, at(panel, 1));
    touch.hold(600);
    touch.up(1);
    touch.up(2);
    let both = &log.keys()[before..];
    let count = |vk| both.iter().filter(|&&k| k == d(vk)).count();
    assert!(count(Z) >= 3 && count(BRACKET) >= 3, "both keys repeat: {both:?}");
}

// AC4
#[test]
fn a_repeat_combines_with_a_latched_modifier_which_comes_up_after() {
    let _desktop = desktop();
    let keys = "\
key = Ctrl | modifier | Ctrl
key = 縮小 | repeat   | [ | interval=100
";
    let (_app, panel) = launch_keys(keys);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.up(1);
    touch.down(1, at(panel, 1));
    touch.hold(500);
    touch.up(1);
    let keys = log.keys();
    let (first, last) = (keys[0], keys[keys.len() - 1]);
    assert_eq!((first, last), (d(VK_LCONTROL), u(VK_LCONTROL)), "Ctrl down throughout: {keys:?}");
    let middle = &keys[1..keys.len() - 1];
    assert!(middle.len() >= 4 && middle.chunks(2).all(|c| c == [d(BRACKET), u(BRACKET)]), "[ alone under the latch: {keys:?}");
}

// AC5
#[test]
fn hiding_the_panel_stops_a_repeat_under_a_finger() {
    let _desktop = desktop();
    let (app, panel) = launch_keys("key = 縮小 | repeat | [ | interval=50\n");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, at(panel, 0));
    touch.hold(400);
    choose(&app, "左辺");
    let hidden = taps(&log, BRACKET);
    pump(500);
    assert_eq!(taps(&log, BRACKET), hidden, "no repeat once the panel is hidden");
    touch.up(1);
    pump(200);
    assert_eq!(taps(&log, BRACKET), hidden, "nor after the finger lifts");
}

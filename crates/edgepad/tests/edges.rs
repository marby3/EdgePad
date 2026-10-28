//! Acceptance tests for Issue #16: panels on all four edges, laid out by the
//! edge's columns / rows and each key's width.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_LCONTROL, VK_LSHIFT};
use common::*;

#[derive(Debug, PartialEq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Which edge `r` sits against. A panel is longer along its edge than it is
/// thick, so a tall one is on the left or right and a wide one on the top or
/// bottom; the tests give every panel more keys than lines to keep it so.
/// This holds even for a panel spanning its whole edge.
fn touching(r: RECT) -> Side {
    let m = primary_monitor();
    match (r.bottom - r.top > r.right - r.left, r.left == m.left, r.top == m.top) {
        (true, true, _) => Side::Left,
        (true, false, _) => Side::Right,
        (false, _, true) => Side::Top,
        (false, _, false) => Side::Bottom,
    }
}

/// The panel against `which`, failing if there is none.
fn panel_on(all: &[RECT], which: Side) -> RECT {
    *all.iter().find(|&&r| touching(r) == which).unwrap_or_else(|| {
        let found: Vec<_> = all.iter().map(|r| (r.left, r.top, r.right, r.bottom)).collect();
        panic!("no panel on {which:?}: {found:?}")
    })
}

fn vk(c: char) -> u16 {
    c as u16
}

/// Touch `p`, hold, lift; return the key events this caused.
fn tap(log: &KeyLog, touch: &mut Touch, p: POINT) -> Vec<(u16, bool)> {
    let before = log.keys().len();
    touch.down(1, p);
    touch.hold(150);
    touch.up(1);
    log.keys()[before..].to_vec()
}

fn pressed(c: char) -> Vec<(u16, bool)> {
    vec![(vk(c), false), (vk(c), true)]
}

// AC1
#[test]
fn a_panel_shows_only_on_enabled_edges_that_have_a_key_set() {
    let _desktop = desktop();
    let text = "\
[edge.top]
enabled = false
[set.a]
key = A | hold | A
key = B | hold | B
[profile.default]
left   = a
right  = a
top    = a
bottom = none
";
    let app = launch_with(Some(text), None);
    let mut sides: Vec<String> = panels(&app).into_iter().map(|r| format!("{:?}", touching(r))).collect();
    sides.sort();
    assert_eq!(sides, ["Left", "Right"], "top is disabled, bottom is none");
}

// AC2
#[test]
fn a_panel_touches_its_edge_is_centred_along_it_and_is_lines_times_key_size_thick() {
    let _desktop = desktop();
    let text = "\
[general]
key_size = 40
[edge.right]
columns = 2
[edge.bottom]
rows = 1
[set.five]
key = A | hold | A
key = B | hold | B
key = C | hold | C
key = D | hold | D
key = E | hold | E
[set.four]
key = A | hold | A
key = B | hold | B
key = C | hold | C
key = D | hold | D
[profile.default]
right  = five
bottom = four
";
    let app = launch_with(Some(text), None);
    let all = panels(&app);
    let (m, k) = (primary_monitor(), px(40));

    let r = panel_on(&all, Side::Right);
    assert_eq!(r.right, m.right, "right panel flush with the right edge");
    assert_eq!((r.right - r.left, r.bottom - r.top), (2 * k, 3 * k), "two columns, five keys in three rows");
    let centred = m.top + (m.bottom - m.top - 3 * k) / 2;
    assert!((r.top - centred).abs() <= 1, "right panel centred vertically: {} vs {centred}", r.top);

    let b = panel_on(&all, Side::Bottom);
    // Kept 16 px off the bottom since Issue #117.
    assert_eq!(b.bottom, m.bottom - px(16), "bottom panel off the bottom edge by its gap");
    assert_eq!((b.right - b.left, b.bottom - b.top), (4 * k, k), "one row of four keys");
    let centred = m.left + (m.right - m.left - 4 * k) / 2;
    assert!((b.left - centred).abs() <= 1, "bottom panel centred horizontally: {} vs {centred}", b.left);
}

// AC3
#[test]
fn keys_run_along_the_edge_filling_the_outermost_line_first_and_wide_keys_take_more_room() {
    let _desktop = desktop();
    let text = "\
[edge.left]
columns = 2
[edge.right]
columns = 2
[edge.top]
rows = 2
[set.left]
key = A | hold | A
key = B | hold | B | width=2
key = C | hold | C
key = D | hold | D
[set.right]
key = H | hold | H
key = I | hold | I
key = J | hold | J
key = K | hold | K
key = L | hold | L
[set.top]
key = E | hold | E
key = F | hold | F
key = G | hold | G
key = M | hold | M
key = N | hold | N
[profile.default]
left  = left
right = right
top   = top
";
    let app = launch_with(Some(text), None);
    let all = panels(&app);
    let k = key_px();
    let log = KeyLog::start();
    let mut touch = Touch::new();

    // Left, two columns: A and the double-height B fill the outer column,
    // C and D the inner one.
    let left = panel_on(&all, Side::Left);
    assert_eq!((left.right - left.left, left.bottom - left.top), (2 * k, 3 * k), "left panel 2 x 3 keys");
    for (col, row, key) in [(0, 0, 'A'), (0, 1, 'B'), (0, 2, 'B'), (1, 0, 'C'), (1, 1, 'D')] {
        assert_eq!(tap(&log, &mut touch, cell(left, k, col, row)), pressed(key), "left column {col} row {row}");
    }
    assert_eq!(tap(&log, &mut touch, cell(left, k, 1, 2)), [], "the cell after D is empty");

    // Right: the outer column is the one at the screen's right edge.
    let right = panel_on(&all, Side::Right);
    for (col, row, key) in [(1, 0, 'H'), (1, 1, 'I'), (1, 2, 'J'), (0, 0, 'K'), (0, 1, 'L')] {
        assert_eq!(tap(&log, &mut touch, cell(right, k, col, row)), pressed(key), "right column {col} row {row}");
    }

    // Top, two rows: left to right along the top edge first.
    let top = panel_on(&all, Side::Top);
    assert_eq!((top.right - top.left, top.bottom - top.top), (3 * k, 2 * k), "top panel 3 x 2 keys");
    for (col, row, key) in [(0, 0, 'E'), (1, 0, 'F'), (2, 0, 'G'), (0, 1, 'M'), (1, 1, 'N')] {
        assert_eq!(tap(&log, &mut touch, cell(top, k, col, row)), pressed(key), "top column {col} row {row}");
    }
}

// AC4
#[test]
fn keys_on_every_edge_are_held_while_pressed_by_touch_or_mouse() {
    let _desktop = desktop();
    // Two keys per set, so no panel is square (see `touching`).
    let text = "\
[set.r]
key = R | hold | R
key = R | hold | R
[set.t]
key = T | hold | T
key = T | hold | T
[set.b]
key = Sh | modifier | Shift
key = Sh | modifier | Shift
[profile.default]
right  = r
top    = t
bottom = b
";
    let app = launch_with(Some(text), None);
    let all = panels(&app);
    let k = key_px();
    let log = KeyLog::start();
    let mut touch = Touch::new();

    for (which, vk) in [(Side::Right, vk('R')), (Side::Top, vk('T')), (Side::Bottom, VK_LSHIFT)] {
        let p = cell(panel_on(&all, which), k, 0, 0);
        let before = log.keys().len();
        touch.down(1, p);
        touch.hold(300);
        assert_eq!(log.keys()[before..], [(vk, false)], "finger held on the key");
        touch.up(1);
        assert_eq!(log.keys()[before..], [(vk, false), (vk, true)], "finger lifted");

        let before = log.keys().len();
        mouse_down_at(p);
        pump(300);
        assert_eq!(log.keys()[before..], [(vk, false)], "mouse held on the key");
        mouse_up_at(p);
        assert_eq!(log.keys()[before..], [(vk, false), (vk, true)], "mouse released");
    }
}

// AC5
#[test]
fn the_same_key_on_two_edges_goes_down_once_and_up_once_both_are_released() {
    let _desktop = desktop();
    let text = "\
[set.mods]
key = Ctrl  | modifier | Ctrl
key = Shift | modifier | Shift
[profile.default]
left  = mods
right = mods
";
    let app = launch_with(Some(text), None);
    let all = panels(&app);
    let k = key_px();
    let (left, right) = (cell(panel_on(&all, Side::Left), k, 0, 0), cell(panel_on(&all, Side::Right), k, 0, 0));
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, left);
    touch.down(2, right);
    touch.hold(200);
    assert_eq!(log.keys(), [(VK_LCONTROL, false)], "one key-down for two fingers");
    touch.up(1);
    assert_eq!(log.keys(), [(VK_LCONTROL, false)], "still held by the right edge");
    touch.up(2);
    assert_eq!(log.keys(), [(VK_LCONTROL, false), (VK_LCONTROL, true)], "released with the last finger");
}

// AC6
#[test]
fn keys_that_do_not_fit_along_the_edge_are_left_out() {
    let _desktop = desktop();
    let m = primary_monitor();
    let k = px(200);
    let fit = (m.bottom - m.top) / k;
    let mut text = String::from("[general]\nkey_size = 200\n[set.many]\n");
    for c in ('A'..).take(fit as usize + 2) {
        text += &format!("key = {c} | hold | {c}\n");
    }
    text += "[profile.default]\nleft = many\n";

    let app = launch_with(Some(&text), None);
    let left = panel_on(&panels(&app), Side::Left);
    assert_eq!(left.bottom - left.top, fit * k, "only the {fit} keys that fit");
    let last = (b'A' + fit as u8 - 1) as char;
    let log = KeyLog::start();
    let mut touch = Touch::new();
    assert_eq!(tap(&log, &mut touch, cell(left, k, 0, fit - 1)), pressed(last), "the last key that fits works");
}

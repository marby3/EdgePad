//! Acceptance tests for Issue #157: flicks, a key's actions up, down, left
//! and right, written `flick = direction | label | kind | content` after
//! its key line; and how a press is told when it lifts.

use edgepad_core::config::*;
use edgepad_core::flick::{Dir, Release, direction, release};
use edgepad_core::keys::{Combo, Key, Mods};

const PROFILE: &str = "[profile.default]\nleft = pad\n";

fn read(text: &str) -> Result<Config, Vec<Error>> {
    parse_with(text.as_bytes(), &mut |_: &str| Ok(br#"["A","B"]"#.to_vec()))
}

fn lines(errors: &[Error]) -> Vec<usize> {
    errors.iter().map(|e| e.line).collect()
}

fn ctrl(c: u8) -> Action {
    Action::Shortcut(Combo { mods: Mods::CTRL, key: Some(Key::Char(c)) })
}

/// C with Copy up and Select all left; X, sending nothing, with a
/// calculator down.
const PAD: &str = "\
[set.pad]
key = C | shortcut | C
flick = up   | コピー | shortcut | Ctrl+C
flick = left | 全選択 | macro    | Ctrl+A
key = X | none | -
flick = down | 電卓 | run | calc.exe
";

fn pad() -> Config {
    read(&format!("{PAD}{PROFILE}")).unwrap_or_else(|e| panic!("{e:?}"))
}

// AC1
#[test]
fn flick_lines_give_the_key_before_them_its_directions() {
    let config = pad();
    let keys = &config.set("pad").expect("pad").keys;
    let flick = |k: usize, d: Dir| keys[k].flicks[d as usize].as_ref().map(|f| (f.label.as_str(), f.action.clone()));
    assert_eq!(flick(0, Dir::Up), Some(("コピー", ctrl(b'C'))));
    assert_eq!(flick(0, Dir::Left).map(|(l, _)| l), Some("全選択"));
    assert_eq!(flick(0, Dir::Down), None);
    assert_eq!(flick(1, Dir::Down), Some(("電卓", Action::Run("calc.exe".into()))));
    assert_eq!(keys[0].action, Action::Shortcut(Combo { mods: Mods::NONE, key: Some(Key::Char(b'C')) }), "its own as before");

    // Each refused on its line.
    let wrong = [
        ("[set.pad]\nflick = up | a | shortcut | A\nkey = A | shortcut | A\n", 2, "before any key"),
        ("[set.pad]\nkey = A | shortcut | A\nflick = upward | a | shortcut | A\n", 3, "no such direction"),
        ("[set.pad]\nkey = A | shortcut | A\nflick = up | a | shortcut | A\nflick = up | b | shortcut | B\n", 4, "up twice"),
        ("[set.pad]\nkey = A | shortcut | A\nflick = up | a | hold | A\n", 3, "a hold up"),
        ("[set.pad]\nkey = A | shortcut | A\nflick = up | a | layer | pad\n", 3, "a layer up"),
        ("[set.pad]\nkey = A | hold | A\nflick = up | a | shortcut | A\n", 3, "on a hold key"),
        ("[set.pad]\nkey = A | modifier | Ctrl\nflick = up | a | shortcut | A\n", 3, "on a modifier"),
        ("[set.pad]\nkey = A | repeat | A\nflick = up | a | shortcut | A\n", 3, "on a repeat key"),
        ("[set.pad]\nkey = P | pie | items\n[set.items]\nkey = A | shortcut | A\nflick = up | a | shortcut | A\nkey = B | shortcut | B\n", 5, "on a pie's item"),
    ];
    for (text, line, what) in wrong {
        let errors = read(&format!("{text}{PROFILE}")).expect_err(what);
        assert_eq!(lines(&errors), [line], "{what}: {errors:?}");
    }
    let layer = "[set.kb]\nlayout = kle:kb.json\nkey = A | shortcut | A\n[set.kb.fn]\nkey = - | trans | -\nflick = up | a | shortcut | A\n\
        [profile.default]\nleft = kb\n";
    assert_eq!(lines(&read(layer).expect_err("on trans")), [6], "on a key through");
}

// AC2
#[test]
fn a_press_moved_half_a_key_or_more_points_the_way_it_went_most() {
    let key = 40;
    assert_eq!(direction(0, 0, key), None);
    assert_eq!(direction(19, 0, key), None, "under half a key");
    assert_eq!(direction(0, -20, key), Some(Dir::Up), "half a key");
    assert_eq!(direction(0, 25, key), Some(Dir::Down));
    assert_eq!(direction(-30, 10, key), Some(Dir::Left));
    assert_eq!(direction(30, -29, key), Some(Dir::Right));
    assert_eq!(direction(30, -30, key), Some(Dir::Right), "45°: across");
    assert_eq!(direction(-30, 30, key), Some(Dir::Left), "45°: across");
}

// AC3
#[test]
fn a_flick_key_lifts_to_its_own_its_direction_or_nothing_never_hiding() {
    let config = pad();
    let keys = &config.set("pad").expect("pad").keys;
    let (c, key, half) = (&keys[0], 40, 20);
    assert_eq!(release(c, Edge::Left, 0, 0, key, half), Release::Own);
    assert_eq!(release(c, Edge::Left, 5, -10, key, half), Release::Own, "not half a key");
    assert_eq!(release(c, Edge::Left, 0, -30, key, half), Release::Flick(Dir::Up));
    assert_eq!(release(c, Edge::Left, 0, 30, key, half), Release::Nothing, "nothing down");
    // Toward the left edge: its flick, not a swipe that hides.
    assert_eq!(release(c, Edge::Left, -40, 0, key, half), Release::Flick(Dir::Left));
    // X has nothing left: nothing, and still no hiding.
    assert_eq!(release(&keys[1], Edge::Left, -40, 0, key, half), Release::Nothing);

    // A key with no flick, as before: toward the edge hides, else its own.
    let plain = read(&format!("[set.pad]\nkey = A | shortcut | A\n{PROFILE}")).unwrap();
    let a = &plain.set("pad").unwrap().keys[0];
    assert_eq!(release(a, Edge::Left, -40, 0, key, half), Release::Hide);
    assert_eq!(release(a, Edge::Bottom, 0, 40, key, half), Release::Hide);
    assert_eq!(release(a, Edge::Left, 0, -40, key, half), Release::Own);
    assert_eq!(release(a, Edge::Left, 40, 0, key, half), Release::Own);
}

// AC4
#[test]
fn a_key_through_in_a_layer_has_its_sets_flicks_and_one_of_its_own_its_own() {
    let text = "[set.kb]\nlayout = kle:kb.json\nkey = A | shortcut | A\nflick = up | a | shortcut | Ctrl+A\nkey = B | shortcut | B\n\
        [set.kb.fn]\nkey = - | trans | -\nkey = 2 | shortcut | 2\nflick = down | b | shortcut | Ctrl+B\n\
        [profile.default]\nleft = kb\n";
    let config = read(text).unwrap_or_else(|e| panic!("{e:?}"));
    let (kb, layer) = (&config.set("kb").unwrap().keys, &config.set("kb.fn").unwrap().keys);
    assert_eq!(layer[0].flicks, kb[0].flicks, "A's, through");
    assert_eq!(layer[1].flicks[Dir::Down as usize].as_ref().map(|f| f.action.clone()), Some(ctrl(b'B')));
}

// Malformed input: a flick line comes from outside.
#[test]
fn a_flick_line_that_does_not_read_is_an_error_not_a_panic() {
    for value in [
        "",
        "up",
        "up |",
        "up | a",
        "| a | shortcut | A",
        "up | a | shortcut |",
        "up | a | nonsense | A",
        "ｕｐ | a | shortcut | A",
        "up | a | shortcut | A | width=2",
        "up | a | shortcut | A | x=1, y=1",
        "up | a | shortcut | Ctrl+Nokey",
        "up | a | | |",
    ] {
        let text = format!("[set.pad]\nkey = A | shortcut | A\nflick = {value}\n{PROFILE}");
        let errors = read(&text).expect_err(value);
        assert_eq!(lines(&errors), [3], "{value:?}: {errors:?}");
    }
}

//! Acceptance tests for the modifier state machine (roadmap v0.3, spec 4.3):
//! the core half, timed by the caller so no test waits.

use std::time::{Duration, Instant};

use edgepad_core::config::{General, parse};
use edgepad_core::modifier::{LatchMode, Modifier};

const LATCH: Duration = Duration::from_millis(250);

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// AC1
#[test]
fn a_short_tap_with_no_other_key_latches_and_leaves_the_key_down() {
    let t = Instant::now();
    let mut m = Modifier::default();
    assert!(m.press(t), "the first press sends a key-down");
    assert!(!m.release(t + ms(100), LATCH, false), "a short tap sends no key-up");
    assert!(m.is_latched() && m.is_down(), "{m:?}");
}

// AC2
#[test]
fn tapping_or_holding_a_latched_key_lets_it_go() {
    let t = Instant::now();
    for held in [ms(50), ms(1000)] {
        let mut m = Modifier::default();
        m.press(t);
        m.release(t + ms(100), LATCH, false);
        assert!(!m.press(t + ms(500)), "a latched key is down already: no second key-down");
        assert!(m.is_down() && !m.is_latched(), "pressed again: {m:?}");
        assert!(m.release(t + ms(500) + held, LATCH, false), "released after {held:?}: key-up");
        assert_eq!(m, Modifier::Up);
    }
}

// AC3
#[test]
fn a_long_press_or_one_used_with_another_key_comes_up_on_release() {
    let t = Instant::now();
    let mut m = Modifier::default();
    m.press(t);
    assert!(m.release(t + LATCH, LATCH, false), "held for latch_ms: key-up on release");
    assert_eq!(m, Modifier::Up);

    m.press(t);
    m.other_key();
    assert!(m.release(t + ms(100), LATCH, false), "another key pressed meanwhile: key-up however short");
    assert_eq!(m, Modifier::Up);

    m.press(t);
    assert!(m.release(t + ms(100), LATCH, true), "a cancelled contact never latches");
    assert_eq!(m, Modifier::Up);

    // Another key while latched, or with nothing pressed, changes nothing.
    m.other_key();
    assert_eq!(m, Modifier::Up);
    m.press(t);
    m.release(t + ms(100), LATCH, false);
    m.other_key();
    assert!(m.is_latched());
}

// AC4
#[test]
fn a_shortcut_ends_a_one_shot_latch_but_not_a_locked_one_nor_a_held_key() {
    let t = Instant::now();
    let latched = || {
        let mut m = Modifier::default();
        m.press(t);
        m.release(t + ms(100), LATCH, false);
        m
    };

    let mut m = latched();
    assert!(m.shortcut_sent(LatchMode::OneShot), "one-shot: key-up after the shortcut");
    assert_eq!(m, Modifier::Up);

    let mut m = latched();
    assert!(!m.shortcut_sent(LatchMode::Lock), "lock: stays down");
    assert!(m.is_latched());

    // Under a finger the key comes up when the finger lifts, not before.
    let mut m = Modifier::default();
    m.press(t);
    m.other_key();
    assert!(!m.shortcut_sent(LatchMode::OneShot));
    assert!(m.is_down() && !m.is_latched());
}

// AC5
#[test]
fn letting_go_releases_a_latched_or_pressed_key() {
    let t = Instant::now();
    let mut m = Modifier::default();
    assert!(!m.let_go(), "nothing to release");
    m.press(t);
    assert!(m.let_go(), "pressed: key-up");
    m.press(t);
    m.release(t + ms(100), LATCH, false);
    assert!(m.let_go(), "latched: key-up");
    assert_eq!(m, Modifier::Up);
    assert!(!m.release(t + ms(200), LATCH, false), "a finger lifting after letting go sends nothing");
}

// AC4: the setting.
#[test]
fn latch_reads_oneshot_or_lock_and_defaults_to_oneshot() {
    let read = |general: &str| parse(format!("[general]\n{general}\n[profile.default]\n").as_bytes()).map(|c| c.general.latch);
    assert_eq!(General::default().latch, LatchMode::OneShot);
    assert_eq!(read(""), Ok(LatchMode::OneShot));
    assert_eq!(read("latch = oneshot"), Ok(LatchMode::OneShot));
    assert_eq!(read("latch = Lock"), Ok(LatchMode::Lock));
}

// Malformed input: the settings file comes from outside.
#[test]
fn any_other_latch_value_is_an_error_with_its_line() {
    for value in ["", "off", "one-shot", "lock lock", "1"] {
        let text = format!("[general]\nlatch = {value}\n[profile.default]\n");
        let errors = parse(text.as_bytes()).expect_err(value);
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2], "{value:?}: {errors:?}");
        assert!(errors[0].reason.contains("latch"), "{:?}", errors[0].reason);
    }
}

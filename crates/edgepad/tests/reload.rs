//! Acceptance tests for Issue #24: EdgePad reads its settings file again when
//! it is saved, and keeps the settings it has when the new file is unusable.

mod common;

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::RECT;
use common::*;

/// A settings file whose `edge` has `n` hold keys (A, B, C, ...).
fn keys_on(edge: &str, n: usize) -> String {
    let mut text = String::from("[set.pad]\n");
    for c in (b'A'..).take(n) {
        let c = c as char;
        text += &format!("key = {c} | hold | {c}\n");
    }
    text + &format!("[profile.default]\n{edge} = pad\n")
}

/// Save `text` as the settings file beside the exe of `app`.
fn save(app: &Running, text: &str) {
    std::fs::write(exe_dir(app).join("edgepad.ini"), text).expect("save edgepad.ini");
}

/// The single visible panel of `app`, if there is exactly one.
fn the_panel(app: &Running) -> Option<RECT> {
    match panels_now(app)[..] {
        [r] => Some(r),
        _ => None,
    }
}

/// How many keys long the single panel against the left edge is.
fn left_keys(app: &Running) -> Option<i32> {
    let m = primary_monitor();
    the_panel(app).filter(|r| r.left == m.left).map(|r| (r.bottom - r.top) / key_px())
}

/// The visible panels as (left, top, right, bottom), for messages.
fn rects(app: &Running) -> Vec<(i32, i32, i32, i32)> {
    panels_now(app).iter().map(|r| (r.left, r.top, r.right, r.bottom)).collect()
}

/// Wait up to `ms` for `done`; whether it came true.
fn within(ms: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        pump(20);
    }
}

const A: u16 = b'A' as u16;

const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

// AC1
#[test]
fn saving_the_settings_file_rebuilds_the_panels_within_a_second() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on("left", 2)), None);
    find_panel(&app);
    assert_eq!(left_keys(&app), Some(2));

    save(&app, &keys_on("right", 3));
    let m = primary_monitor();
    let on_the_right = || the_panel(&app).is_some_and(|r| r.right == m.right && r.bottom - r.top == 3 * key_px());
    assert!(within(1000, on_the_right), "the right panel of 3 keys, alone, within 1s: {:?}", rects(&app));
}

// AC2
#[test]
fn an_unusable_file_leaves_the_panels_as_they_were_until_it_is_fixed() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on("left", 2)), None);
    find_panel(&app);

    let broken = "[set.pad]\nkey = A | nosuchkind | A\n[profile.default]\nleft = pad\n";
    let oversized = keys_on("left", 3) + &";padding\n".repeat(64 * 1024 / 9 + 1);
    for (what, text) in [("a syntax error", broken.to_string()), ("over 64KiB", oversized)] {
        save(&app, &text);
        pump(1000);
        assert_eq!(left_keys(&app), Some(2), "{what}: the panel stays as it was");
        let log = KeyLog::start();
        let mut touch = Touch::new();
        touch.down(1, key_centre(left_panel(&app), 0));
        touch.up(1);
        assert_eq!(log.keys(), [d(A), u(A)], "{what}: its keys still send");
    }

    save(&app, &keys_on("left", 3));
    assert!(within(1000, || left_keys(&app) == Some(3)), "the fixed file is applied");
}

// AC3
#[test]
fn a_settings_file_created_beside_the_exe_replaces_the_fixed_keys() {
    let _desktop = desktop();
    let app = launch();
    find_panel(&app);
    assert_eq!(left_keys(&app), Some(KEY_COUNT), "the fixed keys first");

    save(&app, &keys_on("left", 2));
    assert!(within(1000, || left_keys(&app) == Some(2)), "the new file is applied");
}

// AC4
#[test]
fn keys_held_on_the_panels_are_released_before_they_are_rebuilt() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on("left", 2)), None);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, 0));
    touch.hold(150);
    assert_eq!(log.keys(), [d(A)]);

    save(&app, &keys_on("left", 3));
    assert!(within(1000, || left_keys(&app) == Some(3)), "rebuilt");
    assert_eq!(log.keys(), [d(A), u(A)], "A is released with the old panel");
    touch.up(1);
    pump(200);
    assert_eq!(log.keys(), [d(A), u(A)], "lifting the finger afterwards sends nothing more");
}

// AC5
#[test]
fn other_files_and_unchanged_content_leave_the_panels_alone() {
    let _desktop = desktop();
    let text = keys_on("left", 2);
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, 0));
    touch.hold(150);

    std::fs::write(exe_dir(&app).join("other.txt"), "something else").unwrap();
    save(&app, &text);
    touch.hold(1000);
    assert_eq!(log.keys(), [d(A)], "A stays held: nothing was rebuilt");
    assert_eq!(rects(&app), [(panel.left, panel.top, panel.right, panel.bottom)], "the same panel");
    touch.up(1);

    save(&app, &keys_on("left", 3));
    assert!(within(1000, || left_keys(&app) == Some(3)), "a real change is still applied");
}

// AC6
#[test]
fn waiting_for_changes_uses_no_cpu() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on("left", 2)), None);
    find_panel(&app);
    save(&app, &keys_on("left", 3));
    assert!(within(1000, || left_keys(&app) == Some(3)), "the watch is running");

    pump(500);
    let before = cpu_time_100ns(&app);
    std::thread::sleep(Duration::from_secs(3));
    let used_ms = (cpu_time_100ns(&app) - before) as f64 / 10_000.0;
    assert!(used_ms < 10.0, "CPU {used_ms:.1}ms in 3s of waiting");
}

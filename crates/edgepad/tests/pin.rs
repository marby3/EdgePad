//! Acceptance tests for pinning the profile from the tray (roadmap v0.3,
//! spec 4.1 and 4.4): while pinned, the app in front changes nothing.
//!
//! As in tests/profile.rs, this test process and a copy of it under another
//! name take turns in the foreground.

mod common;

use std::process::Command;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, RECT};
use common::*;

const OTHER_CLASS: &str = "EdgePadTestPinOther";
const OTHER_ENV: &str = "EDGEPAD_TEST_PIN_OTHER";

/// Not a test: the window of the other app, when this binary runs as it.
#[test]
#[ignore]
fn other_app() {
    if std::env::var_os(OTHER_ENV).is_none() {
        return;
    }
    let m = primary_monitor();
    let r = RECT { left: m.left + 400, top: m.top + 100, right: m.left + 700, bottom: m.top + 300 };
    let _window = test_window(OTHER_CLASS, 0, r);
    pump(120_000);
}

fn me() -> String {
    std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned()
}

fn other_app_window() -> (Running, HWND) {
    let dir = std::env::temp_dir().join(format!("edgepad-pin-other-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("edgepad-pin-other.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).expect("copy the test binary");
    let child = Command::new(&exe)
        .args(["other_app", "--exact", "--ignored", "--nocapture"])
        .env(OTHER_ENV, "1")
        .spawn()
        .expect("start the other app");
    let other = Running(child, Some(dir));
    let hwnd = find_window(&other, OTHER_CLASS, true);
    (other, hwnd)
}

fn left_keys(app: &Running) -> i32 {
    let m = primary_monitor();
    panels_now(app).iter().find(|r| r.left == m.left).map_or(0, |r| (r.bottom - r.top) / key_px())
}

fn wait_for_left_keys(app: &Running, keys: i32, why: &str) {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        if left_keys(app) == keys {
            return;
        }
        pump(50);
    }
    panic!("{why}: the left panel is {} keys, not {keys}", left_keys(app));
}

/// Default: two keys on the left. This process (profile "me"): `mine` keys.
fn settings(mine: usize) -> String {
    let mut text = String::from("[set.two]\nkey = A | hold | A\nkey = B | hold | B\n[set.mine]\n");
    for c in ('K'..).take(mine) {
        text += &format!("key = {c} | hold | {c}\n");
    }
    text + &format!("[profile.default]\nleft = two\n[profile.me]\nprocess = {}\nleft = mine\n", me())
}

// AC1
#[test]
fn a_pinned_profile_stays_whatever_comes_to_the_front_until_unpinned() {
    let _desktop = desktop();
    let m = primary_monitor();
    let mine = test_window("EdgePadTestPinMine", 0, RECT { left: m.left + 400, top: m.top + 350, right: m.left + 700, bottom: m.top + 550 });
    force_foreground(mine.0);
    let app = launch_with(Some(&settings(3)), None);
    wait_for_left_keys(&app, 3, "this app's profile");

    let item = menu_item(&app, "プロファイル");
    assert!(item.text.contains("me") && !item.checked && !item.grayed, "{item:?}");
    choose(&app, "プロファイル");
    assert!(menu_item(&app, "プロファイル").checked, "pinned");

    let (_other, other) = other_app_window();
    force_foreground(other);
    pump(500);
    assert_eq!(left_keys(&app), 3, "pinned: another app in front changes nothing");

    choose(&app, "プロファイル");
    assert!(!menu_item(&app, "プロファイル").checked, "unpinned");
    wait_for_left_keys(&app, 2, "unpinned: the profile of the app in front, the default");
}

// AC2
#[test]
fn a_pin_holds_through_a_reload_while_the_profile_is_still_there() {
    let _desktop = desktop();
    let m = primary_monitor();
    let mine = test_window("EdgePadTestPinMine", 0, RECT { left: m.left + 400, top: m.top + 350, right: m.left + 700, bottom: m.top + 550 });
    force_foreground(mine.0);
    let app = launch_with(Some(&settings(3)), None);
    wait_for_left_keys(&app, 3, "this app's profile");
    choose(&app, "プロファイル");
    let (_other, other) = other_app_window();
    force_foreground(other);

    std::fs::write(exe_dir(&app).join("edgepad.ini"), settings(4)).unwrap();
    wait_for_left_keys(&app, 4, "reloaded, still on the pinned profile");
    assert!(menu_item(&app, "プロファイル").checked, "still pinned");

    let gone = settings(4).replace("[profile.me]", "[profile.renamed]");
    std::fs::write(exe_dir(&app).join("edgepad.ini"), gone).unwrap();
    pump(1000);
    let item = menu_item(&app, "プロファイル");
    assert!(!item.checked && item.text.contains("default"), "the pinned profile is gone: unpinned, {item:?}");
}

// AC3
#[test]
fn without_a_settings_file_there_is_no_profile_to_pin() {
    let _desktop = desktop();
    let app = launch();
    find_panel(&app);
    assert!(menu_item(&app, "プロファイル").grayed, "greyed out with the fixed keys");
}

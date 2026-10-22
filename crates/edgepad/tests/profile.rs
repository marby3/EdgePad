//! Acceptance tests for switching profiles by the foreground app (roadmap
//! v0.3, spec 4.4).
//!
//! Two apps take turns in the foreground: this test process, and a copy of
//! it under another name that only opens a window (`other_app`). A profile
//! names this test's own executable, so which one is in front decides the
//! profile EdgePad shows.

mod common;

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use common::*;

const OTHER_CLASS: &str = "EdgePadTestOtherApp";
const OTHER_ENV: &str = "EDGEPAD_TEST_OTHER_APP";

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

/// The executable name of this test process, as a profile names it.
fn me() -> String {
    std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned()
}

/// Start the other app and return it with its window.
fn other_app_window() -> (Running, HWND) {
    let dir = std::env::temp_dir().join(format!("edgepad-other-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe: PathBuf = dir.join("edgepad-other-app.exe");
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

/// A window of this process, to bring to the front.
fn my_window() -> TestWindow {
    let m = primary_monitor();
    let r = RECT { left: m.left + 400, top: m.top + 350, right: m.left + 700, bottom: m.top + 550 };
    test_window("EdgePadTestMyApp", 0, r)
}

/// How many keys tall the left panel of `app` is now; 0 without one.
fn left_keys(app: &Running) -> i32 {
    let m = primary_monitor();
    panels_now(app).iter().find(|r| r.left == m.left).map_or(0, |r| (r.bottom - r.top) / key_px())
}

/// Wait up to 3 s for the left panel of `app` to be `keys` keys tall.
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

/// Default: two keys on the left. This process: `mine` in its profile.
fn settings(mine: &str) -> String {
    format!(
        "[set.two]\nkey = Ctrl | modifier | Ctrl\nkey = A | hold | A\n\
         [set.three]\nkey = X | hold | X\nkey = Y | hold | Y\nkey = Z | hold | Z\n\
         [profile.default]\nleft = two\n\
         [profile.me]\nprocess = {}\n{mine}",
        me()
    )
}

// AC1
#[test]
fn the_panels_follow_the_profile_of_the_app_in_front() {
    let _desktop = desktop();
    let mine = my_window();
    click_to_front(mine.0);
    let app = launch_with(Some(&settings("left = three\n")), None);
    wait_for_left_keys(&app, 3, "started with this app in front");

    let (_other, other) = other_app_window();
    click_to_front(other);
    wait_for_left_keys(&app, 2, "another app in front: the default profile");

    click_to_front(mine.0);
    wait_for_left_keys(&app, 3, "this app in front again");
}

// AC2
#[test]
fn a_profile_naming_no_edges_shows_no_panel() {
    let _desktop = desktop();
    let (_other, other) = other_app_window();
    click_to_front(other);
    let app = launch_with(Some(&settings("")), None);
    wait_for_left_keys(&app, 2, "the default profile");

    let mine = my_window();
    click_to_front(mine.0);
    wait_for_left_keys(&app, 0, "this app's profile shows nothing");
    assert_eq!(panels_now(&app).len(), 0, "no panel on any edge");

    click_to_front(other);
    wait_for_left_keys(&app, 2, "back to the default profile");
}

// AC3
#[test]
fn switching_profiles_lets_go_of_held_and_latched_keys_but_staying_in_one_does_not() {
    let _desktop = desktop();
    // This process is not named by any profile: both apps get the default.
    let text = settings("").replace(&format!("process = {}", me()), "process = nothing-here.exe");
    let (_other, other) = other_app_window();
    click_to_front(other);
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, key_centre(panel, 1));
    touch.hold(300);
    let mine = my_window();
    click_to_front(mine.0);
    pump(300);
    assert_eq!(log.keys(), [(b'A' as u16, false)], "same profile: A stays down");
    touch.up(1);
    assert_eq!(log.keys()[1..], [(b'A' as u16, true)]);

    // Now with this process named: switching lets go.
    drop(app);
    let app = launch_with(Some(&settings("left = three\n")), None);
    click_to_front(other);
    wait_for_left_keys(&app, 2, "the default profile");
    let panel = left_panel(&app);
    let before = log.keys().len();
    touch.down(1, key_centre(panel, 0));
    touch.up(1);
    touch.down(1, key_centre(panel, 1));
    touch.hold(300);
    assert_eq!(log.keys()[before..], [(VK_LCONTROL, false), (b'A' as u16, false)], "Ctrl latched, A held");

    click_to_front(mine.0);
    wait_for_left_keys(&app, 3, "this app's profile");
    let mut released = log.keys()[before + 2..].to_vec();
    released.sort();
    assert_eq!(released, [(b'A' as u16, true), (VK_LCONTROL, true)], "both let go on the switch");
    touch.up(1);
    pump(200);
    assert_eq!(log.keys().len(), before + 4, "lifting the finger afterwards sends nothing");
}

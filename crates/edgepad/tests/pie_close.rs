//! Acceptance tests for Issue #74: a pie menu closes, running nothing, when
//! its panel goes or is built anew, when the screen locks or the machine
//! sleeps, and when EdgePad ends (spec 4.6).
//!
//! Each way is tried on a menu held open by the finger on the pie key, slid
//! onto an item, and on one a tap left open. Profiles follow the app in
//! front as in tests/pin.rs: this test process and a copy of it under
//! another name take turns in the foreground.

mod common;

use std::process::{Command, ExitStatus};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const PIE_CLASS: &str = "EdgePadPie";
const OTHER_CLASS: &str = "EdgePadTestPieCloseOther";
const OTHER_ENV: &str = "EDGEPAD_TEST_PIE_CLOSE_OTHER";

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

/// Start the other app, a new one each time, and return it once its window
/// is in front.
fn other_app_window() -> Running {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("edgepad-pie-close-other-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("edgepad-pie-close-other.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).expect("copy the test binary");
    let child = Command::new(&exe)
        .args(["other_app", "--exact", "--ignored", "--nocapture"])
        .env(OTHER_ENV, "1")
        .spawn()
        .expect("start the other app");
    let other = Running(child, Some(dir));
    let hwnd = find_window(&other, OTHER_CLASS, true);
    wait_in_front(hwnd);
    other
}

/// A new window of this process, once it is in front.
fn my_window() -> TestWindow {
    let m = primary_monitor();
    let r = RECT { left: m.left + 400, top: m.top + 350, right: m.left + 700, bottom: m.top + 550 };
    let window = test_window("EdgePadTestPieCloseMine", 0, r);
    wait_in_front(window.0);
    window
}

/// The pie key and A on the left, in every profile: "me" when this process
/// is in front, the default otherwise. The pie opens B and C.
fn settings() -> String {
    format!(
        "[general]\nhotkey = Ctrl+Alt+E\n[set.pad]\nkey = 変形 | pie | menu\nkey = A | shortcut | A\n\
         [set.menu]\nkey = B | shortcut | B\nkey = C | shortcut | C\n\
         [profile.default]\nleft = pad\n[profile.me]\nprocess = {}\nleft = pad\n",
        me()
    )
}

/// The pie menu window of `app` now, without waiting for one.
fn pie_window(app: &Running) -> Option<HWND> {
    let name = wide(PIE_CLASS);
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() {
            return Some(hwnd);
        }
    }
}

/// Wait up to 3 s for `done`: a debug build of EdgePad takes a while to
/// draw a menu, and handles what comes after once done.
fn within_3s(mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while !done() {
        if Instant::now() > end {
            return false;
        }
        pump(20);
    }
    true
}

fn opened(app: &Running) -> bool {
    within_3s(|| pie_window(app).is_some())
}

fn closed(app: &Running) -> bool {
    within_3s(|| pie_window(app).is_none())
}

/// The pie key of `app`'s left panel.
fn pie_key(app: &Running) -> POINT {
    key_centre(left_panel(app), 0)
}

/// Toward item 0 (B), two keys out from `p`: up and in on the left edge.
fn on_b(p: POINT) -> POINT {
    let k = key_px() as f64;
    let a = (-45.0f64).to_radians();
    POINT { x: p.x + (2.0 * k * a.cos()).round() as i32, y: p.y + (2.0 * k * a.sin()).round() as i32 }
}

/// Open the menu of `app` with a finger held on the pie key and slid onto
/// B, run `close` (which must close it), then lift the finger: B is never
/// sent. Then the same with a menu a tap left open.
fn closes_on(app: &Running, how: &str, mut close: impl FnMut()) {
    let log = KeyLog::start();
    let mut touch = Touch::new();

    let p = pie_key(app);
    touch.down(1, p);
    touch.move_to(1, on_b(p));
    assert!(opened(app), "{how}: precondition, held open on B");
    close();
    assert!(closed(app), "{how}: a held menu closes");
    touch.up(1);
    pump(300);
    assert_eq!(log.keys(), [], "{how}: lifting the finger afterwards runs nothing");
    drop(touch);
}

/// As `closes_on`, for a menu a tap left open.
fn tapped_closes_on(app: &Running, how: &str, mut close: impl FnMut()) {
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let p = pie_key(app);
    touch.down(1, p);
    touch.up(1);
    assert!(opened(app), "{how}: precondition, left open by a tap");
    close();
    assert!(closed(app), "{how}: a menu left open closes");
    pump(300);
    assert_eq!(log.keys(), [], "{how}: and runs nothing");
}

/// Press and release Ctrl+Alt+E as a physical keyboard would.
fn hotkey() {
    for (vk, down) in [(VK_LCONTROL, true), (VK_LMENU, true), (b'E' as u16, true), (b'E' as u16, false), (VK_LMENU, false), (VK_LCONTROL, false)] {
        keyboard(vk, down);
    }
    pump(200);
}

fn shown(app: &Running) -> bool {
    within_3s(|| !panels_now(app).is_empty())
}

// AC1
#[test]
fn hiding_the_panel_closes_the_menu() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    for (how, toggle) in [("the tray's edge item", "左辺"), ("pause", "一時停止")] {
        closes_on(&app, how, || choose(&app, toggle));
        choose(&app, toggle);
        assert!(shown(&app), "{how}: shown again");
        tapped_closes_on(&app, how, || choose(&app, toggle));
        choose(&app, toggle);
        assert!(shown(&app), "{how}: shown again");
    }
    closes_on(&app, "the hotkey", hotkey);
    hotkey();
    assert!(shown(&app), "the hotkey: shown again");
    tapped_closes_on(&app, "the hotkey", hotkey);
}

// AC2
#[test]
fn building_the_panels_anew_closes_the_menu() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let save = |n: usize| {
        let text = format!("{}; saved {n}\n", settings());
        std::fs::write(exe_dir(&app).join("edgepad.ini"), text).expect("save edgepad.ini");
    };
    closes_on(&app, "reloading the settings", || save(1));
    tapped_closes_on(&app, "reloading the settings", || save(2));

    closes_on(&app, "pinning the profile", || choose(&app, "固定"));
    choose(&app, "固定");
    tapped_closes_on(&app, "pinning the profile", || choose(&app, "固定"));
    choose(&app, "固定");

    // This process in front: profile "me". Another app: the default.
    let mut keep = vec![];
    let _mine = my_window();
    closes_on(&app, "another app in front", || keep.push(other_app_window()));
    let _mine = my_window();
    tapped_closes_on(&app, "another app in front", || keep.push(other_app_window()));
}

// AC3
#[test]
fn locking_switching_users_and_sleeping_close_the_menu() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let tray = find_window(&app, TRAY_CLASS, false);
    let send = |msg: u32, wp: u32| move || unsafe {
        SendMessageW(tray, msg, wp as usize, 0);
    };
    for (how, msg, wp) in [
        ("locking", WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK),
        ("switching users", WM_WTSSESSION_CHANGE, WTS_CONSOLE_DISCONNECT),
        ("sleeping", WM_POWERBROADCAST, PBT_APMSUSPEND),
    ] {
        closes_on(&app, how, send(msg, wp));
        tapped_closes_on(&app, how, send(msg, wp));
    }
}

fn wait_exit(app: &mut Running) -> ExitStatus {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if let Some(status) = app.0.try_wait().expect("try_wait") {
            return status;
        }
        pump(50);
    }
    panic!("edgepad.exe still running 5s after being told to exit");
}

// AC4
#[test]
fn ending_edgepad_with_a_menu_open_runs_nothing() {
    let _desktop = desktop();
    let log = KeyLog::start();

    // From the tray, a finger on B.
    let mut app = launch_with(Some(&settings()), None);
    let mut touch = Touch::new();
    let p = pie_key(&app);
    touch.down(1, p);
    touch.move_to(1, on_b(p));
    assert!(opened(&app), "precondition: held open on B");
    choose(&app, "終了");
    wait_exit(&mut app);
    touch.up(1);
    pump(300);
    assert_eq!(log.keys(), [], "ended from the tray: B is not sent");
    drop(touch);

    // On logoff, a menu a tap left open.
    let mut app = launch_with(Some(&settings()), None);
    let tray = find_window(&app, TRAY_CLASS, false);
    let mut touch = Touch::new();
    let p = pie_key(&app);
    touch.down(1, p);
    touch.up(1);
    assert!(opened(&app), "precondition: left open by a tap");
    unsafe {
        SendMessageW(tray, WM_QUERYENDSESSION, 0, ENDSESSION_LOGOFF as isize);
        SendMessageW(tray, WM_ENDSESSION, 1, ENDSESSION_LOGOFF as isize);
    }
    wait_exit(&mut app);
    pump(300);
    assert_eq!(log.keys(), [], "ended on logoff: nothing is sent");
}

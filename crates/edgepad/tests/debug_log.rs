//! Acceptance tests for Issue #36: with --debug, and only then, EdgePad
//! writes a text log beside the exe.

mod common;

use std::time::{Duration, Instant};

use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK};
use common::*;

const LOG: &str = "edgepad-debug.log";

fn log_lines(app: &Running) -> Vec<String> {
    std::fs::read_to_string(exe_dir(app).join(LOG)).unwrap_or_default().lines().map(String::from).collect()
}

/// Wait up to a second for a log line containing `text`.
fn logged(app: &Running, text: &str) -> bool {
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        if log_lines(app).iter().any(|l| l.contains(text)) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        pump(20);
    }
}

/// `2026-10-22 23:59:01.123 ` at the start of a line.
fn has_time(line: &str) -> bool {
    let b = line.as_bytes();
    let digits = |r: std::ops::Range<usize>| r.into_iter().all(|i| b.get(i).is_some_and(u8::is_ascii_digit));
    b.len() > 24
        && digits(0..4) && b[4] == b'-' && digits(5..7) && b[7] == b'-' && digits(8..10) && b[10] == b' '
        && digits(11..13) && b[13] == b':' && digits(14..16) && b[16] == b':' && digits(17..19) && b[19] == b'.'
        && digits(20..23) && b[23] == b' '
}

const SETTINGS: &str = "\
[general]
hotkey = Ctrl+Alt+F13
[set.pad]
key = A | hold | A
key = B | hold | B
[profile.default]
left = pad
";

// AC1
#[test]
fn without_debug_no_log_is_written() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    find_panel(&app);
    pump(500);
    assert!(!exe_dir(&app).join(LOG).exists(), "no log beside the exe");
    assert!(!appdata_dir(&app).join("EdgePad").join(LOG).exists(), "nor under %APPDATA%");
    drop(app);

    // The same, with --debug: it is the switch that makes the log.
    let app = launch_with_args(Some(SETTINGS), None, &["--debug"]);
    find_panel(&app);
    assert!(exe_dir(&app).join(LOG).exists(), "with --debug, a log beside the exe");
}

// AC2
#[test]
fn with_debug_a_fresh_timestamped_log_starts_with_version_and_settings_and_ends_with_exit() {
    let _desktop = desktop();
    let mut app = launch_with_args(Some(SETTINGS), None, &["--debug"]);
    find_panel(&app);
    let lines = log_lines(&app);
    let first = lines.first().expect("a log beside the exe");
    assert!(lines.iter().all(|l| has_time(l)), "every line starts with the time: {lines:?}");
    assert!(first.contains(&format!("EdgePad {}", env!("CARGO_PKG_VERSION"))), "the version: {first}");
    let ini = exe_dir(&app).join("edgepad.ini");
    assert!(first.contains(&ini.display().to_string()), "the settings file in use: {first}");

    choose(&app, "終了");
    let end = Instant::now() + Duration::from_secs(5);
    while app.0.try_wait().unwrap().is_none() && Instant::now() < end {
        pump(50);
    }
    let lines = log_lines(&app);
    assert!(lines.last().is_some_and(|l| l.ends_with("終了")), "the last line records the exit: {lines:?}");

    // Started again, the log begins anew.
    let exe = exe_dir(&app).join("edgepad.exe");
    let appdata = appdata_dir(&app);
    let mut again = std::process::Command::new(&exe).arg("--debug").env("APPDATA", &appdata).spawn().unwrap();
    pump(1500);
    let lines = log_lines(&app);
    let _ = again.kill();
    let _ = again.wait();
    assert_eq!(lines.iter().filter(|l| l.contains("起動")).count(), 1, "one start, the old log gone: {lines:?}");
    assert!(!lines.iter().any(|l| l.ends_with("終了")), "no exit line from the run before: {lines:?}");
}

// AC3
#[test]
fn with_debug_settings_panels_hotkey_keys_and_releases_are_logged() {
    let _desktop = desktop();
    let app = launch_with_args(Some(SETTINGS), None, &["--debug"]);
    let panel = left_panel(&app);
    assert!(logged(&app, "パネル 1 枚"), "the settings read: {:?}", log_lines(&app));
    assert!(logged(&app, "ホットキー Ctrl+Alt+F13"), "the hotkey registration: {:?}", log_lines(&app));

    let _keys = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, 0));
    touch.up(1);
    assert!(logged(&app, "送信 0x41 押す") && logged(&app, "送信 0x41 離す"), "keys sent: {:?}", log_lines(&app));

    touch.down(1, key_centre(panel, 1));
    touch.hold(100);
    let tray = find_window(&app, TRAY_CLASS, false);
    unsafe { SendMessageW(tray, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize, 0) };
    assert!(logged(&app, "全キー解放: 画面ロック"), "the release on lock: {:?}", log_lines(&app));
    touch.up(1);

    choose(&app, "左辺");
    assert!(logged(&app, "パネル 左辺: 非表示"), "the panel hidden: {:?}", log_lines(&app));

    std::fs::write(exe_dir(&app).join("edgepad.ini"), "[set.pad]\nkey = A | nosuchkind | A\n[profile.default]\nleft = pad\n").unwrap();
    assert!(logged(&app, "2 行目"), "the reload error with its line: {:?}", log_lines(&app));
}

// AC4
#[test]
fn writing_the_log_neither_reloads_the_settings_nor_keeps_the_cpu_busy() {
    let _desktop = desktop();
    let app = launch_with_args(Some(SETTINGS), None, &["--debug"]);
    let panel = left_panel(&app);
    let _keys = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, 0));
    touch.up(1);
    drop(touch);
    assert!(logged(&app, "送信 0x41 離す"), "the presses were logged: {:?}", log_lines(&app));
    pump(1000);
    let lines = log_lines(&app);
    assert!(!lines.iter().any(|l| l.contains("再読込")), "log writes caused a reload: {lines:?}");

    let before = cpu_time_100ns(&app);
    std::thread::sleep(Duration::from_secs(3));
    let used_ms = (cpu_time_100ns(&app) - before) as f64 / 10_000.0;
    assert!(used_ms < 10.0, "CPU {used_ms:.1}ms in 3s with --debug");
    assert_eq!(log_lines(&app).len(), lines.len(), "nothing logged while idle");
}

//! Acceptance tests for the notice that keys cannot reach an app running as
//! administrator (roadmap v0.3, spec 6.4).
//!
//! The test machine cannot be relied on to run an app at a higher integrity
//! level than EdgePad. The test build therefore takes a message
//! (EdgePadTestElevated, wParam 1 or 0) that makes it treat every app coming
//! to the front as out of reach, or stop doing so; everything after that
//! decision runs as it does for a real elevated app.

mod common;

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
use windows_sys::w;
use common::*;

fn me() -> String {
    std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned()
}

fn notices(app: &Running) -> Vec<String> {
    test_events(app).into_iter().filter(|e| e.starts_with("notice: ")).collect()
}

fn window(class: &str, top: i32) -> TestWindow {
    let m = primary_monitor();
    test_window(class, 0, RECT { left: m.left + 400, top: m.top + top, right: m.left + 700, bottom: m.top + top + 150 })
}

// AC1
#[test]
fn pressing_a_key_with_an_app_out_of_reach_in_front_notifies_once_per_time_it_comes_to_the_front() {
    let _desktop = desktop();
    let app = launch_with(Some("[set.pad]\nkey = A | hold | A\n[profile.default]\nleft = pad\n"), None);
    let panel = left_panel(&app);
    let tray = find_window(&app, TRAY_CLASS, false);
    let elevated = unsafe { RegisterWindowMessageW(w!("EdgePadTestElevated")) };
    let (first, second) = (window("EdgePadTestFirst", 100), window("EdgePadTestSecond", 300));
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let tap = |touch: &mut Touch| {
        touch.down(1, key_centre(panel, 0));
        touch.up(1);
        pump(100);
    };

    unsafe { PostMessageW(tray, elevated, 1, 0) };
    pump(100);
    click_to_front(first.0);
    pump(200);
    assert_eq!(notices(&app), Vec::<String>::new(), "nothing until a key is pressed");
    tap(&mut touch);
    let got = notices(&app);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("管理者") && got[0].contains(&me()), "says why, and which app: {got:?}");
    assert_eq!(log.keys().len(), 2, "the keys are still sent");

    tap(&mut touch);
    assert_eq!(notices(&app).len(), 1, "once while the app stays in front");

    click_to_front(second.0);
    pump(200);
    tap(&mut touch);
    assert_eq!(notices(&app).len(), 2, "again for the next app out of reach");

    unsafe { PostMessageW(tray, elevated, 0, 0) };
    pump(100);
    click_to_front(first.0);
    pump(200);
    tap(&mut touch);
    assert_eq!(notices(&app).len(), 2, "an app within reach: no notice");
}

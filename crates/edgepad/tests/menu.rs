//! Acceptance tests for Issue #26: the full tray menu, error notifications
//! in the tray, and a single running instance.
//!
//! The test build records each notification and each file it would open in
//! %APPDATA%\edgepad-test-events.txt (`test_events`) instead of only showing
//! a balloon or starting an editor.

mod common;

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
use windows_sys::w;
use common::*;

/// A settings file with `n` hold keys (A, B, ...) on each of `edges`.
fn keys_on(edges: &[&str], n: usize) -> String {
    let mut text = String::from("[set.pad]\n");
    for c in (b'A'..).take(n) {
        let c = c as char;
        text += &format!("key = {c} | hold | {c}\n");
    }
    text += "[profile.default]\n";
    for edge in edges {
        text += &format!("{edge} = pad\n");
    }
    text
}

fn save(app: &Running, text: &str) {
    std::fs::write(exe_dir(app).join("edgepad.ini"), text).expect("save edgepad.ini");
}

/// Which edges of the primary monitor have a visible panel, as "left" etc.
fn shown(app: &Running) -> Vec<&'static str> {
    let m = primary_monitor();
    let mut edges: Vec<&str> = panels_now(app)
        .iter()
        .map(|r| {
            let (tall, wide) = (r.bottom - r.top > r.right - r.left, r.right - r.left > r.bottom - r.top);
            match () {
                _ if tall && r.left == m.left => "left",
                _ if tall && r.right == m.right => "right",
                _ if wide && r.top == m.top => "top",
                _ if wide && r.bottom == m.bottom => "bottom",
                _ => "elsewhere",
            }
        })
        .collect();
    edges.sort();
    edges
}

fn panel_on(app: &Running, edge: &str) -> RECT {
    let m = primary_monitor();
    *panels_now(app)
        .iter()
        .find(|r| match edge {
            "left" => r.left == m.left && r.bottom - r.top > r.right - r.left,
            _ => r.right == m.right && r.bottom - r.top > r.right - r.left,
        })
        .unwrap_or_else(|| panic!("no {edge} panel"))
}

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

fn notices(app: &Running) -> Vec<String> {
    test_events(app).into_iter().filter(|e| e.starts_with("notice: ")).collect()
}

const A: u16 = b'A' as u16;

// KeyLog records (virtual key, is up): `d` is a key-down, `u` a key-up.
const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

// AC1
#[test]
fn the_tray_menu_lists_every_item_and_greys_edges_without_a_panel() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on(&["left"], 2)), None);
    find_panel(&app);

    let items = tray_menu(&app);
    let wanted = ["左辺", "右辺", "上辺", "下辺", "設定ファイルを開く", "設定を再読込", "一時停止", "全キー解放", "自動起動", "終了"];
    let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(items.len(), wanted.len(), "items: {texts:?}");
    for (item, want) in items.iter().zip(wanted) {
        assert!(item.text.contains(want), "{want} expected, found {texts:?}");
    }
    let edge = |name: &str| items.iter().find(|i| i.text.contains(name)).unwrap().clone();
    assert!(edge("左辺").checked && !edge("左辺").grayed, "the left edge is shown and can be chosen");
    for name in ["右辺", "上辺", "下辺"] {
        assert!(edge(name).grayed && !edge(name).checked, "{name} has no panel: greyed");
    }
}

// AC2
#[test]
fn an_edge_item_hides_and_shows_its_panel_and_survives_a_reload_but_not_a_restart() {
    let _desktop = desktop();
    let text = keys_on(&["left", "right"], 2);
    let app = launch_with(Some(&text), None);
    find_panel(&app);
    assert_eq!(shown(&app), ["left", "right"]);

    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel_on(&app, "left"), 0));
    touch.hold(150);
    choose(&app, "左辺");
    assert_eq!(shown(&app), ["right"], "the left panel is hidden");
    assert_eq!(log.keys(), [d(A), u(A)], "the key held on it is released");
    assert!(!menu_item(&app, "左辺").checked, "no check while hidden");
    touch.up(1);

    save(&app, &keys_on(&["left", "right"], 3));
    assert!(within(1000, || panels_now(&app).iter().any(|r| r.bottom - r.top == 3 * key_px())), "reloaded");
    pump(300);
    assert_eq!(shown(&app), ["right"], "still hidden after the reload");

    choose(&app, "左辺");
    assert_eq!(shown(&app), ["left", "right"], "shown again");
    assert!(menu_item(&app, "左辺").checked);

    choose(&app, "左辺");
    drop(app);
    let app = launch_with(Some(&text), None);
    find_panel(&app);
    assert_eq!(shown(&app), ["left", "right"], "a restart goes back to the settings file");
}

// AC3
#[test]
fn opening_the_settings_file_opens_the_one_in_use_or_creates_a_template_beside_the_exe() {
    let _desktop = desktop();

    let app = launch_with(Some(&keys_on(&["left"], 2)), None);
    find_panel(&app);
    choose(&app, "設定ファイルを開く");
    let ini = exe_dir(&app).join("edgepad.ini");
    assert!(test_events(&app).contains(&format!("open: {}", ini.display())), "events: {:?}", test_events(&app));
    drop(app);

    let app = launch();
    find_panel(&app);
    choose(&app, "設定ファイルを開く");
    let ini = exe_dir(&app).join("edgepad.ini");
    assert!(ini.is_file(), "a template is created beside the exe");
    assert!(test_events(&app).contains(&format!("open: {}", ini.display())), "events: {:?}", test_events(&app));
    pump(800);
    assert!(notices(&app).is_empty(), "the template reads without errors: {:?}", notices(&app));
    let r = left_panel(&app);
    assert_eq!((r.bottom - r.top) / key_px(), KEY_COUNT, "the template makes the same panel as the fixed keys");
}

// AC4
#[test]
fn reloading_from_the_menu_reads_the_file_again_even_when_unchanged() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on(&["left"], 2)), None);
    find_panel(&app);

    save(&app, "[set.pad]\nkey = A | nosuchkind | A\n[profile.default]\nleft = pad\n");
    assert!(within(1000, || notices(&app).len() == 1), "the saved error is reported: {:?}", notices(&app));
    choose(&app, "設定を再読込");
    assert!(within(1000, || notices(&app).len() == 2), "reloading again reports it again: {:?}", notices(&app));
    let r = left_panel(&app);
    assert_eq!((r.bottom - r.top) / key_px(), 2, "the panel stays as it was");
}

// AC5
#[test]
fn pausing_hides_every_panel_until_it_is_chosen_again() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on(&["left", "right"], 2)), None);
    find_panel(&app);
    choose(&app, "左辺");
    assert_eq!(shown(&app), ["right"]);

    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel_on(&app, "right"), 0));
    touch.hold(150);
    choose(&app, "一時停止");
    assert_eq!(shown(&app), Vec::<&str>::new(), "every panel is hidden");
    assert_eq!(log.keys(), [d(A), u(A)], "the held key is released");
    assert!(menu_item(&app, "一時停止").checked);
    touch.up(1);

    assert_eq!(launch_again(&app).code(), Some(0));
    assert_eq!(shown(&app), Vec::<&str>::new(), "a second launch shows nothing while paused");

    choose(&app, "一時停止");
    assert_eq!(shown(&app), ["right"], "back to how it was before the pause");
    assert!(!menu_item(&app, "一時停止").checked);
}

// AC6
#[test]
fn releasing_every_key_from_the_menu_lets_go_of_held_keys() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on(&["left"], 2)), None);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(left_panel(&app), 0));
    touch.hold(150);

    choose(&app, "全キー解放");
    assert_eq!(log.keys(), [d(A), u(A)], "A is released");
    touch.up(1);
    pump(200);
    assert_eq!(log.keys(), [d(A), u(A)], "lifting the finger afterwards sends nothing more");
}

// AC7
#[test]
fn the_autostart_item_creates_and_removes_a_startup_shortcut_to_this_exe() {
    let _desktop = desktop();
    let app = launch();
    find_panel(&app);
    let lnk = appdata_dir(&app).join(r"Microsoft\Windows\Start Menu\Programs\Startup\EdgePad.lnk");
    assert!(!menu_item(&app, "自動起動").checked);

    choose(&app, "自動起動");
    let bytes = std::fs::read(&lnk).expect("EdgePad.lnk in the startup folder");
    let exe = exe_dir(&app).join("edgepad.exe").display().to_string();
    let utf16: Vec<u8> = exe.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    assert!(has(exe.as_bytes()) || has(&utf16), "the shortcut points at {exe}");
    assert!(menu_item(&app, "自動起動").checked);

    choose(&app, "自動起動");
    assert!(!lnk.exists(), "chosen again, the shortcut is removed");
    assert!(!menu_item(&app, "自動起動").checked);
}

// AC8
#[test]
fn settings_errors_are_notified_with_line_and_reason_at_start_and_on_reload() {
    let _desktop = desktop();
    let broken = "[set.pad]\nkey = A | nosuchkind | A\nkey = B | nosuchkind | B\n[profile.default]\nleft = pad\n";
    let app = launch_with(Some(broken), None);
    find_panel(&app);
    assert!(within(1000, || notices(&app).len() == 1), "notified at start: {:?}", notices(&app));
    let first = &notices(&app)[0];
    assert!(first.contains("edgepad.ini の 2 行目") && first.contains("nosuchkind"), "line and reason: {first}");
    assert!(first.contains("ほか 1 件"), "the rest are counted: {first}");

    save(&app, "[set.pad]\nkey = A | hold | A\n\nkey = B | nosuchkind | B\n[profile.default]\nleft = pad\n");
    assert!(within(1000, || notices(&app).len() == 2), "notified on reload: {:?}", notices(&app));
    let second = &notices(&app)[1];
    assert!(second.contains("4 行目") && !second.contains("ほか"), "one error, on line 4: {second}");
}

// AC9
#[test]
fn a_failed_send_is_notified_once_until_a_send_succeeds() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on(&["left"], 2)), None);
    let panel = left_panel(&app);
    let tray = find_window(&app, TRAY_CLASS, false);
    let fail = unsafe { RegisterWindowMessageW(w!("EdgePadTestSendFail")) };
    let _log = KeyLog::start();
    let mut touch = Touch::new();
    let tap = |touch: &mut Touch| {
        touch.down(1, key_centre(panel, 0));
        touch.up(1);
        pump(100);
    };

    unsafe { PostMessageW(tray, fail, 1, 0) };
    pump(100);
    tap(&mut touch);
    tap(&mut touch);
    assert_eq!(notices(&app).len(), 1, "one notice for repeated failures: {:?}", notices(&app));

    unsafe { PostMessageW(tray, fail, 0, 0) };
    pump(100);
    tap(&mut touch);
    unsafe { PostMessageW(tray, fail, 1, 0) };
    pump(100);
    tap(&mut touch);
    assert_eq!(notices(&app).len(), 2, "after a success, a failure is notified again: {:?}", notices(&app));
}

// AC10
#[test]
fn a_second_launch_toggles_every_edge_of_the_running_one_and_exits_zero() {
    let _desktop = desktop();
    let app = launch_with(Some(&keys_on(&["left", "right"], 2)), None);
    find_panel(&app);
    choose(&app, "左辺");
    assert_eq!(shown(&app), ["right"]);

    assert_eq!(launch_again(&app).code(), Some(0), "the second launch exits 0");
    assert_eq!(shown(&app), Vec::<&str>::new(), "one edge was shown: every edge is hidden");
    assert_eq!(launch_again(&app).code(), Some(0));
    assert_eq!(shown(&app), ["left", "right"], "all hidden: every edge with a panel is shown");
}

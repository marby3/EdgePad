//! Acceptance tests for Issue #81: the settings window (spec 7.1), opened
//! from the tray, saving, a file changed outside, and closing with changes
//! not saved; as its parts are since Issue #86, Windows's own controls.
//! The sidebar, the bottom row, the theme and the keyboard have tests of
//! their own (settings_window.rs).
//!
//! A change is made with a message only the test build has
//! ("EdgePadTestEditorSet"): it sets `[general] opacity` in the window's
//! copy of the file past any check, so a change the parser turns away can
//! be saved too. A debug build is slow to draw and to act on a click: what
//! a test checks, it waits for.

mod common;

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::editor::*;
use common::*;

/// Two hold keys on the left by default; three for this process, in front.
fn settings(general: &str) -> String {
    format!(
        "; 手で書いた設定
[general]
opacity = 90   ; 通常表示
{general}
         [set.two]
key = A | hold | A
key = B | hold | B
         [set.three]
key = A | hold | A
key = B | hold | B
key = C | hold | C
         [profile.default]
left = two
[profile.me]
process = {}
left = three
",
        std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy()
    )
}

// AC1
#[test]
fn the_tray_opens_one_window_on_the_taskbar_and_brings_it_back() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    let editor = open(&app, id);

    let ex = unsafe { GetWindowLongW(editor, GWL_EXSTYLE) } as u32;
    assert!(ex & WS_EX_TOOLWINDOW == 0, "not a tool window: on the taskbar and in Alt+Tab");
    assert!(unsafe { GetWindow(editor, GW_OWNER) }.is_null(), "owned by no window");
    assert!(ex & WS_EX_NOACTIVATE == 0, "a window that takes the focus, unlike the panels");

    unsafe { ShowWindow(editor, SW_MINIMIZE) };
    pump(300);
    let again = open(&app, id);
    assert_eq!(again, editor, "the same window");
    assert_eq!(editors(&app).len(), 1, "one window only");
    assert!(within_3s(|| unsafe { IsIconic(editor) } == 0), "brought back from the taskbar");
}

// AC3
#[test]
fn saving_checks_with_the_parser_and_writes_only_what_reads() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app, open_id(&app));
    let before = file(&app);

    change(editor, 150);
    click_button(editor, "保存");
    let _ = within_3s(|| !status(editor).is_empty());
    let said = status(editor);
    assert!(said.contains("3 行目") && said.contains("150"), "the line and the reason: {said:?}");
    assert_eq!(file(&app), before, "nothing written");

    change(editor, 70);
    click_button(editor, "保存");
    let want = before.replace("opacity = 90   ;", "opacity = 70   ;");
    let _ = within_3s(|| file(&app) == want);
    assert_eq!(file(&app), want, "written, the rest as it was");
}

// AC4
#[test]
fn a_file_changed_outside_is_told_and_its_own_save_is_not() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app, open_id(&app));
    let path = exe_dir(&app).join("edgepad.ini");

    // Its own save: no notice once the folder settles.
    change(editor, 80);
    click_button(editor, "保存");
    pump(1000);
    assert!(child(editor, "Button", "読み直す").is_none(), "its own save is not a change outside");

    // Outside, then read again.
    std::fs::write(&path, settings("theme = dark")).unwrap();
    assert!(within_3s(|| child(editor, "Button", "読み直す").is_some()), "told of the change");
    assert!(within_3s(|| child(editor, "Button", "画面の内容を保つ").is_some()), "with a choice");
    click_button(editor, "読み直す");
    assert!(within_3s(|| child(editor, "Button", "読み直す").is_none()), "answered");
    change(editor, 60);
    click_button(editor, "保存");
    assert!(file(&app).contains("theme = dark"), "the window took up the file as changed: {}", file(&app));

    // Outside, and kept: saving then writes the window's own.
    std::fs::write(&path, settings("theme = light")).unwrap();
    assert!(within_3s(|| child(editor, "Button", "画面の内容を保つ").is_some()));
    click_button(editor, "画面の内容を保つ");
    click_button(editor, "保存");
    let _ = within_3s(|| file(&app).contains("theme = dark"));
    let saved = file(&app);
    assert!(saved.contains("theme = dark") && saved.contains("opacity = 60"), "the window's, not the change outside: {saved}");

    // Outside, and broken (not UTF-8): it cannot be read again; the reason
    // shows, the question stays, and the window keeps its own.
    std::fs::write(&path, b"[\xFF\xFE]\n").unwrap();
    assert!(within_3s(|| child(editor, "Button", "読み直す").is_some()), "told of the change");
    click_button(editor, "読み直す");
    let _ = within_3s(|| status(editor).contains("読み直せません"));
    assert!(status(editor).contains("読み直せません"), "the reason: {:?}", status(editor));
    assert!(child(editor, "Button", "画面の内容を保つ").is_some(), "the question stays");
    click_button(editor, "画面の内容を保つ");
    click_button(editor, "保存");
    let _ = within_3s(|| file(&app).contains("opacity = 60"));
    assert!(file(&app).contains("theme = dark") && file(&app).contains("opacity = 60"), "the window's own, saved over it");
}

// AC5
#[test]
fn closing_with_changes_asks_whether_to_save_them() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    let before = file(&app);

    let editor = open(&app, id);
    change(editor, 50);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, "Button", "キャンセル").is_some()), "asked");
    assert!(child(editor, "Button", "保存する").is_some() && child(editor, "Button", "保存せずに閉じる").is_some());
    click_button(editor, "キャンセル");
    assert!(unsafe { IsWindow(editor) } != 0, "cancelled: still open");

    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, "Button", "保存せずに閉じる").is_some()));
    click_button(editor, "保存せずに閉じる");
    assert!(within_3s(|| editors(&app).is_empty()), "closed");
    assert_eq!(file(&app), before, "nothing saved");

    let editor = open(&app, id);
    change(editor, 40);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, "Button", "保存する").is_some()));
    click_button(editor, "保存する");
    assert!(within_3s(|| editors(&app).is_empty()), "saved, then closed");
    assert!(file(&app).contains("opacity = 40"), "{}", file(&app));

    // Saving what the parser cannot read: the reason shows, and the
    // question and the window stay.
    let editor = open(&app, id);
    change(editor, 150);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, "Button", "保存する").is_some()));
    click_button(editor, "保存する");
    let _ = within_3s(|| status(editor).contains("150"));
    assert!(status(editor).contains("150"), "the reason shows: {:?}", status(editor));
    assert!(unsafe { IsWindow(editor) } != 0 && child(editor, "Button", "保存せずに閉じる").is_some(), "still asked");
    click_button(editor, "保存せずに閉じる");
    assert!(within_3s(|| editors(&app).is_empty()));

    // Nothing changed: it closes without asking.
    let editor = open(&app, id);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| editors(&app).is_empty()), "nothing to ask about");

    // A change outside kept off, nothing of the window's changed: nothing
    // to ask about either.
    let path = exe_dir(&app).join("edgepad.ini");
    let editor = open(&app, id);
    std::fs::write(&path, settings("theme = dark")).unwrap();
    assert!(within_3s(|| child(editor, "Button", "画面の内容を保つ").is_some()));
    click_button(editor, "画面の内容を保つ");
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| editors(&app).is_empty()), "kept off, nothing to save: it closes");

    // Changes, and a change outside not yet answered: closing asks about
    // both; cancelled, the change outside is asked about again.
    let editor = open(&app, id);
    change(editor, 30);
    std::fs::write(&path, settings("theme = light")).unwrap();
    assert!(within_3s(|| child(editor, "Button", "読み直す").is_some()));
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, "Button", "キャンセル").is_some()), "asked about closing");
    assert!(status(editor).contains("外でも"), "and told of the change outside: {:?}", status(editor));
    click_button(editor, "キャンセル");
    assert!(within_3s(|| child(editor, "Button", "読み直す").is_some()), "asked about the change outside again");

    // The tray's exit, with changes not saved: EdgePad stays, asking.
    let exit = menu_item(&app, "終了").id;
    let tray = find_window(&app, TRAY_CLASS, false);
    unsafe { PostMessageW(tray, WM_COMMAND, exit as usize, 0) };
    pump(1000);
    assert!(editors(&app).contains(&editor), "EdgePad and the window are still there");
    assert!(child(editor, "Button", "保存せずに閉じる").is_some(), "asking about the changes");
}

/// A new window of this process, once it is in front.
fn my_window() -> TestWindow {
    let m = primary_monitor();
    let r = RECT { left: m.right - 400, top: m.top + 100, right: m.right - 100, bottom: m.top + 300 };
    let window = test_window("EdgePadTestEditorMine", 0, r);
    wait_in_front(window.0);
    window
}

fn left_keys(app: &Running) -> i32 {
    let r = left_panel(app);
    (r.bottom - r.top) / key_px()
}

// AC6
#[test]
fn in_front_the_window_switches_no_profile_and_lets_no_key_go() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    let _mine = my_window();
    assert!(within_3s(|| left_keys(&app) == 3), "precondition: this process in front, profile me");

    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(left_panel(&app), 0));
    let editor = open(&app, id);
    wait_in_front(editor);
    pump(500);
    assert_eq!(left_keys(&app), 3, "the profile stays");
    assert_eq!(log.keys(), [(b'A' as u16, false)], "A stays held");
    touch.up(1);

    // A mouse holding a key on the panel and dragged off it, a finger
    // tapping the sidebar meanwhile: the key comes up when the mouse lets
    // go, off the panel as it is, as ever; and the tap chose a page.
    let before = log.keys().len();
    let m = primary_monitor();
    let off = test_window("EdgePadTestEditorOff", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, RECT { left: m.left + 200, top: m.bottom - 200, right: m.left + 400, bottom: m.bottom - 100 });
    let b = key_centre(left_panel(&app), 1);
    mouse_move_to(b);
    mouse_down_at(b);
    pump(200);
    let away = centre(window_rect(off.0));
    mouse_move_to(away);
    pump(200);
    let item = centre(sidebar_item(editor, 2));
    touch.down(2, item);
    touch.up(2);
    mouse_up_at(away);
    let _ = within_3s(|| log.keys().len() >= before + 2);
    assert_eq!(log.keys()[before..], [(b'B' as u16, false), (b'B' as u16, true)], "B down, and up with the mouse");
    assert_eq!(heading(editor), "辺と表示", "the page tapped");
}


//! Acceptance tests for Issue #81: the settings window (spec 7.1), opened
//! from the tray, with its four tabs, saving, a file changed outside, and
//! closing with changes not saved. The tabs have no fields yet.
//!
//! The window's parts are found by class and text, and clicked as a user
//! would. Until the tabs have fields, a change is made with a message only
//! the test build has ("EdgePadTestEditorSet"): it sets `[general] opacity`
//! in the window's copy of the file. A debug build is slow to draw and to
//! act on a click: what a test checks, it waits for.

mod common;

use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;
use common::*;

const EDITOR_CLASS: &str = "EdgePadSettings";
const TAB_CLASS: &str = "EdgePadSettingsTab";
const BUTTON_CLASS: &str = "EdgePadSettingsButton";
const STATUS_CLASS: &str = "EdgePadSettingsStatus";

/// Sets `[general] opacity` to wParam (test build only).
fn test_set() -> u32 {
    unsafe { RegisterWindowMessageW(w!("EdgePadTestEditorSet")) }
}

/// Two hold keys on the left by default; three for this process, in front.
fn settings(general: &str) -> String {
    format!(
        "; 手で書いた設定\n[general]\nopacity = 90   ; 通常表示\n{general}\n\
         [set.two]\nkey = A | hold | A\nkey = B | hold | B\n\
         [set.three]\nkey = A | hold | A\nkey = B | hold | B\nkey = C | hold | C\n\
         [profile.default]\nleft = two\n[profile.me]\nprocess = {}\nleft = three\n",
        std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy()
    )
}

/// Wait up to 3 s for `done`.
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

/// The settings windows of `app` now.
fn editors(app: &Running) -> Vec<HWND> {
    let name = wide(EDITOR_CLASS);
    let mut found = Vec::new();
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
        if hwnd.is_null() {
            return found;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() {
            found.push(hwnd);
        }
    }
}

/// Choose 設定… in the tray without opening its menu, and return the
/// window once it is shown.
fn open(app: &Running, id: u32) -> HWND {
    let tray = find_window(app, TRAY_CLASS, false);
    unsafe { PostMessageW(tray, WM_COMMAND, id as usize, 0) };
    let mut shown = null_mut();
    assert!(
        within_3s(|| {
            shown = editors(app).into_iter().find(|&w| unsafe { IsWindowVisible(w) } != 0).unwrap_or(null_mut());
            !shown.is_null()
        }),
        "the settings window opens"
    );
    shown
}

fn open_id(app: &Running) -> u32 {
    menu_item(app, "設定…").id
}

/// The child of `parent` of `class` whose text is `text`, if any.
fn child(parent: HWND, class: &str, text: &str) -> Option<HWND> {
    let (class, text) = (wide(class), wide(text));
    let hwnd = unsafe { FindWindowExW(parent, null_mut(), class.as_ptr(), text.as_ptr()) };
    (!hwnd.is_null() && unsafe { IsWindowVisible(hwnd) } != 0).then_some(hwnd)
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

/// Click the child of `editor` of `class` and `text`, in front first: the
/// window opens behind whatever is in front when the tray did not have the
/// focus to give it, as the posted command does not on CI.
fn click_child(editor: HWND, class: &str, text: &str) {
    let c = child(editor, class, text).unwrap_or_else(|| panic!("no {class} 「{text}」"));
    wait_in_front(editor);
    click(centre(window_rect(c)));
    pump(300);
}

fn text_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), 512) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// What the window's status line says.
fn status(editor: HWND) -> String {
    let class = wide(STATUS_CLASS);
    let hwnd = unsafe { FindWindowExW(editor, null_mut(), class.as_ptr(), null()) };
    assert!(!hwnd.is_null(), "a status line");
    text_of(hwnd)
}

fn file(app: &Running) -> String {
    std::fs::read_to_string(exe_dir(app).join("edgepad.ini")).expect("edgepad.ini")
}

fn change(editor: HWND, opacity: usize) {
    unsafe { PostMessageW(editor, test_set(), opacity, 0) };
    pump(200);
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

// AC2
#[test]
fn four_tabs_switch() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app, open_id(&app));
    for name in ["一般", "辺", "キーセット", "プロファイル"] {
        assert!(child(editor, TAB_CLASS, name).is_some(), "a tab 「{name}」");
    }
    // Inside a tab's edge, clear of its label: its fill.
    let fill = |name: &str| {
        let r = window_rect(child(editor, TAB_CLASS, name).expect("a tab"));
        colour_at(POINT { x: r.left + 3, y: r.top + 3 })
    };
    for name in ["キーセット", "辺", "プロファイル", "一般"] {
        click_child(editor, TAB_CLASS, name);
        let want = format!("settings: tab {name}");
        assert!(within_3s(|| test_events(&app).last() == Some(&want)), "{name} is the tab shown: {:?}", test_events(&app));
        let other = if name == "一般" { "辺" } else { "一般" };
        let _ = within_3s(|| fill(name) != fill(other));
        assert_ne!(fill(name), fill(other), "{name} looks shown, unlike {other}");
    }
}

fn colour_at(p: POINT) -> [i32; 3] {
    unsafe { DwmFlush() };
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

// AC3
#[test]
fn saving_checks_with_the_parser_and_writes_only_what_reads() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app, open_id(&app));
    let before = file(&app);

    change(editor, 150);
    click_child(editor, BUTTON_CLASS, "保存");
    let _ = within_3s(|| !status(editor).is_empty());
    let said = status(editor);
    assert!(said.contains("3 行目") && said.contains("150"), "the line and the reason: {said:?}");
    assert_eq!(file(&app), before, "nothing written");

    change(editor, 70);
    click_child(editor, BUTTON_CLASS, "保存");
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
    click_child(editor, BUTTON_CLASS, "保存");
    pump(1000);
    assert!(child(editor, BUTTON_CLASS, "読み直す").is_none(), "its own save is not a change outside");

    // Outside, then read again.
    std::fs::write(&path, settings("theme = dark")).unwrap();
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "読み直す").is_some()), "told of the change");
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "画面の内容を保つ").is_some()), "with a choice");
    click_child(editor, BUTTON_CLASS, "読み直す");
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "読み直す").is_none()), "answered");
    change(editor, 60);
    click_child(editor, BUTTON_CLASS, "保存");
    assert!(file(&app).contains("theme = dark"), "the window took up the file as changed: {}", file(&app));

    // Outside, and kept: saving then writes the window's own.
    std::fs::write(&path, settings("theme = light")).unwrap();
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "画面の内容を保つ").is_some()));
    click_child(editor, BUTTON_CLASS, "画面の内容を保つ");
    click_child(editor, BUTTON_CLASS, "保存");
    let _ = within_3s(|| file(&app).contains("theme = dark"));
    let saved = file(&app);
    assert!(saved.contains("theme = dark") && saved.contains("opacity = 60"), "the window's, not the change outside: {saved}");

    // Outside, and broken (not UTF-8): it cannot be read again; the reason
    // shows, the question stays, and the window keeps its own.
    std::fs::write(&path, b"[\xFF\xFE]\n").unwrap();
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "読み直す").is_some()), "told of the change");
    click_child(editor, BUTTON_CLASS, "読み直す");
    let _ = within_3s(|| status(editor).contains("読み直せません"));
    assert!(status(editor).contains("読み直せません"), "the reason: {:?}", status(editor));
    assert!(child(editor, BUTTON_CLASS, "画面の内容を保つ").is_some(), "the question stays");
    click_child(editor, BUTTON_CLASS, "画面の内容を保つ");
    click_child(editor, BUTTON_CLASS, "保存");
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
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "キャンセル").is_some()), "asked");
    assert!(child(editor, BUTTON_CLASS, "保存する").is_some() && child(editor, BUTTON_CLASS, "保存せずに閉じる").is_some());
    click_child(editor, BUTTON_CLASS, "キャンセル");
    assert!(unsafe { IsWindow(editor) } != 0, "cancelled: still open");

    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "保存せずに閉じる").is_some()));
    click_child(editor, BUTTON_CLASS, "保存せずに閉じる");
    assert!(within_3s(|| editors(&app).is_empty()), "closed");
    assert_eq!(file(&app), before, "nothing saved");

    let editor = open(&app, id);
    change(editor, 40);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "保存する").is_some()));
    click_child(editor, BUTTON_CLASS, "保存する");
    assert!(within_3s(|| editors(&app).is_empty()), "saved, then closed");
    assert!(file(&app).contains("opacity = 40"), "{}", file(&app));

    // Saving what the parser cannot read: the reason shows, and the
    // question and the window stay.
    let editor = open(&app, id);
    change(editor, 150);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "保存する").is_some()));
    click_child(editor, BUTTON_CLASS, "保存する");
    let _ = within_3s(|| status(editor).contains("150"));
    assert!(status(editor).contains("150"), "the reason shows: {:?}", status(editor));
    assert!(unsafe { IsWindow(editor) } != 0 && child(editor, BUTTON_CLASS, "保存せずに閉じる").is_some(), "still asked");
    click_child(editor, BUTTON_CLASS, "保存せずに閉じる");
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
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "画面の内容を保つ").is_some()));
    click_child(editor, BUTTON_CLASS, "画面の内容を保つ");
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| editors(&app).is_empty()), "kept off, nothing to save: it closes");

    // Changes, and a change outside not yet answered: closing asks about
    // both; cancelled, the change outside is asked about again.
    let editor = open(&app, id);
    change(editor, 30);
    std::fs::write(&path, settings("theme = light")).unwrap();
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "読み直す").is_some()));
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "キャンセル").is_some()), "asked about closing");
    assert!(status(editor).contains("外でも"), "and told of the change outside: {:?}", status(editor));
    click_child(editor, BUTTON_CLASS, "キャンセル");
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "読み直す").is_some()), "asked about the change outside again");

    // The tray's exit, with changes not saved: EdgePad stays, asking.
    let exit = menu_item(&app, "終了").id;
    let tray = find_window(&app, TRAY_CLASS, false);
    unsafe { PostMessageW(tray, WM_COMMAND, exit as usize, 0) };
    pump(1000);
    assert!(editors(&app).contains(&editor), "EdgePad and the window are still there");
    assert!(child(editor, BUTTON_CLASS, "保存せずに閉じる").is_some(), "asking about the changes");
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
    // tapping a tab meanwhile: the key comes up when the mouse lets go,
    // off the panel as it is, as ever.
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
    let tab = centre(window_rect(child(editor, TAB_CLASS, "辺").expect("a tab")));
    touch.down(2, tab);
    touch.up(2);
    mouse_up_at(away);
    let _ = within_3s(|| log.keys().len() >= before + 2);
    assert_eq!(log.keys()[before..], [(b'B' as u16, false), (b'B' as u16, true)], "B down, and up with the mouse");
}

/// The palette's background of each theme (spec 5.2).
const DARK: [i32; 3] = [0x20, 0x20, 0x20];
const LIGHT: [i32; 3] = [0xE4, 0xE4, 0xE4];

/// The colour in the middle of `editor`'s client area, where nothing is.
fn middle(editor: HWND) -> [i32; 3] {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(editor, &mut r) };
    let mut p = POINT { x: r.right / 2, y: r.bottom / 2 };
    unsafe { ClientToScreen(editor, &mut p) };
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

fn near(c: [i32; 3], want: [i32; 3]) -> bool {
    (0..3).all(|i| (c[i] - want[i]).abs() <= 8)
}

// AC7
#[test]
fn it_follows_the_dpi_of_its_monitor_and_the_theme() {
    let _desktop = desktop();
    for (theme, want) in [("dark", DARK), ("light", LIGHT)] {
        let app = launch_with(Some(&settings(&format!("theme = {theme}"))), None);
        let editor = open(&app, open_id(&app));
        wait_in_front(editor);
        let _ = within_3s(|| {
            unsafe { DwmFlush() };
            near(middle(editor), want)
        });
        let c = middle(editor);
        assert!(near(c, want), "{theme}: the background is {want:?}, not {c:?}");
        // A tab not shown, filled as a key: its label is what differs.
        let tab = child(editor, TAB_CLASS, "辺").expect("a tab");
        let r = window_rect(tab);
        let ink = || {
            unsafe { DwmFlush() };
            let pixels = capture(r);
            let face = channels(pixels[(3 * (r.right - r.left) + 3) as usize]);
            pixels.iter().map(|&p| channels(p)).filter(|&c| !near(c, face)).count()
        };
        let _ = within_3s(|| ink() > 0);
        assert!(ink() > 0, "{theme}: the tab's label is drawn");
        drop(app);
    }

    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app, open_id(&app));
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let tab = child(editor, TAB_CLASS, "一般").expect("a tab");
    let (h0, w0) = (window_rect(tab).bottom - window_rect(tab).top, window_rect(editor).right - window_rect(editor).left);
    let new = dpi * 3 / 2;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    pump(300);
    let (h1, w1) = (window_rect(tab).bottom - window_rect(tab).top, window_rect(editor).right - window_rect(editor).left);
    assert!((h1 - h0 * 3 / 2).abs() <= 2, "the tab grows with the DPI: {h0} -> {h1}");
    assert!((w1 - w0 * 3 / 2).abs() <= 4, "and the window: {w0} -> {w1}");

    // A DPI no monitor has (0, or far past 500%) changes nothing.
    for bad in [0, 0xFFFF] {
        unsafe { SendMessageW(editor, WM_DPICHANGED, (bad << 16 | bad) as usize, &suggested as *const _ as isize) };
        pump(300);
        let w = window_rect(editor).right - window_rect(editor).left;
        assert_eq!(w, w1, "DPI {bad}: the window stays as it was");
    }
}

fn private_bytes(app: &Running) -> usize {
    let mut c: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let ok = unsafe { GetProcessMemoryInfo(app.0.as_raw_handle(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb) };
    assert!(ok != 0, "GetProcessMemoryInfo");
    c.PrivateUsage
}

// AC8. The first time a window of EdgePad's takes the focus, Windows
// loads its text input (IME, Text Input Framework) into the process, and
// that stays (spec 7.1): count from after one opening, so what is left of
// each window shows.
#[test]
fn closed_it_gives_its_memory_back() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    // Twice, as the measured ones go (in front, a tab clicked: it is taking
    // the focus that loads the text input), and given time: Windows
    // finishes loading in its own time, later still on a busy machine.
    for _ in 0..2 {
        let editor = open(&app, id);
        click_child(editor, TAB_CLASS, "辺");
        unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
        assert!(within_3s(|| editors(&app).is_empty()), "closed");
    }
    pump(1500);
    let before = private_bytes(&app);
    for _ in 0..3 {
        let editor = open(&app, id);
        click_child(editor, TAB_CLASS, "キーセット");
        unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
        assert!(within_3s(|| editors(&app).is_empty()), "closed");
    }
    pump(1500);
    let after = private_bytes(&app);
    assert!(after <= before + 128 * 1024, "private memory back within 128 KiB: {before} -> {after}");
}

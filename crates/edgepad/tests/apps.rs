//! Acceptance tests for Issue #88: the アプリ別 page of the settings window,
//! which edits the profiles, [profile.*], without writing the settings
//! file (spec 7.1).
//!
//! The tests find the page's parts by control id: the list of apps 2000 (a
//! ListBox), 開いているアプリから追加 2001, 削除 2002, the process name to
//! add 2003 (a ComboBox one can type in, of the apps open) with 追加 2004
//! and やめる 2005; the chosen app's process 2011 (an Edit); the figure of
//! the screen 2013, and on its edges the key set shown there, left, right,
//! top and bottom, 2020 to 2023 (drop-down lists). The app whose profile
//! is in effect has ▶ before it in the list.
//!
//! Two apps take turns in the foreground, as in the profile tests: this
//! test process, which a profile names, and a copy of it under another name
//! that only opens a window (`other_app`).

mod common;

use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::process::Command;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::Graphics::Gdi::MapWindowPoints;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::editor::*;
use common::*;

const APPS: i32 = 2000;
const ADD_APP: i32 = 2001;
const REMOVE_APP: i32 = 2002;
const CANDIDATES: i32 = 2003;
const ADD: i32 = 2004;
const PROCESS: i32 = 2011;
const FIGURE: i32 = 2013;
const LEFT: i32 = 2020;
const RIGHT: i32 = 2021;
const TOP: i32 = 2022;
const BOTTOM: i32 = 2023;
const APPS_PAGE: usize = 1;
const DEFAULT: &str = "既定（ほかのアプリ）";

const OTHER_CLASS: &str = "EdgePadTestOtherApp";
const OTHER_ENV: &str = "EDGEPAD_TEST_OTHER_APP";
const OTHER_EXE: &str = "edgepad-other-app.exe";

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

/// Start the other app, a new one each time, and return it once its window
/// is in front.
fn other_app_window() -> (Running, HWND) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("edgepad-apps-other-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe: PathBuf = dir.join(OTHER_EXE);
    std::fs::copy(std::env::current_exe().unwrap(), &exe).expect("copy the test binary");
    let child = Command::new(&exe)
        .args(["other_app", "--exact", "--ignored", "--nocapture"])
        .env(OTHER_ENV, "1")
        .spawn()
        .expect("start the other app");
    let other = Running(child, Some(dir));
    let hwnd = find_window(&other, OTHER_CLASS, true);
    wait_in_front(hwnd);
    (other, hwnd)
}

/// A new window of this process, once it is in front.
fn my_window() -> TestWindow {
    let m = primary_monitor();
    let r = RECT { left: m.left + 400, top: m.top + 350, right: m.left + 700, bottom: m.top + 550 };
    let window = test_window("EdgePadTestMyApp", 0, r);
    wait_in_front(window.0);
    window
}

/// How many keys tall the left panel of `app` is now; 0 without one.
fn left_keys(app: &Running) -> i32 {
    let m = primary_monitor();
    panels_now(app).iter().find(|r| r.left == m.left).map_or(0, |r| (r.bottom - r.top) / key_px())
}

/// Key sets of one to three keys; profiles for every other app, this test,
/// and CLIP STUDIO PAINT; written by hand, with comments.
fn settings() -> String {
    format!(
        "; 手で書いた設定\n\
         [set.mods]\nkey = Esc | shortcut | Esc\n\
         [set.two]\nkey = A | hold | A\nkey = B | hold | B\n\
         [set.three]\nkey = X | hold | X\nkey = Y | hold | Y\nkey = Z | hold | Z\n\
         [set.spare]\nkey = S | hold | S\n\n\
         [profile.default]\nleft = mods\n\n\
         [profile.me]   ; このテスト\nprocess = {}\nleft = three   ; 自分\n\n\
         [profile.csp]\nprocess = CLIPStudioPaint.exe\nleft = mods\nright = spare\n",
        me()
    )
}

fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) != 0 }
}

fn shown(editor: HWND, id: i32) -> bool {
    visible(part(editor, id))
}

fn text(editor: HWND, id: i32) -> String {
    text_of(part(editor, id))
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 128) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn click_id(editor: HWND, id: i32) {
    let hwnd = part(editor, id);
    assert!(visible(hwnd), "part {id} shown");
    wait_in_front(editor);
    click(centre(window_rect(hwnd)));
    pump(300);
}

/// Type `text` as characters, wherever the focus is.
fn type_text(text: &str) {
    for unit in text.encode_utf16() {
        for up in [false, true] {
            unsafe {
                let mut input: INPUT = zeroed();
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki.wScan = unit;
                input.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 };
                input.Anonymous.ki.dwExtraInfo = KEYBOARD_TAG;
                assert_eq!(SendInput(1, &input, size_of::<INPUT>() as i32), 1, "SendInput");
            }
        }
    }
    pump(200);
}

fn press(vk: u16) {
    keyboard(vk, true);
    keyboard(vk, false);
}

/// Press arrow `vk` as the arrow keys do, an extended key.
fn arrow(vk: u16) {
    for up in [false, true] {
        unsafe {
            let mut input: INPUT = zeroed();
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = vk;
            input.Anonymous.ki.dwFlags = KEYEVENTF_EXTENDEDKEY | if up { KEYEVENTF_KEYUP } else { 0 };
            input.Anonymous.ki.dwExtraInfo = KEYBOARD_TAG;
            assert_eq!(SendInput(1, &input, size_of::<INPUT>() as i32), 1, "SendInput");
        }
        pump(100);
    }
    pump(200);
}

/// The items of the ComboBox or ListBox `hwnd`.
fn items(hwnd: HWND) -> Vec<String> {
    let list = class_of(hwnd) == "ListBox";
    let (count, get) = if list { (LB_GETCOUNT, LB_GETTEXT) } else { (CB_GETCOUNT, CB_GETLBTEXT) };
    let n = unsafe { SendMessageW(hwnd, count, 0, 0) }.max(0) as usize;
    (0..n)
        .map(|i| {
            let mut buf = [0u16; 256];
            let len = unsafe { SendMessageW(hwnd, get, i, buf.as_mut_ptr() as isize) };
            String::from_utf16_lossy(&buf[..len.max(0) as usize])
        })
        .collect()
}

/// The item chosen in the ComboBox or ListBox `hwnd`, if any.
fn chosen(hwnd: HWND) -> Option<String> {
    let get = if class_of(hwnd) == "ListBox" { LB_GETCURSEL } else { CB_GETCURSEL };
    let at = unsafe { SendMessageW(hwnd, get, 0, 0) };
    usize::try_from(at).ok().and_then(|i| items(hwnd).get(i).cloned())
}

/// An app in the list as it reads without the mark of the one in effect.
fn unmarked(item: &str) -> &str {
    item.trim_start_matches('▶').trim()
}

fn marked(item: &str) -> bool {
    item.starts_with('▶')
}

/// Click the app `name` in the list.
fn choose_app(editor: HWND, name: &str) {
    let list = part(editor, APPS);
    let at = items(list).iter().position(|i| unmarked(i) == name).unwrap_or_else(|| panic!("no {name} in {:?}", items(list)));
    let mut r: RECT = unsafe { zeroed() };
    unsafe { SendMessageW(list, LB_GETITEMRECT, at, &mut r as *mut RECT as isize) };
    let o = window_rect(list);
    wait_in_front(editor);
    click(POINT { x: o.left + (r.left + r.right) / 2, y: o.top + 2 + (r.top + r.bottom) / 2 });
    assert!(within_3s(|| chosen(list).is_some_and(|c| unmarked(&c) == name)), "{name} chosen: {:?}", chosen(list));
    pump(200);
}

/// Choose `option` in the drop-down list `id`, by keyboard: Home, then Down
/// as far as it is.
fn choose(editor: HWND, id: i32, option: &str) {
    let combo = part(editor, id);
    let at = items(combo).iter().position(|o| o == option).unwrap_or_else(|| panic!("no {option} in {:?}", items(combo)));
    click_id(editor, id);
    press(VK_ESCAPE);
    press(VK_HOME);
    for _ in 0..at {
        press(VK_DOWN);
    }
    pump(300);
    assert!(within_3s(|| chosen(combo).as_deref() == Some(option)), "{option} chosen in {id}: {:?}", chosen(combo));
}

/// Open the window on アプリ別.
fn open_apps(app: &Running) -> HWND {
    let editor = open(app, open_id(app));
    wait_in_front(editor);
    click(centre(sidebar_item(editor, APPS_PAGE)));
    assert!(within_3s(|| heading(editor) == "アプリ別"), "{:?}", heading(editor));
    editor
}

fn save(editor: HWND) {
    click_button(editor, "保存");
}

fn save_until(app: &Running, editor: HWND, line: &str) -> String {
    save(editor);
    assert!(within_3s(|| file(app).contains(line)), "saved with {line:?}, said {:?}:\n{}", status(editor), file(app));
    file(app)
}

// AC1
#[test]
fn the_apps_are_listed_default_first_and_the_one_in_effect_marked() {
    let _desktop = desktop();
    let _mine = my_window();
    let app = launch_with(Some(&settings()), None);
    let editor = open_apps(&app);
    let list = part(editor, APPS);
    assert_eq!(class_of(list), "ListBox");
    let apps: Vec<String> = items(list).iter().map(|i| unmarked(i).to_string()).collect();
    assert_eq!(apps, [DEFAULT.to_string(), me(), "CLIPStudioPaint.exe".to_string()], "default first, then each profile");
    // This app was in front before the window: its profile is in effect.
    let mine_marked = || items(list).iter().any(|i| marked(i) && unmarked(i) == me());
    assert!(within_3s(mine_marked), "this test's profile marked: {:?}", items(list));
    assert_eq!(items(list).iter().filter(|i| marked(i)).count(), 1, "one marked");

    // Another app in front, then the window again: the default in effect.
    let _other = other_app_window();
    wait_in_front(editor);
    let default_marked = || items(list).first().is_some_and(|i| marked(i));
    assert!(within_3s(default_marked), "the default marked: {:?}", items(list));
    assert!(!mine_marked());
}

// AC2
#[test]
fn profiles_are_added_from_the_apps_open_or_by_name_and_removed() {
    let _desktop = desktop();
    let (_other, _) = other_app_window();
    let app = launch_with(Some(&settings()), None);
    let editor = open_apps(&app);
    let list = part(editor, APPS);
    assert!(!shown(editor, CANDIDATES), "the apps open are not gathered until asked");
    click_id(editor, ADD_APP);
    assert!(within_3s(|| shown(editor, CANDIDATES)), "the apps open, to choose from");
    let candidates = items(part(editor, CANDIDATES));
    assert!(candidates.iter().any(|c| c.eq_ignore_ascii_case(OTHER_EXE)), "the other app is open: {candidates:?}");
    assert!(!candidates.iter().any(|c| c.eq_ignore_ascii_case("edgepad.exe")), "EdgePad is not one: {candidates:?}");
    // Chosen from the list.
    click_id(editor, CANDIDATES);
    for _ in 0..candidates.len() {
        if text(editor, CANDIDATES).eq_ignore_ascii_case(OTHER_EXE) {
            break;
        }
        arrow(VK_DOWN);
    }
    assert!(text(editor, CANDIDATES).eq_ignore_ascii_case(OTHER_EXE), "{:?}", text(editor, CANDIDATES));
    click_id(editor, ADD);
    assert!(within_3s(|| items(list).iter().any(|i| unmarked(i).eq_ignore_ascii_case(OTHER_EXE))), "{:?}", items(list));
    // Typed by hand.
    for name in ["Foo.exe", "foo.EXE"] {
        click_id(editor, ADD_APP);
        click_id(editor, CANDIDATES);
        type_text(name);
        click_id(editor, ADD);
    }
    assert!(within_3s(|| status(editor).contains("foo.EXE") && status(editor).contains("もうあります")), "a process with a profile: {:?}", status(editor));
    assert_eq!(items(list).iter().filter(|i| unmarked(i).eq_ignore_ascii_case("foo.exe")).count(), 1, "{:?}", items(list));

    // The default cannot be removed; another can.
    choose_app(editor, DEFAULT);
    assert!(unsafe { IsWindowEnabled(part(editor, REMOVE_APP)) } == 0, "the default stays");
    choose_app(editor, "Foo.exe");
    click_id(editor, REMOVE_APP);
    assert!(within_3s(|| !items(list).iter().any(|i| unmarked(i) == "Foo.exe")), "{:?}", items(list));
    let saved = save_until(&app, editor, "[profile.edgepad-other-app]");
    assert!(saved.contains(&format!("[profile.edgepad-other-app]\nprocess = {OTHER_EXE}")), "{saved}");
    assert!(!saved.to_ascii_lowercase().contains("foo"), "{saved}");
}

// AC3
#[test]
fn an_app_chosen_shows_its_process_and_the_key_set_of_each_edge() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open_apps(&app);
    choose_app(editor, "CLIPStudioPaint.exe");
    assert_eq!(class_of(part(editor, PROCESS)), "Edit");
    assert!(within_3s(|| text(editor, PROCESS) == "CLIPStudioPaint.exe"), "{:?}", text(editor, PROCESS));
    assert!(shown(editor, FIGURE), "the screen's figure");
    let edges = || [LEFT, RIGHT, TOP, BOTTOM].map(|id| chosen(part(editor, id)).unwrap_or_default());
    assert_eq!(edges(), ["mods", "spare", "なし", "なし"]);
    // On the figure's edges.
    let f = window_rect(part(editor, FIGURE));
    for id in [LEFT, RIGHT, TOP, BOTTOM] {
        let r = window_rect(part(editor, id));
        assert!(r.left >= f.left && r.right <= f.right && r.top >= f.top && r.bottom <= f.bottom, "{id} on the figure");
    }
    let (l, r, t, b) = (window_rect(part(editor, LEFT)), window_rect(part(editor, RIGHT)), window_rect(part(editor, TOP)), window_rect(part(editor, BOTTOM)));
    assert!(l.right <= r.left && t.bottom <= b.top, "left of right, top above bottom");

    choose_app(editor, DEFAULT);
    assert!(within_3s(|| edges() == ["mods", "なし", "なし", "なし"]), "{:?}", edges());
    assert!(!shown(editor, PROCESS), "the default has no process");
}

// AC4
#[test]
fn an_edge_of_the_figure_chooses_its_key_set_or_none() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open_apps(&app);
    choose_app(editor, "CLIPStudioPaint.exe");
    let right = part(editor, RIGHT);
    assert_eq!(items(right), ["なし", "mods", "two", "three", "spare"]);
    click_id(editor, RIGHT);
    assert!(within_3s(|| unsafe { SendMessageW(right, CB_GETDROPPEDSTATE, 0, 0) } != 0), "pressed, the edge's list opens");
    press(VK_ESCAPE);
    choose(editor, RIGHT, "two");
    choose(editor, LEFT, "なし");

    // By keyboard: the figure focused, ← chooses the left edge, Enter opens it.
    let figure = window_rect(part(editor, FIGURE));
    wait_in_front(editor);
    click(centre(figure));
    assert!(within_3s(|| focus_of(editor) == part(editor, FIGURE)), "the figure takes the focus");
    arrow(VK_UP);
    press(VK_RETURN);
    let top = part(editor, TOP);
    assert!(within_3s(|| unsafe { SendMessageW(top, CB_GETDROPPEDSTATE, 0, 0) } != 0), "↑ then Enter opens the top edge's list");
    press(VK_ESCAPE);

    let saved = save_until(&app, editor, "right = two");
    let csp = &saved[saved.find("[profile.csp]").unwrap()..];
    assert!(csp.contains("left = none"), "{csp}");
}

// AC5
#[test]
fn a_change_rewrites_only_its_line_and_the_panels_follow() {
    let _desktop = desktop();
    let _mine = my_window();
    let app = launch_with(Some(&settings()), None);
    let before = file(&app);
    find_panel(&app);
    assert_eq!(left_keys(&app), 3, "this test's profile: three");
    let editor = open_apps(&app);
    choose_app(editor, &me());
    choose(editor, LEFT, "two");
    choose(editor, BOTTOM, "spare");
    let saved = save_until(&app, editor, "left = two");
    let (old, new): (Vec<&str>, Vec<&str>) = (before.lines().collect(), saved.lines().collect());
    assert_eq!(new.len(), old.len() + 1, "one line added:\n{saved}");
    for line in &old {
        if line.contains("; 自分") {
            assert!(new.contains(&"left = two   ; 自分"), "its comment kept:\n{saved}");
        } else {
            assert!(new.contains(line), "kept: {line}\n{saved}");
        }
    }
    let me_section = &saved[saved.find("[profile.me]").unwrap()..saved.find("[profile.csp]").unwrap()];
    assert!(me_section.contains("bottom = spare"), "added to its profile:\n{saved}");
    // This app's profile is still the one in effect: its panel reads the file.
    let _mine = my_window();
    assert!(within_3s(|| left_keys(&app) == 2), "the left panel two keys now: {}", left_keys(&app));
}

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Windows's app mode set dark for the test, put back on drop.
struct Dark(Option<u32>);

fn announce() {
    let what = wide("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, what.as_ptr() as isize, SMTO_ABORTIFHUNG, 2000, null_mut());
    }
    pump(300);
}

impl Dark {
    fn set() -> Dark {
        let (key, name) = (wide(PERSONALIZE), wide("AppsUseLightTheme"));
        let (mut value, mut size) = (0u32, 4u32);
        let found = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, null_mut(), &mut value as *mut u32 as _, &mut size)
        };
        let dark = 0u32;
        unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, &dark as *const u32 as _, 4) };
        announce();
        Dark((found == 0).then_some(value))
    }
}

impl Drop for Dark {
    fn drop(&mut self) {
        let (key, name) = (wide(PERSONALIZE), wide("AppsUseLightTheme"));
        unsafe {
            match self.0 {
                Some(v) => {
                    RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, &v as *const u32 as _, 4);
                }
                None => {
                    RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
                }
            }
        }
        announce();
    }
}

fn ltrb(r: RECT) -> [i32; 4] {
    [r.left, r.top, r.right, r.bottom]
}

fn colour_at(p: POINT) -> [i32; 3] {
    unsafe { DwmFlush() };
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

fn brightness(c: [i32; 3]) -> i32 {
    (c[0] + c[1] + c[2]) / 3
}

// AC6
#[test]
fn the_page_follows_the_theme_the_dpi_and_the_keyboard() {
    let _desktop = desktop();
    let dark = Dark::set();
    let app = launch_with(Some(&settings()), None);
    let editor = open_apps(&app);
    choose_app(editor, "CLIPStudioPaint.exe");
    let inside = || {
        let r = window_rect(part(editor, PROCESS));
        colour_at(POINT { x: r.right - 4, y: (r.top + r.bottom) / 2 })
    };
    let _ = within_3s(|| brightness(inside()) < 90);
    assert!(brightness(inside()) < 90, "the process field dark: {:?}", inside());
    let figure = window_rect(part(editor, FIGURE));
    let corner = colour_at(POINT { x: figure.left + 2, y: figure.top + 2 });
    assert!(brightness(corner) < 90, "the figure dark: {corner:?}");

    let width = |id: i32| {
        let r = window_rect(part(editor, id));
        r.right - r.left
    };
    let (process0, figure0) = (width(PROCESS), width(FIGURE));
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let new = dpi * 3 / 2;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    pump(300);
    assert!((width(PROCESS) - process0 * 3 / 2).abs() <= 3, "the process field grows: {process0} -> {}", width(PROCESS));
    // The figure keeps inside the window, off the list, and the edges'
    // lists inside it, apart: on CI's 1024 x 768 screen too, where the
    // window has no room to grow as much (Issue #99).
    let mut client: RECT = unsafe { zeroed() };
    unsafe { GetClientRect(editor, &mut client) };
    let mut corners = [POINT { x: 0, y: 0 }, POINT { x: client.right, y: client.bottom }];
    unsafe { MapWindowPoints(editor, null_mut(), corners.as_mut_ptr(), 2) };
    let (f, list) = (window_rect(part(editor, FIGURE)), window_rect(part(editor, APPS)));
    assert!(f.left >= corners[0].x && f.right <= corners[1].x && f.top >= corners[0].y && f.bottom <= corners[1].y, "the figure in the window: {:?}", ltrb(f));
    assert!(f.left >= list.right, "the figure off the list: {:?} {:?}", ltrb(f), ltrb(list));
    for id in [LEFT, RIGHT, TOP, BOTTOM] {
        let r = window_rect(part(editor, id));
        assert!(r.left >= f.left && r.right <= f.right && r.top >= f.top && r.bottom <= f.bottom, "{id} on the figure: {:?} {:?}", ltrb(r), ltrb(f));
    }
    let (l, r, t, b) = (window_rect(part(editor, LEFT)), window_rect(part(editor, RIGHT)), window_rect(part(editor, TOP)), window_rect(part(editor, BOTTOM)));
    assert!(l.right <= r.left && t.bottom <= l.top && l.bottom <= b.top, "the edges' lists apart: {:?}", [l, r, t, b].map(ltrb));
    unsafe { SendMessageW(editor, WM_DPICHANGED, (dpi << 16 | dpi) as usize, &suggested as *const _ as isize) };
    pump(300);
    assert_eq!(width(FIGURE), figure0, "the figure back as it was");
    drop(dark);

    // The keyboard: from the list, Tab reaches the figure; → and Enter
    // open the right edge's list.
    click_id(editor, APPS);
    let figure = part(editor, FIGURE);
    let mut reached = false;
    for _ in 0..20 {
        press(VK_TAB);
        if focus_of(editor) == figure {
            reached = true;
            break;
        }
    }
    assert!(reached, "the figure takes the focus");
    arrow(VK_RIGHT);
    press(VK_RETURN);
    let right = part(editor, RIGHT);
    assert!(within_3s(|| unsafe { SendMessageW(right, CB_GETDROPPEDSTATE, 0, 0) } != 0), "→ then Enter opens the right edge's list");
    press(VK_ESCAPE);
}

// A process name the file cannot hold is not added, and nothing is left of it.
#[test]
fn a_process_name_the_file_cannot_hold_is_not_added() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open_apps(&app);
    let list = part(editor, APPS);
    let before = items(list).len();
    click_id(editor, ADD_APP);
    click_id(editor, CANDIDATES);
    type_text("Bad ;x.exe");
    click_id(editor, ADD);
    assert!(within_3s(|| status(editor).contains("Bad ;x.exe")), "why: {:?}", status(editor));
    assert_eq!(items(list).len(), before, "no app added: {:?}", items(list));
    save(editor);
    assert!(within_3s(|| status(editor).contains("保存しました")), "the rest saves: {:?}", status(editor));
    assert!(!file(&app).contains("[profile.bad"), "{}", file(&app));
}

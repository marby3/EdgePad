//! Acceptance tests for Issue #107: the settings window's pages by use,
//! names in Japanese, ranges only where looked at, drop-down lists, and a
//! band for what is done (spec 7.1).
//!
//! A row is found by its setting's name (control 900 + i, beside its label
//! 600 + i); its field is 400 + i, its note 700 + i, its reset 500 + i and
//! its mark 800 + i. The band is control 15; accent's #RRGGBB field is 22.
//! Values are typed as characters. A debug build is slow: what a test
//! checks, it waits for.

mod common;

use std::mem::{size_of, zeroed};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{GetObjectW, LOGFONTW};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::editor::*;
use common::*;

const FIELD: i32 = 400;
const RESET: i32 = 500;
const LABEL: i32 = 600;
const NOTE: i32 = 700;
const MARK: i32 = 800;
const NAME: i32 = 900;
const NOTICE: i32 = 15;
const ACCENT_HEX: i32 = 22;

const COMMON_PAGE: usize = 3;
const EDGE_PAGE: usize = 4;
const LOOK_PAGE: usize = 5;
const TOUCH_PAGE: usize = 6;
const SHOW_PAGE: usize = 7;

const SETTINGS: &str = "[general]\nopacity = 70\n[set.two]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nleft = two\n";

fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) != 0 }
}

/// The index of the shown row of setting `name`.
fn row_of(editor: HWND, name: &str) -> Option<i32> {
    (0..64).find(|i| {
        let n = unsafe { GetDlgItem(editor, NAME + i) };
        !n.is_null() && visible(n) && text_of(n) == name
    })
}

fn row(editor: HWND, name: &str) -> i32 {
    row_of(editor, name).unwrap_or_else(|| panic!("no row of {name} shown on {:?}", heading(editor)))
}

fn field(editor: HWND, name: &str) -> HWND {
    part(editor, FIELD + row(editor, name))
}

fn click_on(editor: HWND, hwnd: HWND) {
    wait_in_front(editor);
    click(centre(window_rect(hwnd)));
    pump(300);
}

fn show_page(editor: HWND, page: usize) {
    for _ in 0..2 {
        wait_in_front(editor);
        click(centre(sidebar_item(editor, page)));
        if within_3s(|| heading(editor) == PAGES[page]) {
            return;
        }
    }
    panic!("page {}, not {:?}", PAGES[page], heading(editor));
}

fn open_page(app: &Running, page: usize) -> HWND {
    let editor = open(app, open_id(app));
    show_page(editor, page);
    editor
}

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
    pump(100);
}

/// Type `text` into `hwnd`, in place of what it has, and press Enter.
fn enter_into(editor: HWND, hwnd: HWND, text: &str) {
    click_on(editor, hwnd);
    keyboard(VK_CONTROL, true);
    press(b'A' as u16);
    keyboard(VK_CONTROL, false);
    type_text(text);
    press(VK_RETURN);
    pump(300);
}

fn items(combo: HWND) -> Vec<String> {
    let n = unsafe { SendMessageW(combo, CB_GETCOUNT, 0, 0) }.max(0) as usize;
    (0..n)
        .map(|i| {
            let mut buf = [0u16; 64];
            let len = unsafe { SendMessageW(combo, CB_GETLBTEXT, i, buf.as_mut_ptr() as isize) };
            String::from_utf16_lossy(&buf[..len.max(0) as usize])
        })
        .collect()
}

/// Choose `option` in the drop-down `combo` by keyboard, trying again if a
/// key is lost as the list opens.
fn choose(editor: HWND, combo: HWND, option: &str) {
    let at = items(combo).iter().position(|o| o == option).unwrap_or_else(|| panic!("no {option} in {:?}", items(combo)));
    for _ in 0..3 {
        click_on(editor, combo);
        press(VK_ESCAPE);
        press(VK_HOME);
        for _ in 0..at {
            press(VK_DOWN);
        }
        pump(300);
        let chosen = unsafe { SendMessageW(combo, CB_GETCURSEL, 0, 0) };
        if chosen == at as isize {
            return;
        }
    }
    panic!("{option} not chosen");
}

fn save_until(app: &Running, editor: HWND, line: &str) -> String {
    click_button(editor, "保存");
    assert!(within_3s(|| file(app).contains(line)), "saved with {line:?}, said {:?}:\n{}", status(editor), file(app));
    file(app)
}

fn font_height(hwnd: HWND) -> i32 {
    let font = unsafe { SendMessageW(hwnd, WM_GETFONT, 0, 0) };
    let mut lf: LOGFONTW = unsafe { zeroed() };
    unsafe { GetObjectW(font as _, size_of::<LOGFONTW>() as i32, &mut lf as *mut LOGFONTW as _) };
    lf.lfHeight.abs()
}

/// The setting names on each page, as its rows name them.
const ON_PAGES: [(usize, &[&str]); 5] = [
    (COMMON_PAGE, &["enabled", "dock", "monitor", "columns", "rows", "handle", "handle の幅", "handle の長さ", "handle の位置"]),
    (EDGE_PAGE, &["enabled", "dock", "monitor", "columns", "handle"]),
    (LOOK_PAGE, &["key_size", "opacity", "dim_opacity", "theme", "accent", "slide_ms"]),
    (TOUCH_PAGE, &["latch_ms", "latch", "pen_presses"]),
    (SHOW_PAGE, &["hotkey", "hover_ms", "auto_hide_ms"]),
];

// AC1
#[test]
fn the_sidebar_has_eight_pages_by_use_and_every_setting_is_on_one() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open(&app, open_id(&app));
    let sidebar = part(editor, SIDEBAR);
    let n = unsafe { SendMessageW(sidebar, LB_GETCOUNT, 0, 0) };
    assert_eq!(n, 8, "eight pages");
    assert_eq!(
        PAGES,
        ["キー配置", "キーマッピング", "アプリ別", "辺（共通）", "辺ごとの設定", "見た目", "タッチと修飾キー", "表示と非表示"]
    );
    for (page, names) in ON_PAGES {
        show_page(editor, page);
        for name in names {
            assert!(within_3s(|| row_of(editor, name).is_some()), "{name} on {}", PAGES[page]);
        }
    }
    // The left edge's: columns, not rows.
    show_page(editor, EDGE_PAGE);
    assert!(row_of(editor, "rows").is_none(), "rows is the top and bottom edges'");
}

// AC2
#[test]
fn each_row_is_named_in_japanese_its_setting_s_name_beside_it_smaller() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_page(&app, LOOK_PAGE);
    for (name, label) in [("key_size", "キーの大きさ"), ("opacity", "ふだんの不透明度"), ("theme", "テーマ")] {
        let i = row(editor, name);
        let (l, n) = (part(editor, LABEL + i), part(editor, NAME + i));
        assert_eq!(text_of(l), label, "{name}'s label");
        assert!(window_rect(n).left >= window_rect(l).left, "{name} beside its label");
        assert!(font_height(n) < font_height(l), "{name} smaller: {} < {}", font_height(n), font_height(l));
    }
    show_page(editor, TOUCH_PAGE);
    assert_eq!(text_of(part(editor, LABEL + row(editor, "latch_ms"))), "ラッチになる長さ");
    show_page(editor, SHOW_PAGE);
    assert_eq!(text_of(part(editor, LABEL + row(editor, "auto_hide_ms"))), "自動で隠すまでの時間");
}

// AC3
#[test]
fn a_range_shows_only_while_its_field_has_the_focus_or_the_mouse() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_page(&app, LOOK_PAGE);
    let note = part(editor, NOTE + row(editor, "key_size"));
    // The mouse on the sidebar, the focus there too.
    mouse_at(centre(sidebar_item(editor, LOOK_PAGE)), 0);
    pump(300);
    assert!(!visible(note), "no range at first");
    click_on(editor, field(editor, "key_size"));
    assert!(within_3s(|| visible(note) && text_of(note).contains('〜')), "with the focus: {:?}", text_of(note));
    click_on(editor, field(editor, "opacity"));
    mouse_at(centre(sidebar_item(editor, LOOK_PAGE)), 0);
    assert!(within_3s(|| !visible(note)), "gone with the focus");
    mouse_at(centre(window_rect(field(editor, "key_size"))), 0);
    assert!(within_3s(|| visible(note)), "with the mouse over it");
    mouse_at(centre(sidebar_item(editor, LOOK_PAGE)), 0);
    assert!(within_3s(|| !visible(note)), "gone with the mouse");
}

// AC4
#[test]
fn values_to_choose_are_chosen_by_japanese_names() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_page(&app, EDGE_PAGE);
    let monitor = items(field(editor, "monitor"));
    assert_eq!(monitor[..3], ["主モニター", "マウスのあるモニター", "モニター 1"], "{monitor:?}");
    choose(editor, field(editor, "monitor"), "マウスのあるモニター");
    show_page(editor, LOOK_PAGE);
    assert_eq!(items(field(editor, "theme")), ["ライト", "ダーク", "Windows に合わせる"]);
    choose(editor, field(editor, "theme"), "ダーク");
    let accent = field(editor, "accent");
    assert_eq!(items(accent), ["Windows に合わせる", "色を指定"]);
    let hex = part(editor, ACCENT_HEX);
    assert!(!visible(hex), "no #RRGGBB while it follows Windows");
    choose(editor, accent, "色を指定");
    assert!(within_3s(|| visible(hex)), "#RRGGBB to give");
    enter_into(editor, hex, "#112233");
    show_page(editor, TOUCH_PAGE);
    assert_eq!(items(field(editor, "latch")), ["1 回使うと解ける", "もう一度押すまで続く"]);
    choose(editor, field(editor, "latch"), "もう一度押すまで続く");
    let saved = save_until(&app, editor, "latch = lock");
    for line in ["monitor = cursor", "theme = dark", "accent = #112233"] {
        assert!(saved.contains(line), "{line}:\n{saved}");
    }
}

// AC5
#[test]
fn what_is_done_shows_in_a_band_for_a_while_and_what_to_fix_in_the_status_line() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_page(&app, LOOK_PAGE);
    let band = part(editor, NOTICE);
    assert!(!visible(band), "no band at first");
    enter_into(editor, field(editor, "key_size"), "60");
    click_button(editor, "保存");
    assert!(within_3s(|| visible(band) && text_of(band).contains("保存しました")), "the band: {:?}", text_of(band));
    assert!(!status(editor).contains("保存しました"), "not in the status line: {:?}", status(editor));
    let band_rect = window_rect(band);
    let heading_rect = window_rect(part(editor, HEADING));
    assert!(band_rect.top <= heading_rect.bottom, "at the top of the window");
    // Shown for 4 s: gone within 6 s of the save.
    let end = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while visible(band) && std::time::Instant::now() < end {
        pump(100);
    }
    assert!(!visible(band), "gone after a few seconds");
    // A value turned away: in the status line, as before.
    enter_into(editor, field(editor, "key_size"), "5");
    assert!(within_3s(|| status(editor).contains("key_size")), "the reason: {:?}", status(editor));
    assert!(!visible(band), "no band for it");
}

// AC6
#[test]
fn the_common_values_go_to_every_edge_and_an_edge_s_own_go_back_to_them() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_page(&app, COMMON_PAGE);
    enter_into(editor, field(editor, "columns"), "2");
    let saved = save_until(&app, editor, "columns = 2");
    assert!(saved.contains("[edge]"), "in [edge]:\n{saved}");
    show_page(editor, EDGE_PAGE);
    let columns = field(editor, "columns");
    let i = row(editor, "columns");
    assert!(within_3s(|| text_of(columns) == "2"), "the common value: {:?}", text_of(columns));
    assert_eq!(text_of(part(editor, MARK + i)), "共通");
    enter_into(editor, columns, "3");
    let saved = save_until(&app, editor, "columns = 3");
    assert!(saved.contains("[edge.left]"), "the left edge's own:\n{saved}");
    let reset = part(editor, RESET + i);
    assert_eq!(text_of(reset), "共通に戻す");
    click_on(editor, reset);
    click_button(editor, "保存");
    assert!(within_3s(|| !file(&app).contains("columns = 3")), "its line gone:\n{}", file(&app));
    assert!(within_3s(|| text_of(columns) == "2"), "the common value again");
}

// AC7
#[test]
fn the_keyboard_reaches_every_page_and_field_and_the_pages_follow_the_dpi() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open(&app, open_id(&app));
    wait_in_front(editor);
    click(centre(sidebar_item(editor, 0)));
    pump(300);
    for page in 1..PAGES.len() {
        press(VK_DOWN);
        assert!(within_3s(|| heading(editor) == PAGES[page]), "↓ to {}: {:?}", PAGES[page], heading(editor));
    }
    for (page, names) in ON_PAGES {
        show_page(editor, page);
        // From the sidebar, Tab reaches each field of the page, and
        // Shift+Tab goes back to it.
        let fields: Vec<HWND> = names.iter().filter_map(|n| row_of(editor, n)).map(|i| part(editor, FIELD + i)).collect();
        let mut seen = Vec::new();
        for _ in 0..40 {
            press(VK_TAB);
            let f = focus_of(editor);
            if fields.contains(&f) && !seen.contains(&f) {
                seen.push(f);
            }
        }
        let enabled = fields.iter().filter(|&&f| unsafe { IsWindowEnabled(f) } != 0).count();
        assert!(seen.len() >= enabled, "{}: Tab reached {} of {enabled}", PAGES[page], seen.len());
        // Shift+Tab goes back over them.
        let mut back = Vec::new();
        for _ in 0..40 {
            keyboard(VK_SHIFT, true);
            press(VK_TAB);
            keyboard(VK_SHIFT, false);
            let f = focus_of(editor);
            if fields.contains(&f) && !back.contains(&f) {
                back.push(f);
            }
        }
        assert!(back.len() >= enabled, "{}: Shift+Tab reached {} of {enabled}", PAGES[page], back.len());
    }
    // The DPI: a field of 見た目 grows with it.
    show_page(editor, LOOK_PAGE);
    let width = |w: HWND| window_rect(w).right - window_rect(w).left;
    let f = field(editor, "key_size");
    let w0 = width(f);
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let new = dpi * 3 / 2;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    pump(300);
    assert!((width(f) - w0 * 3 / 2).abs() <= 3, "the field grows: {w0} -> {}", width(f));
    unsafe { SendMessageW(editor, WM_DPICHANGED, (dpi << 16 | dpi) as usize, &suggested as *const _ as isize) };
    pump(300);
}

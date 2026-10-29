//! Acceptance tests for Issue #83: the pages of the settings window with a
//! field for every setting of [general] and [edge.*] (辺と表示 and 操作
//! then), and the figure of the screen's four edges (spec 7.1).
//!
//! Since Issue #107 the rows are on the pages 辺ごとの設定 (the figure and
//! the edge's rows), 見た目, タッチと修飾キー and 表示と非表示.
//!
//! A row of a page is a label, the setting's name, its field (an Edit, a
//! ComboBox, or a check box), a note of what it takes, and 既定に戻す
//! (written in the file) or 既定値 (not written). The tests find a row by
//! its setting's name, control 900 + i, and its other parts by control id:
//! label 600 + i, field 400 + i, note 700 + i, 既定に戻す 500 + i, 既定値
//! 800 + i; accent's #RRGGBB is 22. The
//! figure is control 20; it draws the screen as its client area less 8
//! logical pixels all round, and each edge as a band 12 logical pixels
//! wide along that side. Values are typed as characters, so the keyboard
//! layout does not matter. A debug build is slow to draw and to act: what a
//! test checks, it waits for.

mod common;

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::editor::*;
use common::*;

const DIAGRAM: i32 = 20;
const FIELD: i32 = 400;
const RESET: i32 = 500;
const NOTE: i32 = 700;
const MARK: i32 = 800;
const NAME: i32 = 900;
const ACCENT_HEX: i32 = 22;

/// Some of [general] and [edge.left] written, by hand, with comments.
fn settings(general: &str) -> String {
    format!(
        "; 手で書いた設定\n[general]\nopacity = 70   ; 通常表示\n{general}\n\
         [edge.left]\ncolumns = 2   ; 2列\nhandle = 6, 50%, 0%\n\n\
         [edge.top]\nenabled = false\n\n\
         [set.two]\nkey = A | hold | A\nkey = B | hold | B\n\
         [profile.default]\nleft = two\n"
    )
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 128) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) != 0 }
}

/// The index of the row of setting `name`, if its label is shown.
fn row_of(editor: HWND, name: &str) -> Option<i32> {
    (0..64).find(|i| {
        let n = unsafe { GetDlgItem(editor, NAME + i) };
        !n.is_null() && visible(n) && text_of(n) == name
    })
}

fn row(editor: HWND, name: &str) -> i32 {
    row_of(editor, name).unwrap_or_else(|| panic!("no row 「{name}」 shown"))
}

fn field(editor: HWND, name: &str) -> HWND {
    part(editor, FIELD + row(editor, name))
}

/// The value the field of `name` shows: a check box as true or false.
fn value(editor: HWND, name: &str) -> String {
    let f = field(editor, name);
    match class_of(f).as_str() {
        "Button" => (unsafe { SendMessageW(f, BM_GETCHECK, 0, 0) } == 1).to_string(),
        _ => text_of(f),
    }
}

fn note(editor: HWND, name: &str) -> String {
    text_of(part(editor, NOTE + row(editor, name)))
}

/// Whether `name` shows as not written for it: 既定値 (or 共通), not 既定に戻す.
fn is_default(editor: HWND, name: &str) -> bool {
    let i = row(editor, name);
    visible(part(editor, MARK + i)) && !visible(part(editor, RESET + i))
}

fn click_on(editor: HWND, hwnd: HWND) {
    wait_in_front(editor);
    click(centre(window_rect(hwnd)));
    settle(editor);
}

fn show_page(editor: HWND, page: usize) {
    // Twice at most: a click lost to a busy machine is tried again.
    for _ in 0..2 {
        wait_in_front(editor);
        click(centre(sidebar_item(editor, page)));
        if within_3s(|| heading(editor) == PAGES[page]) {
            return;
        }
    }
    panic!("page {}, not {:?}", PAGES[page], heading(editor));
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
    settle(unsafe { GetForegroundWindow() });
}

fn press(vk: u16) {
    keyboard(vk, true);
    keyboard(vk, false);
}

fn press_with(modifier: u16, vk: u16) {
    keyboard(modifier, true);
    press(vk);
    keyboard(modifier, false);
}

/// Press arrow `vk`, as the arrow keys do: an extended key. Sent plain, it
/// would be the number pad's, and Windows lets Shift go before it.
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
}

fn shift_arrow(vk: u16) {
    keyboard(VK_SHIFT, true);
    arrow(vk);
    keyboard(VK_SHIFT, false);
}

/// Click into the field of `name`, select what is there, and type `text`
/// in its place, not yet entered.
fn type_into(editor: HWND, name: &str, text: &str) {
    click_on(editor, field(editor, name));
    press_with(VK_CONTROL, b'A' as u16);
    type_text(text);
}

/// Type `text` into the field of `name`, and press Enter.
fn enter(editor: HWND, name: &str, text: &str) {
    type_into(editor, name, text);
    press(VK_RETURN);
    settle(editor);
}

/// Choose `option` in the drop-down of `name`, by keyboard: Home, then
/// Down as far as it is.
fn choose_option(editor: HWND, name: &str, option: &str) {
    let f = field(editor, name);
    assert_eq!(class_of(f), "ComboBox", "「{name}」 is chosen from a list");
    let n = unsafe { SendMessageW(f, CB_GETCOUNT, 0, 0) } as usize;
    let at = (0..n)
        .find(|&i| {
            let mut buf = [0u16; 64];
            let len = unsafe { SendMessageW(f, CB_GETLBTEXT, i, buf.as_mut_ptr() as isize) };
            String::from_utf16_lossy(&buf[..len.max(0) as usize]) == option
        })
        .unwrap_or_else(|| panic!("no {option} in 「{name}」"));
    click_on(editor, f);
    press(VK_ESCAPE);
    press(VK_HOME);
    for _ in 0..at {
        press(VK_DOWN);
    }
    settle(editor);
}

fn options(editor: HWND, name: &str) -> Vec<String> {
    let f = field(editor, name);
    let n = unsafe { SendMessageW(f, CB_GETCOUNT, 0, 0) } as usize;
    (0..n)
        .map(|i| {
            let mut buf = [0u16; 64];
            let len = unsafe { SendMessageW(f, CB_GETLBTEXT, i, buf.as_mut_ptr() as isize) };
            String::from_utf16_lossy(&buf[..len.max(0) as usize])
        })
        .collect()
}

fn save(editor: HWND) {
    click_button(editor, "保存");
}

fn colour_at(p: POINT) -> [i32; 3] {
    unsafe { DwmFlush() };
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

/// Open the window and show page `page`.
fn open_page(app: &Running, page: usize) -> HWND {
    let editor = open(app, open_id(app));
    show_page(editor, page);
    editor
}

const EDGES_PAGE: usize = 4;
const LOOK_PAGE: usize = 5;
const TOUCH_PAGE: usize = 6;
const SHOW_PAGE: usize = 7;

/// Give accent `colour`: 色を指定, then #RRGGBB typed and entered; or auto.
fn enter_accent(editor: HWND, colour: &str) {
    if colour == "auto" {
        return choose_option(editor, "accent", "Windows に合わせる");
    }
    choose_option(editor, "accent", "色を指定");
    let hex = part(editor, ACCENT_HEX);
    assert!(within_3s(|| visible(hex)), "#RRGGBB to give");
    click_on(editor, hex);
    press_with(VK_CONTROL, b'A' as u16);
    type_text(colour);
    press(VK_RETURN);
    settle(editor);
}

// AC1
#[test]
fn the_look_page_has_a_field_for_each_display_setting_with_its_range() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, LOOK_PAGE);
    for (name, class, takes) in [
        ("key_size", "Edit", "24〜200"),
        ("opacity", "Edit", "10〜100"),
        ("dim_opacity", "Edit", "10〜100"),
        ("slide_ms", "Edit", "0 でオフ、50〜500"),
        ("theme", "ComboBox", ""),
        ("accent", "ComboBox", ""),
    ] {
        assert_eq!(class_of(field(editor, name)), class, "the field of {name}");
        assert!(note(editor, name).contains(takes), "{name} takes {takes}: {:?}", note(editor, name));
    }
    assert_eq!(options(editor, "theme"), ["ライト", "ダーク", "Windows に合わせる"]);
    assert!(row_of(editor, "latch_ms").is_none(), "操作's settings are not here");
}

/// The band of `edge` in the figure, and its middle, on the screen.
fn band(editor: HWND, edge: &str) -> RECT {
    let r = window_rect(part(editor, DIAGRAM));
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let (inset, wide) = (8 * dpi / 96, 12 * dpi / 96);
    let s = RECT { left: r.left + inset, top: r.top + inset, right: r.right - inset, bottom: r.bottom - inset };
    match edge {
        "left" => RECT { left: s.left, top: s.top + wide, right: s.left + wide, bottom: s.bottom - wide },
        "right" => RECT { left: s.right - wide, top: s.top + wide, right: s.right, bottom: s.bottom - wide },
        "top" => RECT { left: s.left + wide, top: s.top, right: s.right - wide, bottom: s.top + wide },
        _ => RECT { left: s.left + wide, top: s.bottom - wide, right: s.right - wide, bottom: s.bottom },
    }
}

/// The colour of `edge`'s band at `along` (0 to 1) of its length, half
/// way across it.
fn band_at(editor: HWND, edge: &str, along: f32) -> [i32; 3] {
    let b = band(editor, edge);
    let p = if b.right - b.left < b.bottom - b.top {
        POINT { x: (b.left + b.right) / 2, y: b.top + ((b.bottom - b.top) as f32 * along) as i32 }
    } else {
        POINT { x: b.left + ((b.right - b.left) as f32 * along) as i32, y: (b.top + b.bottom) / 2 }
    };
    colour_at(p)
}

fn differs(a: [i32; 3], b: [i32; 3]) -> bool {
    (0..3).any(|i| (a[i] - b[i]).abs() > 24)
}

fn select_edge(app: &Running, editor: HWND, edge: &str) {
    click_on(editor, part(editor, DIAGRAM));
    let b = band(editor, edge);
    click(centre(b));
    let want = format!("settings: edge {edge}");
    assert!(within_3s(|| test_events(app).last() == Some(&want)), "{edge} chosen: {:?}", test_events(app));
}

// AC2
#[test]
fn the_figure_shows_the_four_edges_and_chooses_one() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, EDGES_PAGE);
    let figure = part(editor, DIAGRAM);
    assert!(visible(figure), "a figure of the screen");
    let top = window_rect(figure).top;
    assert!(top <= window_rect(field(editor, "enabled")).top, "at the top of the page");
    // Neither chosen: left is, the window opens on it.
    select_edge(&app, editor, "right");
    // Enabled bottom filled, disabled top only outlined: their middles differ.
    // Clear of the handles, which edges have by default in their middle.
    let (top, bottom) = (band_at(editor, "top", 0.9), band_at(editor, "bottom", 0.9));
    assert!(differs(top, bottom), "enabled filled, disabled outlined: top {top:?}, bottom {bottom:?}");
    // The chosen one stands out from one like it.
    let (left, right) = (band_at(editor, "left", 0.75), band_at(editor, "right", 0.75));
    assert!(differs(left, right), "right chosen, unlike left: {right:?} {left:?}");
    // The left handle, 50% long from the top: drawn over the top half only.
    let (on, off) = (band_at(editor, "left", 0.25), band_at(editor, "left", 0.75));
    assert!(differs(on, off), "the handle on the left edge: {on:?} {off:?}");

    // By keyboard, with the figure focused: → chooses right, ← left.
    click_on(editor, figure);
    press(VK_RIGHT);
    assert!(within_3s(|| test_events(&app).last().map(String::as_str) == Some("settings: edge right")), "→ chose right");
    press(VK_LEFT);
    assert!(within_3s(|| test_events(&app).last().map(String::as_str) == Some("settings: edge left")), "← chose left");
    press(VK_UP);
    assert!(within_3s(|| test_events(&app).last().map(String::as_str) == Some("settings: edge top")), "↑ chose top");

    // A field changed shows at once: the top edge enabled, filled now.
    let before = band_at(editor, "top", 0.9);
    click_on(editor, field(editor, "enabled"));
    assert!(within_3s(|| differs(band_at(editor, "top", 0.9), before)), "the top edge redrawn as enabled");
}

// AC3
#[test]
fn the_chosen_edge_has_its_fields_under_the_figure() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, EDGES_PAGE);
    select_edge(&app, editor, "left");
    for (name, class) in [("enabled", "Button"), ("dock", "Button"), ("monitor", "ComboBox"), ("columns", "Edit"), ("handle", "Button")] {
        assert_eq!(class_of(field(editor, name)), class, "the field of {name}");
    }
    assert!(note(editor, "columns").contains("1〜3"));
    assert_eq!(value(editor, "columns"), "2", "the left edge's columns");
    for (name, takes, now) in [("handle の幅", "4〜8", "6"), ("handle の長さ", "1〜100%", "50"), ("handle の位置", "0〜100%", "0")] {
        assert!(note(editor, name).contains(takes), "{name}: {:?}", note(editor, name));
        assert_eq!(value(editor, name), now, "{name}");
    }
    // Under the figure.
    let figure = window_rect(part(editor, DIAGRAM));
    assert!(window_rect(field(editor, "enabled")).top >= figure.bottom, "under the figure");

    select_edge(&app, editor, "top");
    assert!(row_of(editor, "rows").is_some() && row_of(editor, "columns").is_none(), "top and bottom have rows");
    assert_eq!(value(editor, "enabled"), "false", "the top edge's enabled");
    // No handle: its parts cannot be typed in.
    click_on(editor, field(editor, "handle"));
    if value(editor, "handle") == "true" {
        click_on(editor, field(editor, "handle"));
    }
    assert!(within_3s(|| unsafe { IsWindowEnabled(field(editor, "handle の幅")) } == 0), "no handle, no width");
}

// AC4
#[test]
fn the_touch_and_show_pages_have_a_field_for_each_operation_setting_with_its_range() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, TOUCH_PAGE);
    for (page, name, class, takes) in [
        (TOUCH_PAGE, "latch_ms", "Edit", "50〜2000"),
        (TOUCH_PAGE, "latch", "ComboBox", ""),
        (TOUCH_PAGE, "pen_presses", "Button", ""),
        (SHOW_PAGE, "hover_ms", "Edit", "0 でオフ、100〜5000"),
        (SHOW_PAGE, "auto_hide_ms", "Edit", "0 でオフ、1000〜3600000"),
        (SHOW_PAGE, "hotkey", "Edit", "-"),
    ] {
        if heading(editor) != PAGES[page] {
            show_page(editor, page);
        }
        assert_eq!(class_of(field(editor, name)), class, "the field of {name}");
        // hotkey's 押して取り込む is where its note would be: it has none.
        match takes {
            "-" => assert_eq!(note(editor, name), "", "{name}: no note"),
            _ => assert!(note(editor, name).contains(takes), "{name} takes {takes}: {:?}", note(editor, name)),
        }
    }
    show_page(editor, TOUCH_PAGE);
    assert_eq!(options(editor, "latch"), ["1 回使うと解ける", "もう一度押すまで続く"]);
    assert!(row_of(editor, "opacity").is_none(), "見た目's settings are not here");
}

// AC5
#[test]
fn fields_show_the_file_defaults_marked_and_follow_a_reload() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, LOOK_PAGE);
    assert_eq!(value(editor, "opacity"), "70", "as written");
    assert!(!is_default(editor, "opacity"), "written: 既定に戻す");
    assert_eq!(value(editor, "key_size"), "56", "not written: the default");
    assert!(is_default(editor, "key_size"), "and marked 既定値");
    assert_eq!(value(editor, "theme"), "Windows に合わせる");

    std::fs::write(exe_dir(&app).join("edgepad.ini"), settings("key_size = 64").replace("opacity = 70", "opacity = 40")).unwrap();
    assert!(within_3s(|| child(editor, "Button", "読み直す").is_some()), "told of the change");
    click_button(editor, "読み直す");
    assert!(within_3s(|| value(editor, "opacity") == "40"), "read again: {}", value(editor, "opacity"));
    assert_eq!(value(editor, "key_size"), "64");
    assert!(!is_default(editor, "key_size"));
}

// AC6
#[test]
fn a_change_rewrites_only_its_line_and_adds_what_is_missing() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, LOOK_PAGE);
    let before = file(&app);
    enter(editor, "opacity", "80");
    enter(editor, "key_size", "64");
    choose_option(editor, "theme", "ダーク");
    show_page(editor, EDGES_PAGE);
    select_edge(&app, editor, "bottom");
    click_on(editor, field(editor, "dock"));
    show_page(editor, TOUCH_PAGE);
    click_on(editor, field(editor, "pen_presses"));
    save(editor);
    assert!(before.contains("opacity = 70   ; 通常表示"), "precondition");
    assert!(within_3s(|| file(&app).contains("opacity = 80   ; 通常表示")), "{}", file(&app));
    let saved = file(&app);
    for line in ["; 手で書いた設定", "columns = 2   ; 2列", "handle = 6, 50%, 0%", "key = A | hold | A"] {
        assert!(saved.contains(line), "kept: {line}\n{saved}");
    }
    let general = saved.split("[edge.left]").next().unwrap();
    for line in ["key_size = 64", "theme = dark", "pen_presses = true"] {
        assert!(general.contains(line), "added to [general]: {line}\n{saved}");
    }
    assert!(saved.contains("[edge.bottom]\ndock = true"), "a section added: {saved}");
}

// AC7
#[test]
fn reset_takes_the_line_out_of_the_file() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, LOOK_PAGE);
    let i = row(editor, "opacity");
    click_on(editor, part(editor, RESET + i));
    assert!(within_3s(|| is_default(editor, "opacity")), "shown as the default now");
    let default = value(editor, "opacity");
    assert!(default != "70" && !default.is_empty(), "the default, not what was written: {default}");
    save(editor);
    assert!(within_3s(|| !file(&app).contains("opacity")), "the line gone: {}", file(&app));
}

/// The colour inside the field of `name`, at its right end, clear of what
/// is typed.
fn inside(editor: HWND, name: &str) -> [i32; 3] {
    let r = window_rect(field(editor, name));
    colour_at(POINT { x: r.right - 4, y: (r.top + r.bottom) / 2 })
}

// AC8
#[test]
fn a_value_out_of_range_is_stopped_in_its_field() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, LOOK_PAGE);
    let fine = inside(editor, "opacity");
    // Typing, not yet entered: nothing said.
    type_into(editor, "opacity", "1");
    settle(editor);
    assert!(!status(editor).contains("opacity"), "not stopped while typing: {:?}", status(editor));
    type_into(editor, "opacity", "150");
    press(VK_RETURN);
    assert!(within_3s(|| status(editor).contains("opacity")), "the reason shows: {:?}", status(editor));
    let wrong = inside(editor, "opacity");
    assert!(wrong[0] > wrong[1] + 30 && differs(wrong, fine), "the field shows it wrong: {wrong:?}");
    enter_accent(editor, "#12345");
    assert!(within_3s(|| status(editor).contains("accent")), "accent = #12345 stopped: {:?}", status(editor));
    show_page(editor, SHOW_PAGE);
    enter(editor, "hotkey", "Ctrl+Foo");
    assert!(within_3s(|| status(editor).contains("hotkey")), "hotkey = Ctrl+Foo stopped: {:?}", status(editor));
    save(editor);
    pump(500);
    let saved = file(&app);
    assert!(saved.contains("opacity = 70   ;") && !saved.contains("150") && !saved.contains("Foo") && !saved.contains("#12345"), "{saved}");
    // Put right, the marks go.
    enter(editor, "hotkey", "none");
    show_page(editor, LOOK_PAGE);
    enter_accent(editor, "auto");
    enter(editor, "opacity", "60");
    assert!(within_3s(|| !status(editor).contains("opacity") && !status(editor).contains("accent")), "{:?}", status(editor));
    assert!(within_3s(|| !differs(inside(editor, "opacity"), fine)), "the field looks right again");
}

/// Turn the mouse wheel `notches` over `p` (up for more).
fn wheel(p: POINT, notches: i32) {
    mouse_move_to(p);
    unsafe {
        let mut input: INPUT = zeroed();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.mouseData = (notches * 120) as u32;
        input.Anonymous.mi.dwFlags = MOUSEEVENTF_WHEEL;
        SendInput(1, &input, size_of::<INPUT>() as i32);
    }
    settle(unsafe { WindowFromPoint(p) });
}

// AC9
#[test]
fn the_wheel_and_the_arrows_step_numbers_to_their_ends() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("slide_ms = 50")), None);
    let editor = open_page(&app, LOOK_PAGE);
    click_on(editor, field(editor, "opacity"));
    press(VK_UP);
    assert!(within_3s(|| value(editor, "opacity") == "71"), "↑: {}", value(editor, "opacity"));
    shift_arrow(VK_UP);
    assert!(within_3s(|| value(editor, "opacity") == "81"), "Shift+↑: {}", value(editor, "opacity"));
    for _ in 0..3 {
        shift_arrow(VK_UP);
    }
    assert!(within_3s(|| value(editor, "opacity") == "100"), "stops at the end: {}", value(editor, "opacity"));

    // The wheel steps the field under the cursor, focused or not.
    wheel(centre(window_rect(field(editor, "key_size"))), 1);
    assert!(within_3s(|| value(editor, "key_size") == "57"), "wheel up on key_size: {}", value(editor, "key_size"));
    // Off at 0: down from its least is off, up from off its least.
    click_on(editor, field(editor, "slide_ms"));
    press(VK_DOWN);
    assert!(within_3s(|| value(editor, "slide_ms") == "0"), "50 down: off, {}", value(editor, "slide_ms"));
    press(VK_UP);
    assert!(within_3s(|| value(editor, "slide_ms") == "50"), "off up: 50, {}", value(editor, "slide_ms"));
    press(VK_UP);
    assert!(within_3s(|| value(editor, "slide_ms") == "60"), "by 10: {}", value(editor, "slide_ms"));
    show_page(editor, SHOW_PAGE);
    click_on(editor, field(editor, "auto_hide_ms"));
    press(VK_UP);
    assert!(within_3s(|| value(editor, "auto_hide_ms") == "1000"), "off up: 1000, {}", value(editor, "auto_hide_ms"));
    press(VK_UP);
    assert!(within_3s(|| value(editor, "auto_hide_ms") == "2000"), "by 1000: {}", value(editor, "auto_hide_ms"));

    // Taken at once, as typed and entered.
    save(editor);
    assert!(within_3s(|| file(&app).contains("auto_hide_ms = 2000")), "{}", file(&app));
    let saved = file(&app);
    assert!(saved.contains("opacity = 100   ;") && saved.contains("key_size = 57") && saved.contains("slide_ms = 60"), "{saved}");
}

/// Click the start of the status line, where its link begins.
fn click_link(editor: HWND, line: HWND) {
    wait_in_front(editor);
    let r = window_rect(line);
    click(POINT { x: r.left + 12, y: r.top + 8 });
    settle(editor);
}

// AC10
#[test]
fn a_save_that_fails_leads_to_the_line_at_fault() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, TOUCH_PAGE);
    // Past any field's check, a value the parser turns away.
    change(editor, 150);
    save(editor);
    assert!(within_3s(|| status(editor).contains("150")), "the reason: {:?}", status(editor));
    let line = part(editor, STATUS);
    click_link(editor, line);
    assert!(within_3s(|| heading(editor) == "見た目"), "its page: {:?}", heading(editor));
    assert!(within_3s(|| focus_of(editor) == field(editor, "opacity")), "its field has the focus");
    assert!(status(editor).contains("150"), "the reason stays: {:?}", status(editor));

    // By keyboard: Tab to the reason, Enter.
    show_page(editor, TOUCH_PAGE);
    save(editor);
    let mut reached = false;
    for _ in 0..40 {
        press(VK_TAB);
        if focus_of(editor) == line {
            reached = true;
            break;
        }
    }
    assert!(reached, "the reason takes the focus");
    press(VK_RETURN);
    assert!(within_3s(|| heading(editor) == "見た目" && focus_of(editor) == field(editor, "opacity")), "Enter leads there too");

    // A key's line that does not say what it sends: キーマッピング (Issue
    // #109).
    drop(app);
    let bad = settings("").replace("key = B | hold | B", "key = B | hold");
    let app = launch_with(Some(&bad), None);
    let editor = open_page(&app, TOUCH_PAGE);
    change(editor, 90);
    save(editor);
    assert!(within_3s(|| status(editor).contains("行目")), "the line and reason: {:?}", status(editor));
    click_link(editor, part(editor, STATUS));
    assert!(within_3s(|| heading(editor) == "キーマッピング"), "a key's line: キーマッピング, not {:?}", heading(editor));
    assert!(status(editor).contains("行目"), "the reason stays");

    // An edge's value: that edge chosen, its field focused, the line kept.
    drop(app);
    let bad = settings("").replace("enabled = false", "enabled = maybe");
    let app = launch_with(Some(&bad), None);
    let editor = open_page(&app, TOUCH_PAGE);
    change(editor, 90);
    save(editor);
    assert!(within_3s(|| status(editor).contains("行目")), "the line and reason: {:?}", status(editor));
    click_link(editor, part(editor, STATUS));
    assert!(within_3s(|| heading(editor) == "辺ごとの設定"), "its page");
    assert!(within_3s(|| focus_of(editor) == field(editor, "enabled")), "the top edge's enabled has the focus");
    assert!(test_events(&app).iter().any(|e| e == "settings: edge top"), "the top edge chosen");
    assert!(status(editor).contains("行目"), "the line stays: {:?}", status(editor));
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
    /// Windows's apps light, while the guard lasts.
    fn light(&self) {
        let (key, name) = (wide(PERSONALIZE), wide("AppsUseLightTheme"));
        let light = 1u32;
        unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, &light as *const u32 as _, 4) };
        announce();
    }

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

fn brightness(c: [i32; 3]) -> i32 {
    (c[0] + c[1] + c[2]) / 3
}

// AC11
#[test]
fn the_new_fields_and_figure_follow_the_theme_the_dpi_and_the_keyboard() {
    let _desktop = desktop();
    let dark = Dark::set();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, LOOK_PAGE);
    let _ = within_3s(|| brightness(inside(editor, "opacity")) < 90);
    assert!(brightness(inside(editor, "opacity")) < 90, "a field dark: {:?}", inside(editor, "opacity"));
    show_page(editor, EDGES_PAGE);
    let figure = window_rect(part(editor, DIAGRAM));
    let corner = colour_at(POINT { x: figure.left + 2, y: figure.top + 2 });
    assert!(brightness(corner) < 90, "the figure dark: {corner:?}");
    dark.light();
    show_page(editor, LOOK_PAGE);
    let _ = within_3s(|| brightness(inside(editor, "opacity")) > 170);
    assert!(brightness(inside(editor, "opacity")) > 170, "light again: {:?}", inside(editor, "opacity"));

    // The DPI: fields and the figure grow with it. Across: down the
    // page, rows come closer where the screen is small (CI's 1024 x 768).
    let width = |w: HWND| window_rect(w).right - window_rect(w).left;
    show_page(editor, EDGES_PAGE);
    let (row0, fig0) = (width(field(editor, "monitor")), width(part(editor, DIAGRAM)));
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let new = dpi * 3 / 2;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    settle(editor);
    assert!((width(field(editor, "monitor")) - row0 * 3 / 2).abs() <= 3, "a field grows: {row0} -> {}", width(field(editor, "monitor")));
    assert!((width(part(editor, DIAGRAM)) - fig0 * 3 / 2).abs() <= 3, "the figure grows: {fig0} -> {}", width(part(editor, DIAGRAM)));
    unsafe { SendMessageW(editor, WM_DPICHANGED, (dpi << 16 | dpi) as usize, &suggested as *const _ as isize) };
    settle(editor);
    drop(dark);

    // The keyboard: from the sidebar, Tab reaches the first field, and on
    // 辺ごとの設定 through the figure to the edge's fields.
    wait_in_front(editor);
    click(centre(sidebar_item(editor, LOOK_PAGE)));
    settle(editor);
    press(VK_TAB);
    assert!(within_3s(|| focus_of(editor) == field(editor, "key_size")), "Tab: the first field");
    click(centre(sidebar_item(editor, EDGES_PAGE)));
    settle(editor);
    let figure = part(editor, DIAGRAM);
    let mut reached = false;
    for _ in 0..30 {
        press(VK_TAB);
        if focus_of(editor) == figure {
            reached = true;
            break;
        }
    }
    assert!(reached, "the figure takes the focus");
    press(VK_TAB);
    assert!(within_3s(|| focus_of(editor) == field(editor, "enabled")), "then the edge's first field");
}

// Issue #97 AC5: an edge's field not written takes [edge]'s value, marked
// 共通; 共通に戻す (既定に戻す before Issue #107) goes back to it.
#[test]
fn an_edge_field_not_written_shows_the_common_value_marked() {
    let _desktop = desktop();
    let text = "[edge]\nenabled = false\ncolumns = 2\n\n[edge.left]\ncolumns = 3\n\n\
                [set.two]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nleft = two\n";
    let app = launch_with(Some(text), None);
    let editor = open_page(&app, EDGES_PAGE);
    select_edge(&app, editor, "left");
    let mark = |name: &str| text_of(part(editor, MARK + row(editor, name)));
    assert_eq!(value(editor, "enabled"), "false", "the common value");
    assert!(is_default(editor, "enabled") && mark("enabled") == "共通", "marked 共通: {:?}", mark("enabled"));
    assert!(is_default(editor, "dock") && mark("dock") == "既定値", "no common value: the default");
    assert_eq!(value(editor, "columns"), "3", "the edge's own");
    assert!(!is_default(editor, "columns"));
    click_on(editor, part(editor, RESET + row(editor, "columns")));
    assert!(within_3s(|| value(editor, "columns") == "2"), "back to the common value: {}", value(editor, "columns"));
    assert_eq!(mark("columns"), "共通");
    save(editor);
    assert!(within_3s(|| !file(&app).contains("columns = 3")), "the edge's line gone: {}", file(&app));
}

// Issue #117, AC3: how far the edge keeps off the screen's edge, `gap`, on
// 辺ごとの設定 (and 辺（共通）): a number of logical pixels.
#[test]
fn the_gap_from_the_screens_edge_is_a_field_of_each_edge() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open_page(&app, EDGES_PAGE);
    select_edge(&app, editor, "bottom");
    assert_eq!(class_of(field(editor, "gap")), "Edit", "a number");
    assert!(note(editor, "gap").contains("0〜64"), "{:?}", note(editor, "gap"));
    assert_eq!(value(editor, "gap"), "16", "the bottom's default");
    assert!(is_default(editor, "gap"));
    let label = text_of(part(editor, 600 + row(editor, "gap")));
    assert!(label.contains("画面の端"), "says what it changes: {label:?}");
    let before = file(&app);
    enter(editor, "gap", "24");
    save(editor);
    assert!(within_3s(|| file(&app).contains("gap = 24")), "{}", file(&app));
    let saved = file(&app);
    for line in before.lines() {
        assert!(saved.contains(line), "kept: {line}\n{saved}");
    }
    assert!(saved.contains("[edge.bottom]
gap = 24"), "the bottom's section added with it: {saved}");
}

// Issue #127, AC4: an edge started hidden shows its handle only, at the
// start and when the file is read again; 辺ごとの設定 sets it.
#[test]
fn an_edge_set_to_start_hidden_is_hidden_at_the_start_and_on_a_reload() {
    let _desktop = desktop();
    let text = "[edge.bottom]\nstart = hidden\n[set.pad]\nkey = A | hold | A\n[profile.default]\nleft = pad\nbottom = pad\n";
    let app = launch_with(Some(text), None);
    let m = primary_monitor();
    let bottom_shown = |app: &Running| panels_now(app).iter().any(|r| r.bottom > m.bottom - key_px());
    left_panel(&app);
    pump(300);
    assert!(!bottom_shown(&app), "the bottom edge starts hidden");
    assert!(bottom_handle(&app), "its handle is there to show it");

    // Shown, then the file read again: hidden again.
    choose(&app, "下辺");
    assert!(within_3s(|| bottom_shown(&app)), "precondition: shown from the tray");
    std::fs::write(exe_dir(&app).join("edgepad.ini"), format!("{text}; read again\n")).expect("save edgepad.ini");
    assert!(within_3s(|| !bottom_shown(&app)), "hidden again on a reload");
    left_panel(&app);

    // 辺ごとの設定: 起動した時, shown or hidden.
    let editor = open_page(&app, EDGES_PAGE);
    select_edge(&app, editor, "bottom");
    assert_eq!(options(editor, "start"), ["出す", "隠す"]);
    assert_eq!(value(editor, "start"), "隠す");
    choose_option(editor, "start", "出す");
    save(editor);
    assert!(within_3s(|| file(&app).contains("start = shown")), "{}", file(&app));
}

/// Whether a handle of `app` is shown against the bottom of the primary
/// monitor.
fn bottom_handle(app: &Running) -> bool {
    let m = primary_monitor();
    let name = wide("EdgePadHandle");
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), std::ptr::null()) };
        if hwnd.is_null() {
            return false;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        let r = window_rect(hwnd);
        if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 && r.bottom > m.bottom - key_px() && r.top > m.top + key_px() {
            return true;
        }
    }
}

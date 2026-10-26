//! Acceptance tests for Issue #83: the fields of the settings window's
//! 一般 and 辺 tabs, for every setting of [general] and [edge.*] (spec 7.1).
//!
//! A row of a tab is a label with the setting's name, then its field, a
//! note of what it takes, and 既定に戻す (written in the file) or 既定値 (not
//! written): windows in that order, as a label goes before what it names.
//! A field is a box holding an edit control, or a choice drawn as segments,
//! its text the value chosen. Values are typed as characters, so the
//! keyboard layout does not matter. A debug build is slow to draw and to
//! act: what a test checks, it waits for.

mod common;

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const EDITOR_CLASS: &str = "EdgePadSettings";
const TAB_CLASS: &str = "EdgePadSettingsTab";
const BUTTON_CLASS: &str = "EdgePadSettingsButton";
const STATUS_CLASS: &str = "EdgePadSettingsStatus";
const LABEL_CLASS: &str = "EdgePadSettingsLabel";
const NOTE_CLASS: &str = "EdgePadSettingsNote";
const FIELD_CLASS: &str = "EdgePadSettingsField";
const CHOICE_CLASS: &str = "EdgePadSettingsChoice";

/// Some of [general] and [edge.left] written, by hand, with comments; the
/// left edge shows two keys, in two columns.
fn settings(general: &str) -> String {
    format!(
        "; 手で書いた設定\n[general]\nopacity = 70   ; 通常表示\nkey_size = 64\n{general}\n\
         [edge.left]\ncolumns = 2   ; 2列\nhandle = 5, 30%\n\n\
         [set.two]\nkey = A | hold | A\nkey = B | hold | B\n\
         [profile.default]\nleft = two\n"
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

/// Choose 設定… in the tray without opening its menu, and return the
/// window once it is shown.
fn open(app: &Running) -> HWND {
    let id = menu_item(app, "設定…").id;
    let tray = find_window(app, TRAY_CLASS, false);
    unsafe { PostMessageW(tray, WM_COMMAND, id as usize, 0) };
    let editor = find_window(app, EDITOR_CLASS, true);
    // Its rows placed and shown.
    assert!(within_3s(|| label(editor, "opacity").is_some()), "the window opens on 一般, its rows shown");
    editor
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 128) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The text of `hwnd`, asked of it: GetWindowText reads no edit control
/// of another process.
fn text_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { SendMessageW(hwnd, WM_GETTEXT, 512, buf.as_mut_ptr() as isize) } as i32;
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) != 0 }
}

/// The children of `parent`, in order.
fn children(parent: HWND) -> Vec<HWND> {
    let mut found = Vec::new();
    let mut hwnd = unsafe { GetWindow(parent, GW_CHILD) };
    while !hwnd.is_null() {
        found.push(hwnd);
        hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) };
    }
    found
}

/// The child of `parent` of `class` and `text`, if one is shown.
fn child(parent: HWND, class: &str, text: &str) -> Option<HWND> {
    children(parent).into_iter().find(|&c| class_of(c) == class && text_of(c) == text && visible(c))
}

/// The label of setting `name`, if it is shown.
fn label(editor: HWND, name: &str) -> Option<HWND> {
    child(editor, LABEL_CLASS, name)
}

/// What follows the label of `name`, up to the next label: its field, note,
/// and 既定に戻す or 既定値.
fn row(editor: HWND, name: &str) -> Vec<HWND> {
    let label = label(editor, name).unwrap_or_else(|| panic!("no row 「{name}」 shown"));
    children(editor).into_iter().skip_while(|&c| c != label).skip(1).take_while(|&c| class_of(c) != LABEL_CLASS).collect()
}

fn field(editor: HWND, name: &str) -> HWND {
    row(editor, name)[0]
}

/// The edit control in the field of `name`.
fn edit(editor: HWND, name: &str) -> HWND {
    let f = field(editor, name);
    assert_eq!(class_of(f), FIELD_CLASS, "「{name}」 is typed in");
    let e = children(f).into_iter().find(|&c| class_of(c).eq_ignore_ascii_case("Edit"));
    e.unwrap_or_else(|| panic!("no edit control in 「{name}」"))
}

/// The value the field of `name` shows.
fn value(editor: HWND, name: &str) -> String {
    let f = field(editor, name);
    if class_of(f) == FIELD_CLASS { text_of(edit(editor, name)) } else { text_of(f) }
}

/// The note of what `name` takes.
fn note(editor: HWND, name: &str) -> String {
    row(editor, name)
        .into_iter()
        .filter(|&c| class_of(c) == NOTE_CLASS && visible(c) && text_of(c) != "既定値")
        .map(text_of)
        .collect()
}

/// Whether `name` shows as not written in the file: 既定値, not 既定に戻す.
fn is_default(editor: HWND, name: &str) -> bool {
    let row = row(editor, name);
    let shown = |class: &str, text: &str| row.iter().any(|&c| class_of(c) == class && text_of(c) == text && visible(c));
    shown(NOTE_CLASS, "既定値") && !shown(BUTTON_CLASS, "既定に戻す")
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

fn click_on(editor: HWND, hwnd: HWND) {
    wait_in_front(editor);
    click(centre(window_rect(hwnd)));
    pump(300);
}

fn click_child(editor: HWND, class: &str, text: &str) {
    let c = child(editor, class, text).unwrap_or_else(|| panic!("no {class} 「{text}」"));
    click_on(editor, c);
}

fn show_tab(editor: HWND, name: &str) {
    click_child(editor, TAB_CLASS, name);
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

fn press_with(modifier: u16, vk: u16) {
    keyboard(modifier, true);
    press(vk);
    keyboard(modifier, false);
}

/// Click into the field of `name`, select what is there, and type `text`
/// in its place, not yet entered.
fn type_into(editor: HWND, name: &str, text: &str) {
    click_on(editor, edit(editor, name));
    press_with(VK_CONTROL, b'A' as u16);
    type_text(text);
}

/// Type `text` into the field of `name`, and press Enter.
fn enter(editor: HWND, name: &str, text: &str) {
    type_into(editor, name, text);
    press(VK_RETURN);
    pump(300);
}

/// Click option `i` of the `n` of the choice of `name`.
fn choose_option(editor: HWND, name: &str, i: i32, n: i32) {
    let f = field(editor, name);
    assert_eq!(class_of(f), CHOICE_CLASS, "「{name}」 is chosen");
    let r = window_rect(f);
    let w = r.right - r.left;
    wait_in_front(editor);
    click(POINT { x: r.left + w * (2 * i + 1) / (2 * n), y: (r.top + r.bottom) / 2 });
    pump(300);
}

fn status(editor: HWND) -> String {
    let s = children(editor).into_iter().find(|&c| class_of(c) == STATUS_CLASS).expect("a status line");
    text_of(s)
}

fn file(app: &Running) -> String {
    std::fs::read_to_string(exe_dir(app).join("edgepad.ini")).expect("edgepad.ini")
}

fn save(editor: HWND) {
    click_child(editor, BUTTON_CLASS, "保存");
}

/// The window with the keyboard focus in `editor`'s thread.
fn focus(editor: HWND) -> HWND {
    let thread = unsafe { GetWindowThreadProcessId(editor, null_mut()) };
    let mut info: GUITHREADINFO = unsafe { zeroed() };
    info.cbSize = size_of::<GUITHREADINFO>() as u32;
    unsafe { GetGUIThreadInfo(thread, &mut info) };
    info.hwndFocus
}

fn colour_at(p: POINT) -> [i32; 3] {
    unsafe { DwmFlush() };
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

/// The colour inside the edit of `name`, at its right end, clear of what
/// is typed.
fn inside(editor: HWND, name: &str) -> [i32; 3] {
    let r = window_rect(edit(editor, name));
    colour_at(POINT { x: r.right - 3, y: (r.top + r.bottom) / 2 })
}

/// How `hwnd` looks now.
fn look(hwnd: HWND) -> Vec<u32> {
    unsafe { DwmFlush() };
    capture(window_rect(hwnd))
}

fn left_panel_width(app: &Running) -> i32 {
    let m = primary_monitor();
    panels_now(app).iter().find(|r| r.left == m.left).map_or(0, |r| r.right - r.left)
}

// AC1
#[test]
fn the_general_tab_has_a_field_for_every_setting() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    // Typed in, with what they take.
    for (name, takes) in [
        ("hotkey", "none"),
        ("key_size", "24〜200"),
        ("opacity", "10〜100"),
        ("dim_opacity", "10〜100"),
        ("latch_ms", "50〜2000"),
        ("hover_ms", "100〜5000"),
        ("auto_hide_ms", "1000〜3600000"),
        ("slide_ms", "50〜500"),
        ("accent", "#RRGGBB"),
    ] {
        assert!(label(editor, name).is_some(), "a row 「{name}」");
        let _ = edit(editor, name);
        assert!(note(editor, name).contains(takes), "{name} takes {takes}: {:?}", note(editor, name));
    }
    for name in ["hover_ms", "auto_hide_ms", "slide_ms"] {
        assert!(note(editor, name).contains("0 でオフ"), "{name} is off at 0: {:?}", note(editor, name));
    }
    // Chosen.
    for (name, now) in [("latch", "oneshot"), ("theme", "auto"), ("pen_presses", "false")] {
        assert_eq!(class_of(field(editor, name)), CHOICE_CLASS, "「{name}」 is chosen");
        assert_eq!(value(editor, name), now);
    }
}

// AC2
#[test]
fn the_edge_tab_has_a_field_for_every_setting_of_the_edge_chosen() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    show_tab(editor, "辺");
    assert!(within_3s(|| label(editor, "opacity").is_none()), "[general] is on its own tab");
    assert!(within_3s(|| label(editor, "辺").is_some()), "the edge to edit is chosen");
    assert_eq!(value(editor, "辺"), "left");
    for (i, edge) in ["left", "right", "top", "bottom"].into_iter().enumerate() {
        choose_option(editor, "辺", i as i32, 4);
        assert!(within_3s(|| value(editor, "辺") == edge), "{edge} chosen: {:?}", value(editor, "辺"));
        let side = i < 2;
        let (lines, not) = if side { ("columns", "rows") } else { ("rows", "columns") };
        assert!(within_3s(|| label(editor, lines).is_some()), "{edge}: {lines}");
        assert!(label(editor, not).is_none(), "{edge}: no {not}");
        assert!(note(editor, lines).contains("1〜3"), "{edge}: {lines} takes 1〜3");
        let _ = edit(editor, lines);
        let _ = edit(editor, "monitor");
        for name in ["enabled", "dock", "handle"] {
            assert_eq!(class_of(field(editor, name)), CHOICE_CLASS, "{edge}: 「{name}」 is chosen");
        }
        assert!(note(editor, "monitor").contains("primary / cursor / 1〜16"), "{edge}: {:?}", note(editor, "monitor"));
        for (name, takes) in [("handle の幅", "4〜8"), ("handle の長さ", "1〜100%"), ("handle の位置", "0〜100%")] {
            let _ = edit(editor, name);
            assert!(note(editor, name).contains(takes), "{edge}: {name} takes {takes}: {:?}", note(editor, name));
        }
    }
}

// AC3
#[test]
fn the_fields_show_the_file_and_the_defaults_and_follow_a_change_outside() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    for (name, now) in [("opacity", "70"), ("key_size", "64")] {
        assert_eq!(value(editor, name), now, "{name} as written");
        assert!(!is_default(editor, name), "{name} is written in the file");
    }
    for (name, now) in [("hotkey", "none"), ("dim_opacity", "35"), ("latch", "oneshot"), ("theme", "auto"), ("accent", "auto"), ("hover_ms", "0")] {
        assert_eq!(value(editor, name), now, "{name}: its default");
        assert!(is_default(editor, name), "{name} shows as the default");
    }

    show_tab(editor, "辺");
    assert!(within_3s(|| label(editor, "columns").is_some()));
    for (name, now) in [("columns", "2"), ("handle", "on"), ("handle の幅", "5"), ("handle の長さ", "30"), ("handle の位置", "50")] {
        assert_eq!(value(editor, name), now, "left: {name}");
    }
    assert!(!is_default(editor, "columns") && !is_default(editor, "handle"), "left: written");
    assert!(is_default(editor, "monitor") && value(editor, "monitor") == "primary", "left: monitor, its default");
    choose_option(editor, "辺", 1, 4);
    assert!(within_3s(|| value(editor, "columns") == "1"), "right: columns, its default");
    assert!(is_default(editor, "columns") && is_default(editor, "handle"));
    assert_eq!(value(editor, "handle の幅"), "6", "right: the handle's default width");

    // Changed outside, and read again: the fields show it.
    std::fs::write(exe_dir(&app).join("edgepad.ini"), settings("").replace("opacity = 70", "opacity = 50")).unwrap();
    assert!(within_3s(|| child(editor, BUTTON_CLASS, "読み直す").is_some()), "told of the change");
    click_child(editor, BUTTON_CLASS, "読み直す");
    show_tab(editor, "一般");
    assert!(within_3s(|| label(editor, "opacity").is_some() && value(editor, "opacity") == "50"), "opacity as changed: {:?}", value(editor, "opacity"));
}

// AC4
#[test]
fn a_field_changed_writes_its_line_only_and_saved_it_takes_effect() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    let before = file(&app);

    enter(editor, "opacity", "60");
    choose_option(editor, "theme", 1, 3);
    assert!(within_3s(|| value(editor, "theme") == "dark"), "dark chosen");
    show_tab(editor, "辺");
    assert!(within_3s(|| label(editor, "辺").is_some()));
    choose_option(editor, "辺", 2, 4);
    assert!(within_3s(|| label(editor, "rows").is_some()), "top chosen");
    enter(editor, "rows", "2");
    save(editor);
    let want = before.replace("opacity = 70   ;", "opacity = 60   ;").replace("key_size = 64\n", "key_size = 64\ntheme = dark\n")
        + "\n[edge.top]\nrows = 2\n";
    let _ = within_3s(|| file(&app) == want);
    assert_eq!(file(&app), want, "each line changed in its place, its comment kept; new ones added");

    // Saved, the panels follow it.
    let width = left_panel_width(&app);
    assert!(width > 0, "precondition: the left panel is shown");
    show_tab(editor, "一般");
    enter(editor, "key_size", "80");
    save(editor);
    assert!(within_3s(|| left_panel_width(&app) > width), "larger keys: {width} -> {}", left_panel_width(&app));
}

// AC5
#[test]
fn back_to_the_default_takes_the_line_out() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    let before = file(&app);
    let width = left_panel_width(&app);
    assert!(width > 0, "precondition: the left panel is shown");

    for (name, default) in [("opacity", "90"), ("key_size", "56")] {
        let reset = row(editor, name).into_iter().find(|&c| text_of(c) == "既定に戻す" && visible(c)).expect("既定に戻す");
        click_on(editor, reset);
        assert!(within_3s(|| value(editor, name) == default), "{name}: its default, {:?}", value(editor, name));
        assert!(is_default(editor, name), "{name} shows as the default");
    }
    show_tab(editor, "辺");
    assert!(within_3s(|| label(editor, "handle").is_some()));
    let reset = row(editor, "handle").into_iter().find(|&c| text_of(c) == "既定に戻す" && visible(c)).expect("既定に戻す");
    click_on(editor, reset);
    assert!(within_3s(|| value(editor, "handle の幅") == "6"), "the handle's default width");
    assert!(is_default(editor, "handle"));
    save(editor);
    let want = before.replace("opacity = 70   ; 通常表示\n", "").replace("key_size = 64\n", "").replace("handle = 5, 30%\n", "");
    let _ = within_3s(|| file(&app) == want);
    assert_eq!(file(&app), want, "the lines taken out, nothing else");
    assert!(within_3s(|| left_panel_width(&app) < width), "keys of the default size: {width} -> {}", left_panel_width(&app));
}

// AC6
#[test]
fn a_value_out_of_range_or_malformed_stops_at_its_field() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    let before = file(&app);

    // A field as it is with nothing wrong: dim_opacity's, untouched. Asked
    // each time, once drawn: a debug build on CI draws late.
    let fine = || inside(editor, "dim_opacity");

    // Part way through typing, nothing is said.
    type_into(editor, "opacity", "1");
    pump(500);
    assert_eq!(status(editor), "", "not yet entered");
    let _ = within_3s(|| inside(editor, "opacity") == fine());
    assert_eq!(inside(editor, "opacity"), fine(), "not yet entered: not marked");

    for (name, bad, said) in [("opacity", "150", "150"), ("hotkey", "Ctrl+Foo", "Foo"), ("accent", "#12345", "#12345")] {
        let normal = fine();
        enter(editor, name, bad);
        assert!(within_3s(|| status(editor).contains(said)), "{name} = {bad}: the reason, {:?}", status(editor));
        assert!(status(editor).contains(name), "{name}: named in the reason, {:?}", status(editor));
        let _ = within_3s(|| inside(editor, name) != normal);
        assert_ne!(inside(editor, name), normal, "{name} = {bad}: the field shows it is wrong");
        assert_eq!(value(editor, name), bad, "what was typed stays, to correct");
    }
    save(editor);
    let _ = within_3s(|| status(editor).contains("保存しました"));
    assert_eq!(file(&app), before, "none of them written");
    assert!(status(editor).contains("保存していない"), "saved, still told of what was not: {:?}", status(editor));

    enter(editor, "opacity", "80");
    let _ = within_3s(|| inside(editor, "opacity") == fine());
    assert_eq!(inside(editor, "opacity"), fine(), "corrected: the field as it was");
    assert!(!status(editor).contains("150"), "and the reason gone: {:?}", status(editor));
}

// AC7
#[test]
fn the_keyboard_alone_moves_chooses_and_presses() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    let key_size = field(editor, "key_size");
    let unfocused = look(key_size);

    // Tab and Shift+Tab along the fields, and 既定に戻す of key_size,
    // written in the file.
    click_on(editor, edit(editor, "hotkey"));
    press(VK_TAB);
    assert!(within_3s(|| focus(editor) == edit(editor, "key_size")), "Tab: on to key_size");
    let _ = within_3s(|| look(key_size) != unfocused);
    assert_ne!(look(key_size), unfocused, "key_size looks focused");
    press(VK_TAB);
    let reset = row(editor, "key_size").into_iter().find(|&c| text_of(c) == "既定に戻す").expect("既定に戻す");
    assert!(within_3s(|| focus(editor) == reset), "Tab: on to 既定に戻す");
    press_with(VK_SHIFT, VK_TAB);
    assert!(within_3s(|| focus(editor) == edit(editor, "key_size")), "Shift+Tab: back to key_size");

    // Arrows change a choice; Space and Enter turn one on and off.
    click_on(editor, edit(editor, "latch_ms"));
    press(VK_TAB);
    assert!(within_3s(|| focus(editor) == field(editor, "latch")), "Tab: on to latch");
    press(VK_DOWN);
    assert!(within_3s(|| value(editor, "latch") == "lock"), "Down: lock");
    press(VK_UP);
    assert!(within_3s(|| value(editor, "latch") == "oneshot"), "Up: oneshot");
    // Chosen, latch is written: its 既定に戻す comes next.
    press(VK_TAB);
    press(VK_TAB);
    assert!(within_3s(|| focus(editor) == field(editor, "pen_presses")), "Tab: on to pen_presses");
    press(VK_SPACE);
    assert!(within_3s(|| value(editor, "pen_presses") == "true"), "Space: on");
    press(VK_RETURN);
    assert!(within_3s(|| value(editor, "pen_presses") == "false"), "Enter: off");
    press(VK_SPACE);
    assert!(within_3s(|| value(editor, "pen_presses") == "true"), "Space: on again");

    // Past the last field, 保存, pressed with Enter.
    let save = child(editor, BUTTON_CLASS, "保存").expect("保存");
    let unfocused = look(save);
    click_on(editor, edit(editor, "slide_ms"));
    press(VK_TAB);
    assert!(within_3s(|| focus(editor) == save), "Tab: on to 保存");
    let _ = within_3s(|| look(save) != unfocused);
    assert_ne!(look(save), unfocused, "保存 looks focused");
    press(VK_RETURN);
    assert!(within_3s(|| file(&app).contains("pen_presses = true")), "Enter: saved, {}", file(&app));
    assert!(file(&app).contains("latch = oneshot"), "{}", file(&app));

    // Before the first field, the tabs, shown with Space.
    click_on(editor, edit(editor, "hotkey"));
    press_with(VK_SHIFT, VK_TAB);
    let last = child(editor, TAB_CLASS, "プロファイル").expect("a tab");
    assert!(within_3s(|| focus(editor) == last), "Shift+Tab: back to the tabs");
    press(VK_SPACE);
    let want = "settings: tab プロファイル".to_string();
    assert!(within_3s(|| test_events(&app).last() == Some(&want)), "Space: the tab shown, {:?}", test_events(&app));
}

// AC8
#[test]
fn the_fields_follow_the_theme_and_the_dpi() {
    let _desktop = desktop();
    for theme in ["dark", "light"] {
        let app = launch_with(Some(&settings(&format!("theme = {theme}"))), None);
        let editor = open(&app);
        wait_in_front(editor);
        let dark = |c: [i32; 3]| c.iter().sum::<i32>() < 3 * 128;
        let _ = within_3s(|| dark(inside(editor, "key_size")) == (theme == "dark"));
        let c = inside(editor, "key_size");
        assert_eq!(dark(c), theme == "dark", "{theme}: a field of the theme, not {c:?}");
        let choice = field(editor, "latch");
        let r = window_rect(choice);
        let pixels = look(choice);
        // In lock, the option not chosen: drawn as a key, not in the
        // accent. Inside its right end, past the frame (2 logical px).
        let (w, inset) = (r.right - r.left, 5 * unsafe { GetDpiForWindow(editor) } as i32 / 96);
        let lock = channels(pixels[((r.bottom - r.top) / 2 * w + w - 1 - inset) as usize]);
        assert_eq!(dark(lock), theme == "dark", "{theme}: a choice of the theme too, not {lock:?}");
        drop(app);
    }

    let app = launch_with(Some(&settings("")), None);
    let editor = open(&app);
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let height = |hwnd: HWND| window_rect(hwnd).bottom - window_rect(hwnd).top;
    let (box0, edit0, choice0) = (height(field(editor, "key_size")), height(edit(editor, "key_size")), height(field(editor, "latch")));
    // Half as large again, as far as the window takes (to 480).
    let new = (dpi * 3 / 2).min(480);
    let grown = |v: i32| v * new / dpi;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    pump(300);
    let (box1, edit1, choice1) = (height(field(editor, "key_size")), height(edit(editor, "key_size")), height(field(editor, "latch")));
    assert!((box1 - grown(box0)).abs() <= 2, "the field grows with the DPI: {box0} -> {box1}");
    assert!((edit1 - grown(edit0)).abs() <= 3, "and its text: {edit0} -> {edit1}");
    assert!((choice1 - grown(choice0)).abs() <= 2, "and a choice: {choice0} -> {choice1}");
}

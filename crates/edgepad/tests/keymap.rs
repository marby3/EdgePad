//! Acceptance tests for Issue #87: the キー配置 page of the settings window,
//! which edits key sets and their keys without writing the settings file
//! (spec 7.1).
//!
//! The tests find the page's parts by control id:
//! - the key sets: the drop-down list 1000, the name field 1001 (an Edit,
//!   shown while a set is named), 追加 1002, 名前の変更 1003, 削除 1004, and
//!   where the set is used 1005 (a Static);
//! - the sample of the panel 1006, and under it 追加 1007, 削除 1008, ↑ 1009
//!   and ↓ 1010 for its keys;
//! - the chosen key's label 1011 (an Edit), 「パネルでは切れます」 1012 (a
//!   Static, shown when the label is cut), the kind's description 1013 (a
//!   Static), its width 1014 (an Edit), and the nine kinds 1020 to 1028
//!   (buttons, the chosen one checked);
//! - the keys sent: Ctrl, Shift, Alt and Win 1030 to 1033 (buttons, checked
//!   when on), the five groups of keys 1040 to 1044, the keys of the group
//!   1050 (a ListBox, the key sent chosen), 押して取り込む 1051, and the keys
//!   sent as written 1052 (a Static);
//! - the kinds' own: the interval 1053, the macro's steps 1054 (a ListBox)
//!   with キー操作を追加 1055, 待ち時間を追加 1056, 削除 1057, ↑ 1058 and ↓
//!   1059, a wait's time 1060, the text or what is run 1061 (an Edit), the
//!   page's target 1062 and the pie's key set 1063 (drop-down lists), the
//!   pie's figure 1064 and what it says of its items 1065 (a Static);
//! - on 操作, 押して取り込む beside hotkey: 21.
//!
//! The sample draws the panel 8 logical pixels in from its top left corner,
//! keys 44 logical pixels across (as long as the set fits), in the panel's
//! colours: the chosen key as a held one. Around the panel, it is the
//! sidebar's colour. A key's colour is read clear of its label, 10 logical
//! pixels in from its cell's corner. Values are typed as characters,
//! so the keyboard layout does not matter. A debug build is slow to draw
//! and to act: what a test checks, it waits for.

mod common;

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::Graphics::Gdi::MapWindowPoints;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::editor::*;
use common::*;

const SET: i32 = 1000;
const NAME: i32 = 1001;
const ADD_SET: i32 = 1002;
const RENAME: i32 = 1003;
const REMOVE_SET: i32 = 1004;
const USED: i32 = 1005;
const SAMPLE: i32 = 1006;
const ADD_KEY: i32 = 1007;
const REMOVE_KEY: i32 = 1008;
const KEY_UP: i32 = 1009;
const KEY_DOWN: i32 = 1010;
const LABEL: i32 = 1011;
const CUT: i32 = 1012;
const ABOUT: i32 = 1013;
const WIDTH: i32 = 1014;
const KINDS: i32 = 1020;
const MODS: i32 = 1030;
const GROUPS: i32 = 1040;
const KEYS: i32 = 1050;
const CAPTURE: i32 = 1051;
const SENDS: i32 = 1052;
const INTERVAL: i32 = 1053;
const STEPS: i32 = 1054;
const STEP_KEY: i32 = 1055;
const STEP_WAIT: i32 = 1056;
const STEP_REMOVE: i32 = 1057;
const STEP_UP: i32 = 1058;
const WAIT: i32 = 1060;
const CONTENT: i32 = 1061;
const TARGET: i32 = 1062;
const PIE: i32 = 1063;
const PIE_FIGURE: i32 = 1064;
const PIE_NOTE: i32 = 1065;
const HOTKEY_CAPTURE: i32 = 21;

/// The kinds, as the buttons 1020 to 1028 name them.
const KIND_NAMES: [&str; 9] = ["ショートカット", "修飾キー", "押している間", "連打", "マクロ", "文字", "起動", "ページ", "パイ"];
const SHORTCUT: i32 = 0;
const REPEAT: i32 = 3;
const MACRO: i32 = 4;
const TEXT: i32 = 5;
const RUN: i32 = 6;
const PAGE: i32 = 7;
const PIE_KIND: i32 = 8;
const CTRL_ON: i32 = 0;
const ALT_ON: i32 = 2;

/// Logical pixels: the sample's inset, and its keys.
const INSET: i32 = 8;
const KEY: i32 = 44;

/// Key sets used on edges, by a pie key and a page key, and one used
/// nowhere; written by hand, with comments.
const SETTINGS: &str = "; 手で書いた設定\n\
[general]\n\
theme = light\n\
accent = #C00000\n\
\n\
[edge.right]\n\
columns = 2\n\
\n\
[set.mods]   ; 修飾キー\n\
key = Esc   | shortcut | Esc\n\
key = Ctrl  | modifier | Ctrl   ; 左手\n\
key = Space | hold     | Space\n\
\n\
[set.tools]\n\
key = 元に戻す | shortcut | Ctrl+Z\n\
key = ブラシ   | repeat   | [        | interval=60\n\
key = 変形     | pie      | transform\n\
key = 次へ     | page     | next\n\
key = 電卓     | run      | calc.exe\n\
\n\
[set.transform]\n\
key = 拡大 | shortcut | Ctrl+T\n\
key = 反転 | shortcut | F14\n\
\n\
[set.spare]\n\
key = A | shortcut | A\n\
\n\
[profile.default]\n\
left = mods\n\
\n\
[profile.csp]\n\
process = CLIPStudioPaint.exe\n\
left = mods\n\
right = tools\n";

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 128) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) != 0 }
}

fn checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == 1 }
}

fn click_on(editor: HWND, hwnd: HWND) {
    wait_in_front(editor);
    click(centre(window_rect(hwnd)));
    pump(300);
}

/// Click the part `id` of `editor`.
fn click_id(editor: HWND, id: i32) {
    let hwnd = part(editor, id);
    assert!(visible(hwnd), "part {id} shown");
    click_on(editor, hwnd);
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

fn press_with(modifiers: &[u16], vk: u16) {
    for &m in modifiers {
        keyboard(m, true);
    }
    press(vk);
    for &m in modifiers.iter().rev() {
        keyboard(m, false);
    }
}

/// Press arrow `vk` as the arrow keys do, an extended key, with
/// `modifiers` held.
fn arrow(modifiers: &[u16], vk: u16) {
    for &m in modifiers {
        keyboard(m, true);
    }
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
    for &m in modifiers.iter().rev() {
        keyboard(m, false);
    }
    pump(200);
}

/// Click into the Edit `id`, select what is there, type `text` in its
/// place, and press Enter.
fn enter(editor: HWND, id: i32, text: &str) {
    click_id(editor, id);
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text(text);
    press(VK_RETURN);
    pump(300);
}

/// The items of the ComboBox or ListBox `hwnd`.
fn items(hwnd: HWND) -> Vec<String> {
    let list = class_of(hwnd) == "ListBox";
    let (count, text) = if list { (LB_GETCOUNT, LB_GETTEXT) } else { (CB_GETCOUNT, CB_GETLBTEXT) };
    let n = unsafe { SendMessageW(hwnd, count, 0, 0) }.max(0) as usize;
    (0..n)
        .map(|i| {
            let mut buf = [0u16; 128];
            let len = unsafe { SendMessageW(hwnd, text, i, buf.as_mut_ptr() as isize) };
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

/// Choose `option` in the drop-down list `id`, by keyboard: Home, then Down
/// as far as it is.
fn choose(editor: HWND, id: i32, option: &str) {
    let combo = part(editor, id);
    let at = items(combo).iter().position(|o| o == option).unwrap_or_else(|| panic!("no {option} in {:?}", items(combo)));
    click_on(editor, combo);
    press(VK_ESCAPE);
    press(VK_HOME);
    for _ in 0..at {
        press(VK_DOWN);
    }
    pump(300);
    assert!(within_3s(|| chosen(combo).as_deref() == Some(option)), "{option} chosen in {id}");
}

/// Click item `option` of the ListBox `id`.
fn pick(editor: HWND, id: i32, option: &str) {
    let list = part(editor, id);
    let at = items(list).iter().position(|o| o == option).unwrap_or_else(|| panic!("no {option} in {:?}", items(list)));
    unsafe { SendMessageW(list, LB_SETTOPINDEX, at, 0) };
    pump(100);
    let mut r: RECT = unsafe { zeroed() };
    unsafe { SendMessageW(list, LB_GETITEMRECT, at, &mut r as *mut RECT as isize) };
    let mut p = [POINT { x: r.left, y: r.top }, POINT { x: r.right, y: r.bottom }];
    unsafe { MapWindowPoints(list, null_mut(), p.as_mut_ptr(), 2) };
    wait_in_front(editor);
    click(centre(RECT { left: p[0].x, top: p[0].y, right: p[1].x, bottom: p[1].y }));
    pump(300);
}

fn text(editor: HWND, id: i32) -> String {
    text_of(part(editor, id))
}

fn shown(editor: HWND, id: i32) -> bool {
    visible(part(editor, id))
}

fn save(editor: HWND) {
    click_button(editor, "保存");
}

/// The key set section `name` of `saved`: its header to the next section.
fn section<'a>(saved: &'a str, name: &str) -> &'a str {
    let start = saved.find(&format!("[set.{name}]")).unwrap_or_else(|| panic!("no [set.{name}] in\n{saved}"));
    let rest = &saved[start + 1..];
    &saved[start..start + 1 + rest.find("\n[").unwrap_or(rest.len())]
}

/// The key lines of set `name` in `saved`.
fn key_lines(saved: &str, name: &str) -> Vec<String> {
    section(saved, name).lines().filter(|l| l.trim_start().starts_with("key")).map(String::from).collect()
}

/// The fields of each key of set `name` in `saved`, as the parser splits
/// them: label, kind, content and options, without the spaces around.
fn keys_of(saved: &str, name: &str) -> Vec<Vec<String>> {
    key_lines(saved, name)
        .iter()
        .map(|l| {
            let value = l.split_once('=').unwrap().1;
            let value = value.split(" ;").next().unwrap();
            value.split('|').map(|f| f.trim().to_string()).collect()
        })
        .collect()
}

/// The fields of the key labelled `label` of set `name` in `saved`.
fn key_of(saved: &str, name: &str, label: &str) -> Vec<String> {
    keys_of(saved, name).into_iter().find(|k| k[0] == label).unwrap_or_else(|| panic!("no key {label} in\n{saved}"))
}

/// Save, and wait for the file to have `line`.
fn save_until(app: &Running, editor: HWND, line: &str) -> String {
    save(editor);
    assert!(within_3s(|| file(app).contains(line)), "saved with {line:?}:\n{}", file(app));
    file(app)
}

fn colour_at(p: POINT) -> [i32; 3] {
    unsafe { DwmFlush() };
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

fn near(a: [i32; 3], b: [i32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 4)
}

fn s(editor: HWND, v: i32) -> i32 {
    v * unsafe { GetDpiForWindow(editor) } as i32 / 96
}

/// A point of the sample's cell at `column`, `row`, in keys, on the screen:
/// in its key's face, clear of the label.
fn sample_cell(editor: HWND, column: i32, row: i32) -> POINT {
    let r = window_rect(part(editor, SAMPLE));
    POINT { x: r.left + s(editor, INSET + KEY * column + 10), y: r.top + s(editor, INSET + KEY * row + 10) }
}

/// Give the sample the focus, clicking its corner, clear of its keys.
fn focus_sample(editor: HWND) {
    let r = window_rect(part(editor, SAMPLE));
    wait_in_front(editor);
    click(POINT { x: r.right - 3, y: r.bottom - 3 });
    assert!(within_3s(|| focus_of(editor) == part(editor, SAMPLE)), "the sample has the focus");
}

/// Open the window on キー配置, with set `set` chosen.
fn open_set(app: &Running, set: &str) -> HWND {
    let editor = open(app, open_id(app));
    assert!(within_3s(|| heading(editor) == "キー配置"), "opens on キー配置: {:?}", heading(editor));
    if chosen(part(editor, SET)).as_deref() != Some(set) {
        choose(editor, SET, set);
    }
    editor
}

/// Choose key `i` of a set shown down one column, by clicking it in the
/// sample, and wait for its label to show.
fn choose_key(editor: HWND, i: i32, label: &str) {
    wait_in_front(editor);
    click(sample_cell(editor, 0, i));
    pump(300);
    assert!(within_3s(|| text(editor, LABEL) == label), "key {i}, {label}, chosen: {:?}", text(editor, LABEL));
}

// AC1
#[test]
fn key_sets_are_chosen_named_added_and_removed_only_where_unused() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    assert_eq!(class_of(part(editor, SET)), "ComboBox", "the sets in a drop-down list");
    assert_eq!(items(part(editor, SET)), ["mods", "tools", "transform", "spare"]);
    for (id, name) in [(ADD_SET, "追加"), (RENAME, "名前の変更"), (REMOVE_SET, "削除")] {
        assert!(shown(editor, id) && text(editor, id) == name, "{name} beside the list");
    }
    let used = text(editor, USED);
    assert!(used.contains("default の左辺") && used.contains("csp の左辺"), "where mods is used: {used:?}");
    choose(editor, SET, "tools");
    assert!(within_3s(|| text(editor, USED).contains("csp の右辺")), "{:?}", text(editor, USED));
    choose(editor, SET, "transform");
    assert!(within_3s(|| text(editor, USED).contains("変形キーのパイ")), "{:?}", text(editor, USED));

    // Renamed, the edge, page and pie keys that name it follow.
    let source = SETTINGS.replace("key = 次へ     | page     | next", "key = 次へ     | page     | transform");
    drop(app);
    let app = launch_with(Some(&source), None);
    let editor = open_set(&app, "transform");
    click_id(editor, RENAME);
    assert!(within_3s(|| shown(editor, NAME) && focus_of(editor) == part(editor, NAME)), "a name to type");
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text("morph");
    press(VK_RETURN);
    assert!(within_3s(|| items(part(editor, SET)).contains(&"morph".to_string())), "{:?}", items(part(editor, SET)));
    let saved = save_until(&app, editor, "[set.morph]");
    assert_eq!(key_of(&saved, "tools", "変形")[2], "morph", "the pie key follows");
    assert_eq!(key_of(&saved, "tools", "次へ")[2], "morph", "the page key follows");
    assert!(!saved.contains("transform"), "{saved}");

    // Used, it is not removed: where it is used is said.
    choose(editor, SET, "tools");
    click_id(editor, REMOVE_SET);
    assert!(within_3s(|| status(editor).contains("csp の右辺")), "where it is used: {:?}", status(editor));
    assert!(items(part(editor, SET)).contains(&"tools".to_string()), "kept");
    // Not used, it is.
    choose(editor, SET, "spare");
    click_id(editor, REMOVE_SET);
    assert!(within_3s(|| !items(part(editor, SET)).contains(&"spare".to_string())), "spare gone");
    // Added, it is named at once.
    click_id(editor, ADD_SET);
    assert!(within_3s(|| shown(editor, NAME) && focus_of(editor) == part(editor, NAME)), "the new set named");
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text("extra");
    press(VK_RETURN);
    let saved = save_until(&app, editor, "[set.extra]");
    assert!(!saved.contains("[set.spare]") && saved.contains("[set.tools]"), "{saved}");
    assert_eq!(key_lines(&saved, "extra").len(), 1, "a new set has a key: {saved}");
}

// AC2
#[test]
fn the_sample_draws_the_set_as_its_panel_and_chooses_a_key() {
    let _desktop = desktop();
    let source = "[general]\ntheme = light\naccent = #C00000\n\n\
        [set.col]\nkey = X | shortcut | X\nkey = Y | shortcut | Y\n\n\
        [set.row]\nkey = A | shortcut | A\nkey = Wide | shortcut | B | width=2\nkey = C | shortcut | C\n\n\
        [profile.default]\ntop = row\n";
    let app = launch_with(Some(source), None);
    let editor = open_set(&app, "col");
    let (face, held) = ([0xFA, 0xFA, 0xFA], [0xC0, 0x00, 0x00]);
    // Used on no edge: down one column, the first key chosen.
    let _ = within_3s(|| near(colour_at(sample_cell(editor, 0, 1)), face));
    assert!(near(colour_at(sample_cell(editor, 0, 1)), face), "Y as a key of a light panel: {:?}", colour_at(sample_cell(editor, 0, 1)));
    assert!(near(colour_at(sample_cell(editor, 0, 0)), held), "X chosen, as held: {:?}", colour_at(sample_cell(editor, 0, 0)));
    assert!(!near(colour_at(sample_cell(editor, 1, 0)), face), "no key beside X");
    assert_eq!(text(editor, LABEL), "X");

    choose_key(editor, 1, "Y");
    assert!(within_3s(|| near(colour_at(sample_cell(editor, 0, 1)), held)), "Y chosen now");
    assert!(near(colour_at(sample_cell(editor, 0, 0)), face), "X not");

    // Used on the top edge, one row: along it, Wide two keys long.
    choose(editor, SET, "row");
    let _ = within_3s(|| near(colour_at(sample_cell(editor, 2, 0)), face));
    for (column, what) in [(1, "Wide"), (2, "Wide's second cell"), (3, "C")] {
        assert!(near(colour_at(sample_cell(editor, column, 0)), face), "{what} at column {column}: {:?}", colour_at(sample_cell(editor, column, 0)));
    }
    assert!(!near(colour_at(sample_cell(editor, 0, 1)), face), "no key under A");
    wait_in_front(editor);
    click(sample_cell(editor, 3, 0));
    assert!(within_3s(|| text(editor, LABEL) == "C"), "C chosen: {:?}", text(editor, LABEL));
}

// AC3
#[test]
fn keys_are_added_removed_and_reordered() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    assert_eq!(text(editor, LABEL), "Esc");
    click_id(editor, KEY_DOWN);
    assert!(within_3s(|| text(editor, LABEL) == "Esc"), "the key moved stays chosen");
    // Alt+↓ moves it on again: Ctrl, Space, Esc.
    arrow(&[VK_MENU], VK_DOWN);
    click_id(editor, ADD_KEY);
    assert!(within_3s(|| text(editor, LABEL) != "Esc"), "the new key chosen");
    let new = text(editor, LABEL);
    click_id(editor, ADD_KEY);
    assert!(within_3s(|| text(editor, LABEL) == new), "another, chosen");
    click_id(editor, REMOVE_KEY);
    pump(300);
    // Alt+↑ and ↑ move a key back toward the top: Ctrl first now.
    choose_key(editor, 0, "Ctrl");
    click_id(editor, KEY_DOWN);
    click_id(editor, KEY_DOWN);
    click_id(editor, KEY_UP);
    arrow(&[VK_MENU], VK_UP);
    let saved = save_until(&app, editor, &new);
    let labels: Vec<String> = keys_of(&saved, "mods").into_iter().map(|k| k[0].clone()).collect();
    assert_eq!(labels, ["Ctrl", "Space", "Esc", new.as_str()], "{saved}");
    assert!(saved.contains("key = Ctrl  | modifier | Ctrl   ; 左手"), "moved with its comment: {saved}");
}

// AC4
#[test]
fn the_label_field_says_when_the_panel_cuts_it() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    assert_eq!(class_of(part(editor, LABEL)), "Edit");
    assert!(!shown(editor, CUT), "Esc fits");
    click_id(editor, LABEL);
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text("とても長いキーのラベル");
    assert!(within_3s(|| shown(editor, CUT)), "said while typing");
    assert_eq!(text(editor, CUT), "パネルでは切れます");
    press(VK_RETURN);
    pump(300);
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text("戻る");
    assert!(within_3s(|| !shown(editor, CUT)), "a label that fits: not said");
    press(VK_RETURN);
    let saved = save_until(&app, editor, "戻る");
    assert_eq!(key_of(&saved, "mods", "戻る"), ["戻る", "shortcut", "Esc"]);
}

// AC5
#[test]
fn nine_kinds_show_their_description_and_only_their_fields() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    let kinds: Vec<String> = (0..9).map(|k| text(editor, KINDS + k)).collect();
    assert_eq!(kinds, KIND_NAMES, "nine kinds");
    let top = |k: i32| window_rect(part(editor, KINDS + k)).top;
    assert!((1..9).all(|k| top(k) == top(0)), "side by side");
    assert!(checked(part(editor, KINDS + SHORTCUT)), "Esc is a shortcut");

    // Each kind: its description, and its own fields alone.
    let own = [KEYS, INTERVAL, STEPS, CONTENT, TARGET, PIE];
    let fields = [
        (SHORTCUT, vec![KEYS]),
        (REPEAT, vec![KEYS, INTERVAL]),
        (MACRO, vec![STEPS]),
        (TEXT, vec![CONTENT]),
        (RUN, vec![CONTENT]),
        (PAGE, vec![TARGET]),
        (PIE_KIND, vec![PIE]),
    ];
    let mut said = Vec::new();
    for (kind, shows) in fields {
        click_id(editor, KINDS + kind);
        assert!(within_3s(|| checked(part(editor, KINDS + kind))), "{} chosen", KIND_NAMES[kind as usize]);
        let _ = within_3s(|| shows.iter().all(|&id| shown(editor, id)));
        for id in own {
            assert_eq!(shown(editor, id), shows.contains(&id), "{}: part {id}", KIND_NAMES[kind as usize]);
        }
        assert!(shown(editor, WIDTH), "{}: the width", KIND_NAMES[kind as usize]);
        let about = text(editor, ABOUT);
        assert!(!about.is_empty() && !about.contains('\n') && !said.contains(&about), "{}: {about:?}", KIND_NAMES[kind as usize]);
        said.push(about);
    }
}

// AC6
#[test]
fn the_keys_sent_are_chosen_from_lists_or_captured() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    let groups: Vec<String> = (0..5).map(|g| text(editor, GROUPS + g)).collect();
    assert_eq!(groups, ["英数字", "F1〜F24", "移動・編集", "記号", "テンキー"]);
    let list = part(editor, KEYS);
    assert_eq!(chosen(list).as_deref(), Some("Esc"), "Esc chosen in its group");
    assert!(checked(part(editor, GROUPS + 2)), "移動・編集");

    click_id(editor, MODS + CTRL_ON);
    assert!(within_3s(|| text(editor, SENDS) == "Ctrl+Esc"), "Ctrl on: {:?}", text(editor, SENDS));
    assert!(checked(part(editor, MODS + CTRL_ON)));
    click_id(editor, GROUPS + 1);
    assert!(within_3s(|| items(list).first().map(String::as_str) == Some("F1")), "{:?}", items(list));
    assert_eq!(items(list).len(), 24);
    pick(editor, KEYS, "F5");
    assert!(within_3s(|| text(editor, SENDS) == "Ctrl+F5"), "{:?}", text(editor, SENDS));
    assert_eq!(chosen(list).as_deref(), Some("F5"), "F5 shows chosen");
    click_id(editor, GROUPS + 4);
    pick(editor, KEYS, "Numpad5");
    assert!(within_3s(|| text(editor, SENDS) == "Ctrl+Numpad5"), "{:?}", text(editor, SENDS));
    click_id(editor, GROUPS + 3);
    assert!(items(list).contains(&"[".to_string()), "{:?}", items(list));
    click_id(editor, GROUPS);
    assert!(items(list).contains(&"A".to_string()) && items(list).contains(&"0".to_string()), "{:?}", items(list));

    // Captured: the next keys pressed.
    let capture = part(editor, CAPTURE);
    assert_eq!(text_of(capture), "押して取り込む");
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture).contains("取り込み中")), "{:?}", text_of(capture));
    press_with(&[VK_SHIFT], b'K' as u16);
    assert!(within_3s(|| text(editor, SENDS) == "Shift+K"), "{:?}", text(editor, SENDS));
    assert_eq!(text_of(capture), "押して取り込む");
    // Pressed again, the capture is called off.
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture).contains("取り込み中")));
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture) == "押して取り込む"), "called off");
    press(b'J' as u16);
    pump(300);
    assert_eq!(text(editor, SENDS), "Shift+K", "nothing taken once called off");

    // A modifier key: a modifier alone.
    choose_key(editor, 1, "Ctrl");
    assert!(!shown(editor, KEYS) && !shown(editor, GROUPS), "no other keys to choose");
    click_id(editor, MODS + ALT_ON);
    assert!(within_3s(|| text(editor, SENDS) == "Alt"), "{:?}", text(editor, SENDS));
    assert!(!checked(part(editor, MODS + CTRL_ON)), "one at a time");
    save(editor);
    assert!(within_3s(|| status(editor).contains("保存しました")), "the parser reads it: {:?}", status(editor));
    let saved = file(&app);
    assert_eq!(key_of(&saved, "mods", "Ctrl"), ["Ctrl", "modifier", "Alt"]);
    assert_eq!(key_of(&saved, "mods", "Esc"), ["Esc", "shortcut", "Shift+K"]);
}

// AC7
#[test]
fn each_kind_has_its_own_fields() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "tools");
    // The keys, in order, by keyboard: the set is on two columns.
    let next = |label: &str| {
        focus_sample(editor);
        arrow(&[], VK_DOWN);
        assert!(within_3s(|| text(editor, LABEL) == label), "{label} chosen: {:?}", text(editor, LABEL));
    };
    let first = |label: &str| {
        focus_sample(editor);
        for _ in 0..6 {
            arrow(&[], VK_UP);
        }
        assert!(within_3s(|| text(editor, LABEL) == label), "{label} chosen: {:?}", text(editor, LABEL));
    };

    // repeat: the interval, 20 to 1000 ms.
    first("元に戻す");
    next("ブラシ");
    assert_eq!(text(editor, INTERVAL), "60");
    enter(editor, INTERVAL, "120");
    enter(editor, WIDTH, "2");
    // pie: its key set, how many items, a figure, and why it cannot be.
    next("変形");
    assert_eq!(chosen(part(editor, PIE)).as_deref(), Some("transform"));
    assert!(shown(editor, PIE_FIGURE));
    assert!(within_3s(|| text(editor, PIE_NOTE).contains("2")), "two items: {:?}", text(editor, PIE_NOTE));
    choose(editor, PIE, "spare");
    assert!(within_3s(|| text(editor, PIE_NOTE).contains("2〜8")), "one item is too few: {:?}", text(editor, PIE_NOTE));
    choose(editor, PIE, "transform");
    // page: next, previous, or a key set.
    next("次へ");
    let targets = items(part(editor, TARGET));
    assert_eq!(&targets[..2], ["次のページ", "前のページ"], "{targets:?}");
    choose(editor, TARGET, "spare");
    // run: what is run.
    next("電卓");
    assert_eq!(text(editor, CONTENT), "calc.exe");
    enter(editor, CONTENT, "notepad.exe");
    // macro: steps of keys and waits, added, removed and reordered.
    first("元に戻す");
    click_id(editor, KINDS + MACRO);
    assert!(within_3s(|| items(part(editor, STEPS)) == ["Ctrl+Z"]), "{:?}", items(part(editor, STEPS)));
    click_id(editor, STEP_WAIT);
    assert!(within_3s(|| shown(editor, WAIT)), "a wait's time");
    enter(editor, WAIT, "300");
    click_id(editor, STEP_KEY);
    assert!(within_3s(|| items(part(editor, STEPS)).len() == 3), "{:?}", items(part(editor, STEPS)));
    click_id(editor, GROUPS + 2);
    pick(editor, KEYS, "Esc");
    click_id(editor, STEP_UP);
    click_id(editor, STEP_WAIT);
    click_id(editor, STEP_REMOVE);
    // text: what is typed.
    choose(editor, SET, "spare");
    click_id(editor, KINDS + TEXT);
    enter(editor, CONTENT, "こんにちは");

    let saved = save_until(&app, editor, "こんにちは");
    assert_eq!(key_of(&saved, "spare", "A"), ["A", "text", "こんにちは"]);
    let brush = key_of(&saved, "tools", "ブラシ");
    assert!(brush[3].contains("interval=120") && brush[3].contains("width=2"), "{brush:?}");
    assert_eq!(key_of(&saved, "tools", "元に戻す"), ["元に戻す", "macro", "Ctrl+Z, Esc, wait 300"]);
    assert_eq!(key_of(&saved, "tools", "変形"), ["変形", "pie", "transform"]);
    assert_eq!(key_of(&saved, "tools", "次へ"), ["次へ", "page", "spare"]);
    assert_eq!(key_of(&saved, "tools", "電卓"), ["電卓", "run", "notepad.exe"]);
}

// AC8
#[test]
fn the_hotkey_is_captured_too() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open(&app, open_id(&app));
    wait_in_front(editor);
    click(centre(sidebar_item(editor, 3)));
    assert!(within_3s(|| heading(editor) == "操作"), "{:?}", heading(editor));
    let capture = part(editor, HOTKEY_CAPTURE);
    assert!(visible(capture) && text_of(capture) == "押して取り込む", "beside hotkey");
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture).contains("取り込み中")), "{:?}", text_of(capture));
    press_with(&[VK_CONTROL, VK_MENU], b'K' as u16);
    assert!(within_3s(|| text_of(capture) == "押して取り込む"), "taken");
    save_until(&app, editor, "hotkey = Ctrl+Alt+K");
}

// AC9
#[test]
fn a_change_rewrites_only_its_key_line_and_the_panel_follows() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let before = file(&app);
    let left = primary_monitor().left;
    let length = || panels_now(&app).iter().find(|r| r.left == left).map_or(0, |r| (r.bottom - r.top) / key_px());
    left_panel(&app);
    assert_eq!(length(), 3, "three keys");
    let editor = open_set(&app, "mods");
    choose_key(editor, 1, "Ctrl");
    enter(editor, WIDTH, "2");
    enter(editor, LABEL, "Control");
    let saved = save_until(&app, editor, "Control");
    let (old, new): (Vec<&str>, Vec<&str>) = (before.lines().collect(), saved.lines().collect());
    assert_eq!(old.len(), new.len(), "{saved}");
    for (a, b) in old.iter().zip(&new) {
        if a.contains("左手") {
            assert!(b.contains("Control") && b.contains("width=2") && b.ends_with("; 左手"), "{b}");
        } else {
            assert_eq!(a, b, "only the key's line changes");
        }
    }
    assert!(within_3s(|| length() == 4), "the panel reads it: {} keys long", length());
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

fn brightness(c: [i32; 3]) -> i32 {
    (c[0] + c[1] + c[2]) / 3
}

// AC10
#[test]
fn the_page_follows_the_theme_the_dpi_and_the_keyboard() {
    let _desktop = desktop();
    let dark = Dark::set();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    let inside = || {
        let r = window_rect(part(editor, LABEL));
        colour_at(POINT { x: r.right - 4, y: (r.top + r.bottom) / 2 })
    };
    let _ = within_3s(|| brightness(inside()) < 90);
    assert!(brightness(inside()) < 90, "the label's field dark: {:?}", inside());
    let sample = window_rect(part(editor, SAMPLE));
    let corner = colour_at(POINT { x: sample.right - 2, y: sample.bottom - 2 });
    assert!(brightness(corner) < 90, "around the sample dark: {corner:?}");

    // The DPI: the fields and the sample grow with it.
    let width = |id: i32| {
        let r = window_rect(part(editor, id));
        r.right - r.left
    };
    let (label0, sample0) = (width(LABEL), width(SAMPLE));
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let new = dpi * 3 / 2;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    pump(300);
    assert!((width(LABEL) - label0 * 3 / 2).abs() <= 3, "the label's field grows: {label0} -> {}", width(LABEL));
    assert!((width(SAMPLE) - sample0 * 3 / 2).abs() <= 3, "the sample grows: {sample0} -> {}", width(SAMPLE));
    unsafe { SendMessageW(editor, WM_DPICHANGED, (dpi << 16 | dpi) as usize, &suggested as *const _ as isize) };
    pump(300);
    drop(dark);

    // The keyboard: Tab reaches the sample, and the arrows choose its keys.
    click_id(editor, LABEL);
    let sample = part(editor, SAMPLE);
    let mut reached = false;
    for _ in 0..40 {
        press(VK_TAB);
        if focus_of(editor) == sample {
            reached = true;
            break;
        }
    }
    assert!(reached, "the sample takes the focus");
    arrow(&[], VK_DOWN);
    assert!(within_3s(|| text(editor, LABEL) == "Ctrl"), "↓: the next key, {:?}", text(editor, LABEL));
    arrow(&[], VK_UP);
    assert!(within_3s(|| text(editor, LABEL) == "Esc"), "↑: back, {:?}", text(editor, LABEL));
}

//! Acceptance tests for Issue #87: the キー配置 page of the settings window,
//! which edits key sets and their keys without writing the settings file
//! (spec 7.1). Since Issue #109, キー配置 lays the shape out and the page
//! after it, キーマッピング, says what the keys send: the label, the kinds
//! and the kinds' own fields are there, the rest on キー配置; 並べ方 and
//! the place fields, 1070 to 1075, are described with their tests below.
//! Since Issue #111, キーマッピング chooses what keys send from a keyboard:
//! the ten kinds' buttons, the groups and the list of keys below are no
//! longer shown, and its parts are described with its tests.
//!
//! The tests find the page's parts by control id:
//! - the key sets: the drop-down list 1000, the name field 1001 (an Edit,
//!   shown while a set is named), 追加 1002, 名前の変更 1003, 削除 1004, and
//!   where the set is used 1005 (a Static);
//! - the sample of the panel 1006, and under it 追加 1007, 削除 1008, ↑ 1009
//!   and ↓ 1010 for its keys;
//! - the chosen key's label 1011 (an Edit), 「パネルでは切れます」 1012 (a
//!   Static, shown when the label is cut), the kind's description 1013 (a
//!   Static), its width 1014 (an Edit), and the ten kinds 1020 to 1029
//!   (buttons, the chosen one checked; レイヤー since Issue #104);
//! - the keys sent: Ctrl, Shift, Alt and Win 1030 to 1033 (buttons, checked
//!   when on), the five groups of keys 1040 to 1044, the keys of the group
//!   1050 (a ListBox, the key sent chosen), 押して取り込む 1051, and the keys
//!   sent as written 1052 (a Static);
//! - the kinds' own: the interval 1053, the macro's steps 1054 (a ListBox)
//!   with キー操作を追加 1055, 待ち時間を追加 1056, 削除 1057, ↑ 1058 and ↓
//!   1059, a wait's time 1060, the text or what is run 1061 (an Edit), the
//!   page's target 1062 (a drop-down list); the pie's key set 1063, its
//!   figure 1064 and what it says of its items 1065, until Issue #113
//!   edited a pie's items on a fan;
//! - KLE から読み込む 1066 (Issue #101);
//! - the kind レイヤー 1029, its key set 1067 (a drop-down list), and
//!   押している間 1068 and 押すたびに切り替える 1069 (Issue #104);
//! - on 操作, 押して取り込む beside hotkey: 21.
//!
//! The sample draws the panel 8 logical pixels in from its top left corner,
//! keys as large as the window lets them be (since Issue #137, from 40 to 64
//! logical pixels, read from the sample), in the panel's
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
const WIDTH: i32 = 1014;
const KINDS: i32 = 1020;
const MODS: i32 = 1030;
const CAPTURE: i32 = 1051;
const INTERVAL: i32 = 1053;
const STEPS: i32 = 1054;
const STEP_KEY: i32 = 1055;
const STEP_WAIT: i32 = 1056;
const STEP_REMOVE: i32 = 1057;
const STEP_UP: i32 = 1058;
const WAIT: i32 = 1060;
const CONTENT: i32 = 1061;
const TARGET: i32 = 1062;
const PIE_NOTE: i32 = 1065;
const HOTKEY_CAPTURE: i32 = 21;

/// The modifiers, of the buttons 1030 to 1033.
const CTRL_ON: i32 = 0;
const ALT_ON: i32 = 2;

/// Logical pixels: the sample's inset.
const INSET: i32 = 8;

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
    let at = centre(window_rect(hwnd));
    reach(editor, hwnd, at);
    click(at);
    settle(editor);
}

/// Bring `editor` in front with `part` at `at` on top, nothing over it: in
/// front is not enough. On CI, the runner's terminal was seen over the
/// window as the first test opened it, in front, and took the click
/// (Issue #103: CASCADIA_HOSTING_WINDOW_CLASS under the point, run
/// 40511603723). Raised once, it stays over the terminal for the rest of
/// the test, so the other clicks need not check. Raising activates it,
/// as wanted: it is the window clicked.
fn reach(editor: HWND, part: HWND, at: POINT) {
    let on_part = || {
        let under = unsafe { WindowFromPoint(at) };
        under == part || unsafe { IsChild(part, under) } != 0
    };
    for _ in 0..3 {
        wait_in_front(editor);
        if within_3s(on_part) {
            return;
        }
        unsafe { SetWindowPos(editor, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE) };
        settle(editor);
    }
    let under = unsafe { WindowFromPoint(at) };
    let r = window_rect(part);
    let inside = at.x >= r.left && at.x < r.right && at.y >= r.top && at.y < r.bottom;
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(under, &mut pid) };
    panic!(
        "({}, {}) is under {} {under:?} of process {pid}, not the part clicked ({}, id {}); the point {} the part",
        at.x, at.y, class_of(under), class_of(part), unsafe { GetDlgCtrlID(part) }, if inside { "is in" } else { "is outside" },
    );
}

/// Click the part `id` of `editor`.
#[track_caller]
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
    settle(unsafe { GetForegroundWindow() });
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
        settle(unsafe { GetForegroundWindow() });
    }
    for &m in modifiers.iter().rev() {
        keyboard(m, false);
    }
}

/// Click into the Edit `id`, select what is there, type `text` in its
/// place, and press Enter.
fn enter(editor: HWND, id: i32, text: &str) {
    click_id(editor, id);
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text(text);
    press(VK_RETURN);
    settle(editor);
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
    // A key pressed as the list opens can be lost: try again.
    for _ in 0..3 {
        click_on(editor, combo);
        press(VK_ESCAPE);
        press(VK_HOME);
        for _ in 0..at {
            press(VK_DOWN);
        }
        settle(editor);
        if within_3s(|| chosen(combo).as_deref() == Some(option)) {
            return;
        }
    }
    panic!("{option} chosen in {id}");
}

#[track_caller]
fn text(editor: HWND, id: i32) -> String {
    text_of(part(editor, id))
}

#[track_caller]
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
    assert!(within_3s(|| file(app).contains(line)), "saved with {line:?}, said {:?}:\n{}", status(editor), file(app));
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
    let key = sample_key(editor);
    POINT { x: r.left + s(editor, INSET + 10) + key * column, y: r.top + s(editor, INSET + 10) + key * row }
}

/// Give the sample the focus, clicking its corner, clear of its keys and
/// of its scroll bars (Issue #137).
fn focus_sample(editor: HWND) {
    let r = client_rect(part(editor, SAMPLE));
    for _ in 0..3 {
        wait_in_front(editor);
        click(POINT { x: r.right - 3, y: r.bottom - 3 });
        if within_3s(|| focus_of(editor) == part(editor, SAMPLE)) {
            return;
        }
    }
    panic!("the sample has the focus");
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

/// Open the window on キーマッピング (Issue #109), with set `set` chosen.
fn open_map(app: &Running, set: &str) -> HWND {
    let editor = open_set(app, set);
    to_page(editor, MAP_PAGE);
    editor
}

/// Choose key `i` of the set shown by the keyboard, from the sample: Home,
/// then ↓ as far as it is; and wait for its label to show. However the
/// sample is laid out and sized (Issue #111: smaller on キーマッピング).
fn choose_key(editor: HWND, i: i32, label: &str) {
    // Judged by the result too: a key lost another way is pressed again.
    for _ in 0..3 {
        focus_sample(editor);
        arrow(&[], VK_HOME);
        for _ in 0..i {
            arrow(&[], VK_DOWN);
        }
        if within_3s(|| text(editor, LABEL) == label) {
            return;
        }
    }
    panic!("key {i}, {label}, chosen: {:?}", text(editor, LABEL));
}

// Issue #87, AC1
#[test]
fn key_sets_are_chosen_named_added_and_removed_only_where_unused() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    assert_eq!(class_of(part(editor, SET)), "ComboBox", "the sets in a drop-down list");
    // transform, a pie's items, is not listed (Issue #113).
    assert_eq!(items(part(editor, SET)), ["mods", "tools", "spare"]);
    for (id, name) in [(ADD_SET, "追加"), (RENAME, "名前の変更"), (REMOVE_SET, "削除")] {
        assert!(shown(editor, id) && text(editor, id) == name, "{name} beside the list");
    }
    let used = text(editor, USED);
    assert!(used.contains("default の左エッジ") && used.contains("csp の左エッジ"), "where mods is used: {used:?}");
    choose(editor, SET, "tools");
    assert!(within_3s(|| text(editor, USED).contains("csp の右エッジ")), "{:?}", text(editor, USED));

    // Renamed, the edge, page and pie keys that name it follow. A page key
    // names it too: it is listed.
    let source = SETTINGS.replace("key = 次へ     | page     | next", "key = 次へ     | page     | transform");
    drop(app);
    let app = launch_with(Some(&source), None);
    let editor = open_set(&app, "transform");
    assert!(within_3s(|| text(editor, USED).contains("変形キーのパイ")), "{:?}", text(editor, USED));
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
    assert!(within_3s(|| status(editor).contains("csp の右エッジ")), "where it is used: {:?}", status(editor));
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

// Issue #87, AC2
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

    // The panels' theme changed on 見た目 (its field 403): the sample
    // follows it, back on キー配置.
    wait_in_front(editor);
    click(centre(sidebar_item(editor, 5)));
    assert!(within_3s(|| heading(editor) == "見た目"));
    choose(editor, 403, "ダーク");
    wait_in_front(editor);
    click(centre(sidebar_item(editor, 0)));
    assert!(within_3s(|| heading(editor) == "キー配置"));
    let dark_face = [0x3A, 0x3A, 0x3A];
    assert!(within_3s(|| near(colour_at(sample_cell(editor, 0, 0)), dark_face)), "A as a key of a dark panel: {:?}", colour_at(sample_cell(editor, 0, 0)));
}

// A value the parser turns away stays in its field, out of the file.
#[test]
fn a_key_value_that_does_not_read_is_stopped_in_its_field() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    assert!(within_3s(|| text(editor, LABEL) == "Esc" && shown(editor, WIDTH)), "Esc chosen");
    // Twice at most: keys lost to a busy machine are typed again.
    for _ in 0..2 {
        enter(editor, WIDTH, "9");
        if within_3s(|| status(editor).contains("width")) {
            break;
        }
    }
    assert!(status(editor).contains("width"), "why: {:?}", status(editor));
    let r = window_rect(part(editor, WIDTH));
    let inside = colour_at(POINT { x: r.right - 4, y: (r.top + r.bottom) / 2 });
    assert!(inside[0] > inside[1] + 30, "the field shows it wrong: {inside:?}");
    save(editor);
    assert!(within_3s(|| notice(editor).contains("保存しました")), "{:?}", notice(editor));
    assert!(!file(&app).contains("width"), "not written: {}", file(&app));
}

// Issue #87, AC3
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
    settle(editor);
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

// Issue #87, AC4
#[test]
fn the_label_field_says_when_the_panel_cuts_it() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    assert_eq!(class_of(part(editor, LABEL)), "Edit");
    assert!(!shown(editor, CUT), "Esc fits");
    click_id(editor, LABEL);
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text("とても長いキーのラベル");
    assert!(within_3s(|| shown(editor, CUT)), "said while typing");
    assert_eq!(text(editor, CUT), "パネルでは切れます");
    press(VK_RETURN);
    settle(editor);
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text("戻る");
    assert!(within_3s(|| !shown(editor, CUT)), "a label that fits: not said");
    press(VK_RETURN);
    let saved = save_until(&app, editor, "戻る");
    assert_eq!(key_of(&saved, "mods", "戻る"), ["戻る", "shortcut", "Esc"]);
}


// AC6, since Issue #111: the keys sent are chosen from the keyboard there;
// here, captured, and a modifier key's.
#[test]
fn the_keys_sent_are_chosen_from_lists_or_captured() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    // Captured: the next keys pressed.
    let capture = part(editor, CAPTURE);
    assert_eq!(text_of(capture), "押して取り込む");
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture).contains("取り込み中")), "{:?}", text_of(capture));
    press_with(&[VK_SHIFT], b'K' as u16);
    assert!(within_3s(|| text(editor, SUMMARY) == "Shift+K を送る"), "{:?}", text(editor, SUMMARY));
    assert_eq!(text_of(capture), "押して取り込む");
    // Pressed again, the capture is called off.
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture).contains("取り込み中")));
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture) == "押して取り込む"), "called off");
    press(b'J' as u16);
    settle(editor);
    assert_eq!(text(editor, SUMMARY), "Shift+K を送る", "nothing taken once called off");

    // A modifier key: a modifier alone, one at a time.
    choose_key(editor, 1, "Ctrl");
    assert!(within_3s(|| checked(part(editor, GROUP_TABS))), "基本, its modifier keys");
    click_id(editor, button_named(editor, MOD_KEYS, 4, "Alt"));
    assert!(within_3s(|| text(editor, SUMMARY).contains("Alt")), "{:?}", text(editor, SUMMARY));
    assert!(!checked(part(editor, button_named(editor, MOD_KEYS, 4, "Ctrl"))), "one at a time");
    save(editor);
    assert!(within_3s(|| notice(editor).contains("保存しました")), "the parser reads it: {:?}", status(editor));
    let saved = file(&app);
    assert_eq!(key_of(&saved, "mods", "Ctrl"), ["Ctrl", "modifier", "Alt"]);
    assert_eq!(key_of(&saved, "mods", "Esc"), ["Esc", "shortcut", "Shift+K"]);
}

// Issue #87, AC7
#[test]
fn each_kind_has_its_own_fields() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "tools");
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
    // pie: Enter opens its fan on its first item (Issue #113); back to
    // the panel.
    next("変形");
    press(VK_RETURN);
    assert!(within_3s(|| text(editor, LABEL) == "拡大" && shown(editor, PIE_BACK)), "the fan: {:?}", text(editor, LABEL));
    click_id(editor, PIE_BACK);
    assert!(within_3s(|| text(editor, LABEL) == "変形"), "{:?}", text(editor, LABEL));
    // page: next, previous, or a key set.
    next("次へ");
    let targets = items(part(editor, TARGET));
    assert_eq!(&targets[..2], ["次のページ", "前のページ"], "{targets:?}");
    choose(editor, TARGET, "spare");
    // run: what is run.
    next("電卓");
    assert_eq!(text(editor, RUN_CONTENT), "calc.exe");
    enter(editor, RUN_CONTENT, "notepad.exe");
    // macro: steps of keys and waits, added, removed and reordered.
    first("元に戻す");
    click_id(editor, GROUP_TABS + 3);
    click_id(editor, STEP_WAIT);
    assert!(within_3s(|| items(part(editor, STEPS)) == ["Ctrl+Z", "待つ 100 ms"]), "{:?}", items(part(editor, STEPS)));
    assert!(within_3s(|| shown(editor, WAIT)), "a wait's time");
    enter(editor, WAIT, "300");
    click_id(editor, STEP_KEY);
    assert!(within_3s(|| items(part(editor, STEPS)).len() == 3), "{:?}", items(part(editor, STEPS)));
    click_id(editor, GROUP_TABS);
    press_key(editor, "Esc");
    click_id(editor, GROUP_TABS + 3);
    click_id(editor, STEP_UP);
    click_id(editor, STEP_WAIT);
    click_id(editor, STEP_REMOVE);
    // text: what is typed.
    choose(editor, SET, "spare");
    click_id(editor, GROUP_TABS + 4);
    enter(editor, CONTENT, "こんにちは");

    let saved = save_until(&app, editor, "こんにちは");
    assert_eq!(key_of(&saved, "spare", "A"), ["A", "text", "こんにちは"]);
    let brush = key_of(&saved, "tools", "ブラシ");
    assert!(brush[3].contains("interval=120"), "{brush:?}");
    assert_eq!(key_of(&saved, "tools", "元に戻す"), ["元に戻す", "macro", "Ctrl+Z, Esc, wait 300"]);
    assert_eq!(key_of(&saved, "tools", "変形"), ["変形", "pie", "transform"]);
    assert_eq!(key_of(&saved, "tools", "次へ"), ["次へ", "page", "spare"]);
    assert_eq!(key_of(&saved, "tools", "電卓"), ["電卓", "run", "notepad.exe"]);
}

// Issue #87, AC8
#[test]
fn the_hotkey_is_captured_too() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open(&app, open_id(&app));
    wait_in_front(editor);
    click(centre(sidebar_item(editor, 7)));
    assert!(within_3s(|| heading(editor) == "表示と非表示"), "{:?}", heading(editor));
    let capture = part(editor, HOTKEY_CAPTURE);
    assert!(visible(capture) && text_of(capture) == "押して取り込む", "beside hotkey");
    click_on(editor, capture);
    assert!(within_3s(|| text_of(capture).contains("取り込み中")), "{:?}", text_of(capture));
    press_with(&[VK_CONTROL, VK_MENU], b'K' as u16);
    assert!(within_3s(|| text_of(capture) == "押して取り込む"), "taken");
    save_until(&app, editor, "hotkey = Ctrl+Alt+K");
}

// Issue #87, AC9
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
    to_page(editor, MAP_PAGE);
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

// Issue #87, AC10
#[test]
fn the_page_follows_the_theme_the_dpi_and_the_keyboard() {
    let _desktop = desktop();
    let dark = Dark::set();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    let inside = || {
        let r = window_rect(part(editor, LABEL));
        colour_at(POINT { x: r.right - 4, y: (r.top + r.bottom) / 2 })
    };
    let _ = within_3s(|| brightness(inside()) < 90);
    assert!(brightness(inside()) < 90, "the label's field dark: {:?}", inside());
    let sample = client_rect(part(editor, SAMPLE));
    let corner = colour_at(POINT { x: sample.right - 2, y: sample.bottom - 2 });
    assert!(brightness(corner) < 90, "around the sample dark: {corner:?}");

    // The DPI: the fields and the sample grow with it.
    let width = |id: i32| {
        let r = window_rect(part(editor, id));
        r.right - r.left
    };
    // The modifiers' buttons: the sample is as wide as the page.
    let (label0, sample0) = (width(LABEL), width(MODS));
    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let new = dpi * 3 / 2;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    // Laid out again once the window is free: waited for (Issue #123).
    assert!(within_3s(|| (width(LABEL) - label0 * 3 / 2).abs() <= 3), "the label's field grows: {label0} -> {}", width(LABEL));
    assert!(within_3s(|| (width(MODS) - sample0 * 3 / 2).abs() <= 3), "the buttons grow: {sample0} -> {}", width(MODS));
    unsafe { SendMessageW(editor, WM_DPICHANGED, (dpi << 16 | dpi) as usize, &suggested as *const _ as isize) };
    assert!(within_3s(|| width(LABEL) == label0), "back to its size: {label0} -> {}", width(LABEL));
    drop(dark);

    // The keyboard: Shift+Tab reaches the sample, just before the label
    // (the keyboard's buttons after it), and the arrows choose its keys.
    click_id(editor, LABEL);
    let sample = part(editor, SAMPLE);
    let mut reached = false;
    for _ in 0..40 {
        press_with(&[VK_SHIFT], VK_TAB);
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

// Issue #101: KLE から読み込む, and a free set's keys edited.
const KLE: i32 = 1066;

/// The dialog box (a file dialog, or a question) `editor` has open.
fn dialog_of(editor: HWND) -> Option<HWND> {
    let class = wide("#32770");
    let mut after = null_mut();
    loop {
        after = unsafe { FindWindowExW(null_mut(), after, class.as_ptr(), std::ptr::null()) };
        if after.is_null() {
            return None;
        }
        if visible(after) && unsafe { GetWindow(after, GW_OWNER) } == editor {
            return Some(after);
        }
    }
}

/// Press KLE から読み込む and choose `file` in the file dialog.
fn import(editor: HWND, file: &std::path::Path) {
    click_id(editor, KLE);
    assert!(within_3s(|| dialog_of(editor).is_some()), "a file dialog opens");
    wait_in_front(dialog_of(editor).unwrap());
    pump(500);
    type_text(&file.display().to_string());
    press(VK_RETURN);
    pump(500);
}

// Issue #101, AC9
#[test]
fn a_kle_file_is_imported_into_the_settings_folder_for_the_chosen_set() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let dir = exe_dir(&app);
    let from = dir.join("from");
    std::fs::create_dir_all(&from).expect("a folder to import from");
    let raw = r#"["A","B"],["C"]"#;
    std::fs::write(from.join("pad.json"), raw).expect("write the KLE file");
    std::fs::write(from.join("bad.json"), "not kle").expect("write the KLE file");
    let editor = open_set(&app, "spare");

    // A file that does not read: not copied, why on the status line.
    import(editor, &from.join("bad.json"));
    assert!(within_3s(|| dialog_of(editor).is_none() && status(editor).contains("bad.json") && status(editor).contains("raw data")), "why: {:?}", status(editor));
    assert!(!dir.join("bad.json").exists(), "not copied");

    // The same name, other content: asked first; on yes, copied over.
    std::fs::write(dir.join("pad.json"), r#"["X"]"#).expect("write a file in the way");
    import(editor, &from.join("pad.json"));
    // A message box's text is its item 0xFFFF.
    let asked = || dialog_of(editor).is_some_and(|d| text_of(unsafe { GetDlgItem(d, 0xFFFF) }).contains("pad.json"));
    assert!(within_3s(asked), "asked whether to write over pad.json");
    let question = dialog_of(editor).unwrap();
    wait_in_front(question);
    press(b'Y' as u16);
    assert!(within_3s(|| dialog_of(editor).is_none()), "the question answered");
    assert_eq!(std::fs::read_to_string(dir.join("pad.json")).unwrap(), raw, "copied over");

    // Written with the set's keys as they were, on 保存.
    let saved = save_until(&app, editor, "layout = kle:pad.json");
    assert!(section(&saved, "spare").contains("layout = kle:pad.json"), "{saved}");
    assert_eq!(keys_of(&saved, "spare"), [["A", "shortcut", "A"]], "the key kept");
}

// Issue #101, AC10
#[test]
fn a_free_sets_key_edited_keeps_its_place_and_the_layout() {
    let _desktop = desktop();
    let source = SETTINGS.replace(
        "[set.spare]\nkey = A | shortcut | A\n",
        "[set.spare]\nlayout = free\nkey = A | shortcut | A | x=0, y=0\nkey = B | shortcut | B | x=1.5, y=0, w=2, h=0.75\n",
    );
    let app = launch_with(Some(&source), None);
    let editor = open_map(&app, "spare");
    choose_key(editor, 1, "B");
    enter(editor, LABEL, "Bee");
    let saved = save_until(&app, editor, "Bee");
    assert!(section(&saved, "spare").contains("layout = free"), "{saved}");
    let bee = key_of(&saved, "spare", "Bee");
    let options: Vec<String> = bee[3].split(',').map(|o| o.replace(' ', "")).collect();
    assert_eq!(options, ["x=1.5", "y=0", "w=2", "h=0.75"], "{bee:?}");
}

// Issue #104: the kind レイヤー, its key set and how it turns.
const LAYER_SET: i32 = 1067;
const LAYER_HOLD: i32 = 1068;
const LAYER_TOGGLE: i32 = 1069;

// Issue #104, AC8
#[test]
fn a_key_becomes_a_layer_key_of_a_set_held_or_toggled() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    choose_key(editor, 0, "Esc");
    click_id(editor, GROUP_TABS + 2);
    assert!(within_3s(|| shown(editor, LAYER_SET) && shown(editor, LAYER_HOLD) && shown(editor, LAYER_TOGGLE)), "the layer's fields");
    choose(editor, LAYER_SET, "spare");
    click_id(editor, LAYER_TOGGLE);
    let saved = save_until(&app, editor, "mode=toggle");
    assert_eq!(key_of(&saved, "mods", "Esc"), ["Esc", "layer", "spare", "mode=toggle"]);
    click_id(editor, LAYER_HOLD);
    save(editor);
    assert!(within_3s(|| !file(&app).contains("mode=toggle")), "held: no mode written\n{}", file(&app));
    assert_eq!(key_of(&file(&app), "mods", "Esc"), ["Esc", "layer", "spare"]);
}

// Issue #109: キー配置 lays the panel's shape out, and キーマッピング (the
// page after it) says what the keys send. The shape's parts: 並べ方 1070
// (a drop-down list), a free key's x, y, w and h 1071 to 1074 (Edits), and
// the note beside them on a KLE set 1075 (a Static).
const MAP_PAGE: usize = 1;
const ARRANGE: i32 = 1070;
const PLACE_X: i32 = 1071;
const PLACE_W: i32 = 1073;
const PLACE_H: i32 = 1074;
const KLE_NOTE: i32 = 1075;
const ARRANGEMENTS: [&str; 2] = ["エッジに沿って並べる", "自由配列"];

/// Show page `page`, from the sidebar.
fn to_page(editor: HWND, page: usize) {
    for _ in 0..3 {
        wait_in_front(editor);
        click(centre(sidebar_item(editor, page)));
        if within_3s(|| heading(editor) == PAGES[page]) {
            return;
        }
    }
    panic!("page {}, not {:?}", PAGES[page], heading(editor));
}

/// A point of the sample's cell at `column`, `row`, in keys, counted from
/// the sample's right side with `right` and from its bottom with `bottom`.
fn cell_from(editor: HWND, column: i32, row: i32, right: bool, bottom: bool) -> POINT {
    let r = window_rect(part(editor, SAMPLE));
    let key = sample_key(editor);
    let x = if right { r.right - s(editor, INSET - 10) - key * (column + 1) } else { r.left + s(editor, INSET + 10) + key * column };
    let y = if bottom { r.bottom - s(editor, INSET - 10) - key * (row + 1) } else { r.top + s(editor, INSET + 10) + key * row };
    POINT { x, y }
}

/// Drag with the left button from `from` to `to`, in steps.
fn drag(editor: HWND, from: POINT, to: POINT) {
    reach(editor, part(editor, SAMPLE), from);
    mouse_move_to(from);
    mouse_down_at(from);
    for i in 1..=4 {
        mouse_move_to(POINT { x: from.x + (to.x - from.x) * i / 4, y: from.y + (to.y - from.y) * i / 4 });
    }
    mouse_up_at(to);
    settle(editor);
}

/// Choose `option` in 並べ方 from its list, opened, and Enter: the items
/// gone over on the way are not taken.
fn arrange(editor: HWND, option: &str) {
    let combo = part(editor, ARRANGE);
    let at = items(combo).iter().position(|o| o == option).unwrap_or_else(|| panic!("no {option} in {:?}", items(combo)));
    for _ in 0..3 {
        click_on(editor, combo);
        press(VK_HOME);
        for _ in 0..at {
            press(VK_DOWN);
        }
        press(VK_RETURN);
        settle(editor);
        if within_3s(|| chosen(combo).as_deref() == Some(option)) {
            return;
        }
    }
    panic!("{option} chosen in 並べ方");
}

/// The options of key `label` of set `set` in `saved`, spaces taken out.
fn options_of(saved: &str, set: &str, label: &str) -> Vec<String> {
    key_of(saved, set, label).get(3).map_or(Vec::new(), |o| o.split(',').map(|o| o.replace(' ', "")).collect())
}

/// Whether the field `id` shows its value as wrong: tinted red.
fn is_wrong_field(editor: HWND, id: i32) -> bool {
    let r = window_rect(part(editor, id));
    let inside = colour_at(POINT { x: r.right - 4, y: (r.top + r.bottom) / 2 });
    inside[0] > inside[1] + 30
}

/// Make the window's copy differ from the file, and save: the save fails
/// on the line at fault, which is then followed from the status line.
fn follow_fault(editor: HWND) {
    to_page(editor, 6);
    change(editor, 90);
    save(editor);
    assert!(within_3s(|| status(editor).contains("行目")), "the line and reason: {:?}", status(editor));
    wait_in_front(editor);
    let r = window_rect(part(editor, STATUS));
    click(POINT { x: r.left + 12, y: r.top + 8 });
    settle(editor);
}

// Issue #109, AC1
#[test]
fn the_shape_is_laid_out_on_one_page_and_what_keys_send_on_the_next() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    let layout = [SET, ADD_SET, RENAME, REMOVE_SET, KLE, USED, SAMPLE, ADD_KEY, REMOVE_KEY, KEY_UP, KEY_DOWN, ARRANGE, WIDTH];
    let mapping = [LABEL, SUMMARY, MOTIONS, GROUP_TABS, GROUP_TABS + 5];
    assert!(within_3s(|| layout.iter().all(|&id| shown(editor, id))), "キー配置: {:?}", layout.map(|id| shown(editor, id)));
    assert!(mapping.iter().all(|&id| !shown(editor, id)), "no mapping on キー配置: {:?}", mapping.map(|id| shown(editor, id)));

    // The key chosen there is chosen here: Space, the third.
    choose_key(editor, 2, "Space");
    to_page(editor, MAP_PAGE);
    assert_eq!(heading(editor), "キーマッピング");
    assert!(within_3s(|| mapping.iter().all(|&id| shown(editor, id))), "キーマッピング: {:?}", mapping.map(|id| shown(editor, id)));
    assert!(shown(editor, SET) && shown(editor, SAMPLE), "the sets and the sample here too");
    let elsewhere = [ADD_SET, RENAME, REMOVE_SET, KLE, USED, ADD_KEY, REMOVE_KEY, KEY_UP, KEY_DOWN, ARRANGE, WIDTH];
    assert!(elsewhere.iter().all(|&id| !shown(editor, id)), "the shape not here: {:?}", elsewhere.map(|id| shown(editor, id)));
    assert_eq!(chosen(part(editor, SET)).as_deref(), Some("mods"));
    assert_eq!(text(editor, LABEL), "Space", "the same key chosen");
    // And back: the set chosen here is chosen there.
    choose(editor, SET, "spare");
    to_page(editor, 0);
    assert_eq!(chosen(part(editor, SET)).as_deref(), Some("spare"));
    assert_eq!(text(editor, LABEL), "A");

    // A save failing on a key's width leads to キー配置; on what it sends,
    // to キーマッピング.
    drop(app);
    let bad = SETTINGS.replace("key = A | shortcut | A\n", "key = A | shortcut | A | width=9\n");
    let app = launch_with(Some(&bad), None);
    let editor = open(&app, open_id(&app));
    follow_fault(editor);
    assert!(within_3s(|| heading(editor) == "キー配置"), "the width: キー配置, not {:?}", heading(editor));
    assert!(within_3s(|| chosen(part(editor, SET)).as_deref() == Some("spare")), "its set chosen");
    drop(app);
    let bad = SETTINGS.replace("key = A | shortcut | A\n", "key = A | hold | NoSuchKey\n");
    let app = launch_with(Some(&bad), None);
    let editor = open(&app, open_id(&app));
    follow_fault(editor);
    assert!(within_3s(|| heading(editor) == "キーマッピング"), "what it sends: キーマッピング, not {:?}", heading(editor));
    assert!(within_3s(|| chosen(part(editor, SET)).as_deref() == Some("spare")), "its set chosen");
}

// Issue #109, AC2
#[test]
fn the_sample_is_drawn_on_the_side_and_the_way_of_its_edge() {
    let _desktop = desktop();
    let source = "[general]\ntheme = light\naccent = #C00000\n\n\
        [set.side]\nkey = A | shortcut | A\nkey = Wide | shortcut | B | width=2\n\n\
        [set.low]\nkey = A | shortcut | A\nkey = B | shortcut | B\n\n\
        [set.free]\nlayout = free\nkey = A | shortcut | A | x=0, y=0\nkey = B | shortcut | B | x=1, y=0\nkey = C | shortcut | C | x=0, y=1, w=2\n\n\
        [profile.default]\nright = side\nbottom = low\nleft = free\n";
    let app = launch_with(Some(source), None);
    let (face, held) = ([0xFA, 0xFA, 0xFA], [0xC0, 0x00, 0x00]);
    let at = |editor, column, row, right, bottom| colour_at(cell_from(editor, column, row, right, bottom));
    let editor = open(&app, open_id(&app));
    // キーマッピング draws it the same, its keys as small as its room.
    for page in [0] {
        to_page(editor, page);
        // The right edge: against the sample's right side, down it.
        choose(editor, SET, "side");
        let _ = within_3s(|| near(at(editor, 0, 0, true, false), held));
        assert!(near(at(editor, 0, 0, true, false), held), "{}: A at the right, chosen: {:?}", PAGES[page], at(editor, 0, 0, true, false));
        for row in [1, 2] {
            assert!(near(at(editor, 0, row, true, false), face), "{}: Wide down two cells, at {row}", PAGES[page]);
        }
        assert!(!near(at(editor, 1, 0, true, false), face), "{}: one column", PAGES[page]);
        // The bottom edge: against the bottom, along it.
        choose(editor, SET, "low");
        let _ = within_3s(|| near(at(editor, 0, 0, false, true), held));
        assert!(near(at(editor, 0, 0, false, true), held), "{}: A at the bottom: {:?}", PAGES[page], at(editor, 0, 0, false, true));
        assert!(near(at(editor, 1, 0, false, true), face), "{}: B beside it", PAGES[page]);
        // A free layout on the left, turned clockwise: its bottom row, C,
        // against the left side, down two cells; A and B beside it.
        choose(editor, SET, "free");
        let _ = within_3s(|| near(at(editor, 1, 0, false, false), held));
        assert!(near(at(editor, 1, 0, false, false), held), "{}: A, chosen: {:?}", PAGES[page], at(editor, 1, 0, false, false));
        for (column, row, what) in [(1, 1, "B"), (0, 0, "C"), (0, 1, "C's second cell")] {
            assert!(near(at(editor, column, row, false, false), face), "{}: {what} at {column}, {row}", PAGES[page]);
        }
        assert!(!near(at(editor, 2, 0, false, false), face), "{}: two columns", PAGES[page]);
    }
}

// Issue #109, AC3
#[test]
fn the_arrangement_turns_keys_free_and_back_along_the_edge() {
    let _desktop = desktop();
    let source = SETTINGS.replace(
        "[set.spare]\nkey = A | shortcut | A\n",
        "[set.spare]\nkey = A | shortcut | A\n\n\
         [set.row]\nkey = A | shortcut | A\nkey = Wide | shortcut | B | width=2\nkey = C | text | こんにちは\n\n\
         [set.loose]\nlayout = free\nkey = P | shortcut | P | x=1, y=1, w=1.75\nkey = Q | shortcut | Q | x=0, y=1\nkey = R | run | calc.exe | x=0.5, y=0, w=4\n",
    ) + "bottom = row\n";
    let app = launch_with(Some(&source), None);
    let dir = exe_dir(&app);
    std::fs::write(dir.join("from.json"), r#"["A","B"],["C"]"#).expect("write the KLE file");
    let editor = open_set(&app, "row");
    assert_eq!(items(part(editor, ARRANGE)), ARRANGEMENTS);
    assert_eq!(chosen(part(editor, ARRANGE)).as_deref(), Some(ARRANGEMENTS[0]));

    // Along the bottom edge to free: each key where the sample has it.
    arrange(editor, ARRANGEMENTS[1]);
    let saved = save_until(&app, editor, "layout = free\nkey = A");
    assert_eq!(options_of(&saved, "row", "A"), ["x=0", "y=0", "w=1", "h=1"], "{saved}");
    assert_eq!(options_of(&saved, "row", "Wide"), ["x=1", "y=0", "w=2", "h=1"], "{saved}");
    assert_eq!(options_of(&saved, "row", "C"), ["x=3", "y=0", "w=1", "h=1"], "{saved}");
    assert_eq!(key_of(&saved, "row", "C")[..3], ["C", "text", "こんにちは"], "what it sends kept");

    // Free to along the edge: in the order of y, then x; w rounded to 1-3.
    choose(editor, SET, "loose");
    arrange(editor, ARRANGEMENTS[0]);
    save(editor);
    assert!(within_3s(|| !section(&file(&app), "loose").contains("layout")), "{}", file(&app));
    let saved = file(&app);
    assert!(!section(&saved, "loose").contains("x="), "no places: {saved}");
    let labels: Vec<String> = keys_of(&saved, "loose").into_iter().map(|k| k[0].clone()).collect();
    assert_eq!(labels, ["R", "Q", "P"], "{saved}");
    assert_eq!(key_of(&saved, "loose", "R"), ["R", "run", "calc.exe", "width=3"]);
    assert_eq!(key_of(&saved, "loose", "Q"), ["Q", "shortcut", "Q"]);
    assert_eq!(key_of(&saved, "loose", "P"), ["P", "shortcut", "P", "width=2"]);

    // KLE to free: the file's places in the key lines, and its keys past
    // the key lines' too.
    choose(editor, SET, "spare");
    import(editor, &dir.join("from.json"));
    assert!(within_3s(|| items(part(editor, ARRANGE)).len() == 3), "{:?}", items(part(editor, ARRANGE)));
    assert_eq!(items(part(editor, ARRANGE))[2], "KLE（from.json）");
    assert_eq!(chosen(part(editor, ARRANGE)).as_deref(), Some("KLE（from.json）"));
    arrange(editor, ARRANGEMENTS[1]);
    let saved = save_until(&app, editor, "layout = free\nkey = A");
    assert!(!section(&saved, "spare").contains("kle:"), "{saved}");
    assert_eq!(key_of(&saved, "spare", "A"), ["A", "shortcut", "A", "x=0, y=0, w=1, h=1"]);
    assert_eq!(options_of(&saved, "spare", "B"), ["x=1", "y=0", "w=1", "h=1"], "{saved}");
    assert_eq!(options_of(&saved, "spare", "C"), ["x=0", "y=1", "w=1", "h=1"], "{saved}");

    // KLE to along the edge: the layout and the places gone, the keys in
    // the file's order.
    choose(editor, SET, "mods");
    import(editor, &dir.join("from.json"));
    save_until(&app, editor, "[set.mods]   ; 修飾キー
layout = kle:from.json");
    arrange(editor, ARRANGEMENTS[0]);
    save(editor);
    assert!(within_3s(|| !section(&file(&app), "mods").contains("layout")), "{}", file(&app));
    let labels: Vec<String> = keys_of(&file(&app), "mods").into_iter().map(|k| k[0].clone()).collect();
    assert_eq!(labels, ["Esc", "Ctrl", "Space"], "{}", file(&app));
}

// Issue #109, AC4
#[test]
fn a_key_along_the_edge_has_a_width_and_a_new_one_goes_after_the_chosen() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    assert!(within_3s(|| shown(editor, WIDTH)), "the width");
    assert!(!shown(editor, PLACE_X) && !shown(editor, PLACE_W), "no place along the edge");
    choose_key(editor, 1, "Ctrl");
    click_id(editor, ADD_KEY);
    assert!(within_3s(|| text(editor, LABEL) != "Ctrl"), "the new key chosen");
    let new = text(editor, LABEL);
    enter(editor, WIDTH, "2");
    // ↑ moves it before Ctrl, Alt+↓ back after it, and ↑ once more.
    click_id(editor, KEY_UP);
    arrow(&[VK_MENU], VK_DOWN);
    click_id(editor, KEY_UP);
    let saved = save_until(&app, editor, "width=2");
    let labels: Vec<String> = keys_of(&saved, "mods").into_iter().map(|k| k[0].clone()).collect();
    assert_eq!(labels, ["Esc", new.as_str(), "Ctrl", "Space"], "{saved}");
    assert_eq!(options_of(&saved, "mods", &new), ["width=2"]);
}

// Issue #109, AC5
#[test]
fn a_free_key_is_placed_by_its_fields_and_by_dragging() {
    let _desktop = desktop();
    let source = SETTINGS
        .replace(
            "[set.spare]\nkey = A | shortcut | A\n",
            "[set.spare]\nkey = A | shortcut | A\n\n\
             [set.flat]\nlayout = free\nkey = A | shortcut | A | x=0, y=0\nkey = B | shortcut | B | x=2, y=0\n\n\
             [set.up]\nlayout = free\nkey = A | shortcut | A | x=0, y=0\nkey = B | shortcut | B | x=1, y=0\n",
        )
        .replace("[profile.default]\nleft = mods\n", "[profile.default]\nleft = up\ntop = flat\n");
    let app = launch_with(Some(&source), None);
    let dir = exe_dir(&app);
    std::fs::write(dir.join("pad.json"), r#"["A","B"]"#).expect("write the KLE file");
    let editor = open_set(&app, "flat");
    assert!(within_3s(|| shown(editor, PLACE_X) && shown(editor, PLACE_H)), "x, y, w and h");
    assert!(!shown(editor, WIDTH), "no width in a free layout");
    assert_eq!(text(editor, PLACE_X), "0");
    assert_eq!(text(editor, PLACE_W), "1");

    // Typed: in quarters, in range.
    for _ in 0..2 {
        enter(editor, PLACE_W, "9");
        if within_3s(|| is_wrong_field(editor, PLACE_W)) {
            break;
        }
    }
    assert!(is_wrong_field(editor, PLACE_W), "9 is stopped in its field: {:?}", status(editor));
    enter(editor, PLACE_W, "1.25");
    let saved = save_until(&app, editor, "w=1.25");
    assert_eq!(options_of(&saved, "flat", "A"), ["x=0", "y=0", "w=1.25", "h=1"]);

    // Dragged on the top edge, as laid out: a key to the right.
    let key = sample_key(editor);
    let from = cell_from(editor, 0, 0, false, false);
    drag(editor, from, POINT { x: from.x + key, y: from.y });
    assert!(within_3s(|| text(editor, PLACE_X) == "1"), "x follows at once: {:?}", text(editor, PLACE_X));
    let saved = save_until(&app, editor, "x=1,");
    assert_eq!(options_of(&saved, "flat", "A"), ["x=1", "y=0", "w=1.25", "h=1"]);
    // Its right end, dragged out: A, the leftmost still, at the sample's
    // left.
    let r = window_rect(part(editor, SAMPLE));
    let end = POINT { x: r.left + s(editor, INSET) + key * 5 / 4 - s(editor, 2), y: from.y };
    drag(editor, end, POINT { x: end.x + key * 3 / 4, y: end.y });
    let saved = save_until(&app, editor, "w=2,");
    assert_eq!(options_of(&saved, "flat", "A"), ["x=1", "y=0", "w=2", "h=1"]);

    // On the left edge, the layout turned clockwise: B, under A, dragged
    // up goes left in the layout.
    choose(editor, SET, "up");
    let b = cell_from(editor, 0, 1, false, false);
    drag(editor, b, POINT { x: b.x, y: b.y - key });
    let saved = save_until(&app, editor, "B | shortcut | B | x=0");
    assert_eq!(options_of(&saved, "up", "B"), ["x=0", "y=0", "w=1", "h=1"]);

    // A KLE set: read only, with a note, and not dragged.
    choose(editor, SET, "spare");
    import(editor, &dir.join("pad.json"));
    assert!(within_3s(|| shown(editor, KLE_NOTE)), "the note");
    assert_eq!(text(editor, KLE_NOTE), "並べ方を自由配列にすると動かせます");
    let style = unsafe { GetWindowLongW(part(editor, PLACE_X), GWL_STYLE) } as u32;
    assert!(style & ES_READONLY as u32 != 0, "x read only");
    let a = cell_from(editor, 0, 0, false, false);
    drag(editor, a, POINT { x: a.x + key, y: a.y + key });
    let saved = save_until(&app, editor, "kle:pad.json");
    assert!(!section(&saved, "spare").contains("x="), "not moved: {saved}");
}

// Issue #111: キーマッピング as Vial has it. Its parts: what the chosen
// key does, in a sentence 1076 (a Static); the text and the program fields
// of 文字・起動 1061 and 1077 (Edits); the motions 押した時に送る, 押している間
// and 連打 1080 to 1082 and the five groups 1090 to 1094 (buttons, the
// chosen one checked); the layers' tabs from 1200 (buttons, the set shown
// checked); and on 基本, the keyboard from 1300, the number pad from 1400,
// and the modifier keys from 1450 (buttons named as the keys are, the key
// sent checked). 基本 had テンキー and 修飾キー apart at first: too empty to
// read, they are one group. Since Issue #115, the groups are 基本, JIS,
// レイヤー, マクロ, 文字・起動 and ページ・パイ, 1090 to 1095. The
// modifiers that go with a key are
// still 1030 to 1033, 押して取り込む 1051 and the interval 1053.
const SUMMARY: i32 = 1076;
const RUN_CONTENT: i32 = 1077;
const MOTIONS: i32 = 1080;
const MOTION_NAMES: [&str; 3] = ["押した時に送る", "押している間", "連打"];
const GROUP_TABS: i32 = 1090;
const GROUP_TAB_NAMES: [&str; 6] = ["基本", "JIS", "レイヤー", "マクロ", "文字・起動", "ページ・パイ"];
const LAYER_TABS: i32 = 1200;
const KEYBOARD: i32 = 1300;
const NUMPAD: i32 = 1400;
const MOD_KEYS: i32 = 1450;

/// The button of `editor` named `name` among ids `from`..`from + count`:
/// by its lower line, a symbol's key written under what Shift makes of it
/// (Issue #115).
fn button_named(editor: HWND, from: i32, count: i32, name: &str) -> i32 {
    (from..from + count)
        .find(|&id| text(editor, id).lines().last() == Some(name))
        .unwrap_or_else(|| panic!("no button {name} from {from}"))
}

/// Press the key `name` of 基本's keyboard.
fn press_key(editor: HWND, name: &str) {
    click_id(editor, button_named(editor, KEYBOARD, 100, name));
}

/// The texts of the layers' tabs shown, in order.
fn layer_tabs(editor: HWND) -> Vec<String> {
    (LAYER_TABS..LAYER_TABS + 16).filter(|&id| shown(editor, id)).map(|id| text(editor, id)).collect()
}

/// The key `label` of `set` in the file, waited for to read `fields`.
fn saved_as(app: &Running, editor: HWND, set: &str, label: &str, fields: &[&str]) {
    save(editor);
    let read = || keys_of(&file(app), set).into_iter().find(|k| k[0] == label);
    assert!(within_3s(|| read().is_some_and(|k| k == fields)), "{label} as {fields:?}: {:?}, said {:?}", read(), status(editor));
}

// Issue #111, AC1
#[test]
fn the_mapping_page_has_the_layout_above_and_the_keyboard_below() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    let order = [SET, LAYER_TABS, SAMPLE, LABEL, MODS, GROUP_TABS, button_named(editor, KEYBOARD, 100, "Q")];
    assert!(within_3s(|| order.iter().all(|&id| shown(editor, id))), "{:?}", order.map(|id| shown(editor, id)));
    let tops: Vec<i32> = order.iter().map(|&id| window_rect(part(editor, id)).top).collect();
    assert!(tops.windows(2).all(|t| t[0] < t[1]), "top to bottom: {tops:?}");
    assert!(shown(editor, SUMMARY) && shown(editor, MOTIONS), "the sentence and the motions");
    let width = |id: i32| window_rect(part(editor, id)).right - window_rect(part(editor, id)).left;
    assert!(width(SAMPLE) >= width(HEADING) - s(editor, 16), "the layout as wide as the page: {} of {}", width(SAMPLE), width(HEADING));
    assert!((KINDS..KINDS + 10).all(|id| !shown(editor, id)), "no buttons of the ten kinds");

    // Chosen from the layout, by the mouse or the arrows, and said.
    assert!(within_3s(|| text(editor, SUMMARY) == "Esc を送る"), "{:?}", text(editor, SUMMARY));
    choose_key(editor, 2, "Space");
    assert!(within_3s(|| text(editor, SUMMARY) == "押している間 Space"), "{:?}", text(editor, SUMMARY));
    focus_sample(editor);
    arrow(&[], VK_UP);
    assert!(within_3s(|| text(editor, LABEL) == "Ctrl"), "↑: Ctrl, {:?}", text(editor, LABEL));
}

// Issue #111, AC2
#[test]
fn the_layers_tabs_follow_the_layer_keys_and_keep_the_key_chosen() {
    let _desktop = desktop();
    let source = "[set.base]\nkey = A | shortcut | A\nkey = L | layer | num\nkey = T | layer | sym | mode=toggle\n\n\
        [set.num]\nkey = 1 | shortcut | 1\nkey = M | layer | more\n\n\
        [set.sym]\nkey = Q | shortcut | Q\nkey = B | layer | base\n\n\
        [set.more]\nkey = X | shortcut | X\n\n\
        [set.other]\nkey = O | shortcut | O\n\n\
        [profile.default]\nleft = base\n";
    let app = launch_with(Some(source), None);
    let editor = open_map(&app, "base");
    assert!(within_3s(|| layer_tabs(editor) == ["base", "num", "sym", "more"]), "{:?}", layer_tabs(editor));
    assert!(checked(part(editor, LAYER_TABS)), "base shown");
    assert_eq!(text(editor, SUMMARY), "A を送る");
    choose_key(editor, 1, "L");
    assert_eq!(text(editor, SUMMARY), "num に切り替える（押している間）");
    // The same key of num; more has none such: its first.
    click_id(editor, LAYER_TABS + 1);
    assert!(within_3s(|| text(editor, LABEL) == "M"), "num's second key: {:?}", text(editor, LABEL));
    assert!(checked(part(editor, LAYER_TABS + 1)) && !checked(part(editor, LAYER_TABS)), "num shown");
    // Another key chosen there: the tabs stay base's.
    choose_key(editor, 0, "1");
    assert!(within_3s(|| layer_tabs(editor) == ["base", "num", "sym", "more"]), "{:?}", layer_tabs(editor));
    assert_eq!(chosen(part(editor, SET)).as_deref(), Some("base"), "the drop-down list keeps base");
    click_id(editor, LAYER_TABS + 3);
    assert!(within_3s(|| text(editor, LABEL) == "X"), "more's first key: {:?}", text(editor, LABEL));
    // Changed there, it is more's key.
    press_key(editor, "Y");
    saved_as(&app, editor, "more", "X", &["X", "shortcut", "Y"]);
    // No layer keys: one tab.
    choose(editor, SET, "other");
    assert!(within_3s(|| layer_tabs(editor) == ["other"]), "{:?}", layer_tabs(editor));
}

// Issue #111, AC3
#[test]
fn the_modifiers_and_the_motion_go_with_the_key_pressed() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    let names: Vec<String> = (0..3).map(|i| text(editor, MOTIONS + i)).collect();
    assert_eq!(names, MOTION_NAMES);
    let row = window_rect(part(editor, MODS)).top;
    for id in [MOTIONS, MOTIONS + 2, CAPTURE] {
        assert!(shown(editor, id) && (window_rect(part(editor, id)).top - row).abs() <= s(editor, 4), "{id} in the modifiers' row");
    }
    assert!(!shown(editor, INTERVAL), "no interval but for 連打");
    // Esc: a modifier turned on changes it at once.
    click_id(editor, MODS + CTRL_ON);
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "Ctrl+Esc"]);
    click_id(editor, MOTIONS + 1);
    saved_as(&app, editor, "mods", "Esc", &["Esc", "hold", "Esc"]);
    click_id(editor, MOTIONS + 2);
    assert!(within_3s(|| shown(editor, INTERVAL)), "the interval for 連打");
    press_key(editor, "Z");
    saved_as(&app, editor, "mods", "Esc", &["Esc", "repeat", "Ctrl+Z"]);
    click_id(editor, MODS + CTRL_ON);
    click_id(editor, MOTIONS);
    assert!(within_3s(|| !shown(editor, INTERVAL)), "no interval now");
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "Z"]);
    // 押している間 takes a key alone, whatever the modifiers.
    click_id(editor, MODS + ALT_ON);
    click_id(editor, MOTIONS + 1);
    press_key(editor, "X");
    saved_as(&app, editor, "mods", "Esc", &["Esc", "hold", "X"]);
    // Captured, as before.
    click_id(editor, MOTIONS);
    click_id(editor, CAPTURE);
    assert!(within_3s(|| text(editor, CAPTURE).contains("取り込み中")), "{:?}", text(editor, CAPTURE));
    press_with(&[VK_CONTROL, VK_SHIFT], b'K' as u16);
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "Ctrl+Shift+K"]);
}

// Issue #111, AC4
#[test]
fn the_five_groups_assign_every_kind() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    let names: Vec<String> = (0..6).map(|i| text(editor, GROUP_TABS + i)).collect();
    assert_eq!(names, GROUP_TAB_NAMES);
    let rect = |name: &str| window_rect(part(editor, button_named(editor, KEYBOARD, 100, name)));
    // 基本: as a US keyboard.
    for name in ["F24", "Esc", "`", "Backspace", "Tab", "\\", "Enter", "/", "Space", "Home", "↑"] {
        rect(name);
    }
    assert!(rect("Q").right <= rect("W").left && rect("Q").bottom <= rect("A").top, "Q left of W, above A");
    assert!(rect("1").bottom <= rect("Q").top && rect("Space").top >= rect("Z").bottom, "the rows in order");

    // The number pad on its right, the modifier keys over it.
    let seven = button_named(editor, NUMPAD, 20, "7");
    let shift = button_named(editor, MOD_KEYS, 4, "Shift");
    assert!(shown(editor, seven) && shown(editor, shift), "the number pad and the modifier keys on 基本");
    let pad = window_rect(part(editor, seven));
    assert!(pad.left >= rect("Enter").right && pad.left >= rect("→").right, "the number pad right of the keyboard");
    assert!(window_rect(part(editor, shift)).bottom <= pad.top, "the modifier keys over it");
    click_id(editor, seven);
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "Numpad7"]);
    // A modifier key.
    click_id(editor, shift);
    saved_as(&app, editor, "mods", "Esc", &["Esc", "modifier", "Shift"]);
    // レイヤー: a layer key, toggled.
    click_id(editor, GROUP_TABS + 2);
    choose(editor, LAYER_SET, "spare");
    click_id(editor, LAYER_TOGGLE);
    saved_as(&app, editor, "mods", "Esc", &["Esc", "layer", "spare", "mode=toggle"]);
    // マクロ: a step added makes a macro; a key step takes 基本's keys.
    click_id(editor, GROUP_TABS + 3);
    click_id(editor, STEP_KEY);
    assert!(within_3s(|| text(editor, SUMMARY).contains("マクロ")), "{:?}", text(editor, SUMMARY));
    click_id(editor, GROUP_TABS);
    press_key(editor, "B");
    click_id(editor, GROUP_TABS + 3);
    assert!(within_3s(|| items(part(editor, STEPS)).contains(&"B".to_string())), "{:?}", items(part(editor, STEPS)));
    // A wait chosen: a key pressed is no step of keys, and the macro stays.
    click_id(editor, STEP_WAIT);
    let steps = items(part(editor, STEPS));
    click_id(editor, GROUP_TABS);
    press_key(editor, "C");
    assert!(within_3s(|| status(editor).contains("手順")), "why: {:?}", status(editor));
    click_id(editor, GROUP_TABS + 3);
    assert_eq!(items(part(editor, STEPS)), steps, "the macro kept");
    // 文字・起動: the field entered.
    click_id(editor, GROUP_TABS + 4);
    enter(editor, CONTENT, "こんにちは");
    saved_as(&app, editor, "mods", "Esc", &["Esc", "text", "こんにちは"]);
    enter(editor, RUN_CONTENT, "calc.exe");
    saved_as(&app, editor, "mods", "Esc", &["Esc", "run", "calc.exe"]);
    // ページ・パイ: a page, then a pie, its items on a fan (Issue #113).
    click_id(editor, GROUP_TABS + 5);
    choose(editor, TARGET, "次のページ");
    saved_as(&app, editor, "mods", "Esc", &["Esc", "page", "next"]);
    click_id(editor, PIE_MAKE);
    assert!(within_3s(|| shown(editor, PIE_BACK)), "the fan");
    save_until(&app, editor, "| pie-esc");
}

// Issue #111, AC5
#[test]
fn a_key_chosen_shows_its_assignment_in_the_groups_and_the_keyboard() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    // Ctrl: 基本, its modifier key Ctrl on.
    choose_key(editor, 1, "Ctrl");
    assert!(within_3s(|| checked(part(editor, GROUP_TABS))), "基本 opened");
    assert!(checked(part(editor, button_named(editor, MOD_KEYS, 4, "Ctrl"))), "Ctrl on");
    // 元に戻す, Ctrl+Z: 基本, Z and Ctrl on, sent when pressed.
    choose(editor, SET, "tools");
    assert!(within_3s(|| text(editor, LABEL) == "元に戻す"), "{:?}", text(editor, LABEL));
    assert!(within_3s(|| checked(part(editor, GROUP_TABS))), "基本 opened");
    let z = button_named(editor, KEYBOARD, 100, "Z");
    assert!(checked(part(editor, z)), "Z on");
    assert!(!checked(part(editor, button_named(editor, KEYBOARD, 100, "X"))), "X not");
    assert!(checked(part(editor, MODS + CTRL_ON)) && !checked(part(editor, MODS + ALT_ON)), "Ctrl on, Alt not");
    assert!(checked(part(editor, MOTIONS)) && !checked(part(editor, MOTIONS + 2)), "押した時に送る");
    assert_eq!(text(editor, SUMMARY), "Ctrl+Z を送る");
    // Changed: said at once, the keyboard following.
    press_key(editor, "X");
    assert!(within_3s(|| text(editor, SUMMARY) == "Ctrl+X を送る"), "{:?}", text(editor, SUMMARY));
    assert!(checked(part(editor, button_named(editor, KEYBOARD, 100, "X"))) && !checked(part(editor, z)), "X on now");
    // A text key: 文字・起動.
    let text_key = SETTINGS.replace("key = 電卓     | run      | calc.exe", "key = 電卓     | text     | やあ");
    drop(app);
    let app = launch_with(Some(&text_key), None);
    let editor = open_map(&app, "tools");
    focus_sample(editor);
    for _ in 0..4 {
        arrow(&[], VK_DOWN);
    }
    assert!(within_3s(|| text(editor, LABEL) == "電卓"), "{:?}", text(editor, LABEL));
    assert!(within_3s(|| checked(part(editor, GROUP_TABS + 4))), "文字・起動 opened");
    assert_eq!(text(editor, CONTENT), "やあ");
}

// Issue #111, AC6
#[test]
fn an_assignment_rewrites_its_line_alone_keeping_the_rest() {
    let _desktop = desktop();
    let source = SETTINGS.replace("key = A | shortcut | A\n", "key = 戻る | shortcut | Ctrl+Z | width=2   ; 揃えた列\n");
    let app = launch_with(Some(&source), None);
    let before = file(&app);
    let editor = open_map(&app, "spare");
    assert!(within_3s(|| text(editor, LABEL) == "戻る"), "{:?}", text(editor, LABEL));
    press_key(editor, "Y");
    let saved = save_until(&app, editor, "Ctrl+Y");
    let (old, new): (Vec<&str>, Vec<&str>) = (before.lines().collect(), saved.lines().collect());
    assert_eq!(old.len(), new.len(), "{saved}");
    for (a, b) in old.iter().zip(&new) {
        match a.contains("揃えた列") {
            true => assert!(b.contains("戻る") && b.contains("Ctrl+Y") && b.contains("width=2") && b.ends_with("; 揃えた列"), "{b}"),
            false => assert_eq!(a, b, "only the key's line changes"),
        }
    }
    // A value that does not read stops in its field.
    click_id(editor, MOTIONS + 2);
    assert!(within_3s(|| shown(editor, INTERVAL)), "the interval");
    for _ in 0..2 {
        enter(editor, INTERVAL, "5");
        if within_3s(|| is_wrong_field(editor, INTERVAL)) {
            break;
        }
    }
    assert!(is_wrong_field(editor, INTERVAL), "5 ms is stopped in its field: {:?}", status(editor));
    assert!(status(editor).contains("interval"), "why: {:?}", status(editor));
    save(editor);
    assert!(within_3s(|| key_of(&file(&app), "spare", "戻る")[1] == "repeat"), "{}", file(&app));
    assert!(!options_of(&file(&app), "spare", "戻る").contains(&"interval=5".to_string()), "not written: {}", file(&app));
}

// Issue #113: a pie's items edited on a fan. Its parts: パネルに戻る 1083,
// ＋ 1084 and 削除 1085, over the fan, shown while it is; パイにする 1063 on
// ページ・パイ (a button now, in place of the drop-down list of sets);
// and, while the fan shows, why some buttons cannot be pressed 1065 (a
// Static). The fan is drawn in the sample, as the pie menu is from the
// edge the panel is on: its middle the sample's inset in from that side,
// half way along it; for up to four items, its outside as far out as the
// room goes, the inset short of the sample's far side or its ends.
const PIE_MAKE: i32 = 1063;
const PIE_BACK: i32 = 1083;
const PIE_ADD: i32 = 1084;
const PIE_REMOVE: i32 = 1085;

fn enabled(editor: HWND, id: i32) -> bool {
    unsafe { IsWindowEnabled(part(editor, id)) != 0 }
}

/// A point of item `i` of a fan of `items` from the right edge, or the
/// left, near its outside: clear of its label.
fn fan_point(editor: HWND, right: bool, i: usize, items: usize) -> POINT {
    let r = window_rect(part(editor, SAMPLE));
    let inset = s(editor, INSET);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let room = f64::from((w - 2 * inset).min(h / 2 - inset));
    let a = -std::f64::consts::FRAC_PI_2 + (i as f64 + 0.5) * std::f64::consts::PI / items as f64;
    let (inward, along) = (0.93 * room * a.cos(), 0.93 * room * a.sin());
    let x = if right { f64::from(r.right - inset) - inward } else { f64::from(r.left + inset) + inward };
    POINT { x: x.round() as i32, y: (f64::from(r.top + h / 2) + along).round() as i32 }
}

/// Choose key `i` of the set shown by the keyboard, a pie key, and press
/// Enter: its fan opens on its first item, `first`. Gone over with the
/// arrows, it does not.
fn open_fan(editor: HWND, i: i32, first: &str) {
    for _ in 0..3 {
        focus_sample(editor);
        arrow(&[], VK_HOME);
        for _ in 0..i {
            arrow(&[], VK_DOWN);
        }
        assert!(!shown(editor, PIE_BACK), "no fan by the arrows");
        press(VK_RETURN);
        if within_3s(|| text(editor, LABEL) == first && shown(editor, PIE_BACK)) {
            return;
        }
    }
    panic!("the fan of key {i}, on {first}: {:?}", text(editor, LABEL));
}

// Issue #113, AC1
#[test]
fn a_pie_key_chosen_shows_its_fan_whose_items_are_chosen_on_it() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "tools");
    assert!(!shown(editor, PIE_BACK), "no fan for the panel");
    let (face, held) = ([0xFA, 0xFA, 0xFA], [0xC0, 0x00, 0x00]);
    // 変形, on the right edge: its fan opens to the left, 拡大 chosen.
    open_fan(editor, 2, "拡大");
    assert!(!shown(editor, LAYER_TABS), "no layers' tabs over the fan");
    let (first, second) = (fan_point(editor, true, 0, 2), fan_point(editor, true, 1, 2));
    let _ = within_3s(|| near(colour_at(first), held));
    assert!(near(colour_at(first), held), "拡大 chosen, as held: {:?}", colour_at(first));
    assert!(near(colour_at(second), face), "反転 under it: {:?}", colour_at(second));
    // Clicked, 反転.
    wait_in_front(editor);
    click(second);
    assert!(within_3s(|| text(editor, LABEL) == "反転"), "反転 chosen: {:?}", text(editor, LABEL));
    assert!(within_3s(|| near(colour_at(second), held)), "drawn as held: {:?}", colour_at(second));
    assert!(near(colour_at(first), face), "拡大 not");
    // By the keyboard, back up.
    focus_sample(editor);
    arrow(&[], VK_UP);
    assert!(within_3s(|| text(editor, LABEL) == "拡大"), "拡大 by ↑: {:?}", text(editor, LABEL));
    // Back to the panel, 変形 still chosen.
    click_id(editor, PIE_BACK);
    assert!(within_3s(|| text(editor, LABEL) == "変形"), "変形 chosen: {:?}", text(editor, LABEL));
    assert!(within_3s(|| !shown(editor, PIE_BACK)), "the panel again");
    assert!(shown(editor, LAYER_TABS), "the layers' tabs again");
    assert_eq!(text(editor, SUMMARY), "transform のパイメニューを開く");
}

// Issue #113, AC2
#[test]
fn an_item_is_assigned_as_a_key_is_but_not_as_items_cannot_be() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let before = file(&app);
    let editor = open_map(&app, "tools");
    open_fan(editor, 2, "拡大");
    assert!(within_3s(|| checked(part(editor, MODS + CTRL_ON))), "Ctrl on, of Ctrl+T");
    press_key(editor, "Y");
    let saved = save_until(&app, editor, "Ctrl+Y");
    assert_eq!(keys_of(&saved, "transform"), [vec!["拡大", "shortcut", "Ctrl+Y"], vec!["反転", "shortcut", "F14"]]);
    let changed: Vec<(&str, &str)> = before.lines().zip(saved.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(changed.len(), 1, "the item's line alone: {changed:?}");
    // What an item cannot be cannot be pressed, and why is said.
    let shift = button_named(editor, MOD_KEYS, 4, "Shift");
    for (id, what) in [(MOTIONS + 1, "押している間"), (MOTIONS + 2, "連打"), (shift, "a modifier key"), (GROUP_TABS + 2, "レイヤー")] {
        assert!(!enabled(editor, id), "{what} not pressed for an item");
    }
    assert!(enabled(editor, MOTIONS), "押した時に送る");
    assert!(shown(editor, PIE_NOTE) && text(editor, PIE_NOTE).contains("使えません"), "why: {:?}", text(editor, PIE_NOTE));
    click_id(editor, GROUP_TABS + 5);
    assert!(within_3s(|| shown(editor, PIE_MAKE)), "パイにする on ページ・パイ");
    assert!(!enabled(editor, PIE_MAKE), "no pie in a pie");
    // A text item, as a key's.
    click_id(editor, GROUP_TABS + 4);
    enter(editor, CONTENT, "やあ");
    let saved = save_until(&app, editor, "やあ");
    assert_eq!(key_of(&saved, "transform", "拡大"), ["拡大", "text", "やあ"]);
    // The panel's keys again: all of it.
    click_id(editor, PIE_BACK);
    assert!(within_3s(|| enabled(editor, MOTIONS + 1)), "押している間 again");
    assert!(!shown(editor, PIE_NOTE), "nothing to say");
}

// Issue #113, AC3
#[test]
fn items_are_added_and_removed_on_the_fan_from_two_to_eight() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "tools");
    open_fan(editor, 2, "拡大");
    assert_eq!((text(editor, PIE_ADD), text(editor, PIE_REMOVE)), ("＋".to_string(), "削除".to_string()));
    assert!(!enabled(editor, PIE_REMOVE), "two items: none removed");
    // After the chosen, sending nothing.
    click_id(editor, PIE_ADD);
    assert!(within_3s(|| text(editor, SUMMARY) == "何も送らない"), "the new item chosen: {:?}", text(editor, SUMMARY));
    let saved = save_until(&app, editor, "| none");
    let labels: Vec<String> = keys_of(&saved, "transform").into_iter().map(|k| k[0].clone()).collect();
    assert_eq!(labels.len(), 3, "{saved}");
    assert_eq!((labels[0].as_str(), labels[2].as_str()), ("拡大", "反転"), "{saved}");
    assert_eq!(keys_of(&saved, "transform")[1][1..], ["none", "-"]);
    assert!(!status(editor).contains("エラー"), "it reads: {:?}", status(editor));
    for _ in 0..5 {
        click_id(editor, PIE_ADD);
    }
    assert!(within_3s(|| !enabled(editor, PIE_ADD)), "eight: no more");
    let saved = save_until(&app, editor, "反転");
    assert_eq!(keys_of(&saved, "transform").len(), 8, "{saved}");
    for _ in 0..6 {
        click_id(editor, PIE_REMOVE);
    }
    assert!(within_3s(|| !enabled(editor, PIE_REMOVE)), "two: no fewer");
    save(editor);
    assert!(within_3s(|| keys_of(&file(&app), "transform").len() == 2), "{}", file(&app));
    assert!(enabled(editor, PIE_ADD));
}

// Issue #113, AC4
#[test]
fn a_key_made_a_pie_gets_its_own_items_and_loses_them_as_another_kind() {
    let _desktop = desktop();
    let source = SETTINGS.replace("[set.spare]\n", "[set.pie-esc]\nkey = B | shortcut | B\n\n[set.spare]\n");
    let app = launch_with(Some(&source), None);
    let editor = open_map(&app, "mods");
    click_id(editor, GROUP_TABS + 5);
    assert!(within_3s(|| shown(editor, PIE_MAKE)), "パイにする");
    assert_eq!(text(editor, PIE_MAKE), "パイにする");
    assert!(unsafe { GetDlgItem(editor, 1064) }.is_null(), "no figure on the tab");
    click_id(editor, PIE_MAKE);
    assert!(within_3s(|| shown(editor, PIE_BACK)), "the fan opened");
    let saved = save_until(&app, editor, "[set.pie-esc-2]");
    assert_eq!(key_of(&saved, "mods", "Esc"), ["Esc", "pie", "pie-esc-2"]);
    let kinds: Vec<Vec<String>> = keys_of(&saved, "pie-esc-2").iter().map(|k| k[1..].to_vec()).collect();
    assert_eq!(kinds, [["none", "-"], ["none", "-"]]);
    // Another kind: the items go.
    click_id(editor, PIE_BACK);
    assert!(within_3s(|| text(editor, LABEL) == "Esc"), "{:?}", text(editor, LABEL));
    click_id(editor, GROUP_TABS);
    press_key(editor, "A");
    save(editor);
    assert!(within_3s(|| !file(&app).contains("pie-esc-2")), "{}", file(&app));
    assert_eq!(key_of(&file(&app), "mods", "Esc"), ["Esc", "shortcut", "A"]);
    assert!(file(&app).contains("[set.pie-esc]"), "the other set kept");
}

// Issue #113, AC5
#[test]
fn a_pies_item_set_is_listed_nowhere_and_reads_as_it_was() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_set(&app, "mods");
    assert_eq!(items(part(editor, SET)), ["mods", "tools", "spare"], "キー配置");
    to_page(editor, MAP_PAGE);
    assert_eq!(items(part(editor, SET)), ["mods", "tools", "spare"], "キーマッピング");
    click_id(editor, GROUP_TABS + 2);
    assert!(within_3s(|| shown(editor, LAYER_SET)));
    assert!(!items(part(editor, LAYER_SET)).contains(&"transform".to_string()), "{:?}", items(part(editor, LAYER_SET)));
    click_id(editor, GROUP_TABS + 5);
    assert!(within_3s(|| shown(editor, TARGET)));
    assert!(!items(part(editor, TARGET)).contains(&"transform".to_string()), "{:?}", items(part(editor, TARGET)));
    to_page(editor, 2);
    for edge in 2020..2024 {
        assert!(!items(part(editor, edge)).contains(&"transform".to_string()), "{edge}: {:?}", items(part(editor, edge)));
    }
    // Its items, edited on the fan.
    to_page(editor, MAP_PAGE);
    choose(editor, SET, "tools");
    open_fan(editor, 2, "拡大");
}

// Issue #115: JIS, a group after 基本, its keyboard from 1500 (buttons, as
// 基本's). A symbol's button has two lines: what Shift makes of the key
// above, the key under it.
const JIS_KEYBOARD: i32 = 1500;

// Issue #115, AC1
#[test]
fn a_symbols_button_shows_what_shift_makes_above_the_key() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    for (key, both) in [("1", "!\n1"), ("/", "?\n/"), ("`", "~\n`"), ("\\", "|\n\\"), ("=", "+\n=")] {
        assert_eq!(text(editor, button_named(editor, KEYBOARD, 100, key)), both, "{key}");
    }
    assert_eq!(text(editor, button_named(editor, KEYBOARD, 100, "Q")), "Q", "a letter, one line");
    let style = unsafe { GetWindowLongW(part(editor, button_named(editor, KEYBOARD, 100, "1")), GWL_STYLE) } as u32;
    assert!(style & BS_MULTILINE as u32 != 0, "on two lines");
    press_key(editor, "1");
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "1"]);
}

// Issue #115, AC2
#[test]
fn the_jis_group_lays_out_a_jis_keyboard_as_basic_is() {
    let _desktop = desktop();
    let app = launch_with(Some(SETTINGS), None);
    let editor = open_map(&app, "mods");
    let basic_seven = window_rect(part(editor, button_named(editor, NUMPAD, 20, "7")));
    let basic_q = window_rect(part(editor, button_named(editor, KEYBOARD, 100, "Q")));
    click_id(editor, GROUP_TABS + 1);
    assert!(within_3s(|| checked(part(editor, GROUP_TABS + 1))), "JIS open");
    let jis = |name: &str| button_named(editor, JIS_KEYBOARD, 100, name);
    assert!(within_3s(|| shown(editor, jis("@"))), "JIS's keys");
    assert!(!shown(editor, button_named(editor, KEYBOARD, 100, "Q")), "not 基本's");
    for (key, both) in [("2", "\"\n2"), ("@", "`\n@"), (":", "*\n:"), ("¥", "|\n¥"), ("ろ", "_\nろ")] {
        assert_eq!(text(editor, jis(key)), both, "{key}");
    }
    for name in ["半角\n全角", "無変換", "変換", "かな", "Enter", "Space"] {
        let found = (JIS_KEYBOARD..JIS_KEYBOARD + 100).any(|id| text(editor, id) == name);
        assert!(found, "{name:?} on JIS");
    }
    let rect = |name: &str| window_rect(part(editor, jis(name)));
    let corners = |r: RECT| (r.left, r.top, r.right, r.bottom);
    assert_eq!(corners(rect("Q")), corners(basic_q), "as large as 基本's, in its place");
    assert!(rect("@").left >= rect("P").right && rect(":").left >= rect(";").right, "@ after P, : after ;");
    assert!(rect("ろ").left >= rect("/").right, "ろ after /");
    let enter = (JIS_KEYBOARD..JIS_KEYBOARD + 100).find(|&id| text(editor, id) == "Enter").unwrap();
    let enter = window_rect(part(editor, enter));
    assert!(enter.bottom - enter.top > 3 * (basic_q.bottom - basic_q.top) / 2, "Enter two rows tall");
    let seven = button_named(editor, NUMPAD, 20, "7");
    assert!(shown(editor, seven) && corners(window_rect(part(editor, seven))) == corners(basic_seven), "the number pad where it is on 基本");
    assert!(shown(editor, button_named(editor, MOD_KEYS, 4, "Shift")), "the modifier keys too");
    // A JIS key sends what that key sends on Windows: written by the US
    // key of the same virtual key.
    click_id(editor, jis("@"));
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "`"]);
    click_id(editor, jis("¥"));
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "\\"]);
    click_id(editor, jis("2"));
    saved_as(&app, editor, "mods", "Esc", &["Esc", "shortcut", "2"]);
}

// Issue #115, AC3
#[test]
fn jis_only_keys_are_written_by_name_sent_and_shown_on_jis() {
    let _desktop = desktop();
    let source = "[set.jis]\n\
        key = かな | shortcut | Kana\n\
        key = 変換 | shortcut | Ctrl+Henkan\n\
        key = 無変換 | hold | Muhenkan\n\
        key = ろ | repeat | Ro | interval=500\n\
        key = 半角 | macro | Hankaku\n\
        [profile.default]\nleft = jis\n";
    let app = launch_with(Some(source), None);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    for i in 0..5 {
        touch.down(1, key_centre(panel, i));
        touch.hold(100);
        touch.up(1);
        touch.hold(200);
    }
    let downs: Vec<u16> = log.keys().into_iter().filter(|(_, up)| !up).map(|(vk, _)| vk).collect();
    assert_eq!(downs, [0xF2, VK_LCONTROL, 0x1C, 0x1D, 0xE2, 0x19], "かな, Ctrl+変換, 無変換, ろ, 半角/全角");
    // Chosen on キーマッピング: JIS open, its button on.
    let editor = open(&app, open_id(&app));
    to_page(editor, MAP_PAGE);
    assert!(within_3s(|| text(editor, LABEL) == "かな"), "{:?}", text(editor, LABEL));
    assert!(within_3s(|| checked(part(editor, GROUP_TABS + 1))), "JIS opened");
    assert!(checked(part(editor, button_named(editor, JIS_KEYBOARD, 100, "かな"))), "かな on");
    assert_eq!(text(editor, SUMMARY), "Kana を送る");
    choose_key(editor, 3, "ろ");
    assert!(within_3s(|| checked(part(editor, button_named(editor, JIS_KEYBOARD, 100, "ろ")))), "ろ on");
}

// Issue #126: a KLE set with a key turned 45° about its middle (R), and
// one not turned to its right (N), on the top edge.
const TURNED_KLE: &str = r#"[{r:45,rx:0.5,ry:0.5,x:-0.5,y:-0.5},"R"],[{r:0,rx:0,ry:0,x:3},"N"]"#;

fn open_turned(app: &Running) -> HWND {
    std::fs::write(exe_dir(app).join("turn.json"), TURNED_KLE).expect("write the KLE file");
    open_set(app, "turned")
}

fn turned_settings() -> String {
    SETTINGS
        .replace(
            "[set.spare]\nkey = A | shortcut | A\n",
            "[set.spare]\nkey = A | shortcut | A\n\n[set.turned]\nlayout = kle:turn.json\nkey = R | shortcut | R\nkey = N | shortcut | N\n",
        )
        .replace("[profile.default]\nleft = mods\n", "[profile.default]\nleft = mods\ntop = turned\n")
}

/// R's middle in the sample: the layout starts a quarter left of and above
/// R's square.
fn turned_middle(editor: HWND) -> POINT {
    let r = window_rect(part(editor, SAMPLE));
    let key = sample_key(editor);
    POINT { x: r.left + s(editor, INSET) + key * 3 / 4, y: r.top + s(editor, INSET) + key * 3 / 4 }
}

// Issue #126, AC4
#[test]
fn the_sample_draws_a_turned_key_turned_and_picks_it_by_its_shape() {
    let _desktop = desktop();
    let app = launch_with(Some(&turned_settings()), None);
    let editor = open_turned(&app);
    let key = sample_key(editor);
    let c = turned_middle(editor);
    let (tip, corner) = (POINT { x: c.x + key * 55 / 100, y: c.y }, POINT { x: c.x + key * 42 / 100, y: c.y + key * 42 / 100 });
    let r = window_rect(part(editor, SAMPLE));
    let background = colour_at(POINT { x: r.left + s(editor, INSET) + 1, y: r.top + s(editor, INSET) + 1 });
    assert!(near(colour_at(tip), colour_at(POINT { x: c.x, y: c.y + key * 3 / 10 })), "R's tip drawn as R");
    assert!(near(colour_at(corner), background), "the square's corner not drawn");

    // N chosen; its x is 3.
    let n = POINT { x: c.x + key * 3, y: c.y };
    reach(editor, part(editor, SAMPLE), n);
    click(n);
    assert!(within_3s(|| text(editor, PLACE_X) == "3"), "N chosen: x {:?}", text(editor, PLACE_X));
    // The corner is no key's: N stays chosen.
    reach(editor, part(editor, SAMPLE), corner);
    click(corner);
    pump(300);
    assert_eq!(text(editor, PLACE_X), "3", "the corner chooses nothing");
    // The tip is R's.
    reach(editor, part(editor, SAMPLE), tip);
    click(tip);
    assert!(within_3s(|| text(editor, PLACE_X) == "0"), "R chosen by its tip: x {:?}", text(editor, PLACE_X));
}

// Issue #126, AC5
#[test]
fn a_turned_kle_set_made_free_loses_the_turn_and_says_so() {
    let _desktop = desktop();
    let app = launch_with(Some(&turned_settings()), None);
    let editor = open_turned(&app);
    arrange(editor, ARRANGEMENTS[1]);
    assert!(within_3s(|| status(editor).contains("回転")), "said: {:?}", status(editor));
    let saved = save_until(&app, editor, "R | shortcut | R | x=");
    assert_eq!(options_of(&saved, "turned", "R"), ["x=0", "y=0", "w=1", "h=1"], "{saved}");
    assert_eq!(options_of(&saved, "turned", "N"), ["x=3", "y=0", "w=1", "h=1"], "{saved}");
}

// Issue #139: the room after the chosen key, 1015 (a drop-down list of
// なし and 0.25 to 2 keys), on キー配置 beside the width.
const SPACE: i32 = 1015;

const SPACED: &str = "[general]\ntheme = light\naccent = #C00000\n\n\
    [set.col]   ; 一列\n\
    key = X | shortcut | X   ; 一つ目\n\
    key = Y | shortcut | Y\n\
    key = Z | shortcut | Z\n\n\
    [profile.default]\nleft = col\n";

/// Where down the sample's first column its keys are read: a quarter in
/// from their left side, clear of their labels, which the smaller keys of
/// キーマッピング run into, and of their rounded corners, larger on larger
/// keys.
fn down_x(editor: HWND) -> i32 {
    window_rect(part(editor, SAMPLE)).left + s(editor, INSET) + sample_key(editor) / 4
}

/// Down the sample's first column, the runs of the panel's keys (its face,
/// or held for the chosen key) and of what is between them, in pixels:
/// (key or not, length), from the panel's top.
fn runs_down(editor: HWND) -> Vec<(bool, i32)> {
    let r = window_rect(part(editor, SAMPLE));
    let x = down_x(editor);
    let (face, held) = ([0xFA, 0xFA, 0xFA], [0xC0, 0x00, 0x00]);
    let mut runs: Vec<(bool, i32)> = Vec::new();
    for y in r.top + s(editor, INSET)..r.bottom {
        let c = colour_at(POINT { x, y });
        let key = near(c, face) || near(c, held);
        match runs.last_mut() {
            Some((k, n)) if *k == key => *n += 1,
            _ => runs.push((key, 1)),
        }
    }
    runs
}

/// The keys down the sample, and the gaps between them: (key lengths, gap
/// lengths). Two keys side by side have the panel's narrow gap between.
fn keys_and_gaps(editor: HWND) -> (Vec<i32>, Vec<i32>) {
    let runs = runs_down(editor);
    let keys: Vec<i32> = runs.iter().filter(|(k, _)| *k).map(|&(_, n)| n).collect();
    let first = runs.iter().position(|(k, _)| *k).unwrap_or(0);
    let last = runs.iter().rposition(|(k, _)| *k).unwrap_or(0);
    let gaps: Vec<i32> = runs[first..last].iter().filter(|(k, _)| !*k).map(|&(_, n)| n).collect();
    (keys, gaps)
}

// Issue #139, AC3
#[test]
fn the_room_after_a_key_is_added_changed_and_removed_on_key_layout() {
    let _desktop = desktop();
    let app = launch_with(Some(SPACED), None);
    let before = file(&app);
    let editor = open_set(&app, "col");
    choose_key(editor, 0, "X");
    assert!(within_3s(|| shown(editor, SPACE)), "the room after X");
    assert_eq!(items(part(editor, SPACE)), ["なし", "0.25", "0.5", "0.75", "1", "1.25", "1.5", "1.75", "2"]);
    assert_eq!(chosen(part(editor, SPACE)).as_deref(), Some("なし"));

    // A key's room after X: drawn in the sample, written after X's line.
    choose(editor, SPACE, "1");
    assert!(within_3s(|| keys_and_gaps(editor).0.len() == 3), "{:?}", runs_down(editor));
    let (keys, gaps) = keys_and_gaps(editor);
    // A key's room is a key's pitch: its face and the narrow gap.
    let pitch = keys[1] + gaps[1];
    assert!(gaps[1] < keys[1] / 4, "none between Y and Z: {keys:?} {gaps:?}");
    assert!((gaps[0] - gaps[1] - pitch).abs() <= pitch / 8, "a key's room between X and Y: {keys:?} {gaps:?}");
    let saved = save_until(&app, editor, "space = 1");
    let mut expect: Vec<&str> = before.lines().collect();
    let x = expect.iter().position(|l| l.contains("一つ目")).unwrap();
    expect.insert(x + 1, "space = 1");
    assert_eq!(saved.lines().collect::<Vec<_>>(), expect, "only the space line added, after X's:\n{saved}");

    // Changed, then taken away.
    choose(editor, SPACE, "0.5");
    let saved = save_until(&app, editor, "space = 0.5");
    assert_eq!(saved.matches("space").count(), 1, "{saved}");
    choose(editor, SPACE, "なし");
    save(editor);
    assert!(within_3s(|| file(&app) == before), "back as it was:\n{}", file(&app));
    assert!(within_3s(|| keys_and_gaps(editor).1.iter().all(|&g| g < keys[1] / 4)), "{:?}", runs_down(editor));
}

// Issue #139, AC4
#[test]
fn the_room_is_drawn_on_key_mapping_and_cannot_be_chosen() {
    let _desktop = desktop();
    let app = launch_with(Some(&SPACED.replace("; 一つ目\n", "; 一つ目\nspace = 1\n")), None);
    let editor = open_map(&app, "col");
    assert!(within_3s(|| keys_and_gaps(editor).0.len() == 3), "{:?}", runs_down(editor));
    let (keys, gaps) = keys_and_gaps(editor);
    let pitch = keys[1] + gaps[1];
    assert!((gaps[0] - gaps[1] - pitch).abs() <= pitch / 8, "a key's room between X and Y: {keys:?} {gaps:?}");

    // Clicked, the room chooses nothing: X stays chosen.
    choose_key(editor, 0, "X");
    let r = window_rect(part(editor, SAMPLE));
    let start = runs_down(editor).first().map_or(0, |&(key, n)| if key { 0 } else { n });
    let top = r.top + s(editor, INSET) + start;
    let x = down_x(editor);
    wait_in_front(editor);
    click(POINT { x, y: top + keys[0] + gaps[0] / 2 });
    pump(300);
    assert_eq!(text(editor, LABEL), "X", "the room is no key");
    // From X, the next key down is Y.
    choose_key(editor, 1, "Y");
    wait_in_front(editor);
    click(POINT { x, y: top + keys[0] + gaps[0] + keys[1] + gaps[1] + keys[2] / 2 });
    assert!(within_3s(|| text(editor, LABEL) == "Z"), "Z, past the room: {:?}", text(editor, LABEL));
}

// Issue #137: the sample as large as the window lets it be, a key no
// smaller than 40 logical pixels across, scrolled where that does not fit;
// the window opened at 1100 by 860, and where it was when it opens again.
// The sample says how large it draws a key, in pixels, by its property
// "key"; it scrolls by its own scroll bars.
const LEAST_KEY: i32 = 40;
const MOST_KEY: i32 = 64;

/// The shipped 64 keys, and their Fn layer, along the bottom.
fn alice_settings() -> String {
    let preset = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/edgepad.ini"))
        .expect("the shipped settings");
    let set = |name: &str| {
        let at = preset.find(&format!("[set.{name}]")).unwrap_or_else(|| panic!("{name} shipped"));
        let end = preset[at + 1..].find("\n[").map_or(preset.len(), |n| at + 2 + n);
        preset[at..end].to_string()
    };
    format!("[general]\ntheme = light\n\n{}\n{}\n[profile.default]\nbottom = kb-alice64\n", set("kb-alice64"), set("kb-alice64.fn"))
}

/// `count` keys along the top edge, K1 on, the last LAST.
fn row_settings(count: usize) -> String {
    let mut text = String::from("[general]\ntheme = light\naccent = #C00000\n\n[set.row]\n");
    for i in 1..count {
        text += &format!("key = K{i} | shortcut | A\n");
    }
    text + "key = LAST | shortcut | B\n\n[profile.default]\ntop = row\n"
}

/// How large the sample draws a key, in pixels, once it has drawn.
fn sample_key(editor: HWND) -> i32 {
    let name = wide("key");
    let key = || unsafe { GetPropW(part(editor, SAMPLE), name.as_ptr()) as i32 };
    let _ = within_3s(|| key() != 0);
    key()
}

/// Whether the sample shows a scroll bar.
fn scrolls(editor: HWND) -> bool {
    let style = unsafe { GetWindowLongW(part(editor, SAMPLE), GWL_STYLE) } as u32;
    style & (WS_HSCROLL | WS_VSCROLL) != 0
}

/// Size the window `w` by `h` pixels where it is: no smaller than it lets
/// it be.
fn resize(editor: HWND, w: i32, h: i32) {
    unsafe { SetWindowPos(editor, null_mut(), 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE) };
    pump(300);
    settle(editor);
}

/// The work area of the monitor `editor` is on.
fn work_area(editor: HWND) -> RECT {
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    let mut info: MONITORINFO = unsafe { zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    unsafe { GetMonitorInfoW(MonitorFromWindow(editor, MONITOR_DEFAULTTONEAREST), &mut info) };
    info.rcWork
}

/// The point of the sample `from_end` keys in from the right end of its
/// one row, as scrolled to that end: in the key's face.
fn from_right_end(editor: HWND, from_end: i32) -> POINT {
    let sample = part(editor, SAMPLE);
    let r = client_rect(sample);
    let key = sample_key(editor);
    POINT { x: r.right - s(editor, INSET) - key * from_end - key / 2, y: r.top + s(editor, INSET) + key / 2 }
}

/// Close the window, with nothing to save, and open it again.
fn reopen(app: &Running, editor: HWND) -> HWND {
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| editors(app).is_empty()), "closed");
    let editor = open(app, open_id(app));
    settle(editor);
    editor
}

// Issue #137, AC1
#[test]
fn the_window_opens_large_enough_for_the_64_keys_to_read() {
    let _desktop = desktop();
    let app = launch_with(Some(&alice_settings()), None);
    let json = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/kb-alice64.json");
    std::fs::copy(json, exe_dir(&app).join("kb-alice64.json")).expect("the KLE file");
    let editor = open_set(&app, "kb-alice64");

    // 1100 by 860, or as much as the work area has.
    let (c, w, a) = (client_rect(editor), window_rect(editor), work_area(editor));
    let full_w = (c.right - c.left - s(editor, 1100)).abs() <= 1;
    let full_h = (c.bottom - c.top - s(editor, 860)).abs() <= 1;
    let wide_enough = full_w || w.right - w.left >= a.right - a.left - 1;
    let tall_enough = full_h || w.bottom - w.top >= a.bottom - a.top - 1;
    assert!(wide_enough && tall_enough, "client {}x{}, work area {}x{}", c.right - c.left, c.bottom - c.top, a.right - a.left, a.bottom - a.top);

    // Opened as large as that, all of it unscrolled; on a smaller screen
    // (CI's is 1024 x 768), scrolled, its keys no smaller.
    to_page(editor, MAP_PAGE);
    assert!(within_3s(|| sample_key(editor) >= s(editor, LEAST_KEY)), "a key {} px, at least {}", sample_key(editor), s(editor, LEAST_KEY));
    if full_w && full_h {
        assert!(!scrolls(editor), "all of it, unscrolled");
    }
}

// Issue #137, AC2
#[test]
fn a_sample_too_large_for_the_window_scrolls_to_every_key() {
    let _desktop = desktop();
    let app = launch_with(Some(&row_settings(30)), None);
    let editor = open_set(&app, "row");
    resize(editor, 120, 90);

    // On キー配置, then on キーマッピング: scrolled to the end, its keys
    // there are chosen.
    for (page, from_end, label) in [(0, 0, "LAST"), (MAP_PAGE, 1, "K29")] {
        to_page(editor, page);
        assert!(within_3s(|| scrolls(editor)), "{}: scrolls", PAGES[page]);
        assert!(sample_key(editor) >= s(editor, LEAST_KEY), "{}: a key {} px", PAGES[page], sample_key(editor));
        unsafe { SendMessageW(part(editor, SAMPLE), WM_HSCROLL, SB_RIGHT as usize, 0) };
        settle(editor);
        let at = from_right_end(editor, from_end);
        wait_in_front(editor);
        click(at);
        assert!(within_3s(|| text(editor, LABEL) == label), "{}: {label} chosen at its end, not {:?}", PAGES[page], text(editor, LABEL));
    }
}

// Issue #137, AC3
#[test]
fn the_sample_follows_the_window_which_opens_again_as_it_closed() {
    let _desktop = desktop();
    let app = launch_with(Some(&row_settings(11)), None);
    let mut editor = open_map(&app, "row");
    assert!(within_3s(|| sample_key(editor) == s(editor, MOST_KEY)), "as large as it goes: {}", sample_key(editor));

    // Smaller, the sample with it; larger, back.
    resize(editor, 120, 90);
    assert!(within_3s(|| sample_key(editor) < s(editor, MOST_KEY)), "smaller: {}", sample_key(editor));
    assert!(sample_key(editor) >= s(editor, LEAST_KEY) && !scrolls(editor), "{} px, all of it", sample_key(editor));
    unsafe { ShowWindow(editor, SW_MAXIMIZE) };
    settle(editor);
    assert!(within_3s(|| sample_key(editor) == s(editor, MOST_KEY)), "larger: {}", sample_key(editor));

    // Maximized, it opens maximized again.
    editor = reopen(&app, editor);
    assert!(unsafe { IsZoomed(editor) } != 0, "opens maximized");

    // Where it was and as large, not maximized.
    unsafe { ShowWindow(editor, SW_RESTORE) };
    settle(editor);
    let a = work_area(editor);
    let (w, h) = (s(editor, 960).min(a.right - a.left), s(editor, 700).min(a.bottom - a.top));
    let (x, y) = (a.right - w - s(editor, 10), a.bottom - h);
    unsafe { SetWindowPos(editor, null_mut(), x.max(a.left), y.max(a.top), w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    settle(editor);
    let before = window_rect(editor);
    editor = reopen(&app, editor);
    assert!(unsafe { IsZoomed(editor) } == 0, "not maximized");
    let after = window_rect(editor);
    assert_eq!(
        (after.left, after.top, after.right, after.bottom),
        (before.left, before.top, before.right, before.bottom),
        "where it closed"
    );
}

// Issue #138: layers of a KLE set. Beside the layers' tabs, レイヤーを追加
// 1086, 名前の変更 1087 and 削除 1088 (buttons, on a KLE set); in the group
// レイヤー, 透過 1016 (a button). A layer is named in the name field 1001,
// as a set is.
const ADD_LAYER: i32 = 1086;
const RENAME_LAYER: i32 = 1087;
const REMOVE_LAYER: i32 = 1088;
const TRANS: i32 = 1016;
const ROW_KLE: &str = r#"["A","B","C","D"]"#;

/// kb, a row of four keys from a KLE file, and its layer fn; other, along
/// the left edge.
const LAYERED: &str = "[general]\ntheme = light\n\n\
    [set.kb]   ; KLE の 4 キー\n\
    layout = kle:row.json\n\
    key = A  | shortcut | A\n\
    key = Fn | layer    | kb.fn\n\n\
    [set.kb.fn]\n\
    key = 1 | shortcut | 1\n\n\
    [set.other]\n\
    key = O | shortcut | O\n\n\
    [profile.default]\nleft = other\ntop = kb\n";

fn launch_layered() -> Running {
    let app = launch_with(Some(LAYERED), None);
    std::fs::write(exe_dir(&app).join("row.json"), ROW_KLE).expect("write the KLE file");
    app
}

/// Type `name` in the name field, out for it, and press Enter.
fn name_it(editor: HWND, name: &str) {
    assert!(within_3s(|| shown(editor, NAME) && focus_of(editor) == part(editor, NAME)), "a name to type");
    press_with(&[VK_CONTROL], b'A' as u16);
    type_text(name);
    press(VK_RETURN);
    settle(editor);
}

// Issue #138, AC4
#[test]
fn a_kle_sets_layers_are_its_tabs_added_named_removed_and_left_through() {
    let _desktop = desktop();
    let app = launch_layered();
    let editor = open_map(&app, "kb");
    assert_eq!(items(part(editor, SET)), ["kb", "other"], "no layer in the drop-down list");
    assert!(within_3s(|| layer_tabs(editor) == ["kb", "fn"]), "{:?}", layer_tabs(editor));
    for id in [ADD_LAYER, RENAME_LAYER, REMOVE_LAYER] {
        assert!(shown(editor, id), "{id} beside the tabs");
    }

    // Added: every key through to kb's, the new layer shown.
    click_id(editor, ADD_LAYER);
    assert!(within_3s(|| layer_tabs(editor) == ["kb", "fn", "layer1"]), "{:?}", layer_tabs(editor));
    assert!(checked(part(editor, LAYER_TABS + 2)), "the new layer shown");
    choose_key(editor, 2, "C");
    assert!(text(editor, SUMMARY).contains("透過"), "{:?}", text(editor, SUMMARY));
    press_key(editor, "Z");
    let saved = save_until(&app, editor, "[set.kb.layer1]");
    assert_eq!(keys_of(&saved, "kb.layer1"), [["-", "trans", "-"], ["-", "trans", "-"], ["C", "shortcut", "Z"]], "{saved}");
    // 透過 gives it back to kb's.
    click_id(editor, GROUP_TABS + 2);
    click_id(editor, TRANS);
    assert!(within_3s(|| text(editor, SUMMARY).contains("透過")), "{:?}", text(editor, SUMMARY));
    save(editor);
    assert!(within_3s(|| keys_of(&file(&app), "kb.layer1").get(2).is_some_and(|k| k[1] == "trans")), "{}", file(&app));

    // Named, and taken away where nothing turns to it.
    click_id(editor, RENAME_LAYER);
    name_it(editor, "num");
    assert!(within_3s(|| layer_tabs(editor) == ["kb", "fn", "num"]), "{:?}", layer_tabs(editor));
    let saved = save_until(&app, editor, "[set.kb.num]");
    assert!(!saved.contains("layer1"), "{saved}");
    click_id(editor, LAYER_TABS + 1);
    click_id(editor, REMOVE_LAYER);
    assert!(within_3s(|| status(editor).contains("Fn")), "fn is Fn's: {:?}", status(editor));
    assert_eq!(layer_tabs(editor), ["kb", "fn", "num"], "kept");
    click_id(editor, LAYER_TABS + 2);
    click_id(editor, REMOVE_LAYER);
    assert!(within_3s(|| layer_tabs(editor) == ["kb", "fn"]), "{:?}", layer_tabs(editor));

    // kb renamed, on キー配置: its layers and what turns to them follow.
    to_page(editor, 0);
    click_id(editor, RENAME);
    name_it(editor, "board");
    let saved = save_until(&app, editor, "[set.board.fn]");
    assert!(saved.contains("[set.board]") && saved.contains("top = board") && !saved.contains("kb"), "{saved}");
    assert_eq!(key_of(&saved, "board", "Fn"), ["Fn", "layer", "board.fn"]);

    // A set not of a KLE file has no layers of its own.
    choose(editor, SET, "other");
    assert!(within_3s(|| !shown(editor, ADD_LAYER)), "no レイヤーを追加 on other");
}

// Issue #138, AC5
#[test]
fn a_kle_file_imported_again_keeps_each_layers_key_lines() {
    let _desktop = desktop();
    let app = launch_layered();
    let dir = exe_dir(&app);
    let from = dir.join("from");
    std::fs::create_dir_all(&from).expect("a folder to import from");
    std::fs::write(from.join("wide.json"), r#"["A","B","C","D","E"]"#).expect("write the KLE file");
    let before = file(&app);
    let editor = open_set(&app, "kb");
    import(editor, &from.join("wide.json"));
    let saved = save_until(&app, editor, "layout = kle:wide.json");
    assert_eq!(key_lines(&saved, "kb"), key_lines(&before, "kb"), "kb's keys kept");
    assert_eq!(key_lines(&saved, "kb.fn"), key_lines(&before, "kb.fn"), "fn's keys kept");
    assert!(!section(&saved, "kb.fn").contains("layout"), "the layer has kb's layout: {saved}");
}

// Issue #161: a key's flicks assigned on a cross. Its parts: フリック 1089,
// shown on キーマッピング for a key that can flick; over the cross,
// パネルに戻る 1083 and 削除 1085, as over a pie's fan, and why some
// buttons cannot be pressed 1017 (a Static). The cross is drawn in the
// sample: the key in the sample's middle, each way a key from it, its keys
// as large as the sample says ("key"). A way with nothing is chosen as
// sending nothing, labelled with the way: 上, 下, 左 or 右.
const FLICK: i32 = 1089;
const FLICK_NOTE: i32 = 1017;

/// コピー with 全選択 up, and a modifier, which cannot flick.
const FLICKS: &str = "[general]\ntheme = light\naccent = #C00000\n\n\
[set.pad]\n\
key = コピー | shortcut | Ctrl+C\n\
flick = up | 全選択 | shortcut | Ctrl+A\n\
key = Ctrl | modifier | Ctrl\n\
\n\
[profile.default]\n\
left = pad\n";

/// A point of the cross's key `dx`, `dy` keys from its middle, on the
/// screen: in its face, clear of its label.
fn cross_cell(editor: HWND, dx: i32, dy: i32) -> POINT {
    let r = client_rect(part(editor, SAMPLE));
    let key = sample_key(editor);
    POINT { x: (r.left + r.right) / 2 + dx * key - key / 3, y: (r.top + r.bottom) / 2 + dy * key - key / 3 }
}

/// Choose key `i`, `label`, and open its cross.
fn open_cross(editor: HWND, i: i32, label: &str) {
    choose_key(editor, i, label);
    let pid = pid_of(editor);
    click_id(editor, FLICK);
    diagnose(editor, pid);
    assert!(within_3s(|| shown(editor, PIE_BACK)), "the cross of {label}");
}

/// TEMPORARY: how the window went, if it did.
fn diagnose(editor: HWND, pid: u32) {
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    pump(300);
    if unsafe { IsWindow(editor) } != 0 {
        return;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    let mut code = 0u32;
    let got = !process.is_null() && unsafe { GetExitCodeProcess(process, &mut code) } != 0;
    panic!("the window went: process {pid}, exit code read {got}, 0x{code:08X}");
}

/// Click the cross's key `dx`, `dy` keys from its middle, and wait for its
/// label, `label`, to show.
fn choose_way(editor: HWND, dx: i32, dy: i32, label: &str) {
    for _ in 0..3 {
        wait_in_front(editor);
        click(cross_cell(editor, dx, dy));
        if within_3s(|| text(editor, LABEL) == label) {
            return;
        }
    }
    panic!("the way {dx}, {dy}, {label}, chosen: {:?}", text(editor, LABEL));
}

/// The flick lines of set `name` in `saved`: the way and the fields of each.
fn flick_lines(saved: &str, name: &str) -> Vec<Vec<String>> {
    section(saved, name)
        .lines()
        .filter(|l| l.trim_start().starts_with("flick"))
        .map(|l| l.split_once('=').unwrap().1.split('|').map(|f| f.trim().to_string()).collect())
        .collect()
}

// Issue #161, AC1
#[test]
fn a_key_that_can_flick_opens_a_cross_of_its_ways_in_place_of_the_sample() {
    let _desktop = desktop();
    let app = launch_with(Some(FLICKS), None);
    let editor = open_map(&app, "pad");
    choose_key(editor, 1, "Ctrl");
    assert!(within_3s(|| !shown(editor, FLICK)), "no フリック for a modifier");
    choose_key(editor, 0, "コピー");
    assert!(within_3s(|| shown(editor, FLICK)), "フリック for a shortcut");
    assert!(!shown(editor, PIE_BACK), "no cross yet");
    let pid = pid_of(editor);
    click_id(editor, FLICK);
    diagnose(editor, pid);
    assert!(within_3s(|| shown(editor, PIE_BACK)), "the cross");
    assert!(!shown(editor, FLICK) && !shown(editor, LAYER_TABS), "no フリック nor layers' tabs over the cross");
    assert_eq!(text(editor, LABEL), "コピー", "its middle chosen: the key itself");
    let (face, held) = ([0xFA, 0xFA, 0xFA], [0xC0, 0x00, 0x00]);
    let _ = within_3s(|| near(colour_at(cross_cell(editor, 0, 0)), held));
    assert!(near(colour_at(cross_cell(editor, 0, 0)), held), "the key, chosen, as held: {:?}", colour_at(cross_cell(editor, 0, 0)));
    assert!(near(colour_at(cross_cell(editor, 0, -1)), face), "up, a key face: {:?}", colour_at(cross_cell(editor, 0, -1)));
    // Up: its flick.
    choose_way(editor, 0, -1, "全選択");
    assert!(within_3s(|| near(colour_at(cross_cell(editor, 0, -1)), held)), "up chosen, as held");
    assert_eq!(text(editor, SUMMARY), "Ctrl+A を送る");
    // Left: nothing yet.
    choose_way(editor, -1, 0, "左");
    assert_eq!(text(editor, SUMMARY), "何も送らない", "left has nothing");
    // Back to the panel, コピー still chosen.
    click_id(editor, PIE_BACK);
    assert!(within_3s(|| !shown(editor, PIE_BACK) && text(editor, LABEL) == "コピー"), "the panel again: {:?}", text(editor, LABEL));
    assert!(shown(editor, FLICK), "フリック again");
}

// Issue #161, AC2
#[test]
fn a_way_is_assigned_as_a_pie_item_is_its_line_added_with_it_and_removed() {
    let _desktop = desktop();
    let app = launch_with(Some(FLICKS), None);
    let editor = open_map(&app, "pad");
    open_cross(editor, 0, "コピー");
    choose_way(editor, 0, -1, "全選択");
    assert!(within_3s(|| checked(part(editor, MODS + CTRL_ON))), "Ctrl on, of Ctrl+A");
    press_key(editor, "V");
    let saved = save_until(&app, editor, "Ctrl+V");
    assert_eq!(flick_lines(&saved, "pad"), [vec!["up", "全選択", "shortcut", "Ctrl+V"]]);
    assert_eq!(key_of(&saved, "pad", "コピー"), ["コピー", "shortcut", "Ctrl+C"], "the key as it was");
    // What a flick cannot be cannot be pressed, and why is said.
    let shift = button_named(editor, MOD_KEYS, 4, "Shift");
    for (id, what) in [(MOTIONS + 1, "押している間"), (MOTIONS + 2, "連打"), (shift, "a modifier key"), (GROUP_TABS + 2, "レイヤー")] {
        assert!(!enabled(editor, id), "{what} not pressed for a flick");
    }
    assert!(shown(editor, FLICK_NOTE) && text(editor, FLICK_NOTE).contains("使えません"), "why: {:?}", text(editor, FLICK_NOTE));
    // A way with nothing, assigned: its line comes, after the key's others.
    choose_way(editor, -1, 0, "左");
    press_key(editor, "B");
    let saved = save_until(&app, editor, "| B");
    assert_eq!(flick_lines(&saved, "pad"), [vec!["up", "全選択", "shortcut", "Ctrl+V"], vec!["left", "左", "shortcut", "B"]]);
    assert_eq!(key_lines(&saved, "pad").len(), 2, "no key added: {saved}");
    // 削除: up's line goes, and up has nothing.
    choose_way(editor, 0, -1, "全選択");
    click_id(editor, PIE_REMOVE);
    assert!(within_3s(|| text(editor, LABEL) == "上"), "up empty now: {:?}", text(editor, LABEL));
    save(editor);
    assert!(within_3s(|| flick_lines(&file(&app), "pad") == [vec!["left", "左", "shortcut", "B"]]), "{}", file(&app));
}

/// The labels of set `name` in `saved`, in the order written: a key's, and
/// a flick's after its way.
fn labels_in_order(saved: &str, name: &str) -> Vec<String> {
    section(saved, name)
        .lines()
        .filter_map(|l| {
            let (setting, value) = l.split_once('=')?;
            let fields: Vec<&str> = value.split('|').map(str::trim).collect();
            match setting.trim() {
                "key" => fields.first().map(|f| f.to_string()),
                "flick" => fields.get(1).map(|f| f.to_string()),
                _ => None,
            }
        })
        .collect()
}

// Issue #161, AC3
#[test]
fn a_keys_flicks_go_with_it_moved_removed_or_made_a_kind_that_cannot_flick() {
    let _desktop = desktop();
    let written = "[set.pad]\n\
key = A | shortcut | A\n\
flick = up | a上 | shortcut | 1\n\
key = B | shortcut | B\n\
flick = down | b下 | shortcut | 2\n\
key = C | shortcut | C\n\
flick = left | c左 | shortcut | 3\n\
\n\
[profile.default]\n\
left = pad\n";
    let app = launch_with(Some(written), None);
    let editor = open_set(&app, "pad");
    // A moved down: its flick with it.
    choose_key(editor, 0, "A");
    click_id(editor, KEY_DOWN);
    save(editor);
    let want = ["B", "b下", "A", "a上", "C", "c左"];
    assert!(within_3s(|| labels_in_order(&file(&app), "pad") == want), "{}", file(&app));
    // A removed: its flick too.
    choose_key(editor, 1, "A");
    click_id(editor, REMOVE_KEY);
    save(editor);
    let want = ["B", "b下", "C", "c左"];
    assert!(within_3s(|| labels_in_order(&file(&app), "pad") == want), "{}", file(&app));
    // C made a modifier: its flick goes, and it is said.
    to_page(editor, MAP_PAGE);
    choose_key(editor, 1, "C");
    click_id(editor, button_named(editor, MOD_KEYS, 4, "Shift"));
    assert!(within_3s(|| status(editor).contains("フリック")), "said: {:?}", status(editor));
    save(editor);
    assert!(within_3s(|| labels_in_order(&file(&app), "pad") == ["B", "b下", "C"]), "{}", file(&app));
    assert_eq!(key_of(&file(&app), "pad", "C"), ["C", "modifier", "Shift"]);
}

// Issue #161, AC5: malformed input; a flick line comes from outside.
#[test]
fn flick_lines_that_do_not_read_leave_their_ways_empty_on_the_cross() {
    let _desktop = desktop();
    let written = "[set.pad]\n\
key = コピー | shortcut | Ctrl+C\n\
flick = upward | x | shortcut | A\n\
flick = left\n\
flick = | | |\n\
flick = down | 下へ | shortcut | B\n\
\n\
[profile.default]\n\
left = pad\n";
    let app = launch_with(Some(written), None);
    let editor = open_map(&app, "pad");
    open_cross(editor, 0, "コピー");
    choose_way(editor, 0, 1, "下へ");
    choose_way(editor, 0, -1, "上");
    choose_way(editor, -1, 0, "左");
    choose_way(editor, 1, 0, "右");
    assert!(unsafe { IsWindow(editor) } != 0, "the window is still open");
}

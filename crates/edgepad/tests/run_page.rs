//! Acceptance tests for run and page keys (roadmap v0.3, spec 4.2 and 4.4):
//! a run key starts its program or URL on release; page keys turn the pages
//! of a key set too long for its edge, or switch the panel to another set.
//!
//! The test build records what a run key would start in the test events
//! (`run: <program> | <arguments>`) instead of starting it.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use common::*;

const fn d(vk: u16) -> (u16, bool) {
    (vk, false)
}

const fn u(vk: u16) -> (u16, bool) {
    (vk, true)
}

fn tap(touch: &mut Touch, p: POINT) {
    touch.down(1, p);
    touch.up(1);
}

// AC1
#[test]
fn a_run_key_starts_its_program_or_url_with_its_arguments_on_release() {
    let _desktop = desktop();
    let text = "\
[set.pad]
key = 電卓 | run | calc.exe
key = 資料 | run | \"C:\\Program Files\\App\\app.exe\" -a b
key = URL  | run | https://example.com/
[profile.default]
left = pad
";
    let app = launch_with(Some(text), None);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let runs = |app: &Running| test_events(app).into_iter().filter(|e| e.starts_with("run: ")).collect::<Vec<_>>();

    touch.down(1, key_centre(panel, 0));
    touch.hold(200);
    assert_eq!(runs(&app), Vec::<String>::new(), "nothing while it is held");
    touch.up(1);
    tap(&mut touch, key_centre(panel, 1));
    tap(&mut touch, key_centre(panel, 2));
    assert_eq!(
        runs(&app),
        ["run: calc.exe | ", "run: C:\\Program Files\\App\\app.exe | -a b", "run: https://example.com/ | "]
    );
    assert_eq!(log.keys(), [], "a run key sends no keys");
}

/// Key size for the page tests, in logical pixels: big, so few keys fit.
const PAGE_KEY: i32 = 150;

/// How many keys of PAGE_KEY fit along the left edge.
fn fit() -> i32 {
    let m = primary_monitor();
    (m.bottom - m.top) / px(PAGE_KEY)
}

/// A set of Ctrl, the page keys, and shortcut letters from A on, enough to
/// fill two pages and put one key on a third. Returns the settings and how
/// many keys besides the page keys go on a full page.
fn paged(extra: &str) -> (String, i32) {
    let per = fit() - 2;
    // Page 1 needs a letter after Ctrl.
    assert!(per >= 2, "the screen is too short for the page tests");
    let mut text = format!("[general]\nkey_size = {PAGE_KEY}\n[set.pad]\nkey = Ctrl | modifier | Ctrl\nkey = 次 | page | next\nkey = 前 | page | prev\n");
    for c in ('A'..).take(2 * per as usize) {
        text += &format!("key = {c} | shortcut | {c}\n");
    }
    text += extra;
    text += "[profile.default]\nleft = pad\n";
    (text, per)
}

/// Row `row` of the left panel as it is now.
fn row(app: &Running, row: i32) -> POINT {
    cell(left_panel(app), px(PAGE_KEY), 0, row)
}

fn letter(i: i32) -> u16 {
    b'A' as u16 + i as u16
}

// AC2
#[test]
fn page_keys_turn_through_the_keys_that_do_not_fit_and_wrap_around() {
    let _desktop = desktop();
    let (text, per) = paged("");
    let app = launch_with(Some(&text), None);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let (next, prev) = (per, per + 1);
    let sent = |log: &KeyLog, before: usize| log.keys()[before..].to_vec();

    // Page 1: Ctrl, then A.. up to the page keys.
    let before = log.keys().len();
    tap(&mut touch, row(&app, 1));
    assert_eq!(sent(&log, before), [d(letter(0)), u(letter(0))], "page 1 starts with Ctrl, then A");
    assert_eq!(left_panel(&app).bottom - left_panel(&app).top, fit() * px(PAGE_KEY), "a full page");

    // Page 2 carries on where page 1 stopped.
    tap(&mut touch, row(&app, next));
    let before = log.keys().len();
    tap(&mut touch, row(&app, 0));
    assert_eq!(sent(&log, before), [d(letter(per - 1)), u(letter(per - 1))], "page 2 goes on from page 1");

    // Page 3 holds the last letter and the page keys only.
    tap(&mut touch, row(&app, next));
    assert_eq!(left_panel(&app).bottom - left_panel(&app).top, 3 * px(PAGE_KEY), "the last page is shorter");
    let before = log.keys().len();
    tap(&mut touch, row(&app, 0));
    let last = letter(2 * per - 1);
    assert_eq!(sent(&log, before), [d(last), u(last)], "the last letter");

    // Past the last page is the first, and before the first the last.
    tap(&mut touch, row(&app, 1));
    let before = log.keys().len();
    tap(&mut touch, row(&app, 1));
    assert_eq!(sent(&log, before), [d(letter(0)), u(letter(0))], "back on page 1");
    tap(&mut touch, row(&app, prev));
    let before = log.keys().len();
    tap(&mut touch, row(&app, 0));
    assert_eq!(sent(&log, before), [d(last), u(last)], "prev on page 1 goes to the last page");
}

// AC2
#[test]
fn a_latch_stays_through_a_page_turn_and_combines_on_the_next_page() {
    let _desktop = desktop();
    let (text, per) = paged("");
    let app = launch_with(Some(&text), None);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, row(&app, 0));
    tap(&mut touch, row(&app, per));
    tap(&mut touch, row(&app, 0));
    let l = letter(per - 1);
    assert_eq!(log.keys(), [d(VK_LCONTROL), d(l), u(l), u(VK_LCONTROL)], "Ctrl+letter across the turn");
}

// AC3
#[test]
fn a_page_key_naming_a_set_switches_the_panel_to_it_letting_go_of_its_keys() {
    let _desktop = desktop();
    let text = "\
[set.pad]
key = Ctrl | modifier | Ctrl
key = 別   | page     | other
[set.other]
key = B    | shortcut | B
key = C    | shortcut | C
key = 戻る | page     | pad
[profile.default]
left = pad
";
    let app = launch_with(Some(text), None);
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    tap(&mut touch, key_centre(panel, 0));
    tap(&mut touch, key_centre(panel, 1));
    assert_eq!(log.keys(), [d(VK_LCONTROL), u(VK_LCONTROL)], "switching lets go of the latched Ctrl");
    let other = left_panel(&app);
    assert_eq!(other.bottom - other.top, 3 * key_px(), "the other set's three keys");

    tap(&mut touch, key_centre(other, 1));
    assert_eq!(log.keys()[2..], [d(b'C' as u16), u(b'C' as u16)], "the other set's keys work");
    tap(&mut touch, key_centre(other, 2));
    let back: RECT = left_panel(&app);
    assert_eq!(back.bottom - back.top, 2 * key_px(), "back to the first set");
}

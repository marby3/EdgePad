//! Acceptance tests for Issue #14: EdgePad reads its settings file at start
//! and lays out the left panel from the default profile.
//!
//! Each test runs a copy of the exe from a scratch folder, so the settings
//! file beside it and the one under %APPDATA% are both under the test's control.

mod common;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_LSHIFT;
use common::*;

/// A settings file whose left edge has `n` hold keys (A, B, C, ...).
fn keys_on_the_left(n: usize) -> String {
    let mut text = String::from("[set.left]\n");
    for c in (b'A'..).take(n) {
        let c = c as char;
        text += &format!("key = {c} | hold | {c}\n");
    }
    text + "[profile.default]\nleft = left\n"
}

/// How many keys tall the left panel of `app` is.
fn panel_keys(app: &Running) -> i32 {
    let r = left_panel(app);
    assert_eq!((r.bottom - r.top) % key_px(), 0, "panel height is not a whole number of keys");
    (r.bottom - r.top) / key_px()
}

// AC3
#[test]
fn the_settings_file_beside_the_exe_wins_over_the_one_in_appdata() {
    let _desktop = desktop();
    let (beside, appdata) = (keys_on_the_left(3), keys_on_the_left(2));

    let app = launch_with(Some(&beside), Some(&appdata));
    assert_eq!(panel_keys(&app), 3, "the file beside the exe is read first");
    drop(app);

    let app = launch_with(None, Some(&appdata));
    assert_eq!(panel_keys(&app), 2, "without one beside the exe, %APPDATA%\\EdgePad\\edgepad.ini is read");
}

// AC4
#[test]
fn the_left_panel_holds_the_default_profiles_left_keys_and_only_modifier_and_hold_send() {
    let _desktop = desktop();
    let text = "\
[set.pad]
key = A     | hold     | A
key = Shift | modifier | Shift
key = 縮小  | repeat   | [
key = 挨拶  | text     | hello
[set.other]
key = B | hold | B
[profile.default]
left  = pad
right = other
";
    let app = launch_with(Some(text), None);
    assert_eq!(panel_keys(&app), 4, "one key per line of the left set, in one column");
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, key_centre(panel, 0));
    touch.hold(200);
    assert_eq!(log.keys(), [(b'A' as u16, false)], "hold key A goes down while touched");
    touch.up(1);
    assert_eq!(log.keys(), [(b'A' as u16, false), (b'A' as u16, true)]);

    touch.down(1, key_centre(panel, 1));
    touch.hold(200);
    assert_eq!(log.keys()[2..], [(VK_LSHIFT, false)], "modifier Shift goes down while touched");
    touch.up(1);
    assert_eq!(log.keys()[2..], [(VK_LSHIFT, false), (VK_LSHIFT, true)]);

    for i in [2, 3] {
        touch.down(1, key_centre(panel, i));
        touch.hold(200);
        touch.up(1);
    }
    assert_eq!(log.keys().len(), 4, "repeat and text keys send nothing yet (v0.3): {:?}", log.keys());
}

// AC5
#[test]
fn without_a_usable_settings_file_the_fixed_five_keys_come_up() {
    let _desktop = desktop();

    let app = launch_with(None, None);
    assert_eq!(panel_keys(&app), KEY_COUNT, "no settings file at all");
    drop(app);

    let broken = "[set.left]\nkey = A | hold | NoSuchKey\n[profile.default]\nleft = left\n";
    let app = launch_with(Some(broken), None);
    assert_eq!(panel_keys(&app), KEY_COUNT, "a settings file with an error");
    let panel = left_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, ESC));
    touch.up(1);
    assert_eq!(log.keys(), [down(ESC), up(ESC)], "the fixed keys work as in v0.1");
}

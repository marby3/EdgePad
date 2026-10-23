//! Acceptance tests for Issue #66: a pie key takes its place on the panel
//! like any key, and sends nothing until the pie menu comes (spec 4.6).

mod common;

use common::*;

// AC4
#[test]
fn a_pie_key_is_on_the_panel_and_sends_nothing_when_pressed() {
    let _desktop = desktop();
    let text = "\
[set.pad]
key = 変形 | pie      | menu
key = A    | shortcut | A
[set.menu]
key = 拡大縮小 | shortcut | Ctrl+T
key = 確定     | shortcut | Enter
[profile.default]
left = pad
";
    let app = launch_with(Some(text), None);
    let panel = left_panel(&app);
    assert_eq!(panel.bottom - panel.top, 2 * key_px(), "the pie key and A are on the panel");
    let log = KeyLog::start();
    let mut touch = Touch::new();

    touch.down(1, key_centre(panel, 0));
    touch.hold(200);
    touch.up(1);
    touch.down(1, key_centre(panel, 0));
    touch.up(1);
    assert_eq!(log.keys(), [], "held or tapped, a pie key sends nothing");

    touch.down(1, key_centre(panel, 1));
    touch.up(1);
    log.wait_for(2);
    assert_eq!(log.keys(), [(b'A' as u16, false), (b'A' as u16, true)], "the key beside it still works");
}

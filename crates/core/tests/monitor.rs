//! Acceptance test for Issue #20, criterion 1: which monitor an edge's panel
//! goes on. Numbers are Windows's own since Issue #149.

use edgepad_core::config::Monitor;
use edgepad_core::layout::{Rect, pick_monitor};
use edgepad_core::monitor::{Display, choices, display_number};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn display(number: u8, name: &str, internal: bool) -> Display {
    Display { number, name: name.into(), internal }
}

fn labels(list: &[(Monitor, String)]) -> Vec<&str> {
    list.iter().map(|(_, label)| label.as_str()).collect()
}

// Issue #149, AC1
#[test]
fn the_monitor_setting_picks_primary_windows_numbered_or_under_the_cursor() {
    // In the order Windows might list them: a primary in the middle, one to
    // its right, one to its left, and one stacked above the left one.
    let monitors = [
        rect(0, 0, 1920, 1080),         // 0: primary
        rect(1920, 0, 2560, 1440),      // 1: right
        rect(-1280, 0, 1280, 1024),     // 2: left, lower
        rect(-1280, -1024, 1280, 1024), // 3: left, upper
    ];
    // Their GDI device names, \\.\DISPLAY2 and so on: not their places.
    let numbers = [2, 4, 1, 3];
    let primary = 0;
    let nowhere = (100, 100);

    assert_eq!(display_number(r"\\.\DISPLAY2"), Some(2));
    assert_eq!(display_number(r"\\.\DISPLAY12"), Some(12));
    assert_eq!(display_number(r"\\.\DISPLAY"), None);
    assert_eq!(display_number("WinDisc"), None);

    assert_eq!(pick_monitor(&monitors, &numbers, primary, nowhere, Monitor::Primary), 0);

    let numbered: Vec<usize> =
        (1..=4).map(|n| pick_monitor(&monitors, &numbers, primary, nowhere, Monitor::Number(n))).collect();
    assert_eq!(numbered, [2, 0, 3, 1], "Windows's numbers, not left to right");

    assert_eq!(pick_monitor(&monitors, &numbers, primary, nowhere, Monitor::Number(5)), 0, "no monitor 5: primary");

    assert_eq!(pick_monitor(&monitors, &numbers, primary, (2000, 700), Monitor::Cursor), 1, "cursor on the right monitor");
    assert_eq!(pick_monitor(&monitors, &numbers, primary, (-5, -5), Monitor::Cursor), 3, "cursor on the upper left one");
    assert_eq!(pick_monitor(&monitors, &numbers, primary, (100, 5000), Monitor::Cursor), 0, "cursor off every monitor: primary");
}

// Issue #149, AC2
#[test]
fn the_list_names_the_connected_monitors_by_number() {
    let displays = [display(3, "", false), display(2, "34R83Q", false), display(1, "", true)];
    let list = choices(&displays, 2, Monitor::Primary);
    assert_eq!(
        labels(&list)[1..],
        ["マウスのあるモニター", "モニター 1（内蔵ディスプレイ）", "モニター 2（34R83Q）", "モニター 3"]
    );
    let values: Vec<Monitor> = list.iter().map(|&(m, _)| m).collect();
    assert_eq!(values, [Monitor::Primary, Monitor::Cursor, Monitor::Number(1), Monitor::Number(2), Monitor::Number(3)]);

    // A gap in the numbers stays a gap.
    let list = choices(&[display(1, "", true), display(3, "DELL U2720Q", false)], 1, Monitor::Primary);
    assert_eq!(labels(&list)[2..], ["モニター 1（内蔵ディスプレイ）", "モニター 3（DELL U2720Q）"]);
}

// Issue #149, AC3
#[test]
fn the_primary_item_says_which_monitor_is_primary_now() {
    let displays = [display(1, "", true), display(2, "34R83Q", false)];
    assert_eq!(choices(&displays, 2, Monitor::Primary)[0], (Monitor::Primary, "主モニター（今はモニター 2）".to_string()));
    assert_eq!(choices(&displays, 1, Monitor::Primary)[0].1, "主モニター（今はモニター 1）");
}

// Issue #149, AC4
#[test]
fn a_number_in_the_file_with_no_monitor_stays_in_the_list() {
    let displays = [display(1, "", true), display(4, "34R83Q", false)];
    let list = choices(&displays, 1, Monitor::Number(3));
    assert_eq!(
        labels(&list)[2..],
        ["モニター 1（内蔵ディスプレイ）", "モニター 3（つながっていません）", "モニター 4（34R83Q）"]
    );
    assert_eq!(list[3].0, Monitor::Number(3), "chosen as it was, written back as 3");
    assert_eq!(choices(&displays, 1, Monitor::Number(4)).len(), 4, "a connected number is not listed twice");
}

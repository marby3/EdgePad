//! Acceptance test for Issue #20, criterion 1: which monitor an edge's panel
//! goes on.

use edgepad_core::config::Monitor;
use edgepad_core::layout::{Rect, pick_monitor};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

// AC1
#[test]
fn the_monitor_setting_picks_primary_numbered_by_position_or_under_the_cursor() {
    // In the order Windows might list them, which is not the screen order:
    // a primary in the middle, one to its right, one to its left, and one
    // stacked above the left one (same left edge, smaller top).
    let monitors = [
        rect(0, 0, 1920, 1080),        // 0: primary
        rect(1920, 0, 2560, 1440),     // 1: right
        rect(-1280, 0, 1280, 1024),    // 2: left, lower
        rect(-1280, -1024, 1280, 1024), // 3: left, upper
    ];
    let primary = 0;
    let nowhere = (100, 100);

    assert_eq!(pick_monitor(&monitors, primary, nowhere, Monitor::Primary), 0);

    // Numbered left to right, and top to bottom where the left edges match.
    let numbered: Vec<usize> =
        (1..=4).map(|n| pick_monitor(&monitors, primary, nowhere, Monitor::Number(n))).collect();
    assert_eq!(numbered, [3, 2, 0, 1]);

    assert_eq!(pick_monitor(&monitors, primary, nowhere, Monitor::Number(5)), 0, "no 5th monitor: primary");

    assert_eq!(pick_monitor(&monitors, primary, (2000, 700), Monitor::Cursor), 1, "cursor on the right monitor");
    assert_eq!(pick_monitor(&monitors, primary, (-5, -5), Monitor::Cursor), 3, "cursor on the upper left one");
    assert_eq!(pick_monitor(&monitors, primary, (100, 5000), Monitor::Cursor), 0, "cursor off every monitor: primary");
}

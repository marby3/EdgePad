//! Acceptance tests for Issue #3: the fixed five-key panel on the left edge.
//!
//! Each test launches the real exe and inspects its panel from outside, the way
//! a user sees it: window styles, position, the pixels on screen, CPU time.
//! Tests share the desktop, so they run one at a time.

mod common;

use std::time::Duration;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

// AC1
#[test]
fn panel_shows_five_fixed_keys_in_a_column_on_the_left_edge_of_the_primary_monitor() {
    let _desktop = desktop();
    assert_eq!(edgepad_core::keys::M0_LABELS, ["Esc", "Ctrl", "Shift", "Alt", "Space"]);

    let app = launch();
    let r = window_rect(find_panel(&app));
    let monitor = primary_monitor();
    let key = key_px();
    let height = key * KEY_COUNT;
    assert_eq!(r.left, monitor.left, "flush with the left edge");
    assert_eq!(r.right - r.left, key, "one key wide");
    assert_eq!(r.bottom - r.top, height, "five keys tall");
    let centred = monitor.top + (monitor.bottom - monitor.top - height) / 2;
    assert!((r.top - centred).abs() <= 1, "vertically centred: top {} vs {centred}", r.top);
}

// AC2
#[test]
fn panel_window_never_activates_stays_on_top_and_stays_off_taskbar_and_alt_tab() {
    let _desktop = desktop();
    let app = launch();
    let panel = find_panel(&app);
    let ex = unsafe { GetWindowLongPtrW(panel, GWL_EXSTYLE) } as u32;
    for (flag, name) in [
        (WS_EX_NOACTIVATE, "WS_EX_NOACTIVATE"),
        (WS_EX_TOPMOST, "WS_EX_TOPMOST"),
        (WS_EX_TOOLWINDOW, "WS_EX_TOOLWINDOW"),
        (WS_EX_LAYERED, "WS_EX_LAYERED"),
    ] {
        assert!(ex & flag != 0, "{name} missing");
    }
    assert!(ex & WS_EX_APPWINDOW == 0, "WS_EX_APPWINDOW would put it on the taskbar");
}

// AC3
#[test]
fn clicking_the_panel_keeps_focus_on_the_previous_foreground_window() {
    let _desktop = desktop();
    let app = launch();
    let panel = window_rect(find_panel(&app));

    let monitor = primary_monitor();
    let area = RECT {
        left: monitor.left + 300,
        top: monitor.top + 100,
        right: monitor.left + 700,
        bottom: monitor.top + 400,
    };
    let foreground = test_window("EdgePadTestForeground", 0, area);
    force_foreground(foreground.0);
    assert_eq!(unsafe { GetForegroundWindow() }, foreground.0, "precondition: test window in front");

    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    click(POINT { x: (panel.left + panel.right) / 2, y: (panel.top + panel.bottom) / 2 });
    pump(300);
    unsafe { SetCursorPos(saved.x, saved.y) };

    assert_eq!(unsafe { GetForegroundWindow() }, foreground.0, "focus moved off the test window");
}

// AC4
#[test]
fn panel_is_drawn_with_update_layered_window_at_90_percent_with_a_label_on_every_key() {
    let _desktop = desktop();
    // A topmost backdrop made before the panel sits right under it.
    let monitor = primary_monitor();
    let key = key_px();
    let under = RECT {
        left: monitor.left,
        top: monitor.top,
        right: monitor.left + key * 2,
        bottom: monitor.bottom,
    };
    let backdrop = test_window("EdgePadTestBackdrop", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, under);

    let app = launch();
    let hwnd = find_panel(&app);
    let panel = window_rect(hwnd);

    let (mut key_color, mut alpha, mut flags) = (0, 0, 0);
    let attrs = unsafe { GetLayeredWindowAttributes(hwnd, &mut key_color, &mut alpha, &mut flags) };
    assert_eq!(attrs, 0, "GetLayeredWindowAttributes works only without UpdateLayeredWindow");

    // 90% opaque: 10% of the backdrop shows through, so white vs black differs by ~25.
    // Averaged over a 3x3 patch of panel background in the top-left corner.
    let corner = RECT { left: panel.left, top: panel.top, right: panel.left + 3, bottom: panel.top + 3 };
    let average = |pixels: Vec<u32>| {
        let n = pixels.len() as i32;
        let sum = pixels.iter().map(|&p| channels(p)).fold([0; 3], |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2]]);
        sum.map(|v| v / n)
    };
    set_backdrop(backdrop.0, 0x00FF_FFFF);
    let on_white = average(capture(corner));
    set_backdrop(backdrop.0, 0x0000_0000);
    let on_black = average(capture(corner));
    for c in 0..3 {
        let diff = on_white[c] - on_black[c];
        assert!((20..=31).contains(&diff), "backdrop shows through by {diff}/255, want ~25");
    }

    // Each key's middle holds label strokes that stand out from the key face.
    let size = panel.right - panel.left;
    for i in 0..KEY_COUNT {
        let top = panel.top + i * size;
        let middle = RECT {
            left: panel.left + size / 5,
            top: top + size / 5,
            right: panel.right - size / 5,
            bottom: top + size - size / 5,
        };
        let pixels = capture(middle);
        let face = channels(pixels[0]);
        let ink = pixels
            .iter()
            .filter(|&&p| channels(p).iter().zip(face).any(|(a, b)| (a - b).abs() > 60))
            .count();
        assert!(ink >= 20, "key {i} shows no label ({ink} ink pixels)");
    }
}

// AC5
#[test]
fn idle_panel_uses_under_10ms_cpu_in_2_seconds() {
    let _desktop = desktop();
    let app = launch();
    find_panel(&app);
    let before = cpu_time_100ns(&app);
    std::thread::sleep(Duration::from_secs(2));
    let used_ms = (cpu_time_100ns(&app) - before) as f64 / 10_000.0;
    assert!(used_ms < 10.0, "idle CPU {used_ms:.1}ms in 2s");
}

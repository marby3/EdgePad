//! Acceptance tests for Issue #56: light and dark themes, `theme = auto`
//! following Windows, and the accent colour.
//!
//! Colours are read off the screen with the panel fully opaque. Following
//! Windows is checked by writing the user's own settings in the registry and
//! announcing the change as Windows does (WM_SETTINGCHANGE,
//! "ImmersiveColorSet"); each value is put back afterwards, even when the
//! test fails.

mod common;

use std::ptr::null_mut;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const DWM: &str = r"Software\Microsoft\Windows\DWM";

/// A DWORD under HKEY_CURRENT_USER, set for the test and put back on drop.
struct Setting {
    key: Vec<u16>,
    name: Vec<u16>,
    was: Option<u32>,
}

fn read_dword(key: &[u16], name: &[u16]) -> Option<u32> {
    let (mut value, mut size) = (0u32, 4u32);
    let found = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, null_mut(), &mut value as *mut u32 as _, &mut size)
    };
    (found == 0).then_some(value)
}

fn write_dword(key: &[u16], name: &[u16], value: u32) {
    let done = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, &value as *const u32 as _, 4)
    };
    assert_eq!(done, 0, "write the registry value");
}

/// Tell every top-level window the colour settings changed.
fn announce() {
    let what = wide("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, what.as_ptr() as isize, SMTO_ABORTIFHUNG, 2000, null_mut());
    }
    pump(300);
}

impl Setting {
    fn set(key: &str, name: &str, value: u32) -> Setting {
        let (key, name) = (wide(key), wide(name));
        let was = read_dword(&key, &name);
        write_dword(&key, &name, value);
        announce();
        Setting { key, name, was }
    }

    fn change(&self, value: u32) {
        write_dword(&self.key, &self.name, value);
        announce();
    }
}

impl Drop for Setting {
    fn drop(&mut self) {
        match self.was {
            Some(v) => write_dword(&self.key, &self.name, v),
            None => unsafe {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, self.key.as_ptr(), self.name.as_ptr());
            },
        }
        announce();
    }
}

/// Three keys down the left, fully opaque, under `general`.
fn launch_pad(general: &str) -> (Running, RECT) {
    let text = format!(
        "[general]\nopacity = 100\n{general}\n[set.pad]\nkey = A | hold | A\nkey = Ctrl | modifier | Ctrl\nkey = B | hold | B\n[profile.default]\nleft = pad\n"
    );
    let app = launch_with(Some(&text), None);
    let panel = left_panel(&app);
    (app, panel)
}

fn brightness(c: [i32; 3]) -> i32 {
    (c[0] + c[1] + c[2]) / 3
}

/// The panel's own background, in the gap at its top-left corner.
fn background(panel: RECT) -> [i32; 3] {
    channels(capture(RECT { left: panel.left, top: panel.top, right: panel.left + 1, bottom: panel.top + 1 })[0])
}

/// A spot of key `i`'s face, clear of its label.
fn face(panel: RECT, i: usize) -> [i32; 3] {
    let size = panel.right - panel.left;
    let top = panel.top + size * i as i32;
    let (x, y) = (panel.left + size / 4, top + size / 6);
    channels(capture(RECT { left: x, top: y, right: x + 1, bottom: y + 1 })[0])
}

/// Brightness of the label strokes of key `i`, against its face: the
/// average of the pixels that stand out from it.
fn ink(panel: RECT, i: usize) -> i32 {
    let size = panel.right - panel.left;
    let top = panel.top + size * i as i32;
    let middle = RECT { left: panel.left + size / 5, top: top + size / 5, right: panel.right - size / 5, bottom: top + size - size / 5 };
    let face = brightness(face(panel, i));
    let strokes: Vec<i32> =
        capture(middle).iter().map(|&p| brightness(channels(p))).filter(|b| (b - face).abs() > 60).collect();
    assert!(strokes.len() >= 20, "key {i} shows no label ({} ink pixels)", strokes.len());
    strokes.iter().sum::<i32>() / strokes.len() as i32
}

/// Whether the panel is drawn dark: dark background and faces, light label.
fn is_dark(panel: RECT) -> bool {
    brightness(background(panel)) < 80 && brightness(face(panel, 0)) < 110 && ink(panel, 0) > 170
}

/// Whether the panel is drawn light: light background and faces, dark label.
fn is_light(panel: RECT) -> bool {
    brightness(background(panel)) > 170 && brightness(face(panel, 0)) > 150 && ink(panel, 0) < 90
}

/// Wait up to a second for `test` to hold of the left panel.
fn becomes(app: &Running, test: impl Fn(RECT) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        if test(left_panel(app)) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        pump(50);
    }
}

/// Whether `c` is close to `want`, channel by channel.
fn near(c: [i32; 3], want: [i32; 3]) -> bool {
    c.iter().zip(want).all(|(a, b)| (a - b).abs() <= 12)
}

/// The colour of the left handle over a black backdrop.
fn handle_colour(app: &Running) -> [i32; 3] {
    let m = primary_monitor();
    let name = wide("EdgePadHandle");
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        let mut hwnd = null_mut();
        loop {
            hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), std::ptr::null()) };
            if hwnd.is_null() {
                break;
            }
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            let r = window_rect(hwnd);
            if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 && r.left == m.left {
                let (x, y) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
                return channels(capture(RECT { left: x, top: y, right: x + 1, bottom: y + 1 })[0]);
            }
        }
        assert!(Instant::now() < end, "no handle on the left edge");
        pump(20);
    }
}

/// A black topmost backdrop down the left, made before EdgePad starts so its
/// windows go above it.
fn black_backdrop() -> TestWindow {
    let m = primary_monitor();
    let under = RECT { left: m.left, top: m.top, right: m.left + key_px() * 2, bottom: m.bottom };
    let backdrop = test_window("EdgePadTestThemeBackdrop", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, under);
    set_backdrop(backdrop.0, 0);
    backdrop
}

/// What the handle shows of `accent` over black: 30% of it.
fn on_black(accent: [i32; 3]) -> [i32; 3] {
    accent.map(|c| c * 77 / 255)
}

// AC1
#[test]
fn theme_dark_and_light_draw_the_panel_dark_and_light() {
    let _desktop = desktop();
    let (_app, panel) = launch_pad("theme = dark");
    assert!(is_dark(panel), "dark: background {:?}, face {:?}, ink {}", background(panel), face(panel, 0), ink(panel, 0));
    drop(_app);

    let (_app, panel) = launch_pad("theme = light");
    assert!(is_light(panel), "light: background {:?}, face {:?}, ink {}", background(panel), face(panel, 0), ink(panel, 0));
}

// AC2
#[test]
fn theme_auto_follows_the_windows_app_mode_while_running() {
    let _desktop = desktop();
    let mode = Setting::set(PERSONALIZE, "AppsUseLightTheme", 0);
    let (app, panel) = launch_pad("theme = auto");
    assert!(is_dark(panel), "apps dark: the panel is dark, background {:?}", background(panel));

    mode.change(1);
    assert!(becomes(&app, is_light), "apps turned light: the panel follows, background {:?}", background(left_panel(&app)));

    mode.change(0);
    assert!(becomes(&app, is_dark), "and back to dark, background {:?}", background(left_panel(&app)));
}

// AC3
#[test]
fn the_accent_colours_held_and_latched_keys_and_the_handle_and_auto_follows_windows() {
    let _desktop = desktop();
    let _backdrop = black_backdrop();
    let red = [0xE0, 0x20, 0x30];

    // Set in the file.
    let (app, panel) = launch_pad("accent = #E02030");
    let _log = KeyLog::start();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, 0));
    touch.hold(100);
    let held = face(panel, 0);
    touch.up(1);
    assert!(near(held, red), "a held key is the accent, #E02030, not {held:?}");

    touch.down(1, key_centre(panel, 1));
    touch.up(1);
    let latched = face(panel, 1);
    assert!(
        latched[0] > latched[1] + 30 && latched[0] > latched[2] + 30 && latched[0] < red[0] - 30,
        "a latched modifier is the accent darkened, not {latched:?}"
    );
    touch.down(1, key_centre(panel, 1));
    touch.up(1);

    choose(&app, "左辺");
    let handle = handle_colour(&app);
    assert!(near(handle, on_black(red)), "the handle is the accent at 30%, not {handle:?}");
    drop(touch);
    drop(app);

    // Windows' accent, followed while running. AccentColor is 0xAABBGGRR.
    let accent = Setting::set(DWM, "AccentColor", 0xFF20_C040);
    let green = [0x40, 0xC0, 0x20];
    let (app, panel) = launch_pad("accent = auto");
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, 0));
    touch.hold(100);
    let held = face(panel, 0);
    touch.up(1);
    assert!(near(held, green), "auto: a held key is Windows' accent, #40C020, not {held:?}");

    choose(&app, "左辺");
    let handle = handle_colour(&app);
    assert!(near(handle, on_black(green)), "and so is the handle, not {handle:?}");

    accent.change(0xFFD0_3010);
    let blue = [0x10, 0x30, 0xD0];
    let end = Instant::now() + Duration::from_secs(1);
    while !near(handle_colour(&app), on_black(blue)) && Instant::now() < end {
        pump(50);
    }
    let handle = handle_colour(&app);
    assert!(near(handle, on_black(blue)), "Windows' accent changed: the handle follows, not {handle:?}");

    choose(&app, "左辺");
    let panel = left_panel(&app);
    touch.down(1, key_centre(panel, 0));
    touch.hold(100);
    let held = face(panel, 0);
    touch.up(1);
    assert!(near(held, blue), "and so do held keys, not {held:?}");
}

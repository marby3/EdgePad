//! Windows' own colour settings, for `theme = auto` and `accent = auto`.
//!
//! Both are read from the user's registry, where the Settings app keeps
//! them; it announces a change with WM_SETTINGCHANGE ("ImmersiveColorSet").

use std::ptr::null_mut;

use edgepad_core::config::{Accent, Theme};
use edgepad_core::theme::{self, Palette, Rgb};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::w;

/// The accent when Windows gives none: its own default blue.
const DEFAULT_ACCENT: Rgb = [0x00, 0x78, 0xD7];

fn dword(key: *const u16, name: *const u16) -> Option<u32> {
    let (mut value, mut size) = (0u32, 4u32);
    let found = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key, name, RRF_RT_REG_DWORD, null_mut(), &mut value as *mut u32 as _, &mut size)
    };
    (found == 0).then_some(value)
}

/// Whether apps are to be dark ("既定のアプリ モード"). Light when unset,
/// as in Windows.
fn apps_dark() -> bool {
    dword(w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"), w!("AppsUseLightTheme")) == Some(0)
}

/// Windows' accent colour, kept as 0xAABBGGRR.
fn windows_accent() -> Rgb {
    dword(w!(r"Software\Microsoft\Windows\DWM"), w!("AccentColor"))
        .map_or(DEFAULT_ACCENT, |c| [c as u8, (c >> 8) as u8, (c >> 16) as u8])
}

/// The palette `theme` and `accent` come to now.
pub fn palette(theme: Theme, accent: Accent) -> Palette {
    let dark = match theme {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::Auto => apps_dark(),
    };
    let accent = match accent {
        Accent::Rgb(r, g, b) => [r, g, b],
        Accent::Auto => windows_accent(),
    };
    theme::palette(dark, accent)
}

/// `c` as a GDI COLORREF, 0x00BBGGRR.
pub fn colorref([r, g, b]: Rgb) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

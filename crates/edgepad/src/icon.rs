//! EdgePad's icon, icon group 1 of the exe (build.rs), at the size a DPI
//! asks for. Read with LoadImageW, not comctl32's LoadIconMetric: the
//! running EdgePad does without comctl32 (spec 7.1).

use std::ptr::null;

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// The icon at Windows's small icon size for `dpi` (tray, title bar), or
/// its large one (taskbar, Alt+Tab). Null if it cannot be read. The caller
/// destroys it.
pub fn load(dpi: u32, small: bool) -> HICON {
    unsafe {
        let size = GetSystemMetricsForDpi(if small { SM_CXSMICON } else { SM_CXICON }, dpi);
        LoadImageW(GetModuleHandleW(null()), 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
    }
}

/// Give window `hwnd` the icon at both sizes for `dpi`: its title bar's,
/// and the taskbar's and Alt+Tab's. The icons it had are destroyed.
pub unsafe fn set_on(hwnd: HWND, dpi: u32) {
    for (kind, small) in [(ICON_SMALL, true), (ICON_BIG, false)] {
        unsafe {
            let old = SendMessageW(hwnd, WM_SETICON, kind as usize, load(dpi, small) as LPARAM) as HICON;
            if !old.is_null() {
                DestroyIcon(old);
            }
        }
    }
}

//! Acceptance tests for Issue #116: EdgePad's own icon, in the exe, in the
//! tray and on the settings window. AC4 (no crate added, the exe's size and
//! memory) is CI's measuring step.
//!
//! The icon is icon group 1 of the exe: that is the contract. Icons are
//! handles every process can read, so the tray's (named by a test event)
//! and the window's are read here and set beside the exe's own, pixel by
//! pixel.

mod common;

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::FreeLibrary;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{
    FindResourceW, LOAD_LIBRARY_AS_DATAFILE, LOAD_LIBRARY_AS_IMAGE_RESOURCE, LoadLibraryExW, LoadResource, LockResource,
    SizeofResource,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;
use common::editor::*;
use common::*;

/// The exe `app` runs, loaded to read its resources only.
struct Resources(windows_sys::Win32::Foundation::HMODULE);

impl Resources {
    fn of(app: &Running) -> Resources {
        let path = wide(&exe_dir(app).join("edgepad.exe").to_string_lossy());
        let module = unsafe { LoadLibraryExW(path.as_ptr(), null_mut(), LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE) };
        assert!(!module.is_null(), "the exe loads as resources");
        Resources(module)
    }

    /// The sizes in icon group 1, as its directory lists them.
    fn sizes(&self) -> Vec<u32> {
        unsafe {
            let found = FindResourceW(self.0, 1 as _, RT_GROUP_ICON);
            assert!(!found.is_null(), "the exe has icon group 1");
            let len = SizeofResource(self.0, found) as usize;
            let data = std::slice::from_raw_parts(LockResource(LoadResource(self.0, found)) as *const u8, len);
            let count = u16::from_le_bytes([data[4], data[5]]) as usize;
            // 14 bytes an entry, after a 6-byte header; width 0 is 256.
            (0..count).map(|i| match data[6 + i * 14] {
                0 => 256,
                w => w as u32,
            }).collect()
        }
    }

    /// Icon group 1 drawn at `size`, as Windows picks it from the exe.
    fn icon(&self, size: i32) -> HICON {
        let icon = unsafe { LoadImageW(self.0, 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) };
        assert!(!icon.is_null(), "icon group 1 loads at {size}px");
        icon
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.0) };
    }
}

/// The colour pixels of `icon`: its width, its height, and BGRA rows top
/// down.
fn pixels(icon: HICON) -> (i32, i32, Vec<u8>) {
    unsafe {
        let mut info: ICONINFO = zeroed();
        assert!(GetIconInfo(icon, &mut info) != 0, "the icon {icon:?} can be read");
        let mut bitmap: BITMAP = zeroed();
        GetObjectW(info.hbmColor, size_of::<BITMAP>() as i32, &mut bitmap as *mut BITMAP as *mut _);
        let (w, h) = (bitmap.bmWidth, bitmap.bmHeight);
        let mut header: BITMAPINFO = zeroed();
        header.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut bits = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(null_mut());
        GetDIBits(dc, info.hbmColor, 0, h as u32, bits.as_mut_ptr() as *mut _, &mut header, DIB_RGB_COLORS);
        ReleaseDC(null_mut(), dc);
        DeleteObject(info.hbmColor);
        DeleteObject(info.hbmMask);
        (w, h, bits)
    }
}

/// Assert `icon` is the exe's icon at `size`, pixel for pixel.
fn assert_is_ours(icon: HICON, size: i32, exe: &Resources, what: &str) {
    assert!(!icon.is_null(), "{what}: has an icon");
    let (w, h, bits) = pixels(icon);
    assert_eq!((w, h), (size, size), "{what}: {size}px");
    let ours = exe.icon(size);
    assert!(pixels(ours).2 == bits, "{what}: EdgePad's icon, not another");
    unsafe { DestroyIcon(ours) };
}

/// The DPI of the screen the taskbar's tray is on: of the primary screen
/// when there is no taskbar.
fn tray_dpi() -> u32 {
    let taskbar = unsafe { FindWindowW(w!("Shell_TrayWnd"), null()) };
    let dpi = if taskbar.is_null() { 0 } else { unsafe { GetDpiForWindow(taskbar) } };
    if dpi != 0 { dpi } else { unsafe { GetDpiForWindow(GetDesktopWindow()) }.max(96) }
}

// AC1
#[test]
fn the_exe_has_the_icon_in_each_size() {
    let _desktop = desktop();
    let app = launch();
    let mut sizes = Resources::of(&app).sizes();
    sizes.sort();
    assert_eq!(sizes, [16, 20, 24, 32, 40, 48, 256], "the sizes in the exe's icon");
}

// AC2
#[test]
fn the_tray_shows_the_icon_at_the_small_icon_size_of_its_screen() {
    let _desktop = desktop();
    let app = launch();
    find_panel(&app);
    let exe = Resources::of(&app);
    let mut shown = None;
    assert!(
        within_3s(|| {
            shown = test_events(&app).iter().rev().find_map(|e| e.strip_prefix("tray icon: ").map(String::from));
            shown.is_some()
        }),
        "the tray's icon is named: {:?}",
        test_events(&app)
    );
    let handle = usize::from_str_radix(shown.unwrap().trim_start_matches("0x"), 16).expect("a handle");
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, tray_dpi()) };
    assert_is_ours(handle as HICON, size, &exe, "the tray");
}

// AC3
#[test]
fn the_settings_window_has_the_icon() {
    let _desktop = desktop();
    let app = launch();
    let editor = open(&app, open_id(&app));
    let exe = Resources::of(&app);
    let dpi = unsafe { GetDpiForWindow(editor) };
    for (kind, metric, what) in [(ICON_SMALL, SM_CXSMICON, "the title bar"), (ICON_BIG, SM_CXICON, "the taskbar and Alt+Tab")] {
        let icon = unsafe { SendMessageW(editor, WM_GETICON, kind as usize, 0) } as HICON;
        assert_is_ours(icon, unsafe { GetSystemMetricsForDpi(metric, dpi) }, &exe, what);
    }
}

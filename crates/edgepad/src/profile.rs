//! Following the foreground app (spec 4.4): the panels show the key sets of
//! the profile that names the app in front, or of the default profile.
//!
//! Windows reports each new foreground window through a WinEvent hook, so
//! nothing is polled. The hook only posts the window to the tray, and the
//! switch happens there, from the message loop, never in the middle of
//! other work on the panels.

use std::cell::RefCell;
use std::ptr::null_mut;

use edgepad_core::config::Config;
use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetWindowThreadProcessId, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use crate::log::debug_log;
use crate::settings::{self, Panels};
use crate::{panel, tray};

thread_local! {
    // The settings in use; none while EdgePad runs on its fixed keys.
    static CONFIG: RefCell<Option<Config>> = const { RefCell::new(None) };
    // The name of the profile the panels show.
    static ACTIVE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Use `config` from now on (none: the fixed keys), and return the panels of
/// the profile for the app in front.
pub unsafe fn use_config(config: Option<Config>) -> Panels {
    CONFIG.set(config);
    let process = unsafe { process_name(GetForegroundWindow()) };
    panels_for(process.as_deref())
}

/// The panels of the profile for `process`, which becomes the one shown.
fn panels_for(process: Option<&str>) -> Panels {
    CONFIG.with_borrow(|config| {
        let Some(config) = config else { return settings::fallback() };
        let profile = config.profile_for(process);
        debug_log!("プロファイル {}（前面: {}）", profile.name, process.unwrap_or("不明"));
        ACTIVE.set(profile.name.clone());
        settings::panels(config, profile)
    })
}

/// `hwnd` came to the front: if its app has another profile than the one
/// shown, show that one. Rebuilding the panels lets go of every key first.
pub unsafe fn foreground_changed(hwnd: HWND) {
    let process = unsafe { process_name(hwnd) };
    let name = CONFIG.with_borrow(|c| c.as_ref().map(|c| c.profile_for(process.as_deref()).name.clone()));
    // With the fixed keys there is one profile only.
    let Some(name) = name else { return };
    if ACTIVE.with_borrow(|active| *active == name) {
        return;
    }
    let panels = panels_for(process.as_deref());
    if !unsafe { panel::rebuild(&panels) } {
        debug_log!("プロファイルの切替: パネルを作れないものがあった");
    }
}

/// The executable's file name of the process owning `hwnd`, if it can be
/// read.
unsafe fn process_name(hwnd: HWND) -> Option<String> {
    if hwnd.is_null() {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    // Limited rights are enough, and are granted even for most elevated apps.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len) } != 0;
    unsafe { CloseHandle(process) };
    let path = String::from_utf16_lossy(&path[..len as usize]);
    ok.then(|| path.rsplit('\\').next().unwrap_or(&path).to_string())
}

/// Start hearing of foreground changes. Returns false if Windows will not
/// say: the panels then stay on the profile they started with.
pub unsafe fn watch() -> bool {
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    let hook = unsafe {
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, null_mut(), Some(on_foreground), 0, 0, flags)
    };
    !hook.is_null()
}

unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    unsafe { tray::post_foreground(hwnd) };
}

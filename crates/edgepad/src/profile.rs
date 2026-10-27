//! Following the foreground app (spec 4.4): the panels show the key sets of
//! the profile that names the app in front, or of the default profile.
//!
//! Windows reports each new foreground window through a WinEvent hook, so
//! nothing is polled. The hook only posts the window to the tray, and the
//! switch happens there, from the message loop, never in the middle of
//! other work on the panels.
//!
//! The tray can pin the profile shown, so that a dialog of another app
//! coming to the front does not switch it. Pinned or not, the app in front
//! is remembered, and unpinning switches to its profile.

use std::cell::{Cell, RefCell};
use std::ptr::null_mut;

use edgepad_core::config::Config;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    TokenIntegrityLevel,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetWindowThreadProcessId, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use crate::log::debug_log;
use crate::settings::{self, Panels};
use crate::{editor_process, panel, pie_menu, send, tray};

/// Test builds only: treat every app coming to the front as out of reach.
#[cfg(feature = "fault-injection")]
pub static TEST_ELEVATED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    // The settings in use; none while EdgePad runs on its fixed keys.
    static CONFIG: RefCell<Option<Config>> = const { RefCell::new(None) };
    // The name of the profile the panels show.
    static ACTIVE: RefCell<String> = const { RefCell::new(String::new()) };
    // Whether the tray pinned it.
    static PINNED: Cell<bool> = const { Cell::new(false) };
    // The executable of the app last in front, EdgePad's own windows aside.
    static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Use `config` from now on (none: the fixed keys), and return the panels of
/// the profile for the app in front, or of the pinned one while it is still
/// there.
pub unsafe fn use_config(config: Option<Config>) -> Panels {
    CONFIG.set(config);
    // The tray menu brings EdgePad to the front: that is no app to follow.
    let front = unsafe { GetForegroundWindow() };
    if !unsafe { is_own(front) } {
        LAST.set(unsafe { process_name(front) });
        send::set_out_of_reach(unsafe { out_of_reach(front) });
    }
    LAST.with_borrow(|process| panels_for(process.as_deref()))
}

/// The panels of the pinned profile, or else of the one for `process`; the
/// profile becomes the one shown. A pin on a profile no longer there ends.
fn panels_for(process: Option<&str>) -> Panels {
    CONFIG.with_borrow(|config| {
        let Some(config) = config else {
            PINNED.set(false);
            return settings::fallback();
        };
        let pinned = if PINNED.get() { ACTIVE.with_borrow(|active| config.profile(active)) } else { None };
        PINNED.set(pinned.is_some());
        let profile = pinned.unwrap_or_else(|| config.profile_for(process));
        debug_log!("プロファイル {}（前面: {}）", profile.name, process.unwrap_or("不明"));
        ACTIVE.set(profile.name.clone());
        settings::panels(config, profile)
    })
}

/// `hwnd` came to the front: unless pinned, follow its app. EdgePad's own
/// windows, the settings window and the tray menu, are no app to follow
/// (spec 7.1): the profile stays that of the app in front before. The hook
/// already skips this process's events; this says so here too, for any
/// other way a window gets here.
pub unsafe fn foreground_changed(hwnd: HWND) {
    if unsafe { is_own(hwnd) } {
        return;
    }
    LAST.set(unsafe { process_name(hwnd) });
    send::set_out_of_reach(unsafe { out_of_reach(hwnd) });
    if !PINNED.get() {
        unsafe { follow() };
    }
}

/// If the app last in front has another profile than the one shown, show
/// that one. Rebuilding the panels lets go of every key first.
unsafe fn follow() {
    let name = LAST.with_borrow(|process| {
        CONFIG.with_borrow(|c| c.as_ref().map(|c| c.profile_for(process.as_deref()).name.clone()))
    });
    // With the fixed keys there is one profile only.
    let Some(name) = name else { return };
    if ACTIVE.with_borrow(|active| *active == name) {
        return;
    }
    let panels = LAST.with_borrow(|process| panels_for(process.as_deref()));
    if !unsafe { panel::rebuild(&panels) } {
        debug_log!("プロファイルの切替: パネルを作れないものがあった");
    }
}

/// The profile shown and whether it is pinned, for the tray menu; none with
/// the fixed keys.
pub fn shown() -> Option<(String, bool)> {
    let configured = CONFIG.with_borrow(Option::is_some);
    configured.then(|| (ACTIVE.with_borrow(String::clone), PINNED.get()))
}

/// Where the profile shown is among the profiles, as the settings file has
/// them, counting from 1; 0 with the fixed keys. The settings window asks
/// it (`ACTIVE_MESSAGE`), to mark the app whose profile is in effect.
pub fn active_index() -> usize {
    CONFIG.with_borrow(|config| {
        let Some(config) = config else { return 0 };
        ACTIVE.with_borrow(|active| config.profiles.iter().position(|p| p.name == *active).map_or(0, |i| i + 1))
    })
}

/// "EdgePadActiveProfile", sent by the settings window's process to the
/// running EdgePad's tray window: answered with `active_index`. Registered
/// by name: the name is the contract between the two processes.
pub fn active_message() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static ID: AtomicU32 = AtomicU32::new(0);
    if ID.load(Ordering::Relaxed) == 0 {
        ID.store(unsafe { windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(windows_sys::w!("EdgePadActiveProfile")) }, Ordering::Relaxed);
    }
    ID.load(Ordering::Relaxed)
}

/// Pin the profile shown, or unpin it and follow the app in front again.
pub unsafe fn toggle_pin() {
    if shown().is_none() {
        return;
    }
    let pin = !PINNED.get();
    PINNED.set(pin);
    // Pinning from the tray is taken as a profile switch (spec 4.6): an
    // open pie menu closes, though the panels may stay as they are.
    unsafe { pie_menu::close_all() };
    debug_log!("プロファイル {}: {}", ACTIVE.with_borrow(String::clone), if pin { "固定" } else { "固定を解除" });
    if !pin {
        unsafe { follow() };
    }
}

/// The executable of the app last in front, if it could be read.
pub fn front() -> Option<String> {
    LAST.with_borrow(Clone::clone)
}

/// Whether the process owning `hwnd` runs at a higher integrity level than
/// EdgePad, as an app run as administrator does: Windows then drops the
/// keys EdgePad sends it (UIPI). A level that cannot be read counts as
/// within reach, so no notice is given on a guess.
unsafe fn out_of_reach(hwnd: HWND) -> bool {
    #[cfg(feature = "fault-injection")]
    if TEST_ELEVATED.load(std::sync::atomic::Ordering::Relaxed) {
        return true;
    }
    if hwnd.is_null() {
        return false;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let theirs = unsafe { integrity(process) };
    unsafe { CloseHandle(process) };
    let ours = unsafe { integrity(GetCurrentProcess()) };
    matches!((theirs, ours), (Some(theirs), Some(ours)) if theirs > ours)
}

/// The integrity level of `process`: the last part of its token's label SID
/// (0x2000 medium, 0x3000 high, ...).
unsafe fn integrity(process: HANDLE) -> Option<u32> {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        // The label and its SID, which follows it; u64 for alignment.
        let mut buffer = [0u64; 16];
        let mut len = 0;
        let size = size_of_val(&buffer) as u32;
        let ok = GetTokenInformation(token, TokenIntegrityLevel, buffer.as_mut_ptr().cast(), size, &mut len) != 0;
        CloseHandle(token);
        if !ok {
            return None;
        }
        let sid = (*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL)).Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        (count > 0).then(|| *GetSidSubAuthority(sid, count as u32 - 1))
    }
}

/// Whether `hwnd` is one of EdgePad's own windows: this process's, or the
/// settings window's.
unsafe fn is_own(hwnd: HWND) -> bool {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid == unsafe { GetCurrentProcessId() } || editor_process::is_settings(pid)
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

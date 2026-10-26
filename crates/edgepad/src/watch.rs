//! The message loop, which also watches the settings folders and applies
//! the settings file again when it is saved.
//!
//! One thread waits on the messages and on a change notification per folder
//! at once (MsgWaitForMultipleObjectsEx), so waiting costs no CPU: no timer,
//! no polling.

use std::cell::RefCell;
use std::mem::zeroed;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
};
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MWMO_INPUTAVAILABLE, MSG, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
    QS_ALLINPUT, WM_QUIT,
};

use crate::log::debug_log;
use crate::{editor, panel, profile, settings, tray};

/// An editor may write a file several times for one save: read it once the
/// folder has been quiet this long.
const SETTLE: Duration = Duration::from_millis(200);

thread_local! {
    // The settings file's content as last read, usable or not.
    static SEEN: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    // The last notice of a file that could not be read at all. Reported once:
    // each report writes the --debug log into a watched folder, which would
    // wake this up again for the same failure.
    static UNREADABLE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Read the settings file and apply it if it changed since it was last read,
/// or in any case with `force`. A file that cannot be used is reported, and
/// the panels stay as they are.
pub unsafe fn reload(force: bool) {
    let seen = SEEN.take();
    match settings::reload(seen.as_deref(), force) {
        // Recorded as read even when a panel could not be made: retrying the
        // same content on every change in the folder would loop, as the
        // --debug log is written there. The tray's 設定を再読込 (force), or a
        // changed file, tries again.
        settings::Outcome::Loaded(config, bytes) => {
            UNREADABLE.set(None);
            let panels = unsafe { profile::use_config(Some(config)) };
            let built = unsafe { panel::rebuild(&panels) };
            unsafe {
                tray::set_hotkey(panels.hotkey);
                // The theme may have changed with the file.
                editor::restyle();
            }
            match built {
                true => debug_log!("再読込: パネル {} 枚", panels.edges.len()),
                false => debug_log!("再読込: パネルを作れないものがあった"),
            }
            SEEN.set(Some(bytes));
        }
        settings::Outcome::Failed(notice, bytes) => {
            let again = bytes.is_none() && UNREADABLE.with_borrow(|last| last.as_deref() == Some(notice.as_str()));
            if !again {
                debug_log!("再読込できない（そのまま動く）: {notice}");
                tray::notify(&notice);
            }
            UNREADABLE.set(if bytes.is_none() { Some(notice) } else { None });
            SEEN.set(bytes.or(seen));
        }
        settings::Outcome::Unchanged | settings::Outcome::Missing => SEEN.set(seen),
    }
}

/// Dispatch messages until WM_QUIT, applying the settings file whenever it
/// changes, and toggling every panel whenever `toggle` (a second launch) is
/// signalled. `seen` is the settings file's content as read at start, and
/// `unreadable` the notice already given if it could not be read at all.
pub unsafe fn run(seen: Option<Vec<u8>>, unreadable: Option<String>, toggle: HANDLE) {
    SEEN.set(seen);
    // A file that could not be read at start was reported then already.
    UNREADABLE.set(unreadable);
    let watches: Vec<HANDLE> = settings::folders()
        .iter()
        .filter_map(|dir| {
            let path: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
            let filter = FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_SIZE;
            let h = unsafe { FindFirstChangeNotificationW(path.as_ptr(), 0, filter) };
            (h != INVALID_HANDLE_VALUE).then_some(h)
        })
        .collect();
    let folders = watches.len() as u32;
    let mut handles = watches.clone();
    if !toggle.is_null() {
        handles.push(toggle);
    }
    let n = handles.len() as u32;
    // When the folders have been quiet for SETTLE after a change.
    let mut due: Option<Instant> = None;
    loop {
        let timeout = due.map_or(INFINITE, |t| t.saturating_duration_since(Instant::now()).as_millis() as u32);
        let woke = unsafe {
            MsgWaitForMultipleObjectsEx(n, handles.as_ptr(), timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if woke < WAIT_OBJECT_0 + folders {
            unsafe { FindNextChangeNotification(watches[(woke - WAIT_OBJECT_0) as usize]) };
            due = Some(Instant::now() + SETTLE);
        } else if woke < WAIT_OBJECT_0 + n {
            unsafe { panel::toggle_all() };
        } else if woke == WAIT_OBJECT_0 + n {
            let mut msg: MSG = unsafe { zeroed() };
            while unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) } != 0 {
                if msg.message == WM_QUIT {
                    close(&watches);
                    return;
                }
                unsafe { DispatchMessageW(&msg) };
            }
        } else if woke == WAIT_TIMEOUT {
            due = None;
            unsafe {
                reload(false);
                editor::file_changed();
            }
        } else {
            // WAIT_FAILED: nothing sensible to wait on; keep the app alive
            // on messages alone.
            close(&watches);
            let mut msg: MSG = unsafe { zeroed() };
            while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
                unsafe { DispatchMessageW(&msg) };
            }
            return;
        }
    }
}

fn close(watches: &[HANDLE]) {
    for &h in watches {
        unsafe { FindCloseChangeNotification(h) };
    }
}

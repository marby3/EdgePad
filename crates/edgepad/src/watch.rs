//! The message loop, which also watches the settings folders and applies
//! the settings file again when it is saved.
//!
//! One thread waits on the messages and on a change notification per folder
//! at once (MsgWaitForMultipleObjectsEx), so waiting costs no CPU: no timer,
//! no polling.

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

use crate::{panel, settings};

/// An editor may write a file several times for one save: read it once the
/// folder has been quiet this long.
const SETTLE: Duration = Duration::from_millis(200);

/// Dispatch messages until WM_QUIT, applying the settings file whenever it
/// changes. `applied` is the content of the file in use (`None` for the
/// fixed keys).
pub unsafe fn run(mut applied: Option<Vec<u8>>) {
    let watches: Vec<HANDLE> = settings::folders()
        .iter()
        .filter_map(|dir| {
            let path: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
            let filter = FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_SIZE;
            let h = unsafe { FindFirstChangeNotificationW(path.as_ptr(), 0, filter) };
            (h != INVALID_HANDLE_VALUE).then_some(h)
        })
        .collect();
    // When the folders have been quiet for SETTLE after a change.
    let mut due: Option<Instant> = None;
    loop {
        let timeout = due.map_or(INFINITE, |t| t.saturating_duration_since(Instant::now()).as_millis() as u32);
        let n = watches.len() as u32;
        let woke = unsafe {
            MsgWaitForMultipleObjectsEx(n, watches.as_ptr(), timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if woke < WAIT_OBJECT_0 + n {
            unsafe { FindNextChangeNotification(watches[(woke - WAIT_OBJECT_0) as usize]) };
            due = Some(Instant::now() + SETTLE);
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
            if let Some((panels, bytes)) = settings::reload(applied.as_deref()) {
                // Recorded only once every panel is up: after a failure the
                // same content, saved again, is tried again.
                if unsafe { panel::rebuild(&panels) } {
                    applied = Some(bytes);
                }
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

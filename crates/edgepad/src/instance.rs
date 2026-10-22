//! One EdgePad per user: a named mutex, and a named event by which a second
//! launch asks the running one to toggle its panels.
//!
//! The names carry a hash of %APPDATA%: one user's launches share it, and
//! tests, which give each launch its own %APPDATA%, never meet a real EdgePad.

use std::ptr::null;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent};

fn name(what: &str) -> Vec<u16> {
    let appdata: Vec<u16> = std::env::var_os("APPDATA")
        .map(|a| std::os::windows::ffi::OsStrExt::encode_wide(a.as_os_str()).collect())
        .unwrap_or_default();
    // FNV-1a, ASCII case-folded so C:\Users and c:\users are one user
    // (Windows paths compare without case; folding ASCII covers the drive and
    // the usual folder names).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for c in appdata {
        let c = if (b'A' as u16..=b'Z' as u16).contains(&c) { c + 32 } else { c };
        hash = (hash ^ c as u64).wrapping_mul(0x100_0000_01b3);
    }
    format!("Local\\EdgePad-{hash:016x}-{what}").encode_utf16().chain([0]).collect()
}

/// Become the running EdgePad. Returns the event a second launch signals
/// (null if it could not be made), or `None` when another EdgePad already
/// runs: it has been told to toggle, and this one should exit.
pub unsafe fn claim() -> Option<HANDLE> {
    unsafe {
        // Auto-reset: each second launch toggles once. Should it fail, this
        // EdgePad still runs, only deaf to second launches: better than none.
        let toggle = CreateEventW(null(), 0, 0, name("toggle").as_ptr());
        // Kept open, so held, until the process ends.
        let mutex = CreateMutexW(null(), 0, name("instance").as_ptr());
        if !mutex.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            if !toggle.is_null() {
                SetEvent(toggle);
            }
            return None;
        }
        Some(toggle)
    }
}

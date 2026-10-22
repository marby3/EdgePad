//! The --debug log: one timestamped line per event, in edgepad-debug.log
//! beside the exe (or under %APPDATA%\EdgePad when that folder is not
//! writable). Without --debug nothing is opened and every call returns at once.
//!
//! Lines are written straight to the file, unbuffered, so the last ones
//! survive a crash. Writing allocates nothing and never waits for the lock:
//! the panic hook releases keys, and each key sent is logged.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::System::SystemInformation::GetLocalTime;

const FILE_NAME: &str = "edgepad-debug.log";

static LOG: OnceLock<Mutex<File>> = OnceLock::new();

/// Open a fresh log if `debug` is set. Without a writable place, EdgePad
/// runs without a log.
pub fn start(debug: bool) {
    if !debug {
        return;
    }
    let beside = std::env::current_exe().ok().map(|exe| exe.with_file_name(FILE_NAME));
    let appdata = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("EdgePad"));
    let file = beside.and_then(|p| File::create(p).ok()).or_else(|| {
        let dir = appdata?;
        std::fs::create_dir_all(&dir).ok()?;
        File::create(dir.join(FILE_NAME)).ok()
    });
    if let Some(file) = file {
        let _ = LOG.set(Mutex::new(file));
    }
}

/// Write one line, with the local time in front. Use `debug_log!`.
pub fn write(args: std::fmt::Arguments) {
    let Some(log) = LOG.get() else { return };
    let Ok(mut file) = log.try_lock() else { return };
    let mut t = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    let _ = write!(
        file,
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {args}\r\n",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    );
}

/// Log a line with `format!` syntax, only under --debug.
macro_rules! debug_log {
    ($($arg:tt)*) => { $crate::log::write(format_args!($($arg)*)) };
}
pub(crate) use debug_log;

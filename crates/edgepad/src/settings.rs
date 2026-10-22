//! Finding and reading the settings file.
//!
//! edgepad.ini beside the exe wins (portable use); otherwise
//! %APPDATA%\EdgePad\edgepad.ini. Without a usable file EdgePad starts with
//! the fixed M0 keys.

use std::io::Read;
use std::path::PathBuf;

use edgepad_core::config::{self, Edge, KeyDef};

const FILE: &str = "edgepad.ini";

/// The settings file in use, if there is one.
fn path() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.with_file_name(FILE);
    if beside.is_file() {
        return Some(beside);
    }
    let appdata = PathBuf::from(std::env::var_os("APPDATA")?).join("EdgePad").join(FILE);
    appdata.is_file().then_some(appdata)
}

/// The keys of the left edge in the default profile; empty when that edge
/// shows nothing. Falls back to the fixed keys when the file is missing,
/// unreadable or wrong. Reporting what was wrong comes with the tray
/// notifications.
pub fn left_keys() -> Vec<KeyDef> {
    let Some(path) = path() else { return config::fallback_keys() };
    let mut bytes = Vec::new();
    // One byte over the limit is enough for the parser to refuse it.
    let read = std::fs::File::open(path)
        .and_then(|f| f.take(config::MAX_BYTES as u64 + 1).read_to_end(&mut bytes));
    let Ok(config) = read.map_err(drop).and_then(|_| config::parse(&bytes).map_err(drop)) else {
        return config::fallback_keys();
    };
    match &config.default_profile().edges[Edge::Left as usize] {
        Some(set) => config.set(set).map(|s| s.keys.clone()).unwrap_or_default(),
        None => Vec::new(),
    }
}

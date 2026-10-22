//! Finding and reading the settings file.
//!
//! edgepad.ini beside the exe wins (portable use); otherwise
//! %APPDATA%\EdgePad\edgepad.ini. Without a usable file EdgePad starts with
//! the fixed M0 keys on the left edge.

use std::io::Read;
use std::path::PathBuf;

use edgepad_core::config::{self, Edge, KeyDef, Monitor};
use edgepad_core::layout;

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

/// What the panels need from the settings.
pub struct Panels {
    /// Side of a key in logical pixels.
    pub key_size: u32,
    /// One per edge that shows a panel.
    pub edges: Vec<EdgePanel>,
}

pub struct EdgePanel {
    pub edge: Edge,
    pub monitor: Monitor,
    /// Columns on the left and right, rows on the top and bottom.
    pub lines: usize,
    pub keys: Vec<KeyDef>,
}

/// The panels for the settings file as EdgePad starts, and the bytes they
/// came from. Falls back to the fixed keys on the left when the file is
/// missing, unreadable or wrong. Reporting what was wrong comes with the tray
/// notifications.
pub fn load() -> (Panels, Option<Vec<u8>>) {
    match read().and_then(|bytes| Some((panels(&bytes)?, bytes))) {
        Some((panels, bytes)) => (panels, Some(bytes)),
        None => (fallback(), None),
    }
}

/// The panels for the settings file now, if it differs from `applied` (the
/// bytes in use, `None` for the fixed keys) and can be used. A file that is
/// gone, unreadable or wrong gives `None`: EdgePad keeps what it has.
pub fn reload(applied: Option<&[u8]>) -> Option<(Panels, Vec<u8>)> {
    let bytes = read()?;
    if applied == Some(&bytes[..]) {
        return None;
    }
    Some((panels(&bytes)?, bytes))
}

/// The folders a settings file can be in: beside the exe, and
/// %APPDATA%\EdgePad if it exists.
pub fn folders() -> Vec<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from));
    let appdata = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("EdgePad")).filter(|d| d.is_dir());
    beside.into_iter().chain(appdata).collect()
}

fn fallback() -> Panels {
    Panels {
        key_size: layout::KEY_SIZE as u32,
        edges: vec![EdgePanel { edge: Edge::Left, monitor: Monitor::Primary, lines: 1, keys: config::fallback_keys() }],
    }
}

/// The settings file's bytes, cut one byte past the limit: that is enough for
/// the parser to refuse it.
fn read() -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path()?).and_then(|f| f.take(config::MAX_BYTES as u64 + 1).read_to_end(&mut bytes)).ok()?;
    Some(bytes)
}

/// The panels of the default profile: one per enabled edge with a key set.
fn panels(bytes: &[u8]) -> Option<Panels> {
    let config = config::parse(bytes).ok()?;
    let default = config.default_profile();
    let edges = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom]
        .into_iter()
        .filter(|&edge| config.edges[edge as usize].enabled)
        .filter_map(|edge| {
            let set = config.set(default.edges[edge as usize].as_deref()?)?;
            let e = &config.edges[edge as usize];
            Some(EdgePanel { edge, monitor: e.monitor, lines: e.lines as usize, keys: set.keys.clone() })
        })
        .collect();
    Some(Panels { key_size: config.general.key_size, edges })
}

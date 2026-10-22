//! Finding and reading the settings file.
//!
//! edgepad.ini beside the exe wins (portable use); otherwise
//! %APPDATA%\EdgePad\edgepad.ini. Without a usable file EdgePad starts with
//! the fixed M0 keys on the left edge.

use std::io::Read;
use std::path::PathBuf;

use edgepad_core::config::{self, Edge, KeyDef};
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

/// What the panels need from the settings: the key size, and for each edge
/// that shows a panel, its line count and keys.
pub struct Panels {
    pub key_size: u32,
    pub edges: Vec<(Edge, usize, Vec<KeyDef>)>,
}

/// The panels of the default profile: one per enabled edge with a key set.
/// Falls back to the fixed keys on the left when the file is missing,
/// unreadable or wrong. Reporting what was wrong comes with the tray
/// notifications.
pub fn load() -> Panels {
    let fallback = || Panels { key_size: layout::KEY_SIZE as u32, edges: vec![(Edge::Left, 1, config::fallback_keys())] };
    let Some(path) = path() else { return fallback() };
    let mut bytes = Vec::new();
    // One byte over the limit is enough for the parser to refuse it.
    let read = std::fs::File::open(path)
        .and_then(|f| f.take(config::MAX_BYTES as u64 + 1).read_to_end(&mut bytes));
    let Ok(config) = read.map_err(drop).and_then(|_| config::parse(&bytes).map_err(drop)) else {
        return fallback();
    };
    let default = config.default_profile();
    let edges = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom]
        .into_iter()
        .filter(|&edge| config.edges[edge as usize].enabled)
        .filter_map(|edge| {
            let set = config.set(default.edges[edge as usize].as_deref()?)?;
            Some((edge, config.edges[edge as usize].lines as usize, set.keys.clone()))
        })
        .collect();
    Panels { key_size: config.general.key_size, edges }
}

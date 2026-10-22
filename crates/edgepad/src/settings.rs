//! Finding and reading the settings file.
//!
//! edgepad.ini beside the exe wins (portable use); otherwise
//! %APPDATA%\EdgePad\edgepad.ini. Without a usable file EdgePad starts with
//! the fixed M0 keys on the left edge.

use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;

use edgepad_core::config::{self, Edge, Handle, KeyDef, KeySet, Monitor};
use edgepad_core::keys::Combo;
use edgepad_core::layout;
use edgepad_core::modifier::LatchMode;

const FILE: &str = "edgepad.ini";

/// A settings file that makes the same panel as the fixed keys.
const TEMPLATE: &str = "\
; EdgePad の設定ファイル。書き方は docs/spec.md の 7 章を参照。
; 保存すると、EdgePad がすぐに読み直す。

[set.mods]
key = Esc   | hold     | Esc
key = Ctrl  | modifier | Ctrl
key = Shift | modifier | Shift
key = Alt   | modifier | Alt
key = Space | hold     | Space

[profile.default]
left = mods
";

/// The settings file in use, if there is one.
pub fn path() -> Option<PathBuf> {
    let beside = beside_exe()?;
    if beside.is_file() {
        return Some(beside);
    }
    let appdata = appdata_dir()?.join(FILE);
    appdata.is_file().then_some(appdata)
}

fn beside_exe() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.with_file_name(FILE))
}

fn appdata_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("EdgePad"))
}

/// The settings file in use, or a new one made from the template: beside the
/// exe, or under %APPDATA%\EdgePad when the exe's folder is not writable.
pub fn path_or_template() -> Option<PathBuf> {
    if let Some(path) = path() {
        return Some(path);
    }
    let beside = beside_exe()?;
    if std::fs::write(&beside, TEMPLATE).is_ok() {
        return Some(beside);
    }
    let dir = appdata_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(FILE);
    std::fs::write(&path, TEMPLATE).ok()?;
    Some(path)
}

/// What the panels need from the settings.
pub struct Panels {
    /// Side of a key in logical pixels.
    pub key_size: u32,
    /// One per edge that shows a panel.
    pub edges: Vec<EdgePanel>,
    /// Shows and hides every edge, if set.
    pub hotkey: Option<Combo>,
    /// A modifier tapped for less than this many milliseconds latches.
    pub latch_ms: u32,
    /// Whether a shortcut ends a latch.
    pub latch: LatchMode,
    /// Every key set, for page keys that switch a panel to another.
    pub sets: Rc<[KeySet]>,
}

pub struct EdgePanel {
    pub edge: Edge,
    pub monitor: Monitor,
    /// Columns on the left and right, rows on the top and bottom.
    pub lines: usize,
    pub keys: Vec<KeyDef>,
    /// The strip left while the panel is hidden; none with `handle = none`.
    pub handle: Option<Handle>,
}

/// What reading the settings file came to.
pub enum Outcome {
    /// The panels, and the bytes they came from.
    Loaded(Panels, Vec<u8>),
    /// The same bytes as those read last time.
    Unchanged,
    /// No settings file in either place.
    Missing,
    /// The file could not be used: the notice says why, with the bytes read
    /// if it could be read at all.
    Failed(String, Option<Vec<u8>>),
}

/// The panels for the settings file as EdgePad starts, the bytes read, and
/// what was wrong if the file could not be used. Falls back to the fixed
/// keys on the left when there is no usable file.
pub fn load() -> (Panels, Option<Vec<u8>>, Option<String>) {
    match reload(None, true) {
        Outcome::Loaded(panels, bytes) => (panels, Some(bytes), None),
        Outcome::Failed(notice, bytes) => (fallback(), bytes, Some(notice)),
        Outcome::Unchanged | Outcome::Missing => (fallback(), None, None),
    }
}

/// Read the settings file again. `seen` is the content read last time,
/// usable or not; unless `force` is set, the same content again is
/// `Unchanged`, so an error is reported once, not on every change to other
/// files in the folder. EdgePad keeps what it has on anything but `Loaded`.
pub fn reload(seen: Option<&[u8]>, force: bool) -> Outcome {
    let Some(path) = path() else { return Outcome::Missing };
    let mut bytes = Vec::new();
    // One byte over the limit is enough for the parser to refuse it.
    let read = std::fs::File::open(path).and_then(|f| f.take(config::MAX_BYTES as u64 + 1).read_to_end(&mut bytes));
    if let Err(e) = read {
        return Outcome::Failed(format!("{FILE} を読めません: {e}"), None);
    }
    if !force && seen == Some(&bytes[..]) {
        return Outcome::Unchanged;
    }
    match panels(&bytes) {
        Ok(panels) => Outcome::Loaded(panels, bytes),
        Err(errors) => Outcome::Failed(notice(&errors), Some(bytes)),
    }
}

/// The first error, with its line, and how many more there are.
fn notice(errors: &[config::Error]) -> String {
    // The parser never fails without saying why.
    let first = &errors[0];
    let mut text = match first.line {
        0 => format!("{FILE}: {}", first.reason),
        n => format!("{FILE} の {n} 行目: {}", first.reason),
    };
    if errors.len() > 1 {
        text += &format!("（ほか {} 件）", errors.len() - 1);
    }
    text
}

/// The folders a settings file can be in: beside the exe, and
/// %APPDATA%\EdgePad if it exists.
pub fn folders() -> Vec<PathBuf> {
    let beside = beside_exe().and_then(|p| p.parent().map(PathBuf::from));
    let appdata = appdata_dir().filter(|d| d.is_dir());
    beside.into_iter().chain(appdata).collect()
}

fn fallback() -> Panels {
    let general = config::General::default();
    Panels {
        key_size: layout::KEY_SIZE as u32,
        edges: vec![EdgePanel {
            edge: Edge::Left,
            monitor: Monitor::Primary,
            lines: 1,
            keys: config::fallback_keys(),
            handle: config::EdgeConfig::default().handle,
        }],
        hotkey: None,
        latch_ms: general.latch_ms,
        latch: general.latch,
        sets: Rc::new([]),
    }
}

/// The panels of the default profile: one per enabled edge with a key set.
fn panels(bytes: &[u8]) -> Result<Panels, Vec<config::Error>> {
    let config = config::parse(bytes)?;
    let default = config.default_profile();
    let edges = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom]
        .into_iter()
        .filter(|&edge| config.edges[edge as usize].enabled)
        .filter_map(|edge| {
            let set = config.set(default.edges[edge as usize].as_deref()?)?;
            let e = &config.edges[edge as usize];
            Some(EdgePanel { edge, monitor: e.monitor, lines: e.lines as usize, keys: set.keys.clone(), handle: e.handle })
        })
        .collect();
    let g = &config.general;
    let (key_size, hotkey, latch_ms, latch) = (g.key_size, g.hotkey, g.latch_ms, g.latch);
    Ok(Panels { key_size, edges, hotkey, latch_ms, latch, sets: config.sets.into() })
}

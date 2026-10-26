//! Finding and reading the settings file.
//!
//! edgepad.ini beside the exe wins (portable use); otherwise
//! %APPDATA%\EdgePad\edgepad.ini. Without a usable file EdgePad starts with
//! the fixed M0 keys on the left edge.

use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;

use edgepad_core::config::{self, Config, Edge, Handle, KeyDef, KeySet, Monitor, Profile};
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
    /// Opacity in percent of a panel shown normally.
    pub opacity: u8,
    /// Opacity in percent of the panels while a pie menu is open, or of one
    /// a pen is over.
    pub dim_opacity: u8,
    pub theme: config::Theme,
    pub accent: config::Accent,
    /// How long the mouse rests on a handle to show its panel, and how long
    /// after it leaves a panel so shown that it hides. 0: off.
    pub hover_ms: u32,
    /// How long after it was shown or last pressed a panel hides. 0: off.
    pub auto_hide_ms: u32,
    /// How long a panel takes to slide in or out. 0: off.
    pub slide_ms: u32,
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
    /// Registered as an AppBar while shown.
    pub dock: bool,
}

/// What reading the settings file came to.
pub enum Outcome {
    /// The settings, and the bytes they came from.
    Loaded(Config, Vec<u8>),
    /// The same bytes as those read last time.
    Unchanged,
    /// No settings file in either place.
    Missing,
    /// The file could not be used: the notice says why, with the bytes read
    /// if it could be read at all.
    Failed(String, Option<Vec<u8>>),
}

/// The settings file as EdgePad starts, the bytes read, and what was wrong
/// if the file could not be used. Without a usable file there are no
/// settings, and EdgePad runs on the fixed keys.
pub fn load() -> (Option<Config>, Option<Vec<u8>>, Option<String>) {
    match reload(None, true) {
        Outcome::Loaded(config, bytes) => (Some(config), Some(bytes), None),
        Outcome::Failed(notice, bytes) => (None, bytes, Some(notice)),
        Outcome::Unchanged | Outcome::Missing => (None, None, None),
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
    match config::parse(&bytes) {
        Ok(config) => Outcome::Loaded(config, bytes),
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

/// The fixed keys on the left, for when there is no usable settings file.
pub fn fallback() -> Panels {
    let general = config::General::default();
    Panels {
        key_size: layout::KEY_SIZE as u32,
        edges: vec![EdgePanel {
            edge: Edge::Left,
            monitor: Monitor::Primary,
            lines: 1,
            keys: config::fallback_keys(),
            handle: config::EdgeConfig::default().handle,
            dock: false,
        }],
        hotkey: None,
        latch_ms: general.latch_ms,
        latch: general.latch,
        opacity: general.opacity,
        dim_opacity: general.dim_opacity,
        theme: general.theme,
        accent: general.accent,
        hover_ms: general.hover_ms,
        auto_hide_ms: general.auto_hide_ms,
        slide_ms: general.slide_ms,
        sets: Rc::new([]),
    }
}

/// The panels of `profile`: one per enabled edge it gives a key set.
pub fn panels(config: &Config, profile: &Profile) -> Panels {
    let edges = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom]
        .into_iter()
        .filter(|&edge| config.edges[edge as usize].enabled)
        .filter_map(|edge| {
            let set = config.set(profile.edges[edge as usize].as_deref()?)?;
            let e = &config.edges[edge as usize];
            Some(EdgePanel { edge, monitor: e.monitor, lines: e.lines as usize, keys: set.keys.clone(), handle: e.handle, dock: e.dock })
        })
        .collect();
    let g = &config.general;
    let (key_size, hotkey, latch_ms, latch) = (g.key_size, g.hotkey, g.latch_ms, g.latch);
    let (opacity, dim_opacity, theme, accent) = (g.opacity, g.dim_opacity, g.theme, g.accent);
    let (hover_ms, auto_hide_ms, slide_ms) = (g.hover_ms, g.auto_hide_ms, g.slide_ms);
    let sets = config.sets.as_slice().into();
    Panels {
        key_size, edges, hotkey, latch_ms, latch, opacity, dim_opacity, theme, accent, hover_ms, auto_hide_ms, slide_ms, sets,
    }
}

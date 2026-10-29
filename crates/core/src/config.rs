//! The settings file (edgepad.ini, spec chapter 7).
//!
//! INI-like text in UTF-8: `[section]` headers, `name = value` lines, and
//! comments from a `;` at the start of a line or after a space to the end of
//! the line. Names of sections, settings, keys and kinds ignore case. Every
//! mistake is collected with its line number, so one read reports them all.

use crate::keys::{Combo, Key};
use crate::kle::{self, KleKey};
use crate::layout::Place;
use crate::modifier::LatchMode;

/// Largest settings file read; anything bigger is refused unread.
pub const MAX_BYTES: usize = 64 * 1024;

/// A mistake in the settings file. Line 0 means the file as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub reason: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.line {
            0 => f.write_str(&self.reason),
            n => write!(f, "{n} 行目: {}", self.reason),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub general: General,
    /// Indexed by `Edge as usize`.
    pub edges: [EdgeConfig; 4],
    pub sets: Vec<KeySet>,
    /// `default` is always among them.
    pub profiles: Vec<Profile>,
}

impl Config {
    pub fn set(&self, name: &str) -> Option<&KeySet> {
        self.sets.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn default_profile(&self) -> &Profile {
        self.profile("default").expect("parse guarantees [profile.default]")
    }

    /// The profile for the foreground app, whose executable is named
    /// `process` (`CLIPStudioPaint.exe`; none when it cannot be told): the
    /// first in the file whose process matches it, ignoring case, else the
    /// default.
    pub fn profile_for(&self, process: Option<&str>) -> &Profile {
        let process = process.map(str::to_ascii_lowercase);
        self.profiles
            .iter()
            .find(|p| p.process.is_some() && p.process == process)
            .unwrap_or_else(|| self.default_profile())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct General {
    /// Shows and hides every edge; none when not set.
    pub hotkey: Option<Combo>,
    /// Side of a key in logical pixels.
    pub key_size: u32,
    /// Opacity of the shown panel, percent.
    pub opacity: u8,
    /// Opacity of the panels while a pie menu is open, or of one a pen is
    /// over, percent.
    pub dim_opacity: u8,
    /// A modifier tapped for less than this latches.
    pub latch_ms: u32,
    /// Whether a shortcut ends a latch.
    pub latch: LatchMode,
    pub pen_presses: bool,
    pub theme: Theme,
    /// The colour of held and latched keys, and of the handle.
    pub accent: Accent,
    /// The mouse resting on a handle this long shows its panel, which hides
    /// this long after the mouse leaves it. 0: off.
    pub hover_ms: u32,
    /// A panel hides this long after it was shown or last pressed. 0: off.
    pub auto_hide_ms: u32,
    /// Panels slide in and out over this long. 0: off, they come and go at
    /// once (spec 5.2).
    pub slide_ms: u32,
}

impl Default for General {
    fn default() -> Self {
        General {
            hotkey: None,
            key_size: 56,
            opacity: 100,
            dim_opacity: 35,
            latch_ms: 250,
            latch: LatchMode::OneShot,
            pen_presses: false,
            theme: Theme::Auto,
            accent: Accent::Auto,
            hover_ms: 0,
            auto_hide_ms: 0,
            slide_ms: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    /// Windows' accent colour.
    Auto,
    Rgb(u8, u8, u8),
}

impl Accent {
    /// `auto` or `#RRGGBB`.
    pub fn parse(value: &str) -> Option<Accent> {
        if value.eq_ignore_ascii_case("auto") {
            return Some(Accent::Auto);
        }
        let hex = value.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Accent::Rgb(byte(0)?, byte(2)?, byte(4)?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

const EDGES: [(&str, Edge); 4] =
    [("left", Edge::Left), ("right", Edge::Right), ("top", Edge::Top), ("bottom", Edge::Bottom)];

impl Edge {
    /// `left`, `right`, `top` or `bottom`, in any case.
    pub fn parse(name: &str) -> Option<Edge> {
        EDGES.iter().find(|(n, _)| name.eq_ignore_ascii_case(n)).map(|&(_, e)| e)
    }

    /// Left or right: panels there are vertical.
    pub fn is_side(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeConfig {
    pub enabled: bool,
    pub monitor: Monitor,
    /// Columns on the left and right edges, rows on the top and bottom.
    pub lines: u8,
    /// None: `handle = none`, shown and hidden from the tray or hotkey only.
    pub handle: Option<Handle>,
    /// Registered as an AppBar while shown, so maximised windows leave it
    /// room (spec 5.5).
    pub dock: bool,
    /// How far the panel and its handle keep off the screen's edge, in
    /// logical pixels (Issue #117).
    pub gap: u32,
}

/// The bottom edge's `gap` unless written: a swipe up from its handle then
/// starts clear of where Windows takes it for its own (the Start menu).
pub const BOTTOM_GAP: u32 = 16;

impl Default for EdgeConfig {
    fn default() -> Self {
        EdgeConfig {
            enabled: true,
            monitor: Monitor::Primary,
            lines: 1,
            handle: Some(Handle { width: 6, length: 40, position: 50 }),
            dock: false,
            gap: 0,
        }
    }
}

impl EdgeConfig {
    /// Edge `edge`'s settings unless written: the bottom kept off the edge.
    pub fn of(edge: Edge) -> EdgeConfig {
        let gap = if edge == Edge::Bottom { BOTTOM_GAP } else { 0 };
        EdgeConfig { gap, ..EdgeConfig::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Monitor {
    Primary,
    /// 1 is the first monitor.
    Number(u8),
    /// Wherever the cursor is when the panel shows.
    Cursor,
}

/// The strip left on the edge while the panel is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    /// Thickness in logical pixels.
    pub width: u8,
    /// Length as a percentage of the edge.
    pub length: u8,
    /// Where its centre sits along the edge, percent.
    pub position: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeySet {
    pub name: String,
    /// In a free layout (`layout = free` or `kle:`, Issue #101), every
    /// key has its `place`; along the edge, none does.
    pub keys: Vec<KeyDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyDef {
    pub label: String,
    pub action: Action,
    /// In keys: 2 is twice as wide as a plain key.
    pub width: u8,
    /// Where it sits in a free layout (x, y, w, h).
    pub place: Option<Place>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Ctrl, Shift, Alt or Win: held while pressed, latched by a tap.
    Modifier(Key),
    /// Held while pressed.
    Hold(Key),
    /// Sent once on release.
    Shortcut(Combo),
    /// Repeated while pressed.
    Repeat { combo: Combo, interval_ms: u32 },
    Macro(Vec<Step>),
    /// Typed as Unicode characters.
    Text(String),
    /// A program or URL to start.
    Run(String),
    Page(Page),
    /// Opens a pie menu of another key set's keys (spec 4.6).
    Pie(String),
    /// Turns the panel to another key set: while held, or from one press
    /// to the next with `toggle` (Issue #104).
    Layer { set: String, toggle: bool },
    /// Nothing: a key of a KLE layout no key line is given to (Issue #101).
    None,
}

impl Action {
    /// Its kind as the settings file writes it.
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Modifier(_) => "modifier",
            Action::Hold(_) => "hold",
            Action::Shortcut(_) => "shortcut",
            Action::Repeat { .. } => "repeat",
            Action::Macro(_) => "macro",
            Action::Text(_) => "text",
            Action::Run(_) => "run",
            Action::Page(_) => "page",
            Action::Pie(_) => "pie",
            Action::Layer { .. } => "layer",
            Action::None => "none",
        }
    }

    /// Whether it can be an item of a pie menu: items run once, when the
    /// finger lifts. Held keys cannot, and a pie inside a pie would need a
    /// menu opened from a menu. One that sends nothing closes the menu, as
    /// an item the settings window has just added is (Issue #113).
    pub fn is_pie_item(&self) -> bool {
        match self {
            Action::Shortcut(_) | Action::Macro(_) | Action::Text(_) | Action::Run(_) | Action::Page(_) | Action::None => true,
            Action::Modifier(_) | Action::Hold(_) | Action::Repeat { .. } | Action::Pie(_) | Action::Layer { .. } => false,
        }
    }
}

/// How many items a pie menu has, from its key set (spec 4.6).
pub const PIE_ITEMS: std::ops::RangeInclusive<usize> = 2..=8;

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Keys(Combo),
    Wait(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Next,
    Prev,
    /// Switch to another key set.
    Set(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// Lower-case process name (`clipstudiopaint.exe`); none for default.
    pub process: Option<String>,
    /// Key set per edge, by `Edge as usize`; none hides the edge.
    pub edges: [Option<String>; 4],
}

/// The text of a settings file, and whether it began with a BOM; an error
/// when there is too much of it or it is not UTF-8.
pub(crate) fn text_of(bytes: &[u8]) -> Result<(bool, &str), Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error { line: 0, reason: format!("ファイルが大きすぎる（{} KiB まで）", MAX_BYTES / 1024) });
    }
    let (bom, bytes) = match bytes.strip_prefix(b"\xEF\xBB\xBF") {
        Some(rest) => (true, rest),
        None => (false, bytes),
    };
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok((bom, text)),
        Err(e) => {
            let line = 1 + bytes[..e.valid_up_to()].iter().filter(|&&b| b == b'\n').count();
            Err(Error { line, reason: "UTF-8 ではない文字がある".into() })
        }
    }
}

/// Read a settings file, with no other file to read: a KLE layout cannot.
pub fn parse(bytes: &[u8]) -> Result<Config, Vec<Error>> {
    parse_with(bytes, &mut |_| Err("ほかのファイルは読めない".into()))
}

/// Read a settings file, `read` giving the content of the file a KLE
/// layout names (as written after `kle:`), or why it cannot.
pub fn parse_with(bytes: &[u8], read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>) -> Result<Config, Vec<Error>> {
    let (_, text) = text_of(bytes).map_err(|e| vec![e])?;
    let mut parser = Parser { read: Some(read), ..Parser::default() };
    for (i, line) in text.split('\n').enumerate() {
        parser.line(i + 1, strip_comment(line.trim_end_matches('\r')).trim());
    }
    parser.finish()
}

/// `line` without its comment: from a `;` at the start or after whitespace.
pub(crate) fn strip_comment(line: &str) -> &str {
    let mut prev_space = true;
    for (i, c) in line.char_indices() {
        if c == ';' && prev_space {
            return &line[..i];
        }
        prev_space = c.is_whitespace();
    }
    line
}

/// Where the lines being read go.
#[derive(Clone, Copy)]
enum Section {
    /// Before the first header, or inside a header that was wrong.
    None { reported: bool },
    General,
    /// [edge]: every edge's, but where an edge writes its own.
    Edges,
    Edge(Edge),
    Set(usize),
    Profile(usize),
}

/// How a key set's keys are laid out, as its `layout` says.
enum SetLayout {
    Edge,
    Free,
    /// From a KLE file: the `layout` line, and the file's keys.
    Kle(usize, Vec<KleKey>),
}

struct Parser<'a> {
    config: Config,
    /// Gives the files KLE layouts name.
    read: Option<&'a mut dyn FnMut(&str) -> Result<Vec<u8>, String>>,
    /// Each set's layout, and the line of each of its keys.
    layouts: Vec<SetLayout>,
    key_lines: Vec<Vec<usize>>,
    section: Section,
    /// Section names seen so far (lower case).
    seen: Vec<String>,
    /// Settings already given in the current section (lower case).
    given: Vec<String>,
    /// [edge]'s settings, read and checked, to give each edge that does not
    /// write its own once the whole file is read: (name, value).
    shared: Vec<(String, String)>,
    /// The settings each edge writes, by `Edge as usize`: columns and rows
    /// as `lines`.
    edge_given: [Vec<String>; 4],
    /// Header line of each set, and whether it had any key lines.
    set_lines: Vec<(usize, bool)>,
    /// Header line of each profile.
    profile_lines: Vec<usize>,
    /// Key set names to check once all sets are known: (line, name).
    refs: Vec<(usize, String)>,
    /// The key sets pie keys open, to check once all sets are known.
    pies: Vec<(usize, String)>,
    errors: Vec<Error>,
}

impl Default for Parser<'_> {
    fn default() -> Self {
        Parser {
            read: None,
            layouts: Vec::new(),
            key_lines: Vec::new(),
            config: Config {
                general: General::default(),
                edges: [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom].map(EdgeConfig::of),
                sets: Vec::new(),
                profiles: Vec::new(),
            },
            section: Section::None { reported: false },
            seen: Vec::new(),
            given: Vec::new(),
            shared: Vec::new(),
            edge_given: Default::default(),
            set_lines: Vec::new(),
            profile_lines: Vec::new(),
            refs: Vec::new(),
            pies: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl Parser<'_> {
    fn error(&mut self, line: usize, reason: impl Into<String>) {
        self.errors.push(Error { line, reason: reason.into() });
    }

    fn line(&mut self, n: usize, line: &str) {
        if line.is_empty() {
            return;
        }
        if let Some(rest) = line.strip_prefix('[') {
            return self.header(n, rest);
        }
        let Some((name, value)) = line.split_once('=') else {
            return self.error(n, format!("「名前 = 値」の形になっていない: {line}"));
        };
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
        if let Section::None { reported } = self.section {
            // After a wrong header its lines are skipped: one error is enough.
            if !reported {
                self.error(n, format!("セクションの外に設定「{name}」がある"));
            }
            return;
        }
        let is_key = matches!(self.section, Section::Set(_));
        if !(is_key && name == "key") {
            if self.given.contains(&name) {
                return self.error(n, format!("設定「{name}」が2回ある"));
            }
            self.given.push(name.clone());
        }
        let result = match self.section {
            Section::None { .. } => Ok(()),
            Section::General => self.general(&name, value),
            Section::Edges => self.common(&name, value),
            Section::Edge(edge) => {
                let given = if matches!(name.as_str(), "columns" | "rows") { "lines".into() } else { name.clone() };
                self.edge_given[edge as usize].push(given);
                self.edge(edge, &name, value)
            }
            Section::Set(set) => self.key(n, set, &name, value),
            Section::Profile(profile) => self.profile(n, profile, &name, value),
        };
        if let Err(reason) = result {
            self.error(n, reason);
        }
    }

    fn header(&mut self, n: usize, rest: &str) {
        self.section = Section::None { reported: true };
        self.given.clear();
        let Some(name) = rest.strip_suffix(']') else {
            return self.error(n, format!("セクション名が「]」で閉じていない: [{rest}"));
        };
        let name = name.trim().to_ascii_lowercase();
        if self.seen.contains(&name) {
            return self.error(n, format!("セクション [{name}] が2回ある"));
        }
        let section = match name.split_once('.') {
            None if name == "general" => Section::General,
            None if name == "edge" => Section::Edges,
            Some(("edge", edge)) => match Edge::parse(edge) {
                Some(edge) => Section::Edge(edge),
                None => return self.error(n, format!("知らないセクション [{name}]（辺は left / right / top / bottom）")),
            },
            Some(("set", set)) if valid_name(set) => {
                self.config.sets.push(KeySet { name: set.into(), keys: Vec::new() });
                self.set_lines.push((n, false));
                self.layouts.push(SetLayout::Edge);
                self.key_lines.push(Vec::new());
                Section::Set(self.config.sets.len() - 1)
            }
            Some(("profile", profile)) if valid_name(profile) => {
                self.config.profiles.push(Profile { name: profile.into(), process: None, edges: Default::default() });
                self.profile_lines.push(n);
                Section::Profile(self.config.profiles.len() - 1)
            }
            Some(("set" | "profile", _)) => {
                return self.error(n, format!("[{name}] の名前は英数字・-・_ で書く"));
            }
            _ => return self.error(n, format!("知らないセクション [{name}]")),
        };
        self.seen.push(name);
        self.section = section;
    }

    fn general(&mut self, name: &str, value: &str) -> Result<(), String> {
        let g = &mut self.config.general;
        match name {
            "hotkey" if value.eq_ignore_ascii_case("none") => g.hotkey = None,
            "hotkey" => {
                let combo = Combo::parse(value)?;
                if combo.key.is_none() {
                    return Err(format!("hotkey「{value}」に修飾キー以外のキーがない"));
                }
                g.hotkey = Some(combo);
            }
            "key_size" => g.key_size = ranged(name, name, value)?,
            "opacity" => g.opacity = ranged(name, name, value)? as u8,
            "dim_opacity" => g.dim_opacity = ranged(name, name, value)? as u8,
            "latch_ms" => g.latch_ms = ranged(name, name, value)?,
            "latch" => {
                g.latch = match value.to_ascii_lowercase().as_str() {
                    "oneshot" => LatchMode::OneShot,
                    "lock" => LatchMode::Lock,
                    _ => return Err(format!("latch は oneshot か lock（「{value}」）")),
                }
            }
            "pen_presses" => g.pen_presses = boolean(name, value)?,
            "theme" => {
                g.theme = match value.to_ascii_lowercase().as_str() {
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    "auto" => Theme::Auto,
                    _ => return Err(format!("theme は light / dark / auto のどれか（「{value}」）")),
                }
            }
            "hover_ms" => g.hover_ms = ranged(name, name, value)?,
            "auto_hide_ms" => g.auto_hide_ms = ranged(name, name, value)?,
            "slide_ms" => g.slide_ms = ranged(name, name, value)?,
            "accent" => {
                g.accent = Accent::parse(value).ok_or_else(|| format!("accent は auto か #RRGGBB（「{value}」）"))?;
            }
            _ => return Err(format!("[general] に知らない設定「{name}」がある")),
        }
        Ok(())
    }

    /// A setting of [edge]: checked as an edge would read it (columns as
    /// a side edge's, rows as the top's), and kept for `finish`.
    fn common(&mut self, name: &str, value: &str) -> Result<(), String> {
        let edge = if name == "rows" { Edge::Top } else { Edge::Left };
        let mut check = Parser::default();
        check.edge(edge, name, value).map_err(|why| why.replace("[edge.*]", "[edge]"))?;
        self.shared.push((name.into(), value.into()));
        Ok(())
    }

    fn edge(&mut self, edge: Edge, name: &str, value: &str) -> Result<(), String> {
        let e = &mut self.config.edges[edge as usize];
        match name {
            "enabled" => e.enabled = boolean(name, value)?,
            "dock" => e.dock = boolean(name, value)?,
            "gap" => e.gap = ranged(name, name, value)?,
            "monitor" => {
                e.monitor = match value.to_ascii_lowercase().as_str() {
                    "primary" => Monitor::Primary,
                    "cursor" => Monitor::Cursor,
                    _ => Monitor::Number(ranged(name, name, value).map_err(|_| {
                        let r = RANGES.monitor;
                        format!("monitor は primary / cursor / {}〜{} のどれか（「{value}」）", r.min, r.max)
                    })? as u8),
                }
            }
            "columns" | "rows" if (name == "columns") != edge.is_side() => {
                let want = if edge.is_side() { "columns" } else { "rows" };
                return Err(format!("この辺では「{name}」ではなく「{want}」を使う"));
            }
            "columns" | "rows" => e.lines = ranged(name, name, value)? as u8,
            "handle" => e.handle = handle(value)?,
            _ => return Err(format!("[edge.*] に知らない設定「{name}」がある")),
        }
        Ok(())
    }

    fn key(&mut self, n: usize, set: usize, name: &str, value: &str) -> Result<(), String> {
        if name == "layout" {
            return self.layout(n, set, value);
        }
        if name != "key" {
            return Err(format!("[set.*] に書けるのは key と layout だけ（「{name}」）"));
        }
        self.set_lines[set].1 = true;
        let key = key_def(value)?;
        self.key_lines[set].push(n);
        // Only a key that is read checks the sets it names.
        match &key.action {
            Action::Page(Page::Set(set)) => self.refs.push((n, set.clone())),
            Action::Pie(set) => {
                self.refs.push((n, set.clone()));
                self.pies.push((n, set.clone()));
            }
            Action::Layer { set, .. } => self.refs.push((n, set.clone())),
            _ => {}
        }
        self.config.sets[set].keys.push(key);
        Ok(())
    }

    /// A set's `layout`: free, or a KLE file's, read now.
    fn layout(&mut self, n: usize, set: usize, value: &str) -> Result<(), String> {
        if value.eq_ignore_ascii_case("free") {
            self.layouts[set] = SetLayout::Free;
            return Ok(());
        }
        let Some(file) = value.get(..4).filter(|p| p.eq_ignore_ascii_case("kle:")).map(|_| value[4..].trim()) else {
            return Err(format!("layout は free か kle:ファイル名（「{value}」）"));
        };
        if file.is_empty() {
            return Err("layout の kle: のあとにファイル名がない".into());
        }
        let read = match self.read.as_mut() {
            Some(read) => read(file),
            None => Err("ほかのファイルは読めない".to_string()),
        };
        let keys = read
            .map_err(|why| format!("KLE のファイル「{file}」を読めない: {why}"))
            .and_then(|bytes| kle_keys(&bytes).map_err(|why| format!("KLE のファイル「{file}」: {why}")))?;
        self.layouts[set] = SetLayout::Kle(n, keys);
        Ok(())
    }

    fn profile(&mut self, n: usize, profile: usize, name: &str, value: &str) -> Result<(), String> {
        let p = &mut self.config.profiles[profile];
        if name == "process" {
            if value.is_empty() {
                return Err("process が空".into());
            }
            p.process = Some(value.to_ascii_lowercase());
            return Ok(());
        }
        let Some(edge) = Edge::parse(name) else {
            return Err(format!("[profile.*] に知らない設定「{name}」がある"));
        };
        let set = value.to_ascii_lowercase();
        if set != "none" {
            p.edges[edge as usize] = Some(set.clone());
            self.refs.push((n, set));
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Config, Vec<Error>> {
        // [edge]'s settings, to each edge that does not write its own:
        // columns to the side edges, rows to the top and bottom.
        for (name, value) in std::mem::take(&mut self.shared) {
            let lines = matches!(name.as_str(), "columns" | "rows");
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                let given = if lines { "lines" } else { name.as_str() };
                if lines && (name == "columns") != edge.is_side() || self.edge_given[edge as usize].iter().any(|g| g == given) {
                    continue;
                }
                // Checked when read: it reads.
                let _ = self.edge(edge, &name, &value);
            }
        }
        let kle: Vec<bool> = self.layouts.iter().map(|l| matches!(l, SetLayout::Kle(..))).collect();
        self.lay_out_sets();
        for (i, (set, &(line, had_keys))) in self.config.sets.iter().zip(&self.set_lines).enumerate() {
            // A KLE layout has keys of its own.
            if !had_keys && !kle[i] {
                self.errors.push(Error { line, reason: format!("[set.{}] にキーが1つもない", set.name) });
            }
        }
        for (line, name) in std::mem::take(&mut self.refs) {
            if self.config.set(&name).is_none() {
                self.error(line, format!("キーセット「{name}」が定義されていない"));
            }
        }
        for (line, name) in std::mem::take(&mut self.pies) {
            let Some(set) = self.config.set(&name) else { continue };
            for reason in pie_problems(set) {
                self.errors.push(Error { line, reason });
            }
        }
        for (profile, &line) in self.config.profiles.iter().zip(&self.profile_lines) {
            if profile.process.is_none() && profile.name != "default" {
                let reason = format!("[profile.{}] に process がない", profile.name);
                self.errors.push(Error { line, reason });
            }
        }
        if self.config.profile("default").is_none() {
            self.error(0, "[profile.default] がない");
        }
        if self.errors.is_empty() {
            return Ok(self.config);
        }
        self.errors.sort_by_key(|e| e.line);
        Err(self.errors)
    }
}

impl Parser<'_> {
    /// Give each set's keys their places, as its layout says: x, y, w and
    /// h only in a free layout, and there on every key; a KLE layout's
    /// keys to its key lines in order, the rest sending nothing.
    fn lay_out_sets(&mut self) {
        for (i, layout) in std::mem::take(&mut self.layouts).into_iter().enumerate() {
            let lines = std::mem::take(&mut self.key_lines[i]);
            let set = &mut self.config.sets[i];
            let placed: Vec<bool> = set.keys.iter().map(|k| k.place.is_some()).collect();
            let lines_where = |want: bool| lines.iter().zip(&placed).filter(move |(_, p)| **p == want).map(|(l, _)| *l);
            match layout {
                SetLayout::Edge => {
                    for line in lines_where(true) {
                        self.errors.push(Error { line, reason: "x・y・w・h は layout = free のキーセットで書く".into() });
                    }
                }
                SetLayout::Free => {
                    for line in lines_where(false) {
                        self.errors.push(Error { line, reason: "layout = free のキーには x と y を書く".into() });
                    }
                }
                SetLayout::Kle(layout_line, kle) => {
                    for line in lines_where(true) {
                        self.errors.push(Error { line, reason: "layout = kle: のキーの位置は KLE のファイルから読む".into() });
                    }
                    if let Some(&line) = lines.get(kle.len()) {
                        let reason = format!("KLE の配列のキーは {} 個（{layout_line} 行目の layout）で、key 行が多い", kle.len());
                        self.errors.push(Error { line, reason });
                    }
                    for (k, (place, label)) in kle.into_iter().enumerate() {
                        match set.keys.get_mut(k) {
                            Some(key) => key.place = Some(place),
                            None => set.keys.push(KeyDef { label, action: Action::None, width: 1, place: Some(place) }),
                        }
                    }
                }
            }
        }
    }
}

/// The keys of a KLE file's content, or why not. The settings window
/// checks a file with this before taking it in.
pub fn kle_keys(bytes: &[u8]) -> Result<Vec<KleKey>, String> {
    if bytes.len() > MAX_BYTES {
        return Err(format!("ファイルが大きすぎる（{} KiB まで）", MAX_BYTES / 1024));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| "UTF-8 ではない文字がある".to_string())?;
    kle::parse(text)
}

/// Why a pie key cannot open a menu of key set `set`, in the parser's
/// words: none if it can (spec 4.6).
pub fn pie_problems(set: &KeySet) -> Vec<String> {
    let mut problems = Vec::new();
    let count = set.keys.len();
    if !PIE_ITEMS.contains(&count) {
        let (min, max) = (PIE_ITEMS.start(), PIE_ITEMS.end());
        problems.push(format!("pie の項目は{min}〜{max}個（[set.{}] には {count} 個）", set.name));
    }
    for key in &set.keys {
        if !key.action.is_pie_item() {
            problems.push(format!("pie の項目に {} キーは使えない（[set.{}] の「{}」）", key.action.kind(), set.name, key.label));
        }
    }
    problems
}

/// The key `value` writes after `key =`, `label | kind | content |
/// options`, as the parser reads it; or why not, in its words. Whether the
/// key sets a page or pie key names are there is not asked: that is the
/// whole file's to say. The settings window checks a key with this before
/// writing it.
pub fn key_def(value: &str) -> Result<KeyDef, String> {
    let fields: Vec<&str> = value.split('|').map(str::trim).collect();
    if fields.len() < 3 {
        return Err("key は「ラベル | 種類 | 内容」の3つの欄が要る".into());
    }
    if fields.len() > 4 {
        return Err("key の欄が多すぎる（「ラベル | 種類 | 内容 | オプション」まで）".into());
    }
    let (label, kind, content) = (fields[0], fields[1].to_ascii_lowercase(), fields[2]);
    if label.is_empty() {
        return Err("key のラベルの欄が空".into());
    }
    if content.is_empty() {
        return Err("key の内容の欄が空".into());
    }
    let mut options: Vec<(String, &str)> = Vec::new();
    for option in fields.get(3).iter().flat_map(|o| o.split(',')).map(str::trim).filter(|o| !o.is_empty()) {
        let Some((k, v)) = option.split_once('=') else {
            return Err(format!("オプションは「名前=値」で書く（「{option}」）"));
        };
        options.push((k.trim().to_ascii_lowercase(), v.trim()));
    }
    let mut take = |want: &str| options.iter().position(|(k, _)| k == want).map(|i| options.remove(i).1);
    let width = take("width").map(|v| number("width", v, 1, 3)).transpose()?.unwrap_or(1) as u8;
    let mut place_part = |name: &str| {
        let (min, max) = if matches!(name, "x" | "y") { (0, 4 * MAX_PLACE) } else { (1, 4 * MAX_SIZE) };
        take(name).map(|v| quarters(name, v, min, max)).transpose()
    };
    let (x, y, w, h) = (place_part("x")?, place_part("y")?, place_part("w")?, place_part("h")?);
    let place = match (x, y) {
        (Some(x), Some(y)) => Some(Place { x, y, w: w.unwrap_or(4), h: h.unwrap_or(4), tilt: None }),
        (None, None) if w.is_none() && h.is_none() => None,
        _ => return Err("キーの位置は x と y を両方書く".into()),
    };
    let action = match kind.as_str() {
        "modifier" => match Key::parse(content) {
            Some(key) if key.modifier().is_some() => Action::Modifier(key),
            _ => return Err(format!("modifier の内容は Ctrl / Shift / Alt / Win のどれか（「{content}」）")),
        },
        "hold" => Action::Hold(Key::parse(content).ok_or_else(|| format!("知らないキー名「{content}」"))?),
        "shortcut" => Action::Shortcut(Combo::parse(content)?),
        "repeat" => {
            let interval_ms = take("interval").map(|v| number("interval", v, 20, 1000)).transpose()?.unwrap_or(50);
            Action::Repeat { combo: Combo::parse(content)?, interval_ms }
        }
        "macro" => {
            let mut steps = Vec::new();
            for step in content.split(',').map(str::trim) {
                let wait = step.get(..5).filter(|w| w.eq_ignore_ascii_case("wait ")).map(|_| step[5..].trim());
                steps.push(match wait {
                    Some(ms) => Step::Wait(number("wait", ms, 0, 10_000)?),
                    None if step.is_empty() => return Err("macro に空の手順がある".into()),
                    None => Step::Keys(Combo::parse(step)?),
                });
            }
            Action::Macro(steps)
        }
        "text" => Action::Text(content.into()),
        "run" => {
            if let Some(rest) = content.strip_prefix('"') {
                match rest.split_once('"') {
                    None => return Err("run の「\"」が閉じていない".into()),
                    Some(("", _)) => return Err("run の「\"」の中が空".into()),
                    Some(_) => {}
                }
            }
            Action::Run(content.into())
        }
        "page" => Action::Page(match content.to_ascii_lowercase().as_str() {
            "next" => Page::Next,
            "prev" => Page::Prev,
            set => Page::Set(set.into()),
        }),
        "pie" => Action::Pie(content.to_ascii_lowercase()),
        // A key of a KLE layout past the key lines, written out (Issue #109).
        "none" => Action::None,
        "layer" => {
            let toggle = match take("mode").map(str::to_ascii_lowercase).as_deref() {
                None | Some("hold") => false,
                Some("toggle") => true,
                Some(mode) => return Err(format!("layer の mode は hold か toggle（「{mode}」）")),
            };
            Action::Layer { set: content.to_ascii_lowercase(), toggle }
        }
        _ => return Err(format!("知らないキーの種類「{}」", fields[1])),
    };
    if let Some((k, _)) = options.first() {
        return Err(format!("{kind} キーに知らないオプション「{k}」がある"));
    }
    Ok(KeyDef { label: label.into(), action, width, place })
}

/// Furthest a free layout's key sits from its top left, and largest a key
/// is, in keys.
pub const MAX_PLACE: i32 = 40;
pub const MAX_SIZE: i32 = 8;

/// `value` in quarters of a key: a number of keys in steps of 0.25 (1,
/// 1.5, 0.25, 2.75), from `min` to `max` quarters.
fn quarters(name: &str, value: &str, min: i32, max: i32) -> Result<i32, String> {
    let (whole, part) = value.split_once('.').unwrap_or((value, ""));
    let digits = !whole.is_empty() && whole.len() <= 3 && whole.bytes().all(|b| b.is_ascii_digit());
    let part = match part {
        "" | "0" | "00" => Some(0),
        "25" => Some(1),
        "5" | "50" => Some(2),
        "75" => Some(3),
        _ => None,
    };
    let q = match (digits, part) {
        (true, Some(part)) => Some(whole.parse::<i32>().unwrap_or(i32::MAX / 8) * 4 + part),
        _ => None,
    };
    q.filter(|q| (min..=max).contains(q))
        .ok_or_else(|| format!("{name} は {}〜{} の 0.25 刻みの数（「{value}」）", show_quarters(min), show_quarters(max)))
}

/// The place key line `value` writes, as `key_def` reads it, whatever the
/// rest of the line: none without x and y. The settings window draws and
/// edits a free layout by it.
pub fn key_place(value: &str) -> Option<Place> {
    let options = value.split('|').nth(3)?;
    let get = |want: &str, min: i32, max: i32| -> Option<Option<i32>> {
        let (_, v) = options.split(',').filter_map(|o| o.split_once('=')).find(|(k, _)| k.trim().eq_ignore_ascii_case(want))?;
        Some(quarters(want, v.trim(), min, max).ok())
    };
    let (x, y) = (get("x", 0, 4 * MAX_PLACE)??, get("y", 0, 4 * MAX_PLACE)??);
    let size = |name: &str| get(name, 1, 4 * MAX_SIZE).unwrap_or(Some(4));
    Some(Place { x, y, w: size("w")?, h: size("h")?, tilt: None })
}

/// `q` quarters as a number of keys, as the file writes it.
pub fn show_quarters(q: i32) -> String {
    let whole = q / 4;
    match q % 4 {
        0 => format!("{whole}"),
        1 => format!("{whole}.25"),
        2 => format!("{whole}.5"),
        _ => format!("{whole}.75"),
    }
}

/// The program (or file, folder or URL) a run key starts, and the arguments
/// it is given: up to the first space, or between quotes when it has spaces
/// of its own. The parser has made sure any opening quote is closed.
pub fn run_target(command: &str) -> (&str, &str) {
    let (file, rest) = match command.strip_prefix('"').and_then(|r| r.split_once('"')) {
        Some(quoted) => quoted,
        None => command.split_once(char::is_whitespace).unwrap_or((command, "")),
    };
    (file, rest.trim_start())
}

/// A handle as `handle =` takes it: none, or `width, length%[, position%]`.
pub fn handle(value: &str) -> Result<Option<Handle>, String> {
    let name = "handle";
    if value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(format!("handle は「幅, 長さ%[, 位置%]」で書く（「{value}」）"));
    }
    let percent = |p: &str, part| ranged(part, name, p.strip_suffix('%').unwrap_or(p).trim());
    let mut handle = EdgeConfig::default().handle.unwrap_or(Handle { width: 6, length: 40, position: 50 });
    handle.width = ranged("handle.width", name, parts[0].strip_suffix("px").unwrap_or(parts[0]).trim())? as u8;
    handle.length = percent(parts[1], "handle.length")? as u8;
    if let Some(p) = parts.get(2) {
        handle.position = percent(p, "handle.position")? as u8;
    }
    Ok(Some(handle))
}

/// Whether `value` reads as setting `name` of `section` (`general`, or an
/// edge's: `edge.left` and so on) as the parser reads it there; if not,
/// why not, in the parser's words. The settings window checks a field
/// with this before writing it.
pub fn check(section: &str, name: &str, value: &str) -> Result<(), String> {
    let mut parser = Parser::default();
    let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
    match section.split_once('.') {
        None if section.eq_ignore_ascii_case("general") => parser.general(&name, value),
        None if section.eq_ignore_ascii_case("edge") => parser.common(&name, value),
        Some((kind, edge)) if kind.eq_ignore_ascii_case("edge") => match Edge::parse(edge) {
            Some(edge) => parser.edge(edge, &name, value),
            None => Err(format!("知らないセクション [{section}]")),
        },
        _ => Err(format!("[{section}] の設定はここでは確かめない")),
    }
}

/// Whether `name` can name a key set or a profile: `[set.name]`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// What a number setting takes: `min` to `max`, and 0 for off with `off`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub min: u32,
    pub max: u32,
    pub off: bool,
}

const fn range(min: u32, max: u32, off: bool) -> Range {
    Range { min, max, off }
}

/// The ranges of the number settings, as the parser takes them. The
/// settings window shows them beside its fields.
pub struct Ranges {
    pub key_size: Range,
    pub opacity: Range,
    pub latch_ms: Range,
    pub hover_ms: Range,
    pub auto_hide_ms: Range,
    pub slide_ms: Range,
    pub monitor: Range,
    /// columns or rows.
    pub lines: Range,
    pub handle_width: Range,
    pub handle_length: Range,
    pub handle_position: Range,
    pub gap: Range,
}

pub const RANGES: Ranges = Ranges {
    key_size: range(24, 200, false),
    opacity: range(10, 100, false),
    latch_ms: range(50, 2000, false),
    hover_ms: range(100, 5000, true),
    auto_hide_ms: range(1000, 3_600_000, true),
    slide_ms: range(50, 500, true),
    monitor: range(1, 16, false),
    lines: range(1, 3, false),
    handle_width: range(4, 8, false),
    handle_length: range(1, 100, false),
    handle_position: range(0, 100, false),
    gap: range(0, 64, false),
};

/// The range of number setting `name`: `opacity`, `columns`, or a part of
/// the handle, `handle.width`, `handle.length` or `handle.position`.
pub fn range_of(name: &str) -> Option<Range> {
    let r = &RANGES;
    Some(match name {
        "key_size" => r.key_size,
        "opacity" | "dim_opacity" => r.opacity,
        "latch_ms" => r.latch_ms,
        "hover_ms" => r.hover_ms,
        "auto_hide_ms" => r.auto_hide_ms,
        "slide_ms" => r.slide_ms,
        "monitor" => r.monitor,
        "columns" | "rows" => r.lines,
        "handle.width" => r.handle_width,
        "handle.length" => r.handle_length,
        "handle.position" => r.handle_position,
        "gap" => r.gap,
        _ => return None,
    })
}

/// `value` as number setting `key` of `range_of` takes it, said to be
/// `name`'s if not.
fn ranged(key: &str, name: &str, value: &str) -> Result<u32, String> {
    match range_of(key) {
        Some(Range { min, max, off: true }) => off_or_number(name, value, min, max),
        Some(Range { min, max, off: false }) => number(name, value, min, max),
        None => Err(format!("{name} は数ではない")),
    }
}

/// What setting `name` of `section` (`general`, or an edge's) is when the
/// file does not have it, written as the file would have it.
pub fn default_value(section: &str, name: &str) -> Option<String> {
    let name = name.to_ascii_lowercase();
    if section.eq_ignore_ascii_case("general") {
        let g = General::default();
        let on = |b: bool| if b { "true" } else { "false" }.to_string();
        return Some(match name.as_str() {
            "hotkey" => g.hotkey.map_or_else(|| "none".into(), |c| c.to_string()),
            "key_size" => g.key_size.to_string(),
            "opacity" => g.opacity.to_string(),
            "dim_opacity" => g.dim_opacity.to_string(),
            "latch_ms" => g.latch_ms.to_string(),
            "latch" => match g.latch {
                LatchMode::OneShot => "oneshot",
                LatchMode::Lock => "lock",
            }
            .into(),
            "pen_presses" => on(g.pen_presses),
            "theme" => match g.theme {
                Theme::Light => "light",
                Theme::Dark => "dark",
                Theme::Auto => "auto",
            }
            .into(),
            "accent" => match g.accent {
                Accent::Auto => "auto".into(),
                Accent::Rgb(r, g, b) => format!("#{r:02X}{g:02X}{b:02X}"),
            },
            "hover_ms" => g.hover_ms.to_string(),
            "auto_hide_ms" => g.auto_hide_ms.to_string(),
            "slide_ms" => g.slide_ms.to_string(),
            _ => return None,
        });
    }
    // [edge], every edge's, has an edge's defaults.
    let (kind, edge) = section.split_once('.').unwrap_or((section, "left"));
    if !kind.eq_ignore_ascii_case("edge") || Edge::parse(edge).is_none() {
        return None;
    }
    let e = Edge::parse(edge).map_or_else(EdgeConfig::default, EdgeConfig::of);
    Some(match name.as_str() {
        "enabled" => e.enabled.to_string(),
        "dock" => e.dock.to_string(),
        "gap" => e.gap.to_string(),
        "monitor" => match e.monitor {
            Monitor::Primary => "primary".into(),
            Monitor::Cursor => "cursor".into(),
            Monitor::Number(n) => n.to_string(),
        },
        "columns" | "rows" => e.lines.to_string(),
        "handle" => match e.handle {
            Some(h) => format!("{}, {}%, {}%", h.width, h.length, h.position),
            None => "none".into(),
        },
        _ => return None,
    })
}

fn number(name: &str, value: &str, min: u32, max: u32) -> Result<u32, String> {
    // Digits only: u32's parser would also take a leading "+".
    let digits = !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    match value.parse::<u32>() {
        Ok(v) if digits && (min..=max).contains(&v) => Ok(v),
        _ => Err(format!("{name} は {min}〜{max} の整数（「{value}」）")),
    }
}

/// 0 for off, or a number from `min` to `max`, written as `number` takes it.
fn off_or_number(name: &str, value: &str, min: u32, max: u32) -> Result<u32, String> {
    match number(name, value, 0, max) {
        Ok(v) if v == 0 || v >= min => Ok(v),
        _ => Err(format!("{name} は 0（オフ）か {min}〜{max} の整数（「{value}」）")),
    }
}

fn boolean(name: &str, value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{name} は true か false（「{value}」）")),
    }
}

/// The keys used when there is no usable settings file: the M0 column, each
/// held while pressed.
pub fn fallback_keys() -> Vec<KeyDef> {
    let actions = [
        Action::Hold(Key::Esc),
        Action::Modifier(Key::Ctrl),
        Action::Modifier(Key::Shift),
        Action::Modifier(Key::Alt),
        Action::Hold(Key::Space),
    ];
    crate::keys::M0_LABELS
        .iter()
        .zip(actions)
        .map(|(label, action)| KeyDef { label: (*label).into(), action, width: 1, place: None })
        .collect()
}

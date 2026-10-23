//! The settings file (edgepad.ini, spec chapter 7).
//!
//! INI-like text in UTF-8: `[section]` headers, `name = value` lines, and
//! comments from a `;` at the start of a line or after a space to the end of
//! the line. Names of sections, settings, keys and kinds ignore case. Every
//! mistake is collected with its line number, so one read reports them all.

use crate::keys::{Combo, Key};
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
    /// Opacity of the dimmed panel, percent.
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
}

impl Default for General {
    fn default() -> Self {
        General {
            hotkey: None,
            key_size: 56,
            opacity: 90,
            dim_opacity: 35,
            latch_ms: 250,
            latch: LatchMode::OneShot,
            pen_presses: false,
            theme: Theme::Auto,
            accent: Accent::Auto,
            hover_ms: 0,
            auto_hide_ms: 0,
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
    fn parse(value: &str) -> Option<Accent> {
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
    fn parse(name: &str) -> Option<Edge> {
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
}

impl Default for EdgeConfig {
    fn default() -> Self {
        EdgeConfig {
            enabled: true,
            monitor: Monitor::Primary,
            lines: 1,
            handle: Some(Handle { width: 6, length: 40, position: 50 }),
            dock: false,
        }
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
    pub keys: Vec<KeyDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyDef {
    pub label: String,
    pub action: Action,
    /// In keys: 2 is twice as wide as a plain key.
    pub width: u8,
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
}

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

/// Read a settings file.
pub fn parse(bytes: &[u8]) -> Result<Config, Vec<Error>> {
    if bytes.len() > MAX_BYTES {
        return Err(vec![Error { line: 0, reason: format!("ファイルが大きすぎる（{} KiB まで）", MAX_BYTES / 1024) }]);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => {
            let line = 1 + bytes[..e.valid_up_to()].iter().filter(|&&b| b == b'\n').count();
            return Err(vec![Error { line, reason: "UTF-8 ではない文字がある".into() }]);
        }
    };
    let mut parser = Parser::default();
    for (i, line) in text.split('\n').enumerate() {
        parser.line(i + 1, strip_comment(line.trim_end_matches('\r')).trim());
    }
    parser.finish()
}

/// `line` without its comment: from a `;` at the start or after whitespace.
fn strip_comment(line: &str) -> &str {
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
    Edge(Edge),
    Set(usize),
    Profile(usize),
}

struct Parser {
    config: Config,
    section: Section,
    /// Section names seen so far (lower case).
    seen: Vec<String>,
    /// Settings already given in the current section (lower case).
    given: Vec<String>,
    /// Header line of each set, and whether it had any key lines.
    set_lines: Vec<(usize, bool)>,
    /// Header line of each profile.
    profile_lines: Vec<usize>,
    /// Key set names to check once all sets are known: (line, name).
    refs: Vec<(usize, String)>,
    errors: Vec<Error>,
}

impl Default for Parser {
    fn default() -> Self {
        Parser {
            config: Config {
                general: General::default(),
                edges: [EdgeConfig::default(); 4],
                sets: Vec::new(),
                profiles: Vec::new(),
            },
            section: Section::None { reported: false },
            seen: Vec::new(),
            given: Vec::new(),
            set_lines: Vec::new(),
            profile_lines: Vec::new(),
            refs: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl Parser {
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
            Section::Edge(edge) => self.edge(edge, &name, value),
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
            Some(("edge", edge)) => match Edge::parse(edge) {
                Some(edge) => Section::Edge(edge),
                None => return self.error(n, format!("知らないセクション [{name}]（辺は left / right / top / bottom）")),
            },
            Some(("set", set)) if valid_name(set) => {
                self.config.sets.push(KeySet { name: set.into(), keys: Vec::new() });
                self.set_lines.push((n, false));
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
            "key_size" => g.key_size = number(name, value, 24, 200)?,
            "opacity" => g.opacity = number(name, value, 10, 100)? as u8,
            "dim_opacity" => g.dim_opacity = number(name, value, 10, 100)? as u8,
            "latch_ms" => g.latch_ms = number(name, value, 50, 2000)?,
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
            "hover_ms" => g.hover_ms = off_or_number(name, value, 100, 5000)?,
            "auto_hide_ms" => g.auto_hide_ms = off_or_number(name, value, 1000, 3_600_000)?,
            "accent" => {
                g.accent = Accent::parse(value).ok_or_else(|| format!("accent は auto か #RRGGBB（「{value}」）"))?;
            }
            _ => return Err(format!("[general] に知らない設定「{name}」がある")),
        }
        Ok(())
    }

    fn edge(&mut self, edge: Edge, name: &str, value: &str) -> Result<(), String> {
        let e = &mut self.config.edges[edge as usize];
        match name {
            "enabled" => e.enabled = boolean(name, value)?,
            "dock" => e.dock = boolean(name, value)?,
            "monitor" => {
                e.monitor = match value.to_ascii_lowercase().as_str() {
                    "primary" => Monitor::Primary,
                    "cursor" => Monitor::Cursor,
                    _ => Monitor::Number(number(name, value, 1, 16).map_err(|_| {
                        format!("monitor は primary / cursor / 1〜16 のどれか（「{value}」）")
                    })? as u8),
                }
            }
            "columns" | "rows" if (name == "columns") != edge.is_side() => {
                let want = if edge.is_side() { "columns" } else { "rows" };
                return Err(format!("この辺では「{name}」ではなく「{want}」を使う"));
            }
            "columns" | "rows" => e.lines = number(name, value, 1, 3)? as u8,
            "handle" if value.eq_ignore_ascii_case("none") => e.handle = None,
            "handle" => {
                let parts: Vec<&str> = value.split(',').map(str::trim).collect();
                if !(2..=3).contains(&parts.len()) {
                    return Err(format!("handle は「幅, 長さ%[, 位置%]」で書く（「{value}」）"));
                }
                let percent = |p: &str, min| number(name, p.strip_suffix('%').unwrap_or(p).trim(), min, 100);
                let mut handle = Handle { width: 6, length: 40, position: 50 };
                handle.width = number(name, parts[0].strip_suffix("px").unwrap_or(parts[0]).trim(), 4, 8)? as u8;
                handle.length = percent(parts[1], 1)? as u8;
                if let Some(p) = parts.get(2) {
                    handle.position = percent(p, 0)? as u8;
                }
                e.handle = Some(handle);
            }
            _ => return Err(format!("[edge.*] に知らない設定「{name}」がある")),
        }
        Ok(())
    }

    fn key(&mut self, n: usize, set: usize, name: &str, value: &str) -> Result<(), String> {
        if name != "key" {
            return Err(format!("[set.*] に書けるのは key だけ（「{name}」）"));
        }
        self.set_lines[set].1 = true;
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
                set => {
                    self.refs.push((n, set.into()));
                    Page::Set(set.into())
                }
            }),
            _ => return Err(format!("知らないキーの種類「{}」", fields[1])),
        };
        if let Some((k, _)) = options.first() {
            return Err(format!("{kind} キーに知らないオプション「{k}」がある"));
        }
        self.config.sets[set].keys.push(KeyDef { label: label.into(), action, width });
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
        for (set, &(line, had_keys)) in self.config.sets.iter().zip(&self.set_lines) {
            if !had_keys {
                self.errors.push(Error { line, reason: format!("[set.{}] にキーが1つもない", set.name) });
            }
        }
        for (line, name) in std::mem::take(&mut self.refs) {
            if self.config.set(&name).is_none() {
                self.error(line, format!("キーセット「{name}」が定義されていない"));
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

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
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
        .map(|(label, action)| KeyDef { label: (*label).into(), action, width: 1 })
        .collect()
}

//! The キー配置 and キーマッピング pages (spec 7.1): the key sets, [set.*],
//! and their keys. キー配置 lays the panel's shape out; キーマッピング says
//! what each key sends (Issue #109). The key set and the key chosen are the
//! two pages' both.
//!
//! Along the top, the key set edited, chosen from a drop-down list; on
//! キー配置, with 追加, 名前の変更 and 削除 beside it, and under it where it
//! is used. On the left, a sample of its panel, in the panel's colours and
//! shape, laid out and turned as on the first edge that shows it, against
//! that side; a key pressed there is chosen. On キー配置, the buttons under
//! it add, remove and move keys, a free layout's keys are dragged about on
//! it, and on the right are how the set is laid out (並べ方) and the chosen
//! key's width, or its place. On キーマッピング, on the right, the chosen
//! key's label, its kind of the ten, and the fields of that kind alone.
//!
//! The keys a key sends are chosen, not written: modifiers turned on and
//! off, and a key from the list of a group; or captured, the next keys
//! pressed taken as they come (`Capture`), as the 操作 page's hotkey is too.
//!
//! A change rewrites the key's line alone, in the window's copy of the
//! file, and only once the parser reads the key (`config::key_def`): else
//! the field shows it wrong and the status line says why, as on the other
//! pages. A field rewritten keeps the spaces around it, and a shorter value
//! is padded to the old one's length, so columns lined up by hand stay so.
//! Whether the key sets a page or pie key names are there, and a pie's
//! items fit it, is the whole file's to say, on saving; the pie's fields
//! tell of its items at once.

use std::cell::{Cell, RefCell};
use std::mem::zeroed;
use std::ptr::{null, null_mut};

use edgepad_core::config::{self, Accent, Edge, KeySet, Theme};
use edgepad_core::document::Document;
use edgepad_core::keys::{Combo, Key, Mods};
use edgepad_core::layout::{self, Place};
use edgepad_core::theme::Palette;
use windows_sys::Win32::Foundation::{FreeLibrary, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_CONTROL, VK_DOWN, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_UP,
};
use windows_sys::Win32::UI::Controls::Dialogs::{
    OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use super::fields::{EM_GETMODIFY, EM_SETMODIFY, EM_SETREADONLY, EM_SETSEL};
use super::{Editor, MARGIN, PAINT, mark_unsaved, scale, system_proc, tell, wide, window_dpi, with_editor};
use crate::colors::{self, colorref};
use crate::{panel, send};

/// The pages, of `super::PAGES`: the keys' shape, and what they send.
pub(super) const PAGE: usize = 0;
pub(super) const MAP_PAGE: usize = 1;

/// Whether page `page` is one of these.
pub(super) fn is_page(page: usize) -> bool {
    page == PAGE || page == MAP_PAGE
}

/// Control ids: the key sets' row; the sample and its buttons; the chosen
/// key's label, width and kind; the keys it sends; the kinds' own fields;
/// and the titles beside them. The tests find the parts by these.
const SET: usize = 1000;
const NAME: usize = 1001;
const ADD_SET: usize = 1002;
const RENAME: usize = 1003;
const REMOVE_SET: usize = 1004;
const USED: usize = 1005;
const SAMPLE: usize = 1006;
const ADD_KEY: usize = 1007;
const REMOVE_KEY: usize = 1008;
const KEY_UP: usize = 1009;
const KEY_DOWN: usize = 1010;
const LABEL: usize = 1011;
const CUT: usize = 1012;
const ABOUT: usize = 1013;
const WIDTH: usize = 1014;
const KINDS: usize = 1020;
const MODS: usize = 1030;
const GROUPS: usize = 1040;
const KEYS: usize = 1050;
const CAPTURE: usize = 1051;
const SENDS: usize = 1052;
const INTERVAL: usize = 1053;
const STEPS: usize = 1054;
const STEP_KEY: usize = 1055;
const STEP_WAIT: usize = 1056;
const STEP_REMOVE: usize = 1057;
const STEP_UP: usize = 1058;
const STEP_DOWN: usize = 1059;
const WAIT: usize = 1060;
const CONTENT: usize = 1061;
const TARGET: usize = 1062;
const PIE: usize = 1063;
const PIE_FIGURE: usize = 1064;
const PIE_NOTE: usize = 1065;
const KLE: usize = 1066;
const LAYER_SET: usize = 1067;
const LAYER_HOLD: usize = 1068;
const LAYER_TOGGLE: usize = 1069;
/// 並べ方; a free key's x, y, w and h; and what is said beside them.
const ARRANGE: usize = 1070;
const PLACE: usize = 1071;
const KLE_NOTE: usize = 1075;
const TITLE_SET: usize = 1100;
const TITLE_LABEL: usize = 1101;
const TITLE_WIDTH: usize = 1102;
const WIDTH_NOTE: usize = 1103;
const TITLE_SENDS: usize = 1104;
const TITLE_INTERVAL: usize = 1105;
const INTERVAL_NOTE: usize = 1106;
const TITLE_STEPS: usize = 1107;
const TITLE_WAIT: usize = 1108;
const WAIT_NOTE: usize = 1109;
const TITLE_CONTENT: usize = 1110;
const TITLE_TARGET: usize = 1111;
const TITLE_PIE: usize = 1112;
const TITLE_LAYER: usize = 1113;
const TITLE_ARRANGE: usize = 1114;
const TITLE_PLACE: usize = 1115;
const PLACE_NOTE: usize = 1119;

/// The sample's and the pie figure's window classes.
pub(super) const SAMPLE_CLASS: *const u16 = w!("EdgePadSettingsSample");
pub(super) const PIE_CLASS: *const u16 = w!("EdgePadSettingsPie");

/// The kinds, as the file writes them and as their buttons name them, and
/// what each does, in a line.
const KIND_NAMES: [&str; 10] = ["shortcut", "modifier", "hold", "repeat", "macro", "text", "run", "page", "pie", "layer"];
const KIND_LABELS: [&str; 10] = ["ショートカット", "修飾キー", "押している間", "連打", "マクロ", "文字", "起動", "ページ", "パイ", "レイヤー"];
const ABOUTS: [&str; 10] = [
    "離した時に 1 回送ります。ラッチ中の修飾キーと合わせて送ります",
    "押している間だけ押します。短くタップするとラッチします",
    "押している間だけ押します。ラッチはしません",
    "押している間、間隔ごとにくり返し送ります",
    "キー操作と待ち時間の手順を、順に送ります",
    "文字列をそのまま入力します",
    "プログラムやファイル、URL を開きます",
    "パネルのページか、表示するキーセットを切り替えます",
    "押した位置に、別のキーセットを項目にしたパイメニューを開きます",
    "押している間か、押すたびに、パネルを別のキーセットに切り替えます",
];
const SHORTCUT: usize = 0;
const MODIFIER: usize = 1;
const HOLD: usize = 2;
const REPEAT: usize = 3;
const MACRO: usize = 4;
const TEXT: usize = 5;
const RUN: usize = 6;
const PAGE_KIND: usize = 7;
const PIE_KIND: usize = 8;
const LAYER_KIND: usize = 9;

const MOD_KEYS: [(Key, &str); 4] = [(Key::Ctrl, "Ctrl"), (Key::Shift, "Shift"), (Key::Alt, "Alt"), (Key::Win, "Win")];
const GROUP_NAMES: [&str; 5] = ["英数字", "F1〜F24", "移動・編集", "記号", "テンキー"];
/// The page key's targets before the key sets.
const PAGE_TURNS: [(&str, &str); 2] = [("next", "次のページ"), ("prev", "前のページ")];
const EDGE_NAMES: [(&str, &str); 4] = [("left", "左辺"), ("right", "右辺"), ("top", "上辺"), ("bottom", "下辺")];
/// The ways a set is laid out, as 並べ方 names them, before a KLE file's.
const ARRANGEMENTS: [&str; 2] = ["辺に沿って並べる", "自由配列"];
/// A key's place, as the file writes it.
const PLACE_NAMES: [&str; 4] = ["x", "y", "w", "h"];

/// What a key gets when it is added, and a step of each kind.
const NEW_KEY: &str = "新しいキー | shortcut | A";
const NEW_STEP: &str = "A";
const NEW_WAIT: &str = "wait 100";

/// Sizes in logical pixels (96 DPI).
const ROW_H: i32 = 26;
const PITCH: i32 = 32;
const SAMPLE_W: i32 = 200;
/// キー配置's fields, right of its sample.
const SHAPE_W: i32 = 280;
/// How near a chosen key's right or bottom end the mouse takes that end.
const END_ZONE: i32 = 6;
/// The sample's panel, inset from its corner, and its largest keys.
const INSET: i32 = 8;
const SAMPLE_KEY: i32 = 44;
const LIST_H: i32 = 200;
const TITLE_W: i32 = 64;
/// The space beside a kind's or a group's name on its button.
const BUTTON_PAD: i32 = 10;

#[derive(Clone, Copy, PartialEq)]
enum Class {
    /// A Static: a title, a note, or what is said.
    Text,
    Edit,
    Button,
    /// A button that stays in when chosen: a kind, a modifier, a group.
    Toggle,
    /// A drop-down list.
    Combo,
    List,
    /// The key list: in columns.
    Keys,
    Sample,
    Figure,
}

use Class::*;

/// Every part, in the order the Tab key follows.
const PARTS: &[(usize, Class, &str)] = &[
    (TITLE_SET, Text, "キーセット"),
    (SET, Combo, ""),
    (NAME, Edit, ""),
    (ADD_SET, Button, "追加"),
    (RENAME, Button, "名前の変更"),
    (REMOVE_SET, Button, "削除"),
    (KLE, Button, "KLE から読み込む"),
    (USED, Text, ""),
    (SAMPLE, Sample, ""),
    (ADD_KEY, Button, "追加"),
    (REMOVE_KEY, Button, "削除"),
    (KEY_UP, Button, "↑"),
    (KEY_DOWN, Button, "↓"),
    (TITLE_ARRANGE, Text, "並べ方"),
    (ARRANGE, Combo, ""),
    (TITLE_WIDTH, Text, "キーの幅"),
    (WIDTH, Edit, ""),
    (WIDTH_NOTE, Text, "1〜3"),
    (TITLE_PLACE, Text, "x"),
    (PLACE, Edit, ""),
    (TITLE_PLACE + 1, Text, "y"),
    (PLACE + 1, Edit, ""),
    (TITLE_PLACE + 2, Text, "w"),
    (PLACE + 2, Edit, ""),
    (TITLE_PLACE + 3, Text, "h"),
    (PLACE + 3, Edit, ""),
    (PLACE_NOTE, Text, "x・y は 0〜40、w・h は 0.25〜8（1/4 キー刻み）"),
    (KLE_NOTE, Text, "並べ方を自由配列にすると動かせます"),
    (TITLE_LABEL, Text, "ラベル"),
    (LABEL, Edit, ""),
    (CUT, Text, "パネルでは切れます"),
    (KINDS, Toggle, KIND_LABELS[0]),
    (KINDS + 1, Toggle, KIND_LABELS[1]),
    (KINDS + 2, Toggle, KIND_LABELS[2]),
    (KINDS + 3, Toggle, KIND_LABELS[3]),
    (KINDS + 4, Toggle, KIND_LABELS[4]),
    (KINDS + 5, Toggle, KIND_LABELS[5]),
    (KINDS + 6, Toggle, KIND_LABELS[6]),
    (KINDS + 7, Toggle, KIND_LABELS[7]),
    (KINDS + 8, Toggle, KIND_LABELS[8]),
    (KINDS + 9, Toggle, KIND_LABELS[9]),
    (ABOUT, Text, ""),
    (TITLE_STEPS, Text, "手順"),
    (STEPS, List, ""),
    (STEP_KEY, Button, "キー操作を追加"),
    (STEP_WAIT, Button, "待ち時間を追加"),
    (STEP_REMOVE, Button, "削除"),
    (STEP_UP, Button, "↑"),
    (STEP_DOWN, Button, "↓"),
    (TITLE_WAIT, Text, "待ち時間"),
    (WAIT, Edit, ""),
    (WAIT_NOTE, Text, "0〜10000 ms"),
    (TITLE_SENDS, Text, "送るキー"),
    (SENDS, Text, ""),
    (CAPTURE, Button, "押して取り込む"),
    (MODS, Toggle, MOD_KEYS[0].1),
    (MODS + 1, Toggle, MOD_KEYS[1].1),
    (MODS + 2, Toggle, MOD_KEYS[2].1),
    (MODS + 3, Toggle, MOD_KEYS[3].1),
    (TITLE_INTERVAL, Text, "間隔"),
    (INTERVAL, Edit, ""),
    (INTERVAL_NOTE, Text, "20〜1000 ms"),
    (GROUPS, Toggle, GROUP_NAMES[0]),
    (GROUPS + 1, Toggle, GROUP_NAMES[1]),
    (GROUPS + 2, Toggle, GROUP_NAMES[2]),
    (GROUPS + 3, Toggle, GROUP_NAMES[3]),
    (GROUPS + 4, Toggle, GROUP_NAMES[4]),
    (KEYS, Keys, ""),
    (TITLE_CONTENT, Text, ""),
    (CONTENT, Edit, ""),
    (TITLE_TARGET, Text, "切り替え先"),
    (TARGET, Combo, ""),
    (TITLE_PIE, Text, "項目のキーセット"),
    (PIE, Combo, ""),
    (PIE_FIGURE, Figure, ""),
    (PIE_NOTE, Text, ""),
    (TITLE_LAYER, Text, "切り替え先"),
    (LAYER_SET, Combo, ""),
    (LAYER_HOLD, Toggle, "押している間"),
    (LAYER_TOGGLE, Toggle, "押すたびに切り替える"),
];

/// Where the keys captured go: the chosen key (or its macro's step), or
/// the 操作 page's hotkey.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Target {
    Key,
    Hotkey,
}

/// Keys being captured: the modifiers pressed so far, the first of them.
#[derive(Clone, Copy)]
struct Capture {
    target: Target,
    mods: Mods,
    first: Option<Key>,
}

/// The page's state, in the window's.
#[derive(Default)]
pub(super) struct Keymap {
    /// The key set edited, by its lower-case name.
    set: Option<String>,
    /// The chosen key, of the set's; and the chosen step, of its macro's.
    key: usize,
    step: usize,
    /// The group of keys listed.
    group: usize,
    /// Whether the name field is out, naming the set.
    naming: bool,
    capture: Option<Capture>,
    /// The field whose value was turned away, and why, while it stands.
    wrong: Option<(usize, String)>,
    /// Whether the parts were last placed for a macro: its steps above the
    /// keys sent.
    laid_macro: Cell<bool>,
}

/// What the sample draws, kept apart from the window's state as the
/// figure's is (`fields::SHOWN`).
#[derive(Default)]
struct Sample {
    /// Every key drawn: a KLE layout's past the key lines too.
    labels: Vec<Vec<u16>>,
    widths: Vec<usize>,
    /// A free or KLE layout's places, in quarters, as the file has them.
    places: Option<Vec<Place>>,
    /// How many keys are written, and so can be chosen.
    count: usize,
    edge: Option<Edge>,
    lines: usize,
    palette: Option<Palette>,
    chosen: usize,
    /// Whether the keys are dragged about: a free layout's, on キー配置.
    movable: bool,
    drag: Option<Drag>,
}

/// What a drag takes along: the key's place, or the right or the bottom
/// end of it, as the sample shows it.
#[derive(Clone, Copy, PartialEq)]
enum Pull {
    Move,
    Wide,
    Tall,
}

/// A key being dragged: where from, its place then and now, the layout's
/// left, top, width and height then, in quarters, and the sample's keys'
/// size in pixels.
#[derive(Clone, Copy)]
struct Drag {
    key: usize,
    pull: Pull,
    start: POINT,
    from: Place,
    now: Place,
    frame: (i32, i32, i32, i32),
    unit: i32,
}

impl Sample {
    /// Where the panel goes in the sample's client area `client`, at
    /// `dpi`: its keys' size, its own rectangle, and its keys' cells. Laid
    /// out and turned as the panel is, then fitted to the room, against
    /// the side of its edge (Issue #109).
    fn lay_out(&self, client: RECT, dpi: u32) -> (i32, RECT, Vec<RECT>) {
        const UNIT: i32 = 64;
        let edge = self.edge.unwrap_or(Edge::Left);
        let screen = layout::Rect { x: 0, y: 0, w: UNIT * 4096, h: UNIT * 4096 };
        let (panel, keys) = match &self.places {
            Some(places) => layout::free_panel(screen, edge, UNIT, places),
            None => layout::edge_panel(screen, edge, UNIT, self.lines.max(1), &self.widths),
        };
        let inset = scale(INSET, dpi);
        let room = (client.right - 2 * inset, client.bottom - 2 * inset);
        let key = scale(SAMPLE_KEY, dpi).min(room.0 * UNIT / panel.w.max(1)).min(room.1 * UNIT / panel.h.max(1)).max(1);
        let (w, h) = (panel.w * key / UNIT, panel.h * key / UNIT);
        let left = if edge == Edge::Right { client.right - inset - w } else { inset };
        let top = if edge == Edge::Bottom { client.bottom - inset - h } else { inset };
        let back = RECT { left, top, right: left + w, bottom: top + h };
        let cells = keys
            .iter()
            .map(|k| RECT {
                left: left + k.x * key / UNIT,
                top: top + k.y * key / UNIT,
                right: left + (k.x + k.w) * key / UNIT,
                bottom: top + (k.y + k.h) * key / UNIT,
            })
            .collect();
        (key, back, cells)
    }

    /// The key a point of the sample is on: of keys over one another, the
    /// one on top, as drawn (`order`).
    fn hit(&self, client: RECT, dpi: u32, p: POINT) -> Option<(usize, RECT, i32)> {
        let (key, _, cells) = self.lay_out(client, dpi);
        let on = |r: &RECT| p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom;
        let i = self.order(cells.len()).into_iter().rev().find(|&i| on(&cells[i]))?;
        Some((i, cells[i], key))
    }

    /// The order the keys of `count` are drawn in: as written, the chosen
    /// one last, on top of any it is over.
    fn order(&self, count: usize) -> Vec<usize> {
        (0..count).filter(|&i| i != self.chosen).chain(Some(self.chosen).filter(|&c| c < count)).collect()
    }

    /// What a drag of key `i` from `p` in its cell `cell` takes: an end of
    /// the chosen key, near it, or the key.
    fn pull(&self, i: usize, cell: RECT, p: POINT, dpi: u32) -> Pull {
        let zone = scale(END_ZONE, dpi);
        match () {
            _ if i == self.chosen && cell.right - p.x <= zone => Pull::Wide,
            _ if i == self.chosen && cell.bottom - p.y <= zone => Pull::Tall,
            _ => Pull::Move,
        }
    }
}

/// The layout `places` spans: its left, top, width and height, in quarters.
fn frame(places: &[Place]) -> (i32, i32, i32, i32) {
    let left = places.iter().map(|p| p.x).min().unwrap_or(0);
    let top = places.iter().map(|p| p.y).min().unwrap_or(0);
    let right = places.iter().map(|p| p.x + p.w).max().unwrap_or(0);
    let bottom = places.iter().map(|p| p.y + p.h).max().unwrap_or(0);
    (left, top, right - left, bottom - top)
}

/// How a key set is laid out: along its edge, free, or by a KLE file.
#[derive(Clone, PartialEq)]
enum Arrangement {
    Edge,
    Free,
    Kle(String),
}

/// How key set `section` of `doc` is laid out, as its `layout` says.
fn arrangement(doc: &Document, section: &str) -> Arrangement {
    let Some(value) = doc.get(section, "layout").map(str::trim) else { return Arrangement::Edge };
    match value.get(..4) {
        _ if value.eq_ignore_ascii_case("free") => Arrangement::Free,
        Some(kle) if kle.eq_ignore_ascii_case("kle:") => Arrangement::Kle(value[4..].trim().to_string()),
        _ => Arrangement::Edge,
    }
}

/// The keys of KLE file `name`, in the settings file's folder, if it reads.
fn kle_places(e: &Editor, name: &str) -> Option<Vec<(Place, String)>> {
    let bytes = std::fs::read(e.path.parent()?.join(name)).ok()?;
    config::kle_keys(&bytes).ok()
}

/// The place `line` writes, or the top left corner, a key in size.
fn line_place(line: &Line) -> Place {
    config::key_place(&format!("_ | _ | _ | {}", line.options_text())).unwrap_or(Place { x: 0, y: 0, w: 4, h: 4 })
}

/// Write place `p` in `line`, all four of it.
fn set_place(line: &mut Line, p: Place) {
    for (name, q) in PLACE_NAMES.iter().zip([p.x, p.y, p.w, p.h]) {
        line.set_option(name, Some(config::show_quarters(q)));
    }
}

/// The places keys `widths` long have along the edge that first shows key
/// set `set`, as a free layout writes them: as the sample draws them.
fn edge_places(doc: &Document, set: &str, widths: &[usize]) -> Vec<Place> {
    let (edge, lines) = first_edge(doc, set).unwrap_or((Edge::Left, 1));
    let screen = layout::Rect { x: 0, y: 0, w: 4 * 4096, h: 4 * 4096 };
    let (panel, keys) = layout::edge_panel(screen, edge, 4, lines, widths);
    let (wide, tall) = if edge.is_side() { (panel.h, panel.w) } else { (panel.w, panel.h) };
    keys.iter().map(|k| layout::unturn(edge, wide, tall, Place { x: k.x, y: k.y, w: k.w, h: k.h })).collect()
}

thread_local! {
    static SAMPLE_VIEW: RefCell<Sample> = RefCell::new(Sample::default());
    // How many items the pie figure draws.
    static PIE_ITEMS: Cell<usize> = const { Cell::new(0) };
}

/// A key as written after `key =`: its fields, without the spaces around.
#[derive(Clone)]
struct Line {
    label: String,
    kind: String,
    content: String,
    options: Vec<(String, String)>,
}

impl Line {
    fn of(raw: &str) -> Line {
        let fields: Vec<&str> = raw.split('|').map(str::trim).collect();
        let field = |i: usize| fields.get(i).copied().unwrap_or("").to_string();
        let options = fields.get(3).map_or(Vec::new(), |o| {
            o.split(',')
                .map(str::trim)
                .filter(|o| !o.is_empty())
                .map(|o| match o.split_once('=') {
                    Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
                    None => (o.to_string(), String::new()),
                })
                .collect()
        });
        Line { label: field(0), kind: field(1), content: field(2), options }
    }

    /// Its kind, of the ten.
    fn kind(&self) -> Option<usize> {
        KIND_NAMES.iter().position(|k| k.eq_ignore_ascii_case(&self.kind))
    }

    fn option(&self, name: &str) -> Option<&str> {
        self.options.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// Set option `name` to `value`, or take it away with none.
    fn set_option(&mut self, name: &str, value: Option<String>) {
        let at = self.options.iter().position(|(k, _)| k.eq_ignore_ascii_case(name));
        match (at, value) {
            (Some(i), Some(v)) => self.options[i].1 = v,
            (Some(i), None) => {
                self.options.remove(i);
            }
            (None, Some(v)) => self.options.push((name.into(), v)),
            (None, None) => {}
        }
    }

    fn options_text(&self) -> String {
        let one = |(k, v): &(String, String)| if v.is_empty() { k.clone() } else { format!("{k}={v}") };
        self.options.iter().map(one).collect::<Vec<_>>().join(", ")
    }

    /// The macro's steps.
    fn steps(&self) -> Vec<String> {
        self.content.split(',').map(|s| s.trim().to_string()).collect()
    }
}

/// `raw`, a key as written, rewritten to hold `line`: each field changed
/// in its place, the spaces around it kept, and a shorter one padded to
/// the old one's length unless it ends the line.
fn rewrite(raw: &str, line: &Line) -> String {
    let options = line.options_text();
    let fields = [line.label.as_str(), line.kind.as_str(), line.content.as_str(), options.as_str()];
    let mut segments: Vec<String> = raw.split('|').map(String::from).collect();
    let count = if options.is_empty() { 3 } else { 4 };
    segments.resize(count, String::new());
    let last = count - 1;
    let refilled: Vec<String> = segments.iter().enumerate().map(|(i, s)| refill(s, fields[i], i, i == last)).collect();
    refilled.join("|").trim_end().to_string()
}

/// Field `i`, `segment` with the spaces around it, holding `new`.
fn refill(segment: &str, new: &str, i: usize, last: bool) -> String {
    let old = segment.trim();
    let lead = &segment[..segment.len() - segment.trim_start().len()];
    let trail = &segment[segment.trim_end().len()..];
    // A field that ended the line and no longer does is spaced from the
    // `|` after it.
    if old == new && (last || !trail.is_empty()) {
        return segment.to_string();
    }
    let lead = if lead.is_empty() && i > 0 { " " } else { lead };
    let trail = if trail.is_empty() && !last { " " } else { trail };
    let pad = if last { 0 } else { columns(old).saturating_sub(columns(new)) };
    format!("{lead}{new}{}{trail}", " ".repeat(pad))
}

/// How many columns `text` takes in a monospaced editor: two for a wide
/// character (kana, kanji and the like), one for the rest.
fn columns(text: &str) -> usize {
    text.chars().map(|c| if c as u32 >= 0x1100 { 2 } else { 1 }).sum()
}

/// `key` as the file writes it in a combination: the comma key by name, as
/// a macro needs it.
fn key_text(key: Key) -> String {
    if key == Key::Char(b',') { "Comma".into() } else { key.to_string() }
}

fn combo_text(combo: Combo) -> String {
    let mut parts: Vec<String> =
        MOD_KEYS.iter().filter(|(k, _)| k.modifier().is_some_and(|m| combo.mods.contains(m))).map(|(_, n)| n.to_string()).collect();
    parts.extend(combo.key.map(key_text));
    parts.join("+")
}

/// The key alone, as a combination: a modifier as one.
fn combo_of(key: Key) -> Combo {
    match key.modifier() {
        Some(m) => Combo { mods: m, key: None },
        None => Combo { mods: Mods::NONE, key: Some(key) },
    }
}

/// `mods` with `m` turned on or off.
fn toggled(mods: Mods, m: Mods) -> Mods {
    MOD_KEYS
        .iter()
        .filter_map(|(k, _)| k.modifier())
        .filter(|&f| if f == m { !mods.contains(m) } else { mods.contains(f) })
        .fold(Mods::NONE, |a, b| a | b)
}

/// The keys of group `group`, as they are listed.
fn group_keys(group: usize) -> Vec<Key> {
    use Key::*;
    match group {
        0 => (b'A'..=b'Z').chain(b'0'..=b'9').map(Char).collect(),
        1 => (1..=24).map(F).collect(),
        2 => vec![Esc, Tab, Space, Enter, Backspace, Delete, Insert, Home, End, PgUp, PgDn, Left, Right, Up, Down],
        3 => b"[]-=;',./\\`".iter().map(|&c| Char(c)).collect(),
        _ => (b'0'..=b'9').chain(*b"*+-./").map(Pad).collect(),
    }
}

fn group_of(key: Key) -> Option<usize> {
    (0..GROUP_NAMES.len()).find(|&g| group_keys(g).contains(&key))
}

/// The key a virtual key is, of those a key can send, not a modifier.
fn key_of_vk(vk: u16) -> Option<Key> {
    (0..GROUP_NAMES.len()).flat_map(group_keys).find(|&k| send::vk(k) == vk)
}

/// The modifier a virtual key is, either hand's.
fn modifier_of_vk(vk: u16) -> Option<Key> {
    match vk {
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => Some(Key::Ctrl),
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some(Key::Shift),
        VK_MENU | VK_LMENU | VK_RMENU => Some(Key::Alt),
        VK_LWIN | VK_RWIN => Some(Key::Win),
        _ => None,
    }
}

/// The modifiers held, as the message being handled saw them.
fn held_mods() -> Mods {
    let down = |vk: u16| unsafe { GetKeyState(vk as i32) } < 0;
    let pairs = [(VK_CONTROL, Mods::CTRL), (VK_SHIFT, Mods::SHIFT), (VK_MENU, Mods::ALT), (VK_LWIN, Mods::WIN), (VK_RWIN, Mods::WIN)];
    pairs.iter().filter(|(vk, _)| down(*vk)).fold(Mods::NONE, |a, (_, m)| a | *m)
}

/// The combination the chosen key sends, or its macro's chosen step: none
/// for the other kinds, a wait, or one that does not read.
fn sent(line: &Line, step: usize) -> Option<Combo> {
    match line.kind()? {
        SHORTCUT | REPEAT => Combo::parse(&line.content).ok(),
        HOLD | MODIFIER => Key::parse(&line.content).map(combo_of),
        MACRO => line.steps().get(step).filter(|s| !is_wait(s)).and_then(|s| Combo::parse(s).ok()),
        _ => None,
    }
}

fn is_wait(step: &str) -> bool {
    step.get(..5).is_some_and(|w| w.eq_ignore_ascii_case("wait "))
}

/// A step as the list shows it.
fn step_text(step: &str) -> String {
    match is_wait(step) {
        true => format!("待つ {} ms", step[4..].trim()),
        false => step.to_string(),
    }
}

fn part_of(e: &Editor, id: usize) -> HWND {
    unsafe { GetDlgItem(e.hwnd, id as i32) }
}

/// The text of `hwnd`, however long.
fn text_of(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Give `hwnd` `text`, unless it has it: an edit keeps its caret.
fn set_text(hwnd: HWND, text: &str) {
    if text_of(hwnd) != text {
        let text = wide(text);
        unsafe { SetWindowTextW(hwnd, text.as_ptr()) };
    }
}

/// Fill the list or drop-down list `hwnd` with `items`, unless it has them.
unsafe fn fill(hwnd: HWND, items: &[String], list: bool) {
    let (reset, add, count, get) =
        if list { (LB_RESETCONTENT, LB_ADDSTRING, LB_GETCOUNT, LB_GETTEXT) } else { (CB_RESETCONTENT, CB_ADDSTRING, CB_GETCOUNT, CB_GETLBTEXT) };
    unsafe {
        let n = SendMessageW(hwnd, count, 0, 0).max(0) as usize;
        let same = n == items.len()
            && items.iter().enumerate().all(|(i, item)| {
                let mut buf = [0u16; 256];
                let len = SendMessageW(hwnd, get, i, buf.as_mut_ptr() as LPARAM);
                String::from_utf16_lossy(&buf[..len.max(0) as usize]) == *item
            });
        if same {
            return;
        }
        SendMessageW(hwnd, reset, 0, 0);
        for item in items {
            let text = wide(item);
            SendMessageW(hwnd, add, 0, text.as_ptr() as LPARAM);
        }
    }
}

unsafe fn choose_item(hwnd: HWND, at: Option<usize>, list: bool) {
    let set = if list { LB_SETCURSEL } else { CB_SETCURSEL };
    unsafe { SendMessageW(hwnd, set, at.unwrap_or(usize::MAX), 0) };
}

/// The key sets of `doc`, by their lower-case names, in the order written.
fn set_names(doc: &Document) -> Vec<String> {
    doc.sections().into_iter().filter_map(|s| s.strip_prefix("set.").map(String::from)).collect()
}

/// Where key set `set` is used: the profiles' edges that show it, and the
/// pie and page keys that open it.
fn used_by(doc: &Document, set: &str) -> Vec<String> {
    let mut places = Vec::new();
    for section in doc.sections() {
        if let Some(profile) = section.strip_prefix("profile.") {
            for (name, value) in doc.values(&section) {
                if let Some((_, edge)) = EDGE_NAMES.iter().find(|(n, _)| *n == name).filter(|_| value.eq_ignore_ascii_case(set)) {
                    places.push(format!("{profile} の{edge}"));
                }
            }
        } else if section.starts_with("set.") {
            for (name, value) in doc.values(&section) {
                let line = Line::of(&value);
                if name != "key" || !line.content.eq_ignore_ascii_case(set) {
                    continue;
                }
                match line.kind() {
                    Some(PIE_KIND) => places.push(format!("{}キーのパイ", line.label)),
                    Some(LAYER_KIND) => places.push(format!("{}キーのレイヤー", line.label)),
                    Some(PAGE_KIND) if PAGE_TURNS.iter().all(|(t, _)| !t.eq_ignore_ascii_case(set)) => {
                        places.push(format!("{}キーのページ", line.label))
                    }
                    _ => {}
                }
            }
        }
    }
    places
}

/// The first edge that shows key set `set`, of the first profile, as
/// written, and how many lines of keys it has; none if no edge does.
fn first_edge(doc: &Document, set: &str) -> Option<(Edge, usize)> {
    let edge = doc
        .sections()
        .iter()
        .filter(|s| s.starts_with("profile."))
        .flat_map(|s| doc.values(s))
        .find(|(name, value)| Edge::parse(name).is_some() && value.eq_ignore_ascii_case(set))
        .and_then(|(name, _)| Edge::parse(&name))?;
    let (name, _) = EDGE_NAMES[edge as usize];
    let lines_name = if edge.is_side() { "columns" } else { "rows" };
    // Not written for the edge: [edge]'s (Issue #97).
    let written = doc.get(&format!("edge.{name}"), lines_name).or_else(|| doc.get("edge", lines_name));
    let lines = written.and_then(|v| v.trim().parse().ok()).filter(|n| (1..=3).contains(n));
    Some((edge, lines.unwrap_or(1)))
}

/// The panels' colours, as the file's [general] has them.
fn panel_palette(doc: &Document) -> Palette {
    let theme = match doc.get("general", "theme").map(|t| t.trim().to_ascii_lowercase()) {
        Some(t) if t == "light" => Theme::Light,
        Some(t) if t == "dark" => Theme::Dark,
        _ => Theme::Auto,
    };
    let accent = doc.get("general", "accent").and_then(Accent::parse).unwrap_or(Accent::Auto);
    colors::palette(theme, accent)
}

/// The panels' key size, in logical pixels, as the file has it.
fn panel_key(doc: &Document) -> i32 {
    let r = config::RANGES.key_size;
    doc.get("general", "key_size")
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| (r.min..=r.max).contains(n))
        .map_or(layout::KEY_SIZE, |n| n as i32)
}

/// Whether `label` is cut on a panel key `width` keys long, keys `key`
/// pixels across: wider than the key's face in the panel's font.
fn is_cut(label: &str, width: usize, key: i32) -> bool {
    let text: Vec<u16> = label.encode_utf16().collect();
    let mut size = SIZE { cx: 0, cy: 0 };
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        let font = panel::key_font(key);
        let old = SelectObject(dc, font);
        GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
        SelectObject(dc, old);
        DeleteObject(font);
        DeleteDC(dc);
    }
    size.cx > width as i32 * key - 2 * panel::key_gap(key)
}

/// The keys of the set, as written; and the chosen key, if there is one.
fn keys_of(e: &Editor) -> Vec<String> {
    e.keymap.set.as_deref().map_or(Vec::new(), |set| e.doc.keys(&format!("set.{set}")))
}

fn chosen_line(e: &Editor) -> Option<(String, Line)> {
    let raw = keys_of(e).into_iter().nth(e.keymap.key)?;
    let line = Line::of(&raw);
    Some((raw, line))
}

/// Make the page's parts in `parent`, hidden. False, if any could not be
/// made.
pub(super) unsafe fn make(parent: HWND) -> bool {
    let instance = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()) };
    for &(id, class, text) in PARTS {
        let (ex, name, style) = match class {
            Text => (0, w!("Static"), super::SS_NOPREFIX | super::SS_CENTERIMAGE),
            Edit => (WS_EX_CLIENTEDGE, w!("Edit"), WS_TABSTOP | ES_AUTOHSCROLL as u32),
            Button => (0, w!("Button"), WS_TABSTOP | BS_PUSHBUTTON as u32),
            Toggle => (0, w!("Button"), WS_TABSTOP | (BS_CHECKBOX | BS_PUSHLIKE) as u32),
            Combo => (0, w!("ComboBox"), WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32),
            List => (WS_EX_CLIENTEDGE, w!("ListBox"), WS_TABSTOP | WS_VSCROLL | (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32),
            Keys => (
                WS_EX_CLIENTEDGE,
                w!("ListBox"),
                WS_TABSTOP | WS_HSCROLL | (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | LBS_MULTICOLUMN) as u32,
            ),
            Sample => (0, SAMPLE_CLASS, WS_TABSTOP),
            Figure => (0, PIE_CLASS, 0),
        };
        // The pie's note runs on over lines.
        let style = if id == PIE_NOTE { super::SS_NOPREFIX } else { style };
        let text = wide(text);
        let hwnd =
            unsafe { CreateWindowExW(ex, name, text.as_ptr(), WS_CHILD | style, 0, 0, 1, 1, parent, id as HMENU, instance, null()) };
        if hwnd.is_null() {
            return false;
        }
    }
    true
}

/// Every part of the page, for fonts.
pub(super) fn parts(e: &Editor) -> Vec<HWND> {
    PARTS.iter().map(|&(id, ..)| part_of(e, id)).collect()
}

/// The parts to theme as edit controls and drop-downs, and as buttons
/// (lists among them, for their scroll bars).
pub(super) fn themed(e: &Editor) -> (Vec<HWND>, Vec<HWND>) {
    let (mut typed, mut buttons) = (Vec::new(), Vec::new());
    for &(id, class, _) in PARTS {
        match class {
            Edit | Combo => typed.push(part_of(e, id)),
            Button | Toggle | List | Keys => buttons.push(part_of(e, id)),
            _ => {}
        }
    }
    (typed, buttons)
}

/// Whether part `id` is the page's.
pub(super) fn is_part(id: usize) -> bool {
    PARTS.iter().any(|&(p, ..)| p == id)
}

/// Whether part `id` is typed in.
pub(super) fn is_typed_id(id: usize) -> bool {
    PARTS.iter().any(|&(p, class, _)| p == id && class == Edit)
}

/// Whether `hwnd` is one of the page's fields typed in.
pub(super) fn is_typed(e: &Editor, hwnd: HWND) -> bool {
    let id = unsafe { GetDlgCtrlID(hwnd) } as usize;
    !hwnd.is_null() && is_typed_id(id) && part_of(e, id) == hwnd
}

/// Whether the field `hwnd` shows a value turned away.
pub(super) fn is_wrong(hwnd: HWND) -> bool {
    super::EDITOR.with(|e| {
        e.try_borrow().ok().is_some_and(|e| {
            e.as_ref().is_some_and(|e| e.keymap.wrong.as_ref().is_some_and(|(id, _)| part_of(e, *id) == hwnd))
        })
    })
}

/// The reason a field's value was turned away, of one still standing.
pub(super) fn standing_reason(e: &Editor) -> Option<String> {
    e.keymap.wrong.as_ref().map(|(_, why)| why.clone())
}

/// Whether part `id` shows now, on キー配置, for the chosen key, if there
/// is one, of a set laid out as `way`.
fn shows_shape(e: &Editor, id: usize, key: bool, way: &Arrangement) -> bool {
    let k = &e.keymap;
    match id {
        SET => !k.naming,
        NAME => k.naming,
        TITLE_SET | ADD_SET | RENAME | REMOVE_SET | KLE | USED | SAMPLE | ADD_KEY | REMOVE_KEY | KEY_UP | KEY_DOWN => true,
        TITLE_ARRANGE | ARRANGE => k.set.is_some(),
        TITLE_WIDTH | WIDTH | WIDTH_NOTE => key && *way == Arrangement::Edge,
        _ if (PLACE..PLACE + 4).contains(&id) || (TITLE_PLACE..TITLE_PLACE + 4).contains(&id) => key && *way != Arrangement::Edge,
        PLACE_NOTE => key && *way == Arrangement::Free,
        KLE_NOTE => key && matches!(way, Arrangement::Kle(_)),
        _ => false,
    }
}

/// Whether part `id` shows now, on キーマッピング, for the chosen key of
/// kind `kind`, its chosen step a wait or not.
fn shows(id: usize, kind: Option<usize>, wait: bool, cut: bool) -> bool {
    let key = kind.is_some();
    let sends = matches!(kind, Some(SHORTCUT | MODIFIER | HOLD | REPEAT)) || (kind == Some(MACRO) && !wait);
    match id {
        TITLE_SET | SET | SAMPLE => true,
        CUT => key && cut,
        TITLE_LABEL | LABEL | ABOUT => key,
        _ if (KINDS..KINDS + KIND_NAMES.len()).contains(&id) => key,
        TITLE_SENDS | SENDS | CAPTURE => sends,
        _ if (MODS..MODS + 4).contains(&id) => sends,
        _ if (GROUPS..GROUPS + 5).contains(&id) => sends && kind != Some(MODIFIER),
        KEYS => sends && kind != Some(MODIFIER),
        TITLE_INTERVAL | INTERVAL | INTERVAL_NOTE => kind == Some(REPEAT),
        TITLE_STEPS | STEPS | STEP_KEY | STEP_WAIT | STEP_REMOVE | STEP_UP | STEP_DOWN => kind == Some(MACRO),
        TITLE_WAIT | WAIT | WAIT_NOTE => kind == Some(MACRO) && wait,
        TITLE_CONTENT | CONTENT => matches!(kind, Some(TEXT | RUN)),
        TITLE_TARGET | TARGET => kind == Some(PAGE_KIND),
        TITLE_PIE | PIE | PIE_FIGURE | PIE_NOTE => kind == Some(PIE_KIND),
        TITLE_LAYER | LAYER_SET | LAYER_HOLD | LAYER_TOGGLE => kind == Some(LAYER_KIND),
        _ => false,
    }
}

/// Show the page's parts that go with the chosen key, if the page is
/// shown; else hide them all.
pub(super) unsafe fn show(e: &Editor) {
    let line = chosen_line(e).map(|(_, l)| l);
    let kind = line.as_ref().map(|l| l.kind().unwrap_or(SHORTCUT));
    let wait = line.as_ref().is_some_and(|l| l.kind() == Some(MACRO) && l.steps().get(e.keymap.step).is_some_and(|s| is_wait(s)));
    let cut = line.as_ref().is_some_and(|l| {
        let width = l.option("width").and_then(|w| w.parse().ok()).unwrap_or(1);
        is_cut(&text_of(part_of(e, LABEL)), width, panel_key(&e.doc))
    });
    let way = e.keymap.set.as_ref().map_or(Arrangement::Edge, |s| arrangement(&e.doc, &format!("set.{s}")));
    for &(id, ..) in PARTS {
        let on = match e.page {
            PAGE => shows_shape(e, id, kind.is_some(), &way),
            MAP_PAGE => shows(id, kind, wait, cut),
            _ => false,
        };
        unsafe { ShowWindow(part_of(e, id), if on { SW_SHOWNA } else { SW_HIDE }) };
    }
}

/// The label typed so far may be cut on the panel, or no longer: say so.
pub(super) unsafe fn label_typed() {
    // Typed into while the window is in use, as a refresh sets the text:
    // the refresh says it.
    let _ = super::EDITOR.with(|e| {
        let Ok(e) = e.try_borrow() else { return };
        let Some(e) = e.as_ref() else { return };
        let Some((_, line)) = chosen_line(e) else { return };
        let width = line.option("width").and_then(|w| w.parse().ok()).unwrap_or(1);
        let cut = is_cut(&text_of(part_of(e, LABEL)), width, panel_key(&e.doc));
        unsafe { ShowWindow(part_of(e, CUT), if cut && e.page == MAP_PAGE { SW_SHOWNA } else { SW_HIDE }) };
    });
}

/// Place the parts in `left`, `top`, `width` across, down to `bottom`: the
/// key sets' row along the top, the sample down the left with its buttons
/// under it, the key's fields on the right. On キー配置, the sample takes
/// the room its few fields leave; on キーマッピング, the fields take it.
pub(super) unsafe fn place(e: &Editor, left: i32, top: i32, width: i32, bottom: i32) {
    let s = |v| scale(v, e.dpi);
    let (h, pitch, gap) = (s(ROW_H), s(PITCH), s(MARGIN) / 2);
    let at = |id: usize, x: i32, y: i32, w: i32, height: i32| unsafe {
        MoveWindow(part_of(e, id), x, y, w.max(0), height.max(0), 1);
    };
    // The key sets.
    let mut x = left;
    at(TITLE_SET, x, top, s(TITLE_W) + s(8), h);
    x += s(TITLE_W) + s(8) + gap;
    at(SET, x, top, s(180), s(LIST_H));
    at(NAME, x, top, s(180), h);
    x += s(180) + gap;
    for (id, w) in [(ADD_SET, 64), (RENAME, 96), (REMOVE_SET, 64), (KLE, 128)] {
        at(id, x, top, s(w), h);
        x += s(w) + gap;
    }
    at(USED, left, top + pitch, width, s(20));

    // The sample, and its buttons.
    let y0 = top + pitch + s(28);
    let buttons = bottom - h;
    let shape_x = left + width - s(SHAPE_W);
    match e.page {
        MAP_PAGE => at(SAMPLE, left, y0, s(SAMPLE_W), bottom - y0),
        _ => at(SAMPLE, left, y0, shape_x - s(MARGIN) - left, buttons - gap - y0),
    }
    let mut x = left;
    for (id, w) in [(ADD_KEY, 56), (REMOVE_KEY, 56), (KEY_UP, 36), (KEY_DOWN, 36)] {
        at(id, x, buttons, s(w), h);
        x += s(w) + gap;
    }

    // The shape: 並べ方, then the width or the place.
    let (title, fw) = (s(TITLE_W) + s(8), s(SHAPE_W));
    at(TITLE_ARRANGE, shape_x, y0, title, h);
    at(ARRANGE, shape_x + title + gap, y0, fw - title - gap, s(LIST_H));
    let y = y0 + pitch;
    at(TITLE_WIDTH, shape_x, y, title, h);
    at(WIDTH, shape_x + title + gap, y, s(60), h);
    at(WIDTH_NOTE, shape_x + title + s(60) + 2 * gap, y, s(60), h);
    for i in 0..4 {
        let (x, y) = (shape_x + (i % 2) as i32 * fw / 2, y + (i / 2) as i32 * pitch);
        at(TITLE_PLACE + i, x, y, s(20), h);
        at(PLACE + i, x + s(20) + gap, y, s(72), h);
    }
    at(PLACE_NOTE, shape_x, y + 2 * pitch, fw, s(20));
    at(KLE_NOTE, shape_x, y + 2 * pitch, fw, s(20));

    // The key's fields.
    let rx = left + s(SAMPLE_W) + s(MARGIN);
    let rw = (left + width - rx).max(0);
    let mut x = rx;
    for (id, w) in [(TITLE_LABEL, 48), (LABEL, 150)] {
        at(id, x, y0, s(w), h);
        x += s(w) + gap;
    }
    at(CUT, x, y0, rx + rw - x, h);
    let names: Vec<&str> = KIND_LABELS.to_vec();
    unsafe { lay_row(e, KINDS, &names, rx, y0 + pitch, rw, h) };
    at(ABOUT, rx, y0 + 2 * pitch, rw, s(20));

    let y1 = y0 + 2 * pitch + s(28);
    // A macro's steps above the keys a step sends, or a wait's time.
    let list_w = s(220);
    at(TITLE_STEPS, rx, y1, s(48), h);
    at(STEPS, rx + s(52), y1, list_w, s(84));
    let bx = rx + s(52) + list_w + gap;
    at(STEP_KEY, bx, y1, s(108), h);
    at(STEP_WAIT, bx + s(108) + gap, y1, s(108), h);
    at(STEP_REMOVE, bx, y1 + s(30), s(56), h);
    at(STEP_UP, bx + s(56) + gap, y1 + s(30), s(36), h);
    at(STEP_DOWN, bx + s(92) + 2 * gap, y1 + s(30), s(36), h);
    let y2 = y1 + s(92);
    let mut x = rx;
    for (id, w) in [(TITLE_WAIT, TITLE_W), (WAIT, 80), (WAIT_NOTE, 100)] {
        at(id, x, y2, s(w), h);
        x += s(w) + gap;
    }

    // The keys sent: below the steps for a macro, else at the top.
    let macro_ = is_macro(e);
    e.keymap.laid_macro.set(macro_);
    let ys = if macro_ { y2 } else { y1 };
    at(TITLE_SENDS, rx, ys, s(TITLE_W), h);
    at(SENDS, rx + s(TITLE_W) + gap, ys, s(160), h);
    let cx = rx + s(TITLE_W) + s(160) + 2 * gap;
    at(CAPTURE, cx, ys, rx + rw - cx, h);
    let mut x = rx;
    for i in 0..4 {
        at(MODS + i, x, ys + pitch, s(56), h);
        x += s(56) + gap;
    }
    x += gap;
    for (id, w) in [(TITLE_INTERVAL, 36), (INTERVAL, 60), (INTERVAL_NOTE, 100)] {
        at(id, x, ys + pitch, s(w), h);
        x += s(w) + gap;
    }
    unsafe { lay_row(e, GROUPS, &GROUP_NAMES, rx, ys + 2 * pitch, rw, h) };
    let ky = ys + 3 * pitch;
    at(KEYS, rx, ky, rw, (bottom - ky).max(s(48)));
    unsafe { SendMessageW(part_of(e, KEYS), LB_SETCOLUMNWIDTH, s(72) as WPARAM, 0) };

    // A text, a program, a page's target, a pie's set.
    at(TITLE_CONTENT, rx, y1, s(120), h);
    at(CONTENT, rx + s(124), y1, rw - s(124), h);
    at(TITLE_TARGET, rx, y1, s(96), h);
    at(TARGET, rx + s(100), y1, s(200), s(LIST_H));
    at(TITLE_PIE, rx, y1, s(120), h);
    at(PIE, rx + s(124), y1, s(200), s(LIST_H));
    at(PIE_FIGURE, rx, y1 + pitch, s(120), s(100));
    at(PIE_NOTE, rx + s(128), y1 + pitch, rw - s(128), s(100));
    at(TITLE_LAYER, rx, y1, s(96), h);
    at(LAYER_SET, rx + s(100), y1, s(200), s(LIST_H));
    at(LAYER_HOLD, rx, y1 + pitch, s(120), h);
    at(LAYER_TOGGLE, rx + s(120) + gap, y1 + pitch, s(180), h);
}

/// Lay buttons `first`.. side by side across `x`, `width`: each as wide as
/// its name, and the room left over shared among them.
unsafe fn lay_row(e: &Editor, first: usize, names: &[&str], x: i32, y: i32, width: i32, h: i32) {
    unsafe {
        let dc = GetDC(e.hwnd);
        let old = SelectObject(dc, e.font);
        let widths: Vec<i32> = names
            .iter()
            .map(|n| {
                let text: Vec<u16> = n.encode_utf16().collect();
                let mut size = SIZE { cx: 0, cy: 0 };
                GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
                size.cx + 2 * scale(BUTTON_PAD, e.dpi)
            })
            .collect();
        SelectObject(dc, old);
        ReleaseDC(e.hwnd, dc);
        let total: i32 = widths.iter().sum();
        // Too wide for the row: each one narrower, by its share.
        let widths: Vec<i32> = match total > width && total > 0 {
            true => widths.iter().map(|w| w * width / total).collect(),
            false => widths,
        };
        let total: i32 = widths.iter().sum();
        let extra = ((width - total) / names.len() as i32).max(0);
        let mut at = x;
        for (i, w) in widths.iter().enumerate() {
            MoveWindow(GetDlgItem(e.hwnd, (first + i) as i32), at, y, w + extra, h, 1);
            at += w + extra;
        }
    }
}

fn is_macro(e: &Editor) -> bool {
    chosen_line(e).is_some_and(|(_, l)| l.kind() == Some(MACRO))
}

/// Show the page as the window's copy of the file has it, the chosen key
/// set and key kept if they are still there. Called when the page is shown
/// and when Windows's colours change too: the sample follows the
/// panels' look, set on the other pages or by Windows.
pub(super) unsafe fn refresh(e: &mut Editor) {
    let sets = set_names(&e.doc);
    let k = &mut e.keymap;
    if !k.set.as_ref().is_some_and(|s| sets.contains(s)) {
        k.set = sets.first().cloned();
        (k.key, k.step) = (0, 0);
        follow_group(e);
    }
    let count = keys_of(e).len();
    let k = &mut e.keymap;
    k.key = k.key.min(count.saturating_sub(1));
    let set = k.set.clone().unwrap_or_default();
    unsafe {
        let combo = part_of(e, SET);
        fill(combo, &sets, false);
        choose_item(combo, sets.iter().position(|s| *s == set), false);
        let used = used_by(&e.doc, &set);
        let used = if used.is_empty() { "どこでも使っていません".into() } else { used.join("、") };
        set_text(part_of(e, USED), &format!("使っている場所: {used}"));
        let targets: Vec<String> = PAGE_TURNS.iter().map(|(_, n)| n.to_string()).chain(sets.iter().cloned()).collect();
        fill(part_of(e, TARGET), &targets, false);
        fill(part_of(e, PIE), &sets, false);
        fill(part_of(e, LAYER_SET), &sets, false);
        let way = arrangement(&e.doc, &format!("set.{set}"));
        let mut ways: Vec<String> = ARRANGEMENTS.iter().map(|w| w.to_string()).collect();
        if let Arrangement::Kle(name) = &way {
            ways.push(format!("KLE（{name}）"));
        }
        let arrange = part_of(e, ARRANGE);
        fill(arrange, &ways, false);
        choose_item(arrange, Some(match way {
            Arrangement::Edge => 0,
            Arrangement::Free => 1,
            Arrangement::Kle(_) => 2,
        }), false);
        show_key(e, &sets);
    }
    update_sample(e);
    unsafe { show(e) };
    // A macro's steps go above the keys it sends: placed again, once the
    // window is free.
    if is_macro(e) != e.keymap.laid_macro.get() {
        unsafe { PostMessageW(e.hwnd, super::WM_LAY_OUT, 0, 0) };
    }
}

/// Show the chosen key in its fields.
unsafe fn show_key(e: &mut Editor, sets: &[String]) {
    let Some((_, line)) = chosen_line(e) else { return };
    let kind = line.kind();
    let steps = line.steps();
    e.keymap.step = e.keymap.step.min(steps.len().saturating_sub(1));
    let k = &e.keymap;
    unsafe {
        let label = part_of(e, LABEL);
        set_text(label, &line.label);
        SendMessageW(label, EM_SETMODIFY, 0, 0);
        let width = part_of(e, WIDTH);
        set_text(width, line.option("width").unwrap_or("1"));
        SendMessageW(width, EM_SETMODIFY, 0, 0);
        // A KLE layout's place is its file's, read only.
        let way = arrangement(&e.doc, &format!("set.{}", k.set.as_deref().unwrap_or("")));
        let place = match &way {
            Arrangement::Kle(name) => kle_places(e, name).and_then(|keys| keys.get(k.key).map(|(p, _)| *p)),
            _ => Some(line_place(&line)),
        };
        for (i, q) in place.map_or([None; 4], |p| [p.x, p.y, p.w, p.h].map(Some)).iter().enumerate() {
            let field = part_of(e, PLACE + i);
            set_text(field, &q.map_or_else(String::new, config::show_quarters));
            SendMessageW(field, EM_SETMODIFY, 0, 0);
            SendMessageW(field, EM_SETREADONLY, matches!(way, Arrangement::Kle(_)) as WPARAM, 0);
        }
        for i in 0..KIND_NAMES.len() {
            SendMessageW(part_of(e, KINDS + i), BM_SETCHECK, (kind == Some(i)) as WPARAM, 0);
        }
        let about = match kind {
            Some(i) => ABOUTS[i],
            None if line.kind.eq_ignore_ascii_case("none") => "何も送りません。種類を選ぶと割り当てます",
            None => "この種類は読めません",
        };
        set_text(part_of(e, ABOUT), about);

        let combo = sent(&line, k.step);
        set_text(part_of(e, SENDS), &combo.map_or_else(String::new, combo_text));
        for (i, (key, _)) in MOD_KEYS.iter().enumerate() {
            let on = combo.is_some_and(|c| key.modifier().is_some_and(|m| c.mods.contains(m)));
            SendMessageW(part_of(e, MODS + i), BM_SETCHECK, on as WPARAM, 0);
        }
        for g in 0..GROUP_NAMES.len() {
            SendMessageW(part_of(e, GROUPS + g), BM_SETCHECK, (g == k.group) as WPARAM, 0);
        }
        let keys = group_keys(k.group);
        let list = part_of(e, KEYS);
        fill(list, &keys.iter().map(|k| k.to_string()).collect::<Vec<_>>(), true);
        choose_item(list, combo.and_then(|c| c.key).and_then(|key| keys.iter().position(|&k| k == key)), true);
        let capture = if k.capture.is_some_and(|c| c.target == Target::Key) { "取り込み中（もう一度押すと取りやめ）" } else { "押して取り込む" };
        set_text(part_of(e, CAPTURE), capture);

        let interval = part_of(e, INTERVAL);
        set_text(interval, line.option("interval").unwrap_or("50"));
        SendMessageW(interval, EM_SETMODIFY, 0, 0);
        let steps_list = part_of(e, STEPS);
        fill(steps_list, &steps.iter().map(|s| step_text(s)).collect::<Vec<_>>(), true);
        choose_item(steps_list, Some(k.step), true);
        let wait = part_of(e, WAIT);
        let ms = steps.get(k.step).filter(|s| is_wait(s)).map_or("", |s| s[4..].trim());
        set_text(wait, ms);
        SendMessageW(wait, EM_SETMODIFY, 0, 0);

        let content = part_of(e, CONTENT);
        set_text(content, &line.content);
        SendMessageW(content, EM_SETMODIFY, 0, 0);
        set_text(part_of(e, TITLE_CONTENT), if kind == Some(RUN) { "パスか URL" } else { "送る文字列" });
        let turn = PAGE_TURNS.iter().position(|(t, _)| t.eq_ignore_ascii_case(&line.content));
        let target = turn.or_else(|| sets.iter().position(|s| s.eq_ignore_ascii_case(&line.content)).map(|i| i + PAGE_TURNS.len()));
        choose_item(part_of(e, TARGET), target, false);
        choose_item(part_of(e, PIE), sets.iter().position(|s| s.eq_ignore_ascii_case(&line.content)), false);
        choose_item(part_of(e, LAYER_SET), sets.iter().position(|s| s.eq_ignore_ascii_case(&line.content)), false);
        let toggle = line.option("mode").is_some_and(|m| m.eq_ignore_ascii_case("toggle"));
        SendMessageW(part_of(e, LAYER_HOLD), BM_SETCHECK, (!toggle) as WPARAM, 0);
        SendMessageW(part_of(e, LAYER_TOGGLE), BM_SETCHECK, toggle as WPARAM, 0);
        let (items, note) = pie_note(&e.doc, &line.content);
        set_text(part_of(e, PIE_NOTE), &note);
        PIE_ITEMS.set(items);
        InvalidateRect(part_of(e, PIE_FIGURE), null(), 0);
    }
}

/// How many items a pie menu of key set `set` has, and what its note says
/// of them: how many, and why they cannot be, if they cannot.
fn pie_note(doc: &Document, set: &str) -> (usize, String) {
    let section = format!("set.{set}");
    if !doc.sections().contains(&section.to_ascii_lowercase()) {
        return (0, format!("キーセット「{set}」がありません"));
    }
    let raw = doc.keys(&section);
    let keys = raw.iter().filter_map(|k| config::key_def(k).ok()).collect();
    let problems = config::pie_problems(&KeySet { name: set.to_ascii_lowercase(), keys });
    let mut note = format!("項目 {} 個", raw.len());
    for p in problems {
        note += &format!("\n{p}");
    }
    (raw.len(), note)
}

/// Tell the sample what to draw, and draw it.
fn update_sample(e: &Editor) {
    let lines: Vec<Line> = keys_of(e).iter().map(|k| Line::of(k)).collect();
    let set = e.keymap.set.clone().unwrap_or_default();
    let (edge, rows) = first_edge(&e.doc, &set).map_or((None, 1), |(edge, n)| (Some(edge), n));
    let mut labels: Vec<String> = lines.iter().map(|l| l.label.clone()).collect();
    let way = arrangement(&e.doc, &format!("set.{set}"));
    let places = match &way {
        Arrangement::Edge => None,
        Arrangement::Free => Some(lines.iter().map(line_place).collect()),
        // As the parser lays it out: the file's keys to the key lines in
        // order, the rest with the file's labels.
        Arrangement::Kle(name) => kle_places(e, name).map(|keys| {
            labels.extend(keys.iter().skip(lines.len()).map(|(_, l)| l.clone()));
            keys.iter().map(|(p, _)| *p).collect()
        }),
    };
    SAMPLE_VIEW.with_borrow_mut(|v| {
        v.labels = labels.iter().map(|l| l.encode_utf16().collect()).collect();
        v.widths = lines.iter().map(|l| l.option("width").and_then(|w| w.parse().ok()).filter(|w| (1..=3).contains(w)).unwrap_or(1)).collect();
        v.places = places;
        v.count = lines.len();
        v.edge = edge;
        v.lines = rows;
        v.palette = Some(panel_palette(&e.doc));
        v.chosen = e.keymap.key;
        v.movable = way == Arrangement::Free && e.page == PAGE;
    });
    unsafe { InvalidateRect(part_of(e, SAMPLE), null(), 0) };
}

/// Change the chosen key's line with `change`, if the parser reads the
/// key it makes: else mark `field` and say why. Whether it changed.
unsafe fn change(e: &mut Editor, field: usize, edit: impl FnOnce(&mut Line) -> Result<(), String>) -> bool {
    let Some((raw, mut line)) = chosen_line(e) else { return false };
    let set = format!("set.{}", e.keymap.set.clone().unwrap_or_default());
    let changed = edit(&mut line).and_then(|()| {
        if [&line.label, &line.content].iter().any(|f| f.contains('|')) {
            return Err("「|」は欄の区切りなので書けない".into());
        }
        let text = rewrite(&raw, &line);
        config::key_def(&text)?;
        if text == raw || e.doc.set_key(&set, e.keymap.key, &text) { Ok(()) } else { Err("この値はファイルに書けない".into()) }
    });
    unsafe { taken(e, field, changed) }
}

/// What a change of `field` came to: taken, the page shows the file's copy
/// again; turned away, the field is marked and the status line says why.
unsafe fn taken(e: &mut Editor, field: usize, result: Result<(), String>) -> bool {
    match result {
        Ok(()) => {
            let had = e.keymap.wrong.take().is_some();
            unsafe {
                refresh(e);
                mark_unsaved(e);
            }
            if had {
                match super::standing_reason(e) {
                    Some(reason) => tell(e, &reason),
                    None => super::ask_reason(e),
                }
            }
            true
        }
        Err(why) => {
            tell(e, &why);
            e.keymap.wrong = Some((field, why));
            unsafe { InvalidateRect(part_of(e, field), null(), 1) };
            false
        }
    }
}

/// The chosen key turned into kind `kind`: what it sends kept where the
/// new kind can send it too, else the kind's own first value.
fn convert(line: &mut Line, kind: usize, step: usize, sets: &[String], own: &str) {
    let combo = sent(line, step);
    let old = line.kind();
    line.content = match kind {
        MODIFIER => {
            let first = combo.and_then(|c| MOD_KEYS.iter().find(|(k, _)| k.modifier().is_some_and(|m| c.mods.contains(m))));
            first.map_or("Ctrl", |(_, n)| *n).into()
        }
        HOLD => combo.map_or("Space".into(), |c| match c.key {
            Some(key) => key_text(key),
            None => combo_text(c),
        }),
        SHORTCUT | REPEAT | MACRO => combo.map_or(NEW_STEP.into(), combo_text),
        TEXT | RUN if matches!(old, Some(TEXT | RUN)) => line.content.clone(),
        TEXT => line.label.clone(),
        RUN => "notepad.exe".into(),
        PAGE_KIND => PAGE_TURNS[0].0.into(),
        _ => sets.iter().find(|s| !s.eq_ignore_ascii_case(own)).or(sets.first()).cloned().unwrap_or_default(),
    };
    // A held key sends one key: the first modifier stands for a few.
    if kind == HOLD && line.content.contains('+') {
        line.content = line.content.split('+').next().unwrap_or("Space").to_string();
    }
    line.kind = KIND_NAMES[kind].into();
    if kind != REPEAT {
        line.set_option("interval", None);
    }
    if kind != LAYER_KIND {
        line.set_option("mode", None);
    }
}

/// Make the chosen key (or its macro's chosen step) send `combo`, as its
/// kind can: a modifier key its first modifier, a held key one key.
unsafe fn send_combo(e: &mut Editor, field: usize, combo: Combo, first: Option<Key>) -> bool {
    let step = e.keymap.step;
    let done = unsafe {
        change(e, field, |line| {
            let first_mod = || first.or_else(|| MOD_KEYS.iter().map(|(k, _)| *k).find(|k| k.modifier().is_some_and(|m| combo.mods.contains(m))));
            match line.kind() {
                Some(MODIFIER) => line.content = key_text(first_mod().ok_or("修飾キーを選んでください")?),
                Some(HOLD) => line.content = key_text(combo.key.or_else(first_mod).ok_or("キーを選んでください")?),
                Some(SHORTCUT | REPEAT) => line.content = combo_text(combo),
                Some(MACRO) => {
                    let mut steps = line.steps();
                    let Some(s) = steps.get_mut(step).filter(|s| !is_wait(s)) else { return Err("キー操作の手順を選んでください".into()) };
                    *s = combo_text(combo);
                    line.content = steps.join(", ");
                }
                _ => return Err("この種類は送るキーを持たない".into()),
            }
            Ok(())
        })
    };
    if let Some(g) = combo.key.and_then(group_of).filter(|_| done) {
        e.keymap.group = g;
        unsafe { refresh(e) };
    }
    done
}

/// KLE から読み込む: choose a KLE file, copy it into the settings
/// file's folder, and lay the chosen set out by it (Issue #101). The
/// dialogs run with the window free: their messages reach it meanwhile.
unsafe fn import_kle() {
    let Some((hwnd, dir, set)) = with_editor(|e| Some((e.hwnd, e.path.parent()?.to_path_buf(), e.keymap.set.clone()?))).flatten() else {
        return;
    };
    let Some(from) = (unsafe { open_file(hwnd) }) else { return };
    let name = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut bytes = Vec::new();
    // One byte over the limit is enough to refuse it.
    let read = std::fs::File::open(&from).and_then(|f| std::io::Read::read_to_end(&mut std::io::Read::take(f, config::MAX_BYTES as u64 + 1), &mut bytes));
    let checked = read.map_err(|e| e.to_string()).and_then(|_| config::kle_keys(&bytes).map(|_| ()));
    if let Err(why) = checked {
        with_editor(|e| tell(e, &format!("「{name}」を読み込めません: {why}")));
        return;
    }
    let to = dir.join(&name);
    let settings = with_editor(|e| e.path.clone());
    if settings.as_ref().is_some_and(|s| s.file_name() == to.file_name()) {
        with_editor(|e| tell(e, &format!("「{name}」は設定ファイルと同じ名前なので読み込めません")));
        return;
    }
    if to != from {
        if std::fs::read(&to).is_ok_and(|there| there != bytes) {
            let text = wide(&format!("設定ファイルのフォルダに、別の中身の「{name}」があります。上書きしますか？"));
            let answer = unsafe { MessageBoxW(hwnd, text.as_ptr(), w!("KLE から読み込む"), MB_YESNO | MB_ICONQUESTION) };
            if answer != IDYES {
                return;
            }
        }
        if let Err(why) = std::fs::write(&to, &bytes) {
            with_editor(|e| tell(e, &format!("「{name}」を設定ファイルのフォルダに写せません: {why}")));
            return;
        }
    }
    with_editor(|e| unsafe {
        let section = format!("set.{set}");
        if e.doc.set_first(&section, "layout", &format!("kle:{name}")) {
            let key = e.keymap.key;
            choose(e, Some(set), key);
            mark_unsaved(e);
            super::notify(e, &format!("「{name}」の配列を読み込みました"));
        } else {
            tell(e, &format!("「{name}」は設定ファイルに書けない名前です"));
        }
    });
}

/// How far down a free layout's keys reach, in keys, as the file writes it.
fn free_bottom(doc: &Document, section: &str) -> String {
    let bottom = doc.keys(section).iter().filter_map(|k| config::key_def(k).ok()?.place).map(|p| p.y + p.h).max().unwrap_or(0);
    config::show_quarters(bottom)
}

/// The file chosen in an open dialog over `owner`, if one is. comdlg32
/// is loaded for it, not imported: imported, it would bring comctl32 into
/// EdgePad before any window opens (spec 8).
unsafe fn open_file(owner: HWND) -> Option<std::path::PathBuf> {
    type Open = unsafe extern "system" fn(*mut OPENFILENAMEW) -> i32;
    let mut file = [0u16; 1024];
    let filter = wide("KLE の raw data (*.json; *.txt)\0*.json;*.txt\0すべてのファイル\0*.*\0");
    let title = wide("KLE から読み込む");
    let mut ofn: OPENFILENAMEW = unsafe { zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_HIDEREADONLY;
    let (dll, f) = unsafe { system_proc(w!("comdlg32.dll"), b"GetOpenFileNameW\0") }?;
    let chosen = unsafe { std::mem::transmute::<_, Open>(f)(&mut ofn) };
    unsafe { FreeLibrary(dll) };
    if chosen == 0 {
        return None;
    }
    let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    Some(std::path::PathBuf::from(String::from_utf16_lossy(&file[..len])))
}

/// A part of the page was used: `code` is its notification.
pub(super) unsafe fn command(id: usize, code: u32) {
    match (id, code) {
        (LABEL, EN_CHANGE) => return unsafe { label_typed() },
        (_, EN_KILLFOCUS) if is_typed_id(id) => {
            if !unsafe { commit(id) } {
                let hwnd = with_quiet(|e| e.hwnd);
                if let Some(hwnd) = hwnd {
                    unsafe { PostMessageW(hwnd, super::WM_COMMIT, id, 0) };
                }
            }
            return;
        }
        _ => {}
    }
    let clicked = code == BN_CLICKED;
    if id == KLE && clicked {
        return unsafe { import_kle() };
    }
    let focus = with_editor(|e| unsafe {
        let set = e.keymap.set.clone().unwrap_or_default();
        let section = format!("set.{set}");
        let count = keys_of(e).len();
        let k = e.keymap.key;
        match id {
            SET if code == CBN_SELCHANGE => {
                let at = SendMessageW(part_of(e, SET), CB_GETCURSEL, 0, 0);
                if let Some(name) = usize::try_from(at).ok().and_then(|i| set_names(&e.doc).get(i).cloned()) {
                    choose(e, Some(name), 0);
                }
                None
            }
            ADD_SET if clicked => {
                let sets = set_names(&e.doc);
                let name = (1..).map(|n| if n == 1 { "new".to_string() } else { format!("new{n}") }).find(|n| !sets.contains(n)).unwrap_or_default();
                let section = format!("set.{name}");
                if e.doc.add_section(&section) && e.doc.insert_key(&section, 0, NEW_KEY) {
                    choose(e, Some(name), 0);
                    mark_unsaved(e);
                    Some(start_naming(e))
                } else {
                    tell(e, "キーセットを追加できません");
                    None
                }
            }
            RENAME if clicked && e.keymap.set.is_some() => Some(start_naming(e)),
            REMOVE_SET if clicked && e.keymap.set.is_some() => {
                let used = used_by(&e.doc, &set);
                if used.is_empty() {
                    e.doc.remove_section(&section);
                    choose(e, None, 0);
                    mark_unsaved(e);
                } else {
                    tell(e, &format!("「{set}」は {} で使っています。先に外してから削除してください", used.join("、")));
                }
                None
            }
            ADD_KEY if clicked && e.keymap.set.is_some() => {
                let at = if count == 0 { 0 } else { k + 1 };
                // In a free layout, in a row of its own under the rest.
                let free = e.doc.get(&section, "layout").is_some_and(|l| l.trim().eq_ignore_ascii_case("free"));
                let new = match free {
                    true => format!("{NEW_KEY} | x=0, y={}", free_bottom(&e.doc, &section)),
                    false => NEW_KEY.to_string(),
                };
                if e.doc.insert_key(&section, at, &new) {
                    choose(e, Some(set), at);
                    mark_unsaved(e);
                }
                None
            }
            REMOVE_KEY if clicked => {
                if count <= 1 {
                    tell(e, "キーセットにはキーが 1 つ以上要ります（キーセットごと消すには上の「削除」）");
                } else if e.doc.remove_key(&section, k) {
                    choose(e, Some(set), k.min(count - 2));
                    mark_unsaved(e);
                }
                None
            }
            KEY_UP | KEY_DOWN if clicked => {
                move_by(e, if id == KEY_UP { -1 } else { 1 });
                None
            }
            ARRANGE if code == CBN_SELCHANGE => {
                match SendMessageW(part_of(e, ARRANGE), CB_GETCURSEL, 0, 0) {
                    0 => arrange(e, false),
                    1 => arrange(e, true),
                    _ => {}
                }
                None
            }
            _ if clicked && (KINDS..KINDS + KIND_NAMES.len()).contains(&id) => {
                let (sets, step) = (set_names(&e.doc), e.keymap.step);
                change(e, id, |line| {
                    if line.kind() != Some(id - KINDS) {
                        convert(line, id - KINDS, step, &sets, &set);
                    }
                    Ok(())
                });
                None
            }
            _ if clicked && (MODS..MODS + 4).contains(&id) => {
                let Some((_, line)) = chosen_line(e) else { return None };
                let (key, _) = MOD_KEYS[id - MODS];
                let m = key.modifier().unwrap_or(Mods::NONE);
                let now = sent(&line, e.keymap.step).unwrap_or(Combo { mods: Mods::NONE, key: None });
                let combo = match line.kind() {
                    Some(MODIFIER | HOLD) => combo_of(key),
                    _ => Combo { mods: toggled(now.mods, m), key: now.key },
                };
                send_combo(e, id, combo, Some(key));
                None
            }
            _ if clicked && (GROUPS..GROUPS + 5).contains(&id) => {
                e.keymap.group = id - GROUPS;
                refresh(e);
                None
            }
            KEYS if code == LBN_SELCHANGE => {
                let at = SendMessageW(part_of(e, KEYS), LB_GETCURSEL, 0, 0);
                let Some(key) = usize::try_from(at).ok().and_then(|i| group_keys(e.keymap.group).get(i).copied()) else { return None };
                let Some((_, line)) = chosen_line(e) else { return None };
                let now = sent(&line, e.keymap.step).unwrap_or(Combo { mods: Mods::NONE, key: None });
                let combo = if line.kind() == Some(HOLD) { combo_of(key) } else { Combo { mods: now.mods, key: Some(key) } };
                send_combo(e, KEYS, combo, None);
                None
            }
            CAPTURE if clicked => {
                toggle_capture(e, Target::Key);
                None
            }
            STEPS if code == LBN_SELCHANGE => {
                let at = SendMessageW(part_of(e, STEPS), LB_GETCURSEL, 0, 0);
                if let Ok(at) = usize::try_from(at) {
                    e.keymap.step = at;
                    cancel_capture(e);
                    refresh(e);
                }
                None
            }
            STEP_KEY | STEP_WAIT | STEP_REMOVE | STEP_UP | STEP_DOWN if clicked => {
                edit_steps(e, id);
                None
            }
            TARGET if code == CBN_SELCHANGE => {
                let at = SendMessageW(part_of(e, TARGET), CB_GETCURSEL, 0, 0);
                let sets = set_names(&e.doc);
                let Ok(at) = usize::try_from(at) else { return None };
                let target = PAGE_TURNS.get(at).map(|(t, _)| t.to_string()).or_else(|| sets.get(at - PAGE_TURNS.len()).cloned());
                if let Some(target) = target {
                    change(e, TARGET, |line| {
                        line.content = target;
                        Ok(())
                    });
                }
                None
            }
            LAYER_SET if code == CBN_SELCHANGE => {
                let at = SendMessageW(part_of(e, LAYER_SET), CB_GETCURSEL, 0, 0);
                if let Some(target) = usize::try_from(at).ok().and_then(|i| set_names(&e.doc).get(i).cloned()) {
                    change(e, LAYER_SET, |line| {
                        line.content = target;
                        Ok(())
                    });
                }
                None
            }
            // Held is the default: written only when toggled.
            LAYER_HOLD | LAYER_TOGGLE if clicked => {
                change(e, id, |line| {
                    line.set_option("mode", (id == LAYER_TOGGLE).then(|| "toggle".to_string()));
                    Ok(())
                });
                None
            }
            PIE if code == CBN_SELCHANGE => {
                let at = SendMessageW(part_of(e, PIE), CB_GETCURSEL, 0, 0);
                if let Some(target) = usize::try_from(at).ok().and_then(|i| set_names(&e.doc).get(i).cloned()) {
                    change(e, PIE, |line| {
                        line.content = target;
                        Ok(())
                    });
                }
                None
            }
            _ => None,
        }
    })
    .flatten();
    if let Some(focus) = focus {
        unsafe { SetFocus(focus) };
    }
}

/// Run `f` on the window, if it is open and free, saying nothing if not.
fn with_quiet<R>(f: impl FnOnce(&mut Editor) -> R) -> Option<R> {
    super::EDITOR.with(|e| e.try_borrow_mut().ok().and_then(|mut e| e.as_mut().map(f)))
}

/// Choose key set `set` (or the first, with none) and its key `key`.
unsafe fn choose(e: &mut Editor, set: Option<String>, key: usize) {
    let k = &mut e.keymap;
    k.set = set;
    (k.key, k.step) = (key, 0);
    k.wrong = None;
    k.capture = None;
    k.naming = false;
    follow_group(e);
    unsafe { refresh(e) };
}

/// List the group of the key the chosen key sends, if it has one.
fn follow_group(e: &mut Editor) {
    let group = chosen_line(e).and_then(|(_, l)| sent(&l, e.keymap.step)).and_then(|c| c.key).and_then(group_of);
    if let Some(g) = group {
        e.keymap.group = g;
    }
}

/// Choose key `key` of the set shown: from the sample.
pub(super) unsafe fn choose_key(key: usize) {
    with_editor(|e| unsafe {
        if key < keys_of(e).len() {
            let set = e.keymap.set.clone();
            choose(e, set, key);
        }
    });
}

/// Choose key set `name` and its key `key`: the ones a save failed in.
pub(super) unsafe fn choose_set(name: &str, key: usize) {
    with_editor(|e| unsafe {
        if set_names(&e.doc).iter().any(|s| s == name) {
            choose(e, Some(name.into()), key);
        }
    });
}

/// The page that line `line` of `doc` is edited on, and its key, if it is
/// a key set's: キー配置 for its layout and a key's shape, キーマッピング
/// for what a key sends (Issue #109). A key is at fault for its shape when
/// it reads without it, or when it reads and its place is not as its
/// set's layout wants.
pub(super) fn fault(doc: &Document, line: usize) -> Option<(usize, usize)> {
    let (section, name) = doc.place(line);
    let section = section.filter(|s| s.starts_with("set."))?;
    let Some((_, key)) = doc.key_at(line).filter(|_| name.as_deref() == Some("key")) else { return Some((PAGE, 0)) };
    let raw = doc.keys(&section).into_iter().nth(key)?;
    let mut bare = Line::of(&raw);
    for name in ["width"].iter().chain(&PLACE_NAMES) {
        bare.set_option(name, None);
    }
    let shape = match config::key_def(&raw) {
        Err(_) => config::key_def(&rewrite(&raw, &bare)).is_ok(),
        Ok(_) => config::key_place(&raw).is_some() != (arrangement(doc, &section) == Arrangement::Free),
    };
    Some((if shape { PAGE } else { MAP_PAGE }, key))
}

/// Lay the chosen set out along its edge, or free: 並べ方 (Issue #109).
/// Free, each key goes where the sample draws it, a KLE file's places
/// written into the key lines, and its keys past them written as keys
/// that send nothing. Along the edge, the keys go in the order of their
/// places, top to bottom, then left to right, each as long as its place
/// is wide, in whole keys from 1 to 3. What the keys send is kept.
unsafe fn arrange(e: &mut Editor, free: bool) {
    let Some(set) = e.keymap.set.clone() else { return };
    let section = format!("set.{set}");
    let way = arrangement(&e.doc, &section);
    if free == (way == Arrangement::Free) {
        return;
    }
    let raws = e.doc.keys(&section);
    let mut lines: Vec<Line> = raws.iter().map(|r| Line::of(r)).collect();
    let kle = match &way {
        Arrangement::Kle(name) => match kle_places(e, name) {
            Some(keys) => keys,
            None => return tell(e, &format!("KLE のファイル「{name}」を読めません")),
        },
        _ => Vec::new(),
    };
    let places: Vec<Place> = match &way {
        Arrangement::Edge => {
            let widths: Vec<usize> = lines.iter().map(|l| l.option("width").and_then(|w| w.parse().ok()).filter(|w| (1..=3).contains(w)).unwrap_or(1)).collect();
            edge_places(&e.doc, &set, &widths)
        }
        Arrangement::Free => lines.iter().map(line_place).collect(),
        Arrangement::Kle(_) => (0..lines.len()).map(|i| kle.get(i).map_or(Place { x: 0, y: 0, w: 4, h: 4 }, |(p, _)| *p)).collect(),
    };
    let chosen = e.keymap.key;
    let mut order: Vec<usize> = (0..lines.len()).collect();
    for (line, place) in lines.iter_mut().zip(&places) {
        if free {
            line.set_option("width", None);
            set_place(line, *place);
        } else {
            for name in PLACE_NAMES {
                line.set_option(name, None);
            }
            line.set_option("width", Some(((place.w + 2) / 4).clamp(1, 3).to_string()).filter(|w| w != "1"));
        }
    }
    let mut written = lines.iter().zip(&raws).enumerate().all(|(i, (line, raw))| e.doc.set_key(&section, i, &rewrite(raw, line)));
    if free {
        written &= e.doc.set_first(&section, "layout", "free");
        for (i, (place, label)) in kle.iter().enumerate().skip(lines.len()) {
            let label = if label.trim().is_empty() { "-".into() } else { label.replace('|', "/") };
            let mut extra = Line::of(&format!("{label} | none | -"));
            set_place(&mut extra, *place);
            written &= e.doc.insert_key(&section, i, &format!("{label} | none | - | {}", extra.options_text()));
        }
    } else {
        written &= e.doc.set_value(&section, "layout", None);
        order.sort_by_key(|&i| (places[i].y, places[i].x));
        let mut at: Vec<usize> = (0..lines.len()).collect();
        for (to, key) in order.iter().enumerate() {
            let Some(from) = at.iter().position(|k| k == key) else { continue };
            written &= e.doc.move_key(&section, from, to);
            let k = at.remove(from);
            at.insert(to, k);
        }
    }
    if !written {
        tell(e, "並べ方を書き換えられません");
    }
    e.keymap.key = order.iter().position(|&k| k == chosen).unwrap_or(0);
    unsafe {
        refresh(e);
        mark_unsaved(e);
    }
}

/// Key `key` of the sample dragged to `packed`: x, y, w and h, a byte each.
pub(super) unsafe fn place_key(key: usize, packed: u32) {
    let at = |i: u32| (packed >> (8 * i) & 0xFF) as i32;
    let place = Place { x: at(0), y: at(1), w: at(2), h: at(3) };
    with_editor(|e| unsafe {
        if key >= keys_of(e).len() {
            return;
        }
        e.keymap.key = key;
        change(e, SAMPLE, |line| {
            set_place(line, place);
            Ok(())
        });
        // Turned away: the sample shows the file's place again.
        update_sample(e);
    });
}

/// Move the chosen key `delta` places, if it can go.
unsafe fn move_by(e: &mut Editor, delta: isize) {
    let (k, count) = (e.keymap.key, keys_of(e).len());
    let Some(to) = k.checked_add_signed(delta).filter(|&to| to < count) else { return };
    let set = e.keymap.set.clone().unwrap_or_default();
    if e.doc.move_key(&format!("set.{set}"), k, to) {
        e.keymap.key = to;
        unsafe {
            refresh(e);
            mark_unsaved(e);
        }
    }
}

/// Move the chosen key `delta` places: Alt+↑ and Alt+↓.
pub(super) unsafe fn move_key(delta: isize) {
    with_editor(|e| unsafe { move_by(e, delta) });
}

/// The macro's step buttons.
unsafe fn edit_steps(e: &mut Editor, id: usize) {
    let step = e.keymap.step;
    let mut chosen = step;
    unsafe {
        change(e, id, |line| {
            let mut steps = line.steps();
            match id {
                STEP_KEY | STEP_WAIT => {
                    chosen = step + 1;
                    steps.insert(chosen.min(steps.len()), if id == STEP_KEY { NEW_STEP } else { NEW_WAIT }.into());
                }
                STEP_REMOVE if steps.len() <= 1 => return Err("マクロには手順が 1 つ以上要る".into()),
                STEP_REMOVE => {
                    steps.remove(step);
                    chosen = step.min(steps.len() - 1);
                }
                _ => {
                    let to = if id == STEP_UP { step.checked_sub(1) } else { Some(step + 1).filter(|&t| t < steps.len()) };
                    let Some(to) = to else { return Ok(()) };
                    steps.swap(step, to);
                    chosen = to;
                }
            }
            line.content = steps.join(", ");
            Ok(())
        });
        e.keymap.step = chosen;
        refresh(e);
    }
}

/// Put the name field out, over the list of sets, to name the set.
fn start_naming(e: &mut Editor) -> HWND {
    // The name is typed, not captured.
    cancel_capture(e);
    e.keymap.naming = true;
    let name = part_of(e, NAME);
    set_text(name, e.keymap.set.as_deref().unwrap_or(""));
    unsafe {
        SendMessageW(name, EM_SETMODIFY, 1, 0);
        SendMessageW(name, EM_SETSEL, 0, -1);
        show(e);
    }
    name
}

/// The set named in the name field, or why it cannot be.
fn rename(e: &mut Editor, name: &str) -> Result<(), String> {
    let old = e.keymap.set.clone().unwrap_or_default();
    if name.eq_ignore_ascii_case(&old) {
        return Ok(());
    }
    if !config::valid_name(name) {
        return Err("キーセットの名前は英数字・-・_ で書く".into());
    }
    if set_names(&e.doc).iter().any(|s| s.eq_ignore_ascii_case(name)) {
        return Err(format!("「{name}」というキーセットはもうある"));
    }
    if !e.doc.rename_section(&format!("set.{old}"), &format!("set.{name}")) {
        return Err(format!("「{name}」はキーセットの名前にできない（next / prev / none は使えない）"));
    }
    e.keymap.set = Some(name.to_ascii_lowercase());
    Ok(())
}

/// Stop naming the set, the name typed left untaken: Esc.
pub(super) unsafe fn cancel_naming() -> bool {
    let focus = with_editor(|e| unsafe {
        if !e.keymap.naming {
            return None;
        }
        e.keymap.naming = false;
        e.keymap.wrong = None;
        refresh(e);
        super::ask_reason(e);
        Some(part_of(e, SET))
    })
    .flatten();
    if let Some(focus) = focus {
        unsafe { SetFocus(focus) };
    }
    focus.is_some()
}

/// Whether the name field is out.
pub(super) fn naming(hwnd: HWND) -> bool {
    with_quiet(|e| e.keymap.naming && part_of(e, NAME) == hwnd) == Some(true)
}

/// Take what is typed in field `id`, if it changed since it was last
/// taken. False if the window was in use, and nothing was done.
pub(super) unsafe fn commit(id: usize) -> bool {
    let mut focus = None;
    let done = with_editor(|e| unsafe {
        let field = part_of(e, id);
        if SendMessageW(field, EM_GETMODIFY, 0, 0) == 0 {
            return;
        }
        SendMessageW(field, EM_SETMODIFY, 0, 0);
        let typed = text_of(field).trim().to_string();
        match id {
            NAME if e.keymap.naming => {
                let renamed = rename(e, &typed);
                if renamed.is_ok() {
                    e.keymap.naming = false;
                    focus = Some(part_of(e, SET));
                }
                taken(e, NAME, renamed);
            }
            LABEL => {
                change(e, id, |line| {
                    line.label = typed;
                    Ok(())
                });
            }
            CONTENT => {
                change(e, id, |line| {
                    line.content = typed;
                    Ok(())
                });
            }
            WIDTH => {
                change(e, id, |line| {
                    line.set_option("width", (typed != "1").then_some(typed));
                    Ok(())
                });
            }
            // All four written, the one typed as it is: the parser says
            // whether it reads.
            _ if (PLACE..PLACE + 4).contains(&id) => {
                change(e, id, |line| {
                    set_place(line, line_place(line));
                    line.set_option(PLACE_NAMES[id - PLACE], Some(typed));
                    Ok(())
                });
            }
            INTERVAL => {
                change(e, id, |line| {
                    line.set_option("interval", Some(typed));
                    Ok(())
                });
            }
            WAIT => {
                let step = e.keymap.step;
                change(e, id, |line| {
                    let mut steps = line.steps();
                    let Some(s) = steps.get_mut(step).filter(|s| is_wait(s)) else { return Ok(()) };
                    *s = format!("wait {typed}");
                    line.content = steps.join(", ");
                    Ok(())
                });
            }
            _ => {}
        }
        // Turned away, it is entered again as it is, with Enter.
        if e.keymap.wrong.as_ref().is_some_and(|(wrong, _)| *wrong == id) {
            SendMessageW(field, EM_SETMODIFY, 1, 0);
        }
    })
    .is_some();
    if let Some(focus) = focus {
        unsafe { SetFocus(focus) };
    }
    done
}

/// Step the number of the field `hwnd`, if it has one, `delta` steps (ten
/// times as far with `shift`), and take it at once. Whether it had one.
pub(super) unsafe fn step(hwnd: HWND, delta: i32, shift: bool) -> bool {
    let id = unsafe { GetDlgCtrlID(hwnd) } as usize;
    let (min, max, by) = match id {
        WIDTH => (1, 3, 1),
        INTERVAL => (20, 1000, 10),
        WAIT => (0, 10_000, 100),
        _ => return false,
    };
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return false;
    }
    let by = by * if shift { 10 } else { 1 } * delta as i64;
    let now = text_of(hwnd).trim().parse::<i64>().unwrap_or(min);
    set_text(hwnd, &(now + by).clamp(min, max).to_string());
    unsafe {
        SendMessageW(hwnd, EM_SETMODIFY, 1, 0);
        commit(id);
    }
    true
}

/// Start capturing keys for `target`, or call it off if it is under way.
fn toggle_capture(e: &mut Editor, target: Target) {
    e.keymap.capture = match e.keymap.capture {
        Some(_) => None,
        None => Some(Capture { target, mods: Mods::NONE, first: None }),
    };
    show_capture(e);
}

/// The capture buttons say whether keys are being captured.
fn show_capture(e: &Editor) {
    let on = |t| e.keymap.capture.is_some_and(|c| c.target == t);
    let say = |on: bool| if on { "取り込み中（もう一度押すと取りやめ）" } else { "押して取り込む" };
    set_text(part_of(e, CAPTURE), say(on(Target::Key)));
    set_text(part_of(e, super::fields::CAPTURE), say(on(Target::Hotkey)));
}

/// The hotkey's 押して取り込む was pressed.
pub(super) unsafe fn capture_hotkey() {
    with_editor(|e| toggle_capture(e, Target::Hotkey));
}

/// Call off a capture under way: the page left, or the window.
pub(super) fn cancel_capture(e: &mut Editor) {
    if e.keymap.capture.take().is_some() {
        show_capture(e);
    }
}

/// Whether keys are being captured.
pub(super) fn capturing() -> bool {
    with_quiet(|e| e.keymap.capture.is_some()) == Some(true)
}

/// A key while keys are captured: a modifier is kept until a key comes
/// with it, or it is let go alone; a modifier key or a held key takes the
/// first key pressed. Every key is taken, none passed on.
pub(super) unsafe fn capture_key(msg: &MSG) -> bool {
    let vk = msg.wParam as u16;
    let down = matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN);
    let hotkey = with_editor(|e| unsafe {
        let Some(mut c) = e.keymap.capture else { return None };
        let kind = chosen_line(e).and_then(|(_, l)| l.kind());
        let alone = c.target == Target::Key && matches!(kind, Some(MODIFIER | HOLD));
        let combo = match (down, modifier_of_vk(vk)) {
            (true, Some(m)) => {
                c.mods = c.mods | m.modifier().unwrap_or(Mods::NONE);
                c.first.get_or_insert(m);
                e.keymap.capture = Some(c);
                if !alone {
                    return None;
                }
                combo_of(m)
            }
            (true, None) => {
                let Some(key) = key_of_vk(vk) else { return None };
                let mods = if alone { Mods::NONE } else { held_mods() };
                Combo { mods, key: Some(key) }
            }
            (false, Some(_)) if c.mods != Mods::NONE => Combo { mods: c.mods, key: None },
            _ => return None,
        };
        e.keymap.capture = None;
        show_capture(e);
        match c.target {
            Target::Key => {
                send_combo(e, CAPTURE, combo, c.first);
                None
            }
            Target::Hotkey if combo.key.is_none() => {
                tell(e, "hotkey には修飾キーのほかのキーも要ります");
                None
            }
            Target::Hotkey => Some(combo_text(combo)),
        }
    })
    .flatten();
    if let Some(text) = hotkey {
        unsafe { super::fields::set_hotkey(&text) };
    }
    true
}

/// The sample: the set's panel, keys in its colours and shape, the chosen
/// one as held; a key pressed is chosen, and the arrows choose the next
/// or the one before.
pub(super) unsafe extern "system" fn sample_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                if let Some(paint) = PAINT.get() {
                    draw_sample(hwnd, dc, paint.look);
                }
                EndPaint(hwnd, &ps);
                0
            }
            WM_LBUTTONDOWN => {
                SetFocus(hwnd);
                let p = point_of(lp);
                let mut client: RECT = zeroed();
                GetClientRect(hwnd, &mut client);
                let dpi = window_dpi(hwnd);
                let dragged = SAMPLE_VIEW.with_borrow_mut(|v| {
                    let (i, cell, unit) = v.hit(client, dpi, p)?;
                    if i < v.count {
                        PostMessageW(GetParent(hwnd), super::WM_CHOOSE_KEY, i, 0);
                    }
                    let places = v.places.as_ref().filter(|_| v.movable && i < v.count)?;
                    let from = places[i];
                    let pull = v.pull(i, cell, p, dpi);
                    v.drag = Some(Drag { key: i, pull, start: p, from, now: from, frame: frame(places), unit });
                    Some(())
                });
                if dragged.is_some() {
                    SetCapture(hwnd);
                }
                0
            }
            // As the sample shows it: turned back into the layout's place.
            WM_MOUSEMOVE => {
                let p = point_of(lp);
                let moved = SAMPLE_VIEW.with_borrow_mut(|v| {
                    let Some(mut d) = v.drag else { return false };
                    let q = |px: i32| (px as f64 * 4.0 / d.unit as f64).round() as i32;
                    let (dx, dy) = (q(p.x - d.start.x), q(p.y - d.start.y));
                    let (left, top, wide, tall) = d.frame;
                    let edge = v.edge.unwrap_or(Edge::Left);
                    let mut t = layout::turn(edge, wide, tall, Place { x: d.from.x - left, y: d.from.y - top, ..d.from });
                    match d.pull {
                        Pull::Move => (t.x, t.y) = (t.x + dx, t.y + dy),
                        Pull::Wide => t.w = (t.w + dx).max(1),
                        Pull::Tall => t.h = (t.h + dy).max(1),
                    }
                    let u = layout::unturn(edge, wide, tall, t);
                    let (most, largest) = (4 * config::MAX_PLACE, 4 * config::MAX_SIZE);
                    let now = Place { x: (u.x + left).clamp(0, most), y: (u.y + top).clamp(0, most), w: u.w.clamp(1, largest), h: u.h.clamp(1, largest) };
                    if now == d.now {
                        return false;
                    }
                    d.now = now;
                    v.drag = Some(d);
                    if let Some(place) = v.places.as_mut().and_then(|places| places.get_mut(d.key)) {
                        *place = now;
                    }
                    true
                });
                if moved {
                    InvalidateRect(hwnd, null(), 0);
                }
                0
            }
            WM_LBUTTONUP => {
                let done = SAMPLE_VIEW.with_borrow_mut(|v| v.drag.take());
                ReleaseCapture();
                if let Some(d) = done.filter(|d| d.now != d.from) {
                    let packed = d.now.x | d.now.y << 8 | d.now.w << 16 | d.now.h << 24;
                    PostMessageW(GetParent(hwnd), super::WM_PLACE_KEY, d.key, packed as u32 as LPARAM);
                }
                0
            }
            // Taken away mid-drag: the key back where it was.
            WM_CAPTURECHANGED => {
                SAMPLE_VIEW.with_borrow_mut(|v| {
                    if let Some(d) = v.drag.take() {
                        if let Some(place) = v.places.as_mut().and_then(|places| places.get_mut(d.key)) {
                            *place = d.from;
                        }
                    }
                });
                InvalidateRect(hwnd, null(), 0);
                0
            }
            // Over an end of the chosen key that can be dragged out: say so.
            WM_SETCURSOR if wp as HWND == hwnd && (lp & 0xFFFF) as u32 == HTCLIENT => {
                let mut p: POINT = zeroed();
                GetCursorPos(&mut p);
                ScreenToClient(hwnd, &mut p);
                let mut client: RECT = zeroed();
                GetClientRect(hwnd, &mut client);
                let dpi = window_dpi(hwnd);
                let pull = SAMPLE_VIEW.with_borrow(|v| {
                    let (i, cell, _) = v.hit(client, dpi, p).filter(|_| v.movable && v.drag.is_none())?;
                    Some(v.pull(i, cell, p, dpi))
                });
                let shape = match pull {
                    Some(Pull::Wide) => IDC_SIZEWE,
                    Some(Pull::Tall) => IDC_SIZENS,
                    _ => IDC_ARROW,
                };
                SetCursor(LoadCursorW(null_mut(), shape));
                1
            }
            WM_KEYDOWN => {
                let delta: isize = match wp as u16 {
                    VK_UP | VK_LEFT => -1,
                    VK_DOWN | VK_RIGHT => 1,
                    _ => return DefWindowProcW(hwnd, msg, wp, lp),
                };
                let (chosen, count) = SAMPLE_VIEW.with_borrow(|v| (v.chosen, v.count));
                if let Some(to) = chosen.checked_add_signed(delta).filter(|&to| to < count) {
                    PostMessageW(GetParent(hwnd), super::WM_CHOOSE_KEY, to, 0);
                }
                0
            }
            WM_SETFOCUS | WM_KILLFOCUS => {
                InvalidateRect(hwnd, null(), 0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// The point a mouse message's `lp` gives, in the client area.
fn point_of(lp: LPARAM) -> POINT {
    POINT { x: (lp & 0xFFFF) as i16 as i32, y: (lp >> 16 & 0xFFFF) as i16 as i32 }
}

unsafe fn draw_sample(hwnd: HWND, dc: HDC, look: super::Look) {
    unsafe {
        let dpi = window_dpi(hwnd);
        let fill = |r: &RECT, c: [u8; 3]| {
            let brush = CreateSolidBrush(colorref(c));
            FillRect(dc, r, brush);
            DeleteObject(brush);
        };
        let mut client: RECT = zeroed();
        GetClientRect(hwnd, &mut client);
        fill(&client, look.sidebar);
        SAMPLE_VIEW.with_borrow(|v| {
            let Some(palette) = v.palette.filter(|_| !v.labels.is_empty()) else { return };
            let (key, back, cells) = v.lay_out(client, dpi);
            fill(&back, palette.background);

            let (gap, corner) = (panel::key_gap(key), key / 6);
            let font = panel::key_font(key);
            let old_font = SelectObject(dc, font);
            let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
            SetBkMode(dc, TRANSPARENT as i32);
            for i in v.order(cells.len()) {
                let cell = &cells[i];
                let mut face = RECT { left: cell.left + gap, top: cell.top + gap, right: cell.right - gap, bottom: cell.bottom - gap };
                let chosen = i == v.chosen;
                let (colour, ink) = if chosen { (palette.held, palette.held_text) } else { (palette.face, palette.text) };
                let brush = CreateSolidBrush(colorref(colour));
                let old_brush = SelectObject(dc, brush);
                RoundRect(dc, face.left, face.top, face.right + 1, face.bottom + 1, corner, corner);
                SelectObject(dc, old_brush);
                DeleteObject(brush);
                SetTextColor(dc, colorref(ink));
                let label = &v.labels[i];
                DrawTextW(dc, label.as_ptr(), label.len() as i32, &mut face, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }
            SelectObject(dc, old_pen);
            SelectObject(dc, old_font);
            DeleteObject(font);
            if GetFocus() == hwnd {
                if let Some(&r) = cells.get(v.chosen) {
                    SetTextColor(dc, colorref(look.text));
                    SetBkColor(dc, colorref(look.sidebar));
                    let f = RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 };
                    DrawFocusRect(dc, &f);
                }
            }
        });
    }
}

/// The pie's figure: a half ring opening to the right, as from a left
/// edge, split among its items.
pub(super) unsafe extern "system" fn pie_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                if let Some(paint) = PAINT.get() {
                    draw_pie(hwnd, dc, paint.look);
                }
                EndPaint(hwnd, &ps);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

unsafe fn draw_pie(hwnd: HWND, dc: HDC, look: super::Look) {
    unsafe {
        let s = |v| scale(v, window_dpi(hwnd));
        let mut r: RECT = zeroed();
        GetClientRect(hwnd, &mut r);
        let back = CreateSolidBrush(colorref(look.background));
        FillRect(dc, &r, back);
        let items = PIE_ITEMS.get().min(8);
        let Some(palette) = SAMPLE_VIEW.with_borrow(|v| v.palette).filter(|_| items > 0) else {
            DeleteObject(back);
            return;
        };
        let radius = ((r.bottom - r.top) / 2 - s(4)).min(r.right - r.left - s(8)).max(1);
        let (cx, cy) = (r.left + s(4), (r.top + r.bottom) / 2);
        let face = CreateSolidBrush(colorref(palette.face));
        let pen = CreatePen(PS_SOLID, s(2).max(1), colorref(look.background));
        let old_brush = SelectObject(dc, face);
        let old_pen = SelectObject(dc, pen);
        let point = |a: f64| (cx + (radius as f64 * a.cos()).round() as i32, cy + (radius as f64 * a.sin()).round() as i32);
        let share = std::f64::consts::PI / items as f64;
        for i in 0..items {
            // From the top, down: counterclockwise as GDI goes, from the
            // sector's lower radius to its upper.
            let from = -std::f64::consts::FRAC_PI_2 + share * i as f64;
            let (start, end) = (point(from + share), point(from));
            Pie(dc, cx - radius, cy - radius, cx + radius, cy + radius, start.0, start.1, end.0, end.1);
        }
        let hole = radius / 4;
        SelectObject(dc, back);
        SelectObject(dc, GetStockObject(NULL_PEN));
        Ellipse(dc, cx - hole, cy - hole, cx + hole + 1, cy + hole + 1);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(pen);
        DeleteObject(face);
        DeleteObject(back);
    }
}

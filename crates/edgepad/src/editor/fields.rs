//! The rows of the pages of [general] and the edges (spec 7.1): 辺（共通）
//! ([edge]), 辺ごとの設定 ([edge.*], with a figure of the screen whose edges
//! choose the edge its rows edit), 見た目, タッチと修飾キー and 表示と非表示.
//! The edges' rows are the same on both edge pages, editing [edge] on one
//! and the chosen edge on the other (Issue #107).
//!
//! A row is a label in Japanese, the setting's name beside it, smaller, its
//! field (an edit control, a drop-down list, or a check box), a note of
//! what it takes, shown only while the field has the focus or the mouse,
//! and 既定に戻す when the setting is written in the file, or 既定値 when it
//! is not (共通 and 共通に戻す, for an edge's setting, on 辺ごとの設定). The
//! parts have control ids by row (FIELD, RESET, LABEL, NOTE, MARK, NAME +
//! the row's index), which the tests go by too. hotkey has 押して取り込む
//! (CAPTURE) beside it too, to press the keys rather than write them
//! (`keymap::Capture`); accent, its #RRGGBB field (ACCENT_HEX).
//!
//! A value is checked as the parser reads it before it goes into the
//! window's copy of the file: one it would turn away stays in its field,
//! marked, with the reason in the status line, and the file's copy keeps
//! the value it had. The ranges in the notes and the defaults shown are the
//! parser's own (`config::RANGES`, `config::default_value`).

use std::cell::Cell;
use std::ptr::{null, null_mut};

use edgepad_core::config::{self, Edge};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus, VK_DOWN, VK_LEFT, VK_RIGHT, VK_UP};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use super::{Editor, MARGIN, PAINT, mark_unsaved, scale, tell, wide, window_dpi, with_editor};
use crate::colors::colorref;
use crate::tray;

/// Control ids: the figure, hotkey's 押して取り込む; and by row, its field,
/// 既定に戻す, label, note and 既定値.
pub(super) const FIGURE: usize = 20;
pub(super) const CAPTURE: usize = 21;
pub(super) const ACCENT_HEX: usize = 22;
pub(super) const FIELD: usize = 400;
pub(super) const RESET: usize = 500;
const LABEL: usize = 600;
const NOTE: usize = 700;
const MARK: usize = 800;
const NAME: usize = 900;

/// The pages the rows are on, of `super::PAGES`.
pub(super) const COMMON_PAGE: usize = 3;
pub(super) const EDGE_PAGE: usize = 4;
pub(super) const LOOK_PAGE: usize = 5;
pub(super) const TOUCH_PAGE: usize = 6;
pub(super) const SHOW_PAGE: usize = 7;

/// Sizes in logical pixels (96 DPI).
const ROW_H: i32 = 26;
const PITCH: i32 = 30;
const LABEL_W: i32 = 190;
const NAME_W: i32 = 84;
const FIELD_W: i32 = 160;
const NOTE_W: i32 = 130;
const RESET_W: i32 = 96;
/// A drop-down list's height with its list open.
const LIST_H: i32 = 160;
const FIGURE_W: i32 = 160;
/// The figure's screen, inset from its edges; and each edge's band.
const INSET: i32 = 8;
const BAND: i32 = 12;
/// The figure, as tall as this many rows; the most rows a page has under
/// it. A row is PITCH apart, or as little as MIN_PITCH.
const FIGURE_ROWS: i32 = 5;
const EDGE_ROWS: i32 = 8;
const MIN_PITCH: i32 = 22;

const EDGES: [(Edge, &str); 4] = [(Edge::Left, "left"), (Edge::Right, "right"), (Edge::Top, "top"), (Edge::Bottom, "bottom")];

/// The edit controls' messages, of Win32_UI_Controls, a feature taken for
/// these alone otherwise.
pub(super) const EM_SETSEL: u32 = 0x00B1;
pub(super) const EM_GETMODIFY: u32 = 0x00B8;
pub(super) const EM_SETMODIFY: u32 = 0x00B9;
pub(super) const EM_SETREADONLY: u32 = 0x00CF;

#[derive(Clone, Copy, PartialEq)]
enum Group {
    /// [general]'s settings of the look, on 見た目.
    Look,
    /// [edge] on 辺（共通）, the edge chosen in the figure on 辺ごとの設定.
    Edge,
    /// [general]'s settings of what touches do, on タッチと修飾キー.
    Touch,
    /// [general]'s settings of showing and hiding, on 表示と非表示.
    Show,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A number, typed in, stepped by `Spec::step`.
    Number,
    /// Typed in.
    Text,
    /// The monitor: primary, cursor, or a number, chosen by name.
    Monitor,
    /// One of these values, chosen by its name: (value, name).
    Choice(&'static [(&'static str, &'static str)]),
    /// true or false.
    Check,
    /// columns, of the left and right edges; rows, of the top and bottom.
    Columns,
    Rows,
    /// auto, or a colour given in ACCENT_HEX.
    Accent,
    /// Whether the edge has a handle, `handle = none` or not.
    Handle,
    /// The handle's width, length or position: a part of `handle =`.
    HandlePart(usize),
}

struct Spec {
    group: Group,
    /// The setting's name, but for the parts of the handle.
    name: &'static str,
    /// What it changes, in Japanese.
    label: &'static str,
    /// What the field takes, for a setting with no range.
    note: &'static str,
    kind: Kind,
    /// What ↑, ↓ and the wheel step a number by.
    step: u32,
}

const fn spec(group: Group, name: &'static str, label: &'static str, note: &'static str, kind: Kind, step: u32) -> Spec {
    Spec { group, name, label, note, kind, step }
}

use Group::{Edge as E, Look as L, Show as S, Touch as T};
use Kind::*;

/// Every row, in the order the Tab key follows: the look's, then (after
/// the figure) the edges', then touch's and showing's.
const ROWS: [Spec; 22] = [
    spec(L, "key_size", "キーの大きさ", "", Number, 1),
    spec(L, "opacity", "ふだんの不透明度", "", Number, 1),
    spec(L, "dim_opacity", "パイメニュー・ペンの時の不透明度", "", Number, 1),
    spec(L, "theme", "テーマ", "", Choice(&[("light", "ライト"), ("dark", "ダーク"), ("auto", "Windows に合わせる")]), 0),
    spec(L, "accent", "アクセント色", "", Accent, 0),
    spec(L, "slide_ms", "出入りの時間", "", Number, 10),
    spec(E, "enabled", "表示する", "", Check, 0),
    spec(E, "dock", "画面の端を空ける", "", Check, 0),
    spec(E, "gap", "画面の端からの距離", "", Number, 1),
    spec(E, "monitor", "モニター", "", Monitor, 0),
    spec(E, "columns", "列数（左右の辺）", "", Columns, 1),
    spec(E, "rows", "行数（上下の辺）", "", Rows, 1),
    spec(E, "handle", "ハンドル", "", Handle, 0),
    spec(E, "handle の幅", "ハンドルの幅", "", HandlePart(0), 1),
    spec(E, "handle の長さ", "ハンドルの長さ", "", HandlePart(1), 1),
    spec(E, "handle の位置", "ハンドルの位置", "", HandlePart(2), 1),
    spec(T, "latch_ms", "ラッチになる長さ", "", Number, 10),
    spec(T, "latch", "ラッチの解け方", "", Choice(&[("oneshot", "1 回使うと解ける"), ("lock", "もう一度押すまで続く")]), 0),
    spec(T, "pen_presses", "ペンでキーを押す", "", Check, 0),
    spec(S, "hotkey", "ホットキー", "", Text, 0),
    spec(S, "hover_ms", "ハンドルに乗せて出すまでの時間", "", Number, 10),
    spec(S, "auto_hide_ms", "自動で隠すまでの時間", "", Number, 1000),
];

/// hotkey's row, and accent's.
const HOTKEY: usize = 19;
const ACCENT: usize = 4;

/// The names of the monitor's first choices, then the monitors by number.
const MONITOR_NAMES: [(&str, &str); 2] = [("primary", "主モニター"), ("cursor", "マウスのあるモニター")];
const ACCENT_NAMES: [&str; 2] = ["Windows に合わせる", "色を指定"];

/// The range of row `spec`'s number, if it has one.
fn range(spec: &Spec) -> Option<config::Range> {
    let r = &config::RANGES;
    match spec.kind {
        Columns | Rows => Some(r.lines),
        HandlePart(0) => Some(r.handle_width),
        HandlePart(1) => Some(r.handle_length),
        HandlePart(_) => Some(r.handle_position),
        Number => config::range_of(spec.name),
        _ => None,
    }
}

/// What the field of `spec` takes, as its note says it.
fn note_of(spec: &Spec) -> String {
    let span = |r: config::Range| format!("{}〜{}", r.min, r.max);
    match (spec.kind, range(spec)) {
        (HandlePart(1 | 2), Some(r)) => format!("{}%", span(r)),
        (_, Some(r)) if r.off => format!("0 でオフ、{}", span(r)),
        (_, Some(r)) => span(r),
        _ => spec.note.into(),
    }
}

/// The parser's default handle, as its width, length and position.
fn default_handle() -> Option<[u8; 3]> {
    match config::handle(&config::default_value("edge.left", "handle")?) {
        Ok(Some(h)) => Some([h.width, h.length, h.position]),
        _ => None,
    }
}

/// The windows of a row.
pub(super) struct Row {
    label: HWND,
    /// The setting's name, beside the label.
    name: HWND,
    field: HWND,
    note: HWND,
    /// 既定に戻す and 既定値 (or 共通); null on the handle's parts.
    reset: HWND,
    mark: HWND,
    /// Whether the file has the row's setting.
    written: bool,
    /// Why the value typed or chosen was turned away, while it stands.
    reason: Option<String>,
}

/// What the figure draws: the edge chosen, and for each edge whether it is
/// enabled and its handle's start and end along it, in hundredths. Kept
/// apart from the window's state, as `PAINT` is.
#[derive(Clone, Copy)]
struct Figure {
    chosen: usize,
    enabled: [bool; 4],
    handle: [Option<(i32, i32)>; 4],
}

thread_local! {
    static SHOWN: Cell<Figure> = const { Cell::new(Figure { chosen: 0, enabled: [false; 4], handle: [None; 4] }) };
    // The row the mouse is over, if any: its note shows.
    static HOVER: Cell<Option<usize>> = const { Cell::new(None) };
    // Where the rows go, as `place` was last given it: to place them
    // again as the page shown changes.
    static PLACE: Cell<(i32, i32, i32)> = const { Cell::new((0, 0, 0)) };
    // The setting names' smaller font.
    static NAME_FONT: Cell<HFONT> = const { Cell::new(null_mut()) };
    // The page and edge the rows were last shown for.
    static LAID: Cell<(usize, usize)> = const { Cell::new((usize::MAX, 0)) };
}

/// Whether `id` is a row's field: accent's #RRGGBB counts as accent's.
pub(super) fn is_field(id: usize) -> bool {
    (FIELD..FIELD + ROWS.len()).contains(&id) || id == ACCENT_HEX
}

/// The row of field `id`.
fn row_of_id(id: usize) -> usize {
    if id == ACCENT_HEX { ACCENT } else { id - FIELD }
}

pub(super) fn is_reset(id: usize) -> bool {
    (RESET..RESET + ROWS.len()).contains(&id)
}

/// The section row `spec` edits: [edge] on 辺（共通）.
fn section_of(e: &Editor, spec: &Spec) -> String {
    match spec.group {
        E if e.page == COMMON_PAGE => "edge".into(),
        E => format!("edge.{}", EDGES[e.edge].1),
        _ => "general".into(),
    }
}

/// The section of row `i`, and its setting's name there.
fn setting(e: &Editor, i: usize) -> (String, &'static str) {
    let spec = &ROWS[i];
    let name = match spec.kind {
        Handle | HandlePart(_) => "handle",
        _ => spec.name,
    };
    (section_of(e, spec), name)
}

fn text_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), 256) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Give `hwnd` `text`, unless it has it: an edit keeps its caret.
fn set_text(hwnd: HWND, text: &str) {
    if text_of(hwnd) != text {
        let text = wide(text);
        unsafe { SetWindowTextW(hwnd, text.as_ptr()) };
    }
}

unsafe fn control(parent: HWND, ex: u32, class: *const u16, text: &str, id: usize, style: u32) -> HWND {
    unsafe {
        let text = wide(text);
        let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null());
        CreateWindowExW(ex, class, text.as_ptr(), WS_CHILD | style, 0, 0, 1, 1, parent, id as HMENU, instance, null())
    }
}

/// Make the rows' windows and the figure in `parent`, in the order the Tab
/// key follows. None, if any could not be made.
pub(super) unsafe fn make(parent: HWND) -> Option<(Vec<Row>, HWND)> {
    let mut rows = Vec::new();
    let mut figure: HWND = null_mut();
    for (i, spec) in ROWS.iter().enumerate() {
        unsafe {
            if spec.group == E && figure.is_null() {
                figure = control(parent, 0, w!("EdgePadSettingsEdges"), "", FIGURE, WS_TABSTOP);
                if figure.is_null() {
                    return None;
                }
            }
            let label = control(parent, 0, w!("Static"), spec.label, LABEL + i, super::SS_NOPREFIX | super::SS_CENTERIMAGE);
            let name = control(parent, 0, w!("Static"), spec.name, NAME + i, super::SS_NOPREFIX | super::SS_CENTERIMAGE);
            let list = |names: &[&str]| {
                let combo = control(parent, 0, w!("ComboBox"), "", FIELD + i, WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32);
                for option in names {
                    let text = wide(option);
                    SendMessageW(combo, CB_ADDSTRING, 0, text.as_ptr() as LPARAM);
                }
                combo
            };
            let field = match spec.kind {
                Choice(options) => list(&options.iter().map(|(_, n)| *n).collect::<Vec<_>>()),
                // Filled as the monitors are, when shown.
                Monitor => list(&[]),
                Accent => list(&ACCENT_NAMES),
                Check | Handle => control(parent, 0, w!("Button"), "", FIELD + i, WS_TABSTOP | BS_AUTOCHECKBOX as u32),
                _ => control(parent, WS_EX_CLIENTEDGE, w!("Edit"), "", FIELD + i, WS_TABSTOP | ES_AUTOHSCROLL as u32),
            };
            let note = control(parent, 0, w!("Static"), &note_of(spec), NOTE + i, super::SS_NOPREFIX | super::SS_CENTERIMAGE);
            let own = !matches!(spec.kind, HandlePart(_));
            let (reset, mark) = if own {
                (
                    control(parent, 0, w!("Button"), "既定に戻す", RESET + i, WS_TABSTOP | BS_PUSHBUTTON as u32),
                    control(parent, 0, w!("Static"), "既定値", MARK + i, super::SS_NOPREFIX | super::SS_CENTERIMAGE),
                )
            } else {
                (null_mut(), null_mut())
            };
            let made = [label, name, field, note].iter().all(|w| !w.is_null()) && (!own || !reset.is_null() && !mark.is_null());
            rows.push(Row { label, name, field, note, reset, mark, written: false, reason: None });
            if !made {
                return None;
            }
            if i == ACCENT && control(parent, WS_EX_CLIENTEDGE, w!("Edit"), "", ACCENT_HEX, WS_TABSTOP | ES_AUTOHSCROLL as u32).is_null() {
                return None;
            }
            if i == HOTKEY && control(parent, 0, w!("Button"), "押して取り込む", CAPTURE, WS_TABSTOP | BS_PUSHBUTTON as u32).is_null() {
                return None;
            }
        }
    }
    Some((rows, figure))
}

/// Every part of the rows, for fonts and themes; not the names, which have
/// a font of their own (`set_name_font`).
pub(super) fn parts(e: &Editor) -> Vec<HWND> {
    let rows = e.rows.iter().flat_map(|r| [r.label, r.field, r.note, r.reset, r.mark]);
    rows.chain([capture(e), accent_hex(e)]).filter(|h| !h.is_null()).collect()
}

/// Give the setting names a font smaller than `font`, the window's.
pub(super) unsafe fn set_name_font(e: &Editor, font: HFONT) {
    unsafe {
        let mut lf: LOGFONTW = std::mem::zeroed();
        if GetObjectW(font as _, size_of::<LOGFONTW>() as i32, &mut lf as *mut LOGFONTW as _) == 0 {
            return;
        }
        lf.lfHeight = lf.lfHeight * 4 / 5;
        let small = CreateFontIndirectW(&lf);
        if small.is_null() {
            return;
        }
        for row in &e.rows {
            SendMessageW(row.name, WM_SETFONT, small as WPARAM, 1);
        }
        let old = NAME_FONT.replace(small);
        if !old.is_null() {
            DeleteObject(old);
        }
    }
}

/// Let go of the names' font, the window closing.
pub(super) fn drop_name_font() {
    let old = NAME_FONT.replace(null_mut());
    if !old.is_null() {
        unsafe { DeleteObject(old) };
    }
}

/// Whether `hwnd` is a setting's name: drawn quieter.
pub(super) fn is_name(hwnd: HWND) -> bool {
    let id = unsafe { GetDlgCtrlID(hwnd) } as usize;
    (NAME..NAME + ROWS.len()).contains(&id)
}

/// hotkey's 押して取り込む.
fn capture(e: &Editor) -> HWND {
    unsafe { GetDlgItem(e.hwnd, CAPTURE as i32) }
}

/// accent's #RRGGBB.
fn accent_hex(e: &Editor) -> HWND {
    unsafe { GetDlgItem(e.hwnd, ACCENT_HEX as i32) }
}

/// The rows' fields to theme as edit controls and drop-downs, and their
/// buttons (check boxes and 既定に戻す).
pub(super) fn themed(e: &Editor) -> (Vec<HWND>, Vec<HWND>) {
    let (mut fields, mut buttons) = (Vec::new(), Vec::new());
    for (spec, row) in ROWS.iter().zip(&e.rows) {
        match spec.kind {
            Check | Handle => buttons.push(row.field),
            _ => fields.push(row.field),
        }
        if !row.reset.is_null() {
            buttons.push(row.reset);
        }
    }
    buttons.push(capture(e));
    fields.push(accent_hex(e));
    (fields, buttons)
}

/// Whether row `i` is on the page shown: the edges' rows on both edge
/// pages, columns on 辺ごとの設定 for the left and right edges only, and
/// rows for the top and bottom.
fn on_page(e: &Editor, i: usize) -> bool {
    let spec = &ROWS[i];
    match spec.group {
        L => e.page == LOOK_PAGE,
        T => e.page == TOUCH_PAGE,
        S => e.page == SHOW_PAGE,
        E if e.page == COMMON_PAGE => true,
        E if e.page == EDGE_PAGE => match spec.kind {
            Columns => EDGES[e.edge].0.is_side(),
            Rows => !EDGES[e.edge].0.is_side(),
            _ => true,
        },
        E => false,
    }
}

/// Place the rows of the page shown in `left`, `top` to `bottom`, one
/// under another: on 辺ごとの設定, under the figure. Rows come closer where
/// the window has less room than they ask for. Kept, to place them again as
/// the page shown changes.
pub(super) unsafe fn place(e: &Editor, left: i32, top: i32, bottom: i32) {
    PLACE.set((left, top, bottom));
    let s = |v| scale(v, e.dpi);
    let m = s(MARGIN) / 2;
    let gap = s(MARGIN);
    let pitch = ((bottom - top - gap) / (FIGURE_ROWS + EDGE_ROWS)).clamp(s(MIN_PITCH), s(PITCH));
    let h = (pitch - s(PITCH - ROW_H)).max(s(MIN_PITCH - 4));
    let figure_h = pitch * FIGURE_ROWS - (pitch - h);
    let first = if e.page == EDGE_PAGE { top + figure_h + gap } else { top };
    let mut n = 0;
    for (i, (spec, row)) in ROWS.iter().zip(&e.rows).enumerate() {
        if !on_page(e, i) {
            continue;
        }
        let (x, y) = (left, first + pitch * n);
        n += 1;
        unsafe {
            MoveWindow(row.label, x, y, s(LABEL_W), h, 1);
            MoveWindow(row.name, x + s(LABEL_W) + m, y, s(NAME_W), h, 1);
            let fx = x + s(LABEL_W) + s(NAME_W) + 2 * m;
            let list = if matches!(spec.kind, Choice(_) | Monitor | Accent) { s(LIST_H) } else { h };
            let w = if matches!(spec.kind, Check | Handle) { h } else { s(FIELD_W) };
            MoveWindow(row.field, fx, y, w, list, 1);
            let nx = fx + s(FIELD_W) + m;
            MoveWindow(row.note, nx, y, s(NOTE_W), h, 1);
            for part in [row.reset, row.mark] {
                MoveWindow(part, nx + s(NOTE_W) + m, y, s(RESET_W), h, 1);
            }
            // Where the note would be: hotkey's has no range, nor accent's.
            if i == HOTKEY {
                MoveWindow(capture(e), nx, y, s(NOTE_W), h, 1);
            }
            if i == ACCENT {
                MoveWindow(accent_hex(e), nx, y, s(NOTE_W), h, 1);
            }
        }
    }
    unsafe { MoveWindow(e.figure, left, top, s(FIGURE_W), figure_h, 1) };
}

/// Whether accent's list says 色を指定.
fn accent_given(e: &Editor) -> bool {
    unsafe { SendMessageW(e.rows[ACCENT].field, CB_GETCURSEL, 0, 0) == 1 }
}

/// Show the rows of the page shown, as they are now, in their places, and
/// hide the rest. A note shows while its field has the focus or the mouse
/// is over its row.
pub(super) unsafe fn show(e: &Editor) {
    let (left, top, bottom) = PLACE.get();
    if bottom > top {
        unsafe { place(e, left, top, bottom) };
    }
    let shown = |hwnd: HWND, yes: bool| unsafe {
        if !hwnd.is_null() {
            ShowWindow(hwnd, if yes { SW_SHOWNA } else { SW_HIDE });
        }
    };
    let focus = unsafe { GetFocus() };
    let hover = HOVER.get();
    for (i, row) in e.rows.iter().enumerate() {
        let on = on_page(e, i);
        let looked_at = row.field == focus || hover == Some(i) || i == ACCENT && focus == accent_hex(e);
        shown(row.label, on);
        shown(row.name, on);
        shown(row.field, on);
        shown(row.note, on && looked_at && !text_of(row.note).is_empty());
        shown(row.reset, on && row.written);
        shown(row.mark, on && !row.written);
    }
    shown(e.figure, e.page == EDGE_PAGE);
    shown(capture(e), e.page == SHOW_PAGE);
    shown(accent_hex(e), e.page == LOOK_PAGE && accent_given(e));
    // Moved for another page or edge: nothing left of where they were.
    if LAID.replace((e.page, e.edge)) != (e.page, e.edge) {
        unsafe { RedrawWindow(e.hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN) };
    }
}

/// The mouse has left the window, or it is no longer in front: no row is
/// under the mouse.
pub(super) fn unhover() -> bool {
    HOVER.replace(None).is_some()
}

/// The mouse is over `hwnd`: the note of its row shows, and only that.
/// Whether the row it is over changed.
pub(super) fn hover(e: &Editor, hwnd: HWND) -> bool {
    let at = e.rows.iter().position(|r| [r.label, r.name, r.field, r.note, r.reset, r.mark].contains(&hwnd));
    let at = at.or_else(|| (hwnd == accent_hex(e) && !hwnd.is_null()).then_some(ACCENT));
    HOVER.replace(at) != at
}

/// hotkey captured as `text`: put in its field and taken, as if typed.
pub(super) unsafe fn set_hotkey(text: &str) {
    let Some(field) = with_editor(|e| e.rows[HOTKEY].field) else { return };
    set_text(field, text);
    unsafe {
        SendMessageW(field, EM_SETMODIFY, 1, 0);
        commit(FIELD + HOTKEY, true);
    }
}

/// The value of setting `name` an edge's `section` takes when it does not
/// write it: [edge]'s, if [edge] has it (Issue #97).
fn common<'a>(e: &'a Editor, section: &str, name: &str) -> Option<&'a str> {
    section.starts_with("edge.").then(|| e.doc.get("edge", name)).flatten()
}

/// The value of setting `name` in `section` as the file has it: written
/// there, or [edge]'s for an edge.
fn written_or_common<'a>(e: &'a Editor, section: &str, name: &str) -> Option<&'a str> {
    e.doc.get(section, name).or_else(|| common(e, section, name))
}

/// The handle as the file has it for `section`: none, or its parts. Not
/// written there or in [edge], or not as the parser reads it: the default.
fn handle_in(e: &Editor, section: &str) -> Option<[u8; 3]> {
    match written_or_common(e, section, "handle").map(config::handle) {
        Some(Ok(Some(h))) => Some([h.width, h.length, h.position]),
        Some(Ok(None)) => None,
        _ => default_handle(),
    }
}

/// Whether the edge of `section` is enabled, as the file has it or by
/// default.
fn enabled_in(e: &Editor, section: &str) -> bool {
    let value = written_or_common(e, section, "enabled").map(str::to_string).or_else(|| config::default_value(section, "enabled"));
    value.is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

/// Tell the figure what to draw, and draw it.
fn update_figure(e: &Editor) {
    let mut figure = Figure { chosen: e.edge, enabled: [false; 4], handle: [None; 4] };
    for (k, (_, name)) in EDGES.iter().enumerate() {
        let section = format!("edge.{name}");
        figure.enabled[k] = enabled_in(e, &section);
        figure.handle[k] = handle_in(e, &section).map(|[_, length, position]| {
            let (length, position) = (length as i32, position as i32);
            let start = (position - length / 2).clamp(0, 100 - length);
            (start, start + length)
        });
    }
    SHOWN.set(figure);
    unsafe { InvalidateRect(e.figure, null(), 0) };
}

/// Show row `i` as the window's copy of the file has it, a reason it had
/// gone. Whether it had one.
unsafe fn refresh_row(e: &mut Editor, i: usize) -> bool {
    let spec = &ROWS[i];
    let (section, name) = setting(e, i);
    let written = e.doc.get(&section, name).map(str::to_string);
    let wrong = written.as_deref().is_some_and(|v| config::check(&section, name, v).is_err());
    // Not written for the edge: [edge]'s, marked 共通, or the default.
    let shared = common(e, &section, name).map(str::to_string);
    let default = || shared.clone().or_else(|| config::default_value(&section, name)).unwrap_or_default();
    let value = written.clone().unwrap_or_else(default);
    let handle = handle_in(e, &section);
    let edge_page = e.page == EDGE_PAGE && spec.group == E;
    let hex = accent_hex(e);
    let row = &mut e.rows[i];
    row.written = written.is_some();
    let had = row.reason.take().is_some();
    unsafe {
        match spec.kind {
            Choice(options) => {
                let at = options.iter().position(|(o, _)| o.eq_ignore_ascii_case(value.trim()));
                SendMessageW(row.field, CB_SETCURSEL, at.map_or(usize::MAX, |a| a), 0);
            }
            // primary, cursor, then the monitors by number: as many as
            // there are, or as the file names.
            Monitor => {
                let value = value.trim().to_ascii_lowercase();
                let named = value.parse::<usize>().unwrap_or(0);
                let count = (GetSystemMetrics(SM_CMONITORS).max(1) as usize).max(named);
                let names: Vec<String> =
                    MONITOR_NAMES.iter().map(|(_, n)| n.to_string()).chain((1..=count).map(|k| format!("モニター {k}"))).collect();
                SendMessageW(row.field, CB_RESETCONTENT, 0, 0);
                for n in &names {
                    let text = wide(n);
                    SendMessageW(row.field, CB_ADDSTRING, 0, text.as_ptr() as LPARAM);
                }
                let at = MONITOR_NAMES.iter().position(|(v, _)| *v == value).or((named > 0).then(|| MONITOR_NAMES.len() + named - 1));
                SendMessageW(row.field, CB_SETCURSEL, at.map_or(usize::MAX, |a| a), 0);
            }
            Accent => {
                let auto = value.trim().eq_ignore_ascii_case("auto");
                SendMessageW(row.field, CB_SETCURSEL, (!auto) as usize, 0);
                set_text(hex, if auto { "" } else { value.trim() });
                SendMessageW(hex, EM_SETMODIFY, 0, 0);
            }
            Check => {
                let on = value.trim().eq_ignore_ascii_case("true");
                SendMessageW(row.field, BM_SETCHECK, on as WPARAM, 0);
            }
            Handle => {
                SendMessageW(row.field, BM_SETCHECK, handle.is_some() as WPARAM, 0);
            }
            HandlePart(k) => {
                // With no handle, the parts a handle would have by default.
                let shown = handle.or_else(default_handle).map_or_else(String::new, |h| h[k].to_string());
                set_text(row.field, &shown);
                SendMessageW(row.field, EM_SETMODIFY, 0, 0);
                EnableWindow(row.field, handle.is_some().into());
            }
            _ => {
                set_text(row.field, value.trim());
                SendMessageW(row.field, EM_SETMODIFY, 0, 0);
            }
        }
        if !row.mark.is_null() {
            set_text(row.mark, if shared.is_some() { "共通" } else { "既定値" });
            set_text(row.reset, if edge_page { "共通に戻す" } else { "既定に戻す" });
        }
        if wrong {
            row.reason = Some(format!("{name}: ファイルの値を読めない"));
        }
        InvalidateRect(row.field, null(), 1);
    }
    had
}

/// Show the rows `rows` as the window's copy of the file has them; if a
/// reason went with them, the status line says one still standing, or no
/// more than the question asked.
unsafe fn refresh_rows(e: &mut Editor, rows: impl IntoIterator<Item = usize>) {
    let mut had = false;
    for i in rows {
        had |= unsafe { refresh_row(e, i) };
    }
    if had {
        match super::standing_reason(e) {
            Some(reason) => tell(e, &reason),
            None => super::ask_reason(e),
        }
    }
    unsafe { show(e) };
    update_figure(e);
}

/// The reason a field's value was turned away, of one still standing.
pub(super) fn standing_reason(e: &Editor) -> Option<String> {
    e.rows.iter().find_map(|r| r.reason.clone())
}

/// Show every row as the window's copy of the file has it.
pub(super) unsafe fn refresh(e: &mut Editor) {
    unsafe { refresh_rows(e, 0..ROWS.len()) };
}

/// The rows of the setting of row `i`: the handle's go together.
fn rows_of(i: usize) -> Vec<usize> {
    match ROWS[i].kind {
        Handle | HandlePart(_) => (0..ROWS.len()).filter(|&j| matches!(ROWS[j].kind, Handle | HandlePart(_))).collect(),
        _ => vec![i],
    }
}

/// `handle =` as the handle's rows have it now.
fn handle_value(e: &Editor) -> String {
    let parts: Vec<String> = (0..ROWS.len())
        .filter(|&j| matches!(ROWS[j].kind, HandlePart(_)))
        .map(|j| text_of(e.rows[j].field).trim().trim_end_matches('%').trim().to_string())
        .collect();
    format!("{}, {}%, {}%", parts[0], parts[1], parts[2])
}

/// Whether `hwnd` is a row's edit control.
fn is_edit(e: &Editor, hwnd: HWND) -> Option<usize> {
    if hwnd.is_null() {
        return None;
    }
    let i = e.rows.iter().position(|r| r.field == hwnd)?;
    (!matches!(ROWS[i].kind, Choice(_) | Monitor | Accent | Check | Handle)).then_some(i)
}

/// Whether `hwnd` is a row's edit control, to type in.
pub(super) fn is_typed(e: &Editor, hwnd: HWND) -> bool {
    typed_row(e, hwnd).is_some()
}

/// Take the value of field `id` into the window's copy of the file, if the
/// parser reads it; else mark the field and say why. A typed value is
/// taken when it was changed since it was last taken, or with `always`.
/// False if the window was in use, and nothing was done.
pub(super) unsafe fn commit(id: usize, always: bool) -> bool {
    with_editor(|e| unsafe {
        let i = row_of_id(id);
        let spec = &ROWS[i];
        let row = &e.rows[i];
        let typed = is_edit(e, row.field).is_some();
        if typed && !always && SendMessageW(row.field, EM_GETMODIFY, 0, 0) == 0 {
            return;
        }
        let hex = accent_hex(e);
        if id == ACCENT_HEX && !always && SendMessageW(hex, EM_GETMODIFY, 0, 0) == 0 {
            return;
        }
        let (section, name) = setting(e, i);
        let value = match spec.kind {
            Choice(options) => {
                let at = SendMessageW(row.field, CB_GETCURSEL, 0, 0);
                let Some((option, _)) = usize::try_from(at).ok().and_then(|a| options.get(a)) else { return };
                option.to_string()
            }
            Monitor => {
                let Ok(at) = usize::try_from(SendMessageW(row.field, CB_GETCURSEL, 0, 0)) else { return };
                match MONITOR_NAMES.get(at) {
                    Some((value, _)) => value.to_string(),
                    None => (at - MONITOR_NAMES.len() + 1).to_string(),
                }
            }
            // 色を指定 with no colour yet: its field to give one in.
            Accent if !accent_given(e) => "auto".into(),
            // 色を指定 with no colour yet: Windows's accent to start from, in
            // the file at once, so the list and the file agree.
            Accent if text_of(hex).trim().is_empty() => {
                let [r, g, b] = crate::colors::palette(edgepad_core::config::Theme::Auto, edgepad_core::config::Accent::Auto).held;
                let start = format!("#{r:02X}{g:02X}{b:02X}");
                set_text(hex, &start);
                start
            }
            Accent => text_of(hex).trim().to_string(),
            Check => (SendMessageW(row.field, BM_GETCHECK, 0, 0) == 1).to_string(),
            Handle if SendMessageW(row.field, BM_GETCHECK, 0, 0) != 1 => "none".into(),
            Handle | HandlePart(_) => handle_value(e),
            _ => text_of(row.field).trim().to_string(),
        };
        let checked = config::check(&section, name, &value).and_then(|()| {
            let same = e.doc.get(&section, name) == Some(value.as_str());
            if same || e.doc.set_value(&section, name, Some(&value)) { Ok(()) } else { Err("この値はファイルに書けない".into()) }
        });
        match checked {
            Err(why) => {
                // Named as its label is: a part of the handle as that part.
                let label = if let HandlePart(_) = spec.kind { spec.name } else { name };
                let reason = if why.contains(label) { why } else { format!("{label}: {why}") };
                tell(e, &reason);
                e.rows[i].reason = Some(reason);
                InvalidateRect(e.rows[i].field, null(), 1);
                if i == ACCENT {
                    InvalidateRect(hex, null(), 1);
                }
            }
            Ok(()) => {
                refresh_rows(e, rows_of(i));
                mark_unsaved(e);
            }
        }
    })
    .is_some()
}

/// What `hwnd` is to the rows: the row whose typed field it is, accent's
/// #RRGGBB as accent's.
fn typed_row(e: &Editor, hwnd: HWND) -> Option<usize> {
    is_edit(e, hwnd).or_else(|| (hwnd == accent_hex(e) && !hwnd.is_null()).then_some(ACCENT))
}

/// 既定に戻す of row `id - RESET`: take its setting out of the window's
/// copy of the file, and put the focus on its field.
pub(super) unsafe fn reset(id: usize) {
    let focus = with_editor(|e| unsafe {
        let i = id - RESET;
        let (section, name) = setting(e, i);
        e.doc.set_value(&section, name, None);
        refresh_rows(e, rows_of(i));
        mark_unsaved(e);
        e.rows[i].field
    });
    if let Some(focus) = focus {
        unsafe { SetFocus(focus) };
    }
}

/// Whether the field `hwnd` shows a value turned away.
pub(super) fn is_wrong(hwnd: HWND) -> bool {
    super::EDITOR.with(|e| {
        e.try_borrow().ok().is_some_and(|e| {
            e.as_ref().is_some_and(|e| {
                let accent_wrong = hwnd == accent_hex(e) && e.rows[ACCENT].reason.is_some();
                accent_wrong || e.rows.iter().any(|r| r.field == hwnd && r.reason.is_some())
            })
        })
    })
}

/// Step the number of the field `hwnd`, if it has one, `delta` steps (ten
/// times as far with `shift`), and take it at once. Whether it had one.
pub(super) unsafe fn step(hwnd: HWND, delta: i32, shift: bool) -> bool {
    let stepped = with_editor(|e| {
        let i = is_edit(e, hwnd)?;
        let spec = &ROWS[i];
        let r = range(spec)?;
        // The handle's parts step only with a handle.
        if matches!(spec.kind, HandlePart(_)) && unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(hwnd) } == 0 {
            return None;
        }
        let by = spec.step as i64 * if shift { 10 } else { 1 } * delta as i64;
        let (min, max) = (r.min as i64, r.max as i64);
        let next = match text_of(hwnd).trim().trim_end_matches('%').trim().parse::<i64>() {
            Ok(0) if r.off => if by > 0 { min } else { 0 },
            Ok(now) => {
                let n = now + by;
                if r.off && n < min { 0 } else { n.clamp(min, max) }
            }
            // Not a number (primary, cursor): up from below the least.
            Err(_) => if by > 0 { min } else { return Some(()) },
        };
        set_text(hwnd, &next.to_string());
        unsafe { SendMessageW(hwnd, EM_SETMODIFY, 1, 0) };
        Some(())
    })
    .flatten();
    if stepped.is_some() {
        let id = unsafe { GetDlgCtrlID(hwnd) } as usize;
        unsafe { commit(id, true) };
    }
    stepped.is_some()
}

/// Choose edge `k` in the figure: the edge's rows show it.
pub(super) unsafe fn choose_edge(k: usize) {
    let done = with_editor(|e| unsafe {
        e.edge = k;
        let rows: Vec<usize> = (0..ROWS.len()).filter(|&j| ROWS[j].group == E).collect();
        // columns or rows, as the edge has.
        refresh_rows(e, rows);
    });
    if done.is_some() {
        tray::test_event(&format!("settings: edge {}", EDGES[k].1));
    }
}

/// Where a setting is: its page, its row, and its edge, if it is one of the
/// rows; for a line with none, the page of its section.
pub(super) fn locate(section: Option<&str>, name: Option<&str>) -> (usize, Option<usize>, Option<usize>) {
    let section = section.unwrap_or("general");
    let edge = section.strip_prefix("edge.").and_then(|n| EDGES.iter().position(|(_, e)| *e == n));
    let common = section == "edge";
    let group = |spec: &Spec| match spec.group {
        E => edge.is_some() || common,
        _ => section == "general",
    };
    let row = name.and_then(|name| {
        ROWS.iter().position(|spec| {
            let named = match spec.kind {
                Handle => name == "handle",
                _ => spec.name == name,
            };
            group(spec) && named
        })
    });
    // columns on the top or bottom, rows on the left or right: no field.
    let shown = |i: &usize| match (ROWS[*i].kind, edge) {
        (Columns, Some(k)) => EDGES[k].0.is_side(),
        (Rows, Some(k)) => !EDGES[k].0.is_side(),
        _ => true,
    };
    let row = row.filter(shown);
    let page = match (row.map(|i| ROWS[i].group), section) {
        (Some(L), _) => LOOK_PAGE,
        (Some(T), _) => TOUCH_PAGE,
        (Some(S), _) => SHOW_PAGE,
        (_, s) if s.starts_with("set.") => super::keymap::PAGE,
        (_, s) if s.starts_with("profile.") => super::apps::PAGE,
        (_, "edge") => COMMON_PAGE,
        (_, s) if s.starts_with("edge.") => EDGE_PAGE,
        _ => LOOK_PAGE,
    };
    (page, row, edge)
}

/// The field of row `i`.
pub(super) fn field(e: &Editor, i: usize) -> HWND {
    e.rows[i].field
}

/// The edge of the figure nearest `p`, in its client area `r`.
fn edge_at(r: RECT, p: POINT) -> usize {
    let away = [p.x - r.left, r.right - p.x, p.y - r.top, r.bottom - p.y];
    (0..4).min_by_key(|&k| away[k]).unwrap_or(0)
}

/// The figure: the screen, its four edges as bands along it, filled when
/// enabled and outlined when not, the chosen one in the accent; and each
/// handle as a mark along its edge.
pub(super) unsafe extern "system" fn figure_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                if let Some(paint) = PAINT.get() {
                    draw_figure(hwnd, dc, paint.look);
                }
                EndPaint(hwnd, &ps);
                0
            }
            WM_LBUTTONDOWN => {
                SetFocus(hwnd);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let p = POINT { x: (lp & 0xFFFF) as i16 as i32, y: (lp >> 16 & 0xFFFF) as i16 as i32 };
                PostMessageW(GetParent(hwnd), super::WM_CHOOSE_EDGE, edge_at(r, p), 0);
                0
            }
            WM_KEYDOWN => {
                let k = match wp as u16 {
                    VK_LEFT => 0,
                    VK_RIGHT => 1,
                    VK_UP => 2,
                    VK_DOWN => 3,
                    _ => return DefWindowProcW(hwnd, msg, wp, lp),
                };
                PostMessageW(GetParent(hwnd), super::WM_CHOOSE_EDGE, k, 0);
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

unsafe fn draw_figure(hwnd: HWND, dc: HDC, look: super::Look) {
    unsafe {
        let dpi = window_dpi(hwnd);
        let s = |v| scale(v, dpi);
        let fill = |r: &RECT, c: [u8; 3]| {
            let brush = CreateSolidBrush(colorref(c));
            FillRect(dc, r, brush);
            DeleteObject(brush);
        };
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        fill(&r, look.background);
        let screen = RECT { left: r.left + s(INSET), top: r.top + s(INSET), right: r.right - s(INSET), bottom: r.bottom - s(INSET) };
        fill(&screen, look.sidebar);
        let figure = SHOWN.get();
        let band = s(BAND);
        let plain = super::mix(look.text, look.background, 128);
        for k in 0..4 {
            let b = match k {
                0 => RECT { left: screen.left, top: screen.top + band, right: screen.left + band, bottom: screen.bottom - band },
                1 => RECT { left: screen.right - band, top: screen.top + band, right: screen.right, bottom: screen.bottom - band },
                2 => RECT { left: screen.left + band, top: screen.top, right: screen.right - band, bottom: screen.top + band },
                _ => RECT { left: screen.left + band, top: screen.bottom - band, right: screen.right - band, bottom: screen.bottom },
            };
            let chosen = figure.chosen == k;
            let colour = if chosen { look.accent } else { plain };
            if figure.enabled[k] {
                fill(&b, colour);
            } else {
                let w = s(if chosen { 3 } else { 2 });
                for edge in [
                    RECT { bottom: b.top + w, ..b },
                    RECT { top: b.bottom - w, ..b },
                    RECT { right: b.left + w, ..b },
                    RECT { left: b.right - w, ..b },
                ] {
                    fill(&edge, colour);
                }
            }
            if let Some((from, to)) = figure.handle[k] {
                let mark = if chosen && figure.enabled[k] { look.text } else { look.accent };
                let inner = s(3);
                let m = if k < 2 {
                    let h = b.bottom - b.top;
                    RECT { left: b.left + inner, right: b.right - inner, top: b.top + h * from / 100, bottom: b.top + h * to / 100 }
                } else {
                    let w = b.right - b.left;
                    RECT { top: b.top + inner, bottom: b.bottom - inner, left: b.left + w * from / 100, right: b.left + w * to / 100 }
                };
                fill(&m, mark);
            }
        }
        if GetFocus() == hwnd {
            SetTextColor(dc, colorref(look.text));
            SetBkColor(dc, colorref(look.background));
            let f = RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 };
            DrawFocusRect(dc, &f);
        }
    }
}

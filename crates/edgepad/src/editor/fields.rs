//! The rows of the 辺と表示 and 操作 pages: a field for every setting of
//! [general] and [edge.*] (spec 7.1); and the 辺と表示 page's figure of the
//! screen, whose edges choose the edge its rows edit.
//!
//! A row is a label with the setting's name, its field (an edit control, a
//! drop-down list, or a check box), a note of what it takes, and 既定に戻す
//! when the setting is written in the file, or 既定値 when it is not. The
//! parts have control ids by row (FIELD, RESET, LABEL, NOTE, MARK + the
//! row's index), which the tests go by too.
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

/// Control ids: the figure; and by row, its field, 既定に戻す, label, note
/// and 既定値.
pub(super) const FIGURE: usize = 20;
pub(super) const FIELD: usize = 400;
pub(super) const RESET: usize = 500;
const LABEL: usize = 600;
const NOTE: usize = 700;
const MARK: usize = 800;

/// The pages the rows are on, of `super::PAGES`.
pub(super) const EDGES_PAGE: usize = 2;
pub(super) const OPERATION_PAGE: usize = 3;

/// Sizes in logical pixels (96 DPI).
const ROW_H: i32 = 26;
const PITCH: i32 = 30;
const LABEL_W: i32 = 110;
const FIELD_W: i32 = 120;
const NOTE_W: i32 = 140;
const RESET_W: i32 = 96;
/// A drop-down list's height with its list open.
const LIST_H: i32 = 160;
const FIGURE_W: i32 = 160;
/// The figure's screen, inset from its edges; and each edge's band.
const INSET: i32 = 8;
const BAND: i32 = 12;
/// The rows of the edges page: the display rows, beside the figure, and the
/// edge's, under it. A row is PITCH apart, or as little as MIN_PITCH.
const DISPLAY_ROWS: i32 = 6;
const EDGE_ROWS: i32 = 8;
const MIN_PITCH: i32 = 22;

const EDGES: [(Edge, &str); 4] = [(Edge::Left, "left"), (Edge::Right, "right"), (Edge::Top, "top"), (Edge::Bottom, "bottom")];

/// The edit controls' messages, of Win32_UI_Controls, a feature taken for
/// these alone otherwise.
pub(super) const EM_SETSEL: u32 = 0x00B1;
const EM_GETMODIFY: u32 = 0x00B8;
const EM_SETMODIFY: u32 = 0x00B9;

#[derive(Clone, Copy, PartialEq)]
enum Group {
    /// [general]'s settings of the look, on 辺と表示.
    Display,
    /// The edge chosen in the figure, on 辺と表示.
    Edge,
    /// [general]'s settings of what touches do, on 操作.
    Operation,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A number, typed in, stepped by `Spec::step`.
    Number,
    /// Typed in.
    Text,
    /// The monitor: primary, cursor, or a number, stepped.
    Monitor,
    /// One of these values.
    Choice(&'static [&'static str]),
    /// true or false.
    Check,
    /// columns on the left and right edges, rows on the top and bottom.
    Lines,
    /// Whether the edge has a handle, `handle = none` or not.
    Handle,
    /// The handle's width, length or position: a part of `handle =`.
    HandlePart(usize),
}

struct Spec {
    group: Group,
    /// The label: the setting's name, but for the parts of the handle.
    name: &'static str,
    /// What the field takes, for a setting with no range.
    note: &'static str,
    kind: Kind,
    /// What ↑, ↓ and the wheel step a number by.
    step: u32,
}

const fn spec(group: Group, name: &'static str, note: &'static str, kind: Kind, step: u32) -> Spec {
    Spec { group, name, note, kind, step }
}

use Group::{Display as D, Edge as E, Operation as O};
use Kind::*;

/// Every row, in the order the Tab key follows: the display rows, then
/// (after the figure) the edge's, then the operation rows.
const ROWS: [Spec; 20] = [
    spec(D, "key_size", "", Number, 1),
    spec(D, "opacity", "", Number, 1),
    spec(D, "dim_opacity", "", Number, 1),
    spec(D, "slide_ms", "", Number, 10),
    spec(D, "theme", "light / dark / auto", Choice(&["light", "dark", "auto"]), 0),
    spec(D, "accent", "auto か #RRGGBB", Text, 0),
    spec(E, "enabled", "オンで表示", Check, 0),
    spec(E, "dock", "オンで画面の端を空ける", Check, 0),
    spec(E, "monitor", "", Monitor, 1),
    spec(E, "columns", "", Lines, 1),
    spec(E, "handle", "オンでハンドルあり", Handle, 0),
    spec(E, "handle の幅", "", HandlePart(0), 1),
    spec(E, "handle の長さ", "", HandlePart(1), 1),
    spec(E, "handle の位置", "", HandlePart(2), 1),
    spec(O, "latch_ms", "", Number, 10),
    spec(O, "latch", "oneshot / lock", Choice(&["oneshot", "lock"]), 0),
    spec(O, "pen_presses", "オンでペンも押せる", Check, 0),
    spec(O, "hotkey", "キーの組み合わせか none", Text, 0),
    spec(O, "hover_ms", "", Number, 10),
    spec(O, "auto_hide_ms", "", Number, 1000),
];

/// The range of row `spec`'s number, if it has one.
fn range(spec: &Spec) -> Option<config::Range> {
    let r = &config::RANGES;
    match spec.kind {
        Lines => Some(r.lines),
        Monitor => Some(r.monitor),
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
        (Monitor, Some(r)) => format!("primary / cursor / {}", span(r)),
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
    field: HWND,
    note: HWND,
    /// 既定に戻す and 既定値; null on the handle's parts.
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
}

pub(super) fn is_field(id: usize) -> bool {
    (FIELD..FIELD + ROWS.len()).contains(&id)
}

pub(super) fn is_reset(id: usize) -> bool {
    (RESET..RESET + ROWS.len()).contains(&id)
}

fn section_of(e: &Editor, spec: &Spec) -> String {
    match spec.group {
        E => format!("edge.{}", EDGES[e.edge].1),
        _ => "general".into(),
    }
}

/// The section of row `i`, and its setting's name there.
fn setting(e: &Editor, i: usize) -> (String, &'static str) {
    let spec = &ROWS[i];
    let name = match spec.kind {
        Lines if EDGES[e.edge].0.is_side() => "columns",
        Lines => "rows",
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
            let label = control(parent, 0, w!("Static"), spec.name, LABEL + i, super::SS_NOPREFIX | super::SS_CENTERIMAGE);
            let field = match spec.kind {
                Choice(options) => {
                    let combo = control(parent, 0, w!("ComboBox"), "", FIELD + i, WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32);
                    for option in options {
                        let text = wide(option);
                        SendMessageW(combo, CB_ADDSTRING, 0, text.as_ptr() as LPARAM);
                    }
                    combo
                }
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
            let made = [label, field, note].iter().all(|w| !w.is_null()) && (!own || !reset.is_null() && !mark.is_null());
            rows.push(Row { label, field, note, reset, mark, written: false, reason: None });
            if !made {
                return None;
            }
        }
    }
    Some((rows, figure))
}

/// Every part of the rows, for fonts and themes.
pub(super) fn parts(e: &Editor) -> Vec<HWND> {
    e.rows.iter().flat_map(|r| [r.label, r.field, r.note, r.reset, r.mark]).filter(|h| !h.is_null()).collect()
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
    (fields, buttons)
}

/// The page row `spec` is on.
fn page_of(spec: &Spec) -> usize {
    match spec.group {
        O => OPERATION_PAGE,
        _ => EDGES_PAGE,
    }
}

/// Place the rows and the figure in `left`, `top` to `bottom`: on the
/// edges page, the figure at the top, the display rows beside it, and the
/// edge's rows under it; the operation rows from the top. Rows come closer,
/// and lower, where the window has less room than they ask for.
pub(super) unsafe fn place(e: &Editor, left: i32, top: i32, bottom: i32) {
    let s = |v| scale(v, e.dpi);
    let m = s(MARGIN) / 2;
    let gap = s(MARGIN);
    let pitch = ((bottom - top - gap) / (DISPLAY_ROWS + EDGE_ROWS)).clamp(s(MIN_PITCH), s(PITCH));
    let h = (pitch - s(PITCH - ROW_H)).max(s(MIN_PITCH - 4));
    let (mut display, mut edge, mut operation) = (0, 0, 0);
    let display_left = left + s(FIGURE_W) + s(MARGIN);
    let edge_top = top + pitch * DISPLAY_ROWS + gap;
    for (spec, row) in ROWS.iter().zip(&e.rows) {
        let (x, y) = match spec.group {
            D => (display_left, top + pitch * post(&mut display)),
            E => (left, edge_top + pitch * post(&mut edge)),
            O => (left, top + pitch * post(&mut operation)),
        };
        unsafe {
            MoveWindow(row.label, x, y, s(LABEL_W), h, 1);
            let fx = x + s(LABEL_W) + m;
            let list = if matches!(spec.kind, Choice(_)) { s(LIST_H) } else { h };
            let w = if matches!(spec.kind, Check | Handle) { h } else { s(FIELD_W) };
            MoveWindow(row.field, fx, y, w, list, 1);
            let nx = fx + s(FIELD_W) + m;
            MoveWindow(row.note, nx, y, s(NOTE_W), h, 1);
            for part in [row.reset, row.mark] {
                MoveWindow(part, nx + s(NOTE_W) + m, y, s(RESET_W), h, 1);
            }
        }
    }
    unsafe { MoveWindow(e.figure, left, top, s(FIGURE_W), pitch * DISPLAY_ROWS - (pitch - h), 1) };
}

/// `n`, and one more for next time.
fn post(n: &mut i32) -> i32 {
    *n += 1;
    *n - 1
}

/// Show the rows of the page shown, as they are now, and hide the rest.
pub(super) unsafe fn show(e: &Editor) {
    let shown = |hwnd: HWND, yes: bool| unsafe {
        if !hwnd.is_null() {
            ShowWindow(hwnd, if yes { SW_SHOWNA } else { SW_HIDE });
        }
    };
    for (spec, row) in ROWS.iter().zip(&e.rows) {
        let on = page_of(spec) == e.page;
        shown(row.label, on);
        shown(row.field, on);
        shown(row.note, on);
        shown(row.reset, on && row.written);
        shown(row.mark, on && !row.written);
    }
    shown(e.figure, e.page == EDGES_PAGE);
}

/// The handle as the file has it for `section`: none, or its parts. Not
/// written, or not as the parser reads it: the default.
fn handle_in(e: &Editor, section: &str) -> Option<[u8; 3]> {
    match e.doc.get(section, "handle").map(config::handle) {
        Some(Ok(Some(h))) => Some([h.width, h.length, h.position]),
        Some(Ok(None)) => None,
        _ => default_handle(),
    }
}

/// Whether the edge of `section` is enabled, as the file has it or by
/// default.
fn enabled_in(e: &Editor, section: &str) -> bool {
    let value = e.doc.get(section, "enabled").map(str::to_string).or_else(|| config::default_value(section, "enabled"));
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
    let default = || config::default_value(&section, name).unwrap_or_default();
    let value = written.clone().unwrap_or_else(default);
    let handle = handle_in(e, &section);
    let row = &mut e.rows[i];
    row.written = written.is_some();
    let had = row.reason.take().is_some();
    unsafe {
        match spec.kind {
            Choice(options) => {
                let at = options.iter().position(|o| o.eq_ignore_ascii_case(value.trim()));
                SendMessageW(row.field, CB_SETCURSEL, at.map_or(usize::MAX, |a| a), 0);
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
        if spec.kind == Lines {
            set_text(row.label, name);
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
        match standing_reason(e) {
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
    (!matches!(ROWS[i].kind, Choice(_) | Check | Handle)).then_some(i)
}

/// Whether `hwnd` is a row's edit control, to type in.
pub(super) fn is_typed(e: &Editor, hwnd: HWND) -> bool {
    is_edit(e, hwnd).is_some()
}

/// Take the value of field `id` into the window's copy of the file, if the
/// parser reads it; else mark the field and say why. A typed value is
/// taken when it was changed since it was last taken, or with `always`.
/// False if the window was in use, and nothing was done.
pub(super) unsafe fn commit(id: usize, always: bool) -> bool {
    with_editor(|e| unsafe {
        let i = id - FIELD;
        let spec = &ROWS[i];
        let row = &e.rows[i];
        let typed = is_edit(e, row.field).is_some();
        if typed && !always && SendMessageW(row.field, EM_GETMODIFY, 0, 0) == 0 {
            return;
        }
        let (section, name) = setting(e, i);
        let value = match spec.kind {
            Choice(options) => {
                let at = SendMessageW(row.field, CB_GETCURSEL, 0, 0);
                let Some(option) = usize::try_from(at).ok().and_then(|a| options.get(a)) else { return };
                option.to_string()
            }
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
            }
            Ok(()) => {
                refresh_rows(e, rows_of(i));
                mark_unsaved(e);
            }
        }
    })
    .is_some()
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
        e.try_borrow().ok().is_some_and(|e| e.as_ref().is_some_and(|e| e.rows.iter().any(|r| r.field == hwnd && r.reason.is_some())))
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
    let group = |spec: &Spec| match (spec.group, edge) {
        (E, Some(_)) => true,
        (D | O, None) => section == "general",
        _ => false,
    };
    let row = name.and_then(|name| {
        ROWS.iter().position(|spec| {
            let named = match spec.kind {
                Lines => matches!(name, "columns" | "rows"),
                Handle => name == "handle",
                _ => spec.name == name,
            };
            group(spec) && named
        })
    });
    let page = match (row, section) {
        (Some(i), _) => page_of(&ROWS[i]),
        (None, s) if s.starts_with("set.") => 0,
        (None, s) if s.starts_with("profile.") => 1,
        _ => EDGES_PAGE,
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

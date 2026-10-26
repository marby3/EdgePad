//! The rows of the 一般 and 辺 tabs: a field for every setting of
//! [general] and [edge.*] (spec 7.1).
//!
//! A row is a label with the setting's name, its field, a note of what it
//! takes, and 既定に戻す when the setting is written in the file, or 既定値
//! when it is not: made in that order, as a label goes before what it
//! names, and the Tab key follows. A field is a box holding an edit
//! control, its text entered with Enter or on leaving it; or a choice,
//! drawn as segments, its window text the value chosen.
//!
//! A value is checked as the parser reads it before it goes into the
//! window's copy of the file: one it would turn away stays in its field,
//! marked, with the reason in the status line, and the file's copy keeps
//! the value it had.

use std::ptr::{null, null_mut};

use edgepad_core::config::{self, Edge};
use edgepad_core::theme::{self, Rgb};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use super::{
    BUTTON, CHOICE, Editor, LABEL, MARGIN, NOTE, ROW_GAP, ROW_TOP, ask_reason, part, part_dpi, scale, tell, wide,
    with_editor,
};
use crate::colors::colorref;
use crate::panel;

/// Control ids: the fields from FIELD on, their 既定に戻す from RESET on,
/// one for each row.
pub(super) const FIELD: usize = 400;
pub(super) const RESET: usize = 500;

/// Sizes in logical pixels (96 DPI).
const FIELD_H: i32 = 26;
const LABEL_W: i32 = 150;
const FIELD_W: i32 = 150;
const NOTE_W: i32 = 230;
const RESET_W: i32 = 110;
/// The edit control's height in its box, and the box's frame.
const EDIT_H: i32 = 20;
const FRAME: i32 = 2;

/// Edit control messages, of Win32_UI_Controls, a feature taken for
/// these alone otherwise.
pub(super) const EM_SETSEL: u32 = 0x00B1;
const EM_GETMODIFY: u32 = 0x00B8;
const EM_SETMODIFY: u32 = 0x00B9;

/// A part's GWLP_USERDATA with this set: its value is one the parser
/// turns away.
pub(super) const WRONG: isize = 0x100;

const ON_OFF: &[(&str, &str)] = &[("true", "オン"), ("false", "オフ")];
const EDGES: &[(&str, &str)] = &[("left", "left"), ("right", "right"), ("top", "top"), ("bottom", "bottom")];
const HANDLE: &[(&str, &str)] = &[("on", "あり"), ("none", "なし")];

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Edge,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Typed in.
    Text,
    /// One of these values, each shown as its label.
    Choice(&'static [(&'static str, &'static str)]),
    /// The edge the 辺 tab edits: no setting of its own.
    Edge,
    /// columns on the left and right edges, rows on the top and bottom:
    /// typed in.
    Lines,
    /// Whether the edge has a handle, `handle = none` or not.
    Handle,
    /// The handle's width, length or position: a part of `handle =`,
    /// typed in.
    HandlePart(usize),
}

struct Spec {
    tab: Tab,
    /// The label: the setting's name, but for the parts of the handle.
    name: &'static str,
    note: &'static str,
    /// What the field shows when the file does not have the setting: the
    /// parser's default.
    default: &'static str,
    kind: Kind,
}

const fn spec(tab: Tab, name: &'static str, note: &'static str, default: &'static str, kind: Kind) -> Spec {
    Spec { tab, name, note, default, kind }
}

use Kind::*;
use Tab::{Edge as E, General as G};

/// Every row, the 一般 tab's in the file's order, then the 辺 tab's.
const ROWS: [Spec; 21] = [
    spec(G, "hotkey", "キーの組み合わせか none", "none", Text),
    spec(G, "key_size", "24〜200", "56", Text),
    spec(G, "opacity", "10〜100", "90", Text),
    spec(G, "dim_opacity", "10〜100", "35", Text),
    spec(G, "latch_ms", "50〜2000", "250", Text),
    spec(G, "latch", "", "oneshot", Choice(&[("oneshot", "oneshot"), ("lock", "lock")])),
    spec(G, "pen_presses", "", "false", Choice(ON_OFF)),
    spec(G, "theme", "", "auto", Choice(&[("light", "light"), ("dark", "dark"), ("auto", "auto")])),
    spec(G, "accent", "auto か #RRGGBB", "auto", Text),
    spec(G, "hover_ms", "0 でオフ、100〜5000", "0", Text),
    spec(G, "auto_hide_ms", "0 でオフ、1000〜3600000", "0", Text),
    spec(G, "slide_ms", "0 でオフ、50〜500", "0", Text),
    spec(E, "辺", "", "left", Kind::Edge),
    spec(E, "enabled", "", "true", Choice(ON_OFF)),
    spec(E, "dock", "", "false", Choice(ON_OFF)),
    spec(E, "monitor", "primary / cursor / 1〜16", "primary", Text),
    spec(E, "columns", "1〜3", "1", Lines),
    spec(E, "handle", "", "on", Handle),
    spec(E, "handle の幅", "4〜8", "6", HandlePart(0)),
    spec(E, "handle の長さ", "1〜100%", "40", HandlePart(1)),
    spec(E, "handle の位置", "0〜100%", "50", HandlePart(2)),
];

/// The windows of a row.
pub(super) struct Row {
    label: HWND,
    /// The box or the choice.
    field: HWND,
    /// The edit control in the box; null for a choice.
    edit: HWND,
    note: HWND,
    /// 既定に戻す and 既定値; null on rows with no setting of their own.
    reset: HWND,
    mark: HWND,
    /// Whether the file has the row's setting.
    written: bool,
}

/// The values the choice `id` offers, and their labels.
pub(super) fn options(id: usize) -> &'static [(&'static str, &'static str)] {
    match ROWS.get(id.wrapping_sub(FIELD)).map(|s| s.kind) {
        Some(Choice(options)) => options,
        Some(Kind::Edge) => EDGES,
        Some(Handle) => HANDLE,
        _ => &[],
    }
}

/// Whether `id` is a row's field or 既定に戻す.
pub(super) fn is_field(id: usize) -> bool {
    (FIELD..FIELD + ROWS.len()).contains(&id)
}

pub(super) fn is_reset(id: usize) -> bool {
    (RESET..RESET + ROWS.len()).contains(&id)
}

fn edge_of(e: &Editor) -> Edge {
    Edge::parse(EDGES[e.edge].0).unwrap_or(Edge::Left)
}

/// The section row `i` is of, and its setting's name there.
fn setting(e: &Editor, i: usize) -> (String, &'static str) {
    let spec = &ROWS[i];
    let section = match spec.tab {
        G => "general".to_string(),
        E => format!("edge.{}", EDGES[e.edge].0),
    };
    let name = match spec.kind {
        Lines if edge_of(e).is_side() => "columns",
        Lines => "rows",
        Handle | HandlePart(_) => "handle",
        _ => spec.name,
    };
    (section, name)
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

/// Mark `hwnd`, a box or a choice, wrong or not.
fn set_wrong(hwnd: HWND, wrong: bool) {
    unsafe {
        let kind = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        let now = if wrong { kind | WRONG } else { kind & !WRONG };
        if now != kind {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, now);
            RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
        }
    }
}

/// Make the rows' windows in `parent`, in the order the Tab key follows.
/// None, if any could not be made.
pub(super) unsafe fn make(parent: HWND) -> Option<Vec<Row>> {
    let mut rows = Vec::new();
    for (i, spec) in ROWS.iter().enumerate() {
        unsafe {
            let id = FIELD + i;
            let label = part(parent, w!("EdgePadSettingsLabel"), spec.name, 0, LABEL);
            let (field, edit) = match spec.kind {
                Text | Lines | HandlePart(_) => {
                    let field = part(parent, w!("EdgePadSettingsField"), "", id, super::FIELD_BOX);
                    let style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32;
                    let edit = CreateWindowExW(0, w!("EDIT"), w!(""), style, 0, 0, 1, 1, field, 1 as HMENU, null_mut(), null());
                    (field, edit)
                }
                _ => (part(parent, w!("EdgePadSettingsChoice"), spec.default, id, CHOICE), null_mut()),
            };
            let note = part(parent, w!("EdgePadSettingsNote"), spec.note, 0, NOTE);
            let own = !matches!(spec.kind, Kind::Edge | HandlePart(_));
            let (reset, mark) = if own {
                (
                    part(parent, w!("EdgePadSettingsButton"), "既定に戻す", RESET + i, BUTTON),
                    part(parent, w!("EdgePadSettingsNote"), "既定値", 0, NOTE),
                )
            } else {
                (null_mut(), null_mut())
            };
            let typed = matches!(spec.kind, Text | Lines | HandlePart(_));
            let made = [label, field, note].iter().all(|w| !w.is_null())
                && (!typed || !edit.is_null())
                && (!own || !reset.is_null() && !mark.is_null());
            let row = Row { label, field, edit, note, reset, mark, written: false };
            rows.push(row);
            if !made {
                return None;
            }
        }
    }
    Some(rows)
}

/// Give the edit controls `font`.
pub(super) unsafe fn set_font(e: &Editor) {
    for row in e.rows.iter().filter(|r| !r.edit.is_null()) {
        unsafe { SendMessageW(row.edit, WM_SETFONT, e.font as usize, 1) };
    }
}

/// Place the rows in the client area `client`, at `dpi`: each tab's from
/// the top, one under another.
pub(super) unsafe fn place(e: &Editor, client: RECT) {
    let s = |v| scale(v, e.dpi);
    let (m, h) = (s(MARGIN), s(FIELD_H));
    let field_x = m + s(LABEL_W) + m;
    let note_x = field_x + s(FIELD_W) + m;
    let reset_x = client.right - m - s(RESET_W);
    let (mut general, mut edge) = (0, 0);
    for (spec, row) in ROWS.iter().zip(&e.rows) {
        let n = if spec.tab == G { &mut general } else { &mut edge };
        let y = s(ROW_TOP) + *n * (h + s(ROW_GAP));
        *n += 1;
        unsafe {
            MoveWindow(row.label, m, y, s(LABEL_W), h, 1);
            let wide = if row.edit.is_null() { note_x + s(NOTE_W) - field_x } else { s(FIELD_W) };
            MoveWindow(row.field, field_x, y, wide, h, 1);
            MoveWindow(row.note, note_x, y, s(NOTE_W), h, 1);
            for part in [row.reset, row.mark] {
                MoveWindow(part, reset_x, y, s(RESET_W), h, 1);
            }
        }
    }
}

/// Show the rows of tab `tab` of the window, as they are now, and hide
/// the rest.
pub(super) unsafe fn show(e: &Editor) {
    for (spec, row) in ROWS.iter().zip(&e.rows) {
        let on = match spec.tab {
            G => e.tab == 0,
            E => e.tab == 1,
        };
        let shown = |hwnd: HWND, yes: bool| unsafe {
            if !hwnd.is_null() {
                ShowWindow(hwnd, if on && yes { SW_SHOWNA } else { SW_HIDE });
            }
        };
        shown(row.label, true);
        shown(row.field, true);
        shown(row.note, !spec.note.is_empty());
        shown(row.reset, row.written);
        shown(row.mark, !row.written);
    }
}

/// The handle as the file has it for the edge shown: none, or its parts.
/// Not written, or not as the parser reads it: the default.
fn handle_now(e: &Editor) -> Option<[u8; 3]> {
    let section = format!("edge.{}", EDGES[e.edge].0);
    let default = Some([6, 40, 50]);
    match e.doc.get(&section, "handle").map(config::handle) {
        Some(Ok(Some(h))) => Some([h.width, h.length, h.position]),
        Some(Ok(None)) => None,
        _ => default,
    }
}

/// Show row `i` as the window's copy of the file has it.
unsafe fn refresh_row(e: &mut Editor, i: usize) {
    let spec = &ROWS[i];
    let (section, name) = setting(e, i);
    let written = e.doc.get(&section, name).map(str::to_string);
    let wrong = spec.kind != Kind::Edge && written.as_deref().is_some_and(|v| config::check(&section, name, v).is_err());
    let shown = match spec.kind {
        Text | Lines => written.clone().unwrap_or_else(|| spec.default.into()),
        Choice(_) => written.as_deref().map_or_else(|| spec.default.into(), str::to_ascii_lowercase),
        Kind::Edge => EDGES[e.edge].0.into(),
        Handle => if written.is_some() && handle_now(e).is_none() { "none" } else { "on" }.into(),
        HandlePart(k) => handle_now(e).unwrap_or([6, 40, 50])[k].to_string(),
    };
    let row = &mut e.rows[i];
    row.written = written.is_some();
    unsafe {
        if row.edit.is_null() {
            set_text(row.field, &shown);
        } else {
            set_text(row.edit, &shown);
            SendMessageW(row.edit, EM_SETMODIFY, 0, 0);
        }
        set_wrong(row.field, wrong);
        if spec.kind == Lines {
            set_text(row.label, name);
        }
    }
    if let HandlePart(_) = spec.kind {
        let on = handle_now(e).is_some();
        unsafe { EnableWindow(e.rows[i].edit, on.into()) };
    }
}

/// Show every row as the window's copy of the file has it.
pub(super) unsafe fn refresh(e: &mut Editor) {
    for i in 0..ROWS.len() {
        unsafe { refresh_row(e, i) };
    }
    unsafe { show(e) };
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
        .map(|j| text_of(e.rows[j].edit).trim().trim_end_matches('%').trim().to_string())
        .collect();
    format!("{}, {}%, {}%", parts[0], parts[1], parts[2])
}

/// Take the value of field `id` into the window's copy of the file, if
/// the parser reads it; else mark the field and say why. A typed value is
/// taken once only, when it has changed since.
pub(super) unsafe fn commit(id: usize) {
    let shown = with_editor(|e| unsafe {
        let i = id - FIELD;
        let spec = &ROWS[i];
        let row = &e.rows[i];
        if !row.edit.is_null() {
            if SendMessageW(row.edit, EM_GETMODIFY, 0, 0) == 0 {
                return false;
            }
            SendMessageW(row.edit, EM_SETMODIFY, 0, 0);
        }
        let text = text_of(if row.edit.is_null() { row.field } else { row.edit });
        if spec.kind == Kind::Edge {
            e.edge = EDGES.iter().position(|&(v, _)| v == text).unwrap_or(0);
            refresh(e);
            return false;
        }
        let (section, name) = setting(e, i);
        let value = match spec.kind {
            Handle if text == "none" => "none".to_string(),
            Handle | HandlePart(_) => handle_value(e),
            _ => text.trim().to_string(),
        };
        let checked = config::check(&section, name, &value).and_then(|()| {
            let same = e.doc.get(&section, name) == Some(value.as_str());
            if same || e.doc.set_value(&section, name, Some(&value)) { Ok(()) } else { Err("この値はファイルに書けない".into()) }
        });
        match checked {
            Err(why) => {
                set_wrong(e.rows[i].field, true);
                e.wrong_said = true;
                let reason = if why.contains(name) { why } else { format!("{name}: {why}") };
                tell(e, &reason);
                false
            }
            Ok(()) => {
                for j in rows_of(i) {
                    refresh_row(e, j);
                }
                if std::mem::take(&mut e.wrong_said) {
                    ask_reason(e);
                }
                true
            }
        }
    });
    if shown == Some(true) {
        with_editor(|e| unsafe { show(e) });
    }
}

/// 既定に戻す of row `id - RESET`: take its setting out of the window's
/// copy of the file, and put the focus on its field, 既定に戻す going.
pub(super) unsafe fn reset(id: usize) {
    let focus = with_editor(|e| unsafe {
        let i = id - RESET;
        let (section, name) = setting(e, i);
        e.doc.set_value(&section, name, None);
        for j in rows_of(i) {
            refresh_row(e, j);
        }
        show(e);
        let row = &e.rows[i];
        if row.edit.is_null() { row.field } else { row.edit }
    });
    if let Some(focus) = focus {
        unsafe { SetFocus(focus) };
    }
}

/// The edit control of a row's field, if `hwnd` is one.
pub(super) fn is_edit(e: &Editor, hwnd: HWND) -> bool {
    !hwnd.is_null() && e.rows.iter().any(|r| r.edit == hwnd)
}

/// Choose the option `delta` along from the one chosen in `choice`: round
/// from the last to the first with `wrap`, else no further than either.
pub(super) unsafe fn step(choice: HWND, delta: isize, wrap: bool) {
    unsafe {
        let id = GetDlgCtrlID(choice) as usize;
        let options = options(id);
        if options.is_empty() {
            return;
        }
        let now = text_of(choice);
        let at = options.iter().position(|&(v, _)| v.eq_ignore_ascii_case(&now)).unwrap_or(0) as isize;
        let n = options.len() as isize;
        let next = if wrap { (at + delta).rem_euclid(n) } else { (at + delta).clamp(0, n - 1) } as usize;
        if options[next].0 != now {
            set_text(choice, options[next].0);
            PostMessageW(GetParent(choice), WM_COMMAND, id, 0);
        }
    }
}

/// A box's frame and fill: wrong, focused, or neither.
fn box_colours(wrong: bool, focused: bool) -> (Rgb, Rgb) {
    let c = panel::palette();
    let dark = theme::is_dark(c.background);
    let fill = match (wrong, dark) {
        (true, true) => [0x5C, 0x24, 0x24],
        (true, false) => [0xF8, 0xD0, 0xD0],
        _ => c.face,
    };
    let frame = if wrong {
        [0xD0, 0x30, 0x30]
    } else if focused {
        c.held
    } else {
        fill
    };
    (frame, fill)
}

/// `a` mixed with `b`, `part` of 256 of `b`.
pub(super) fn mix(a: Rgb, b: Rgb, part: u16) -> Rgb {
    std::array::from_fn(|i| ((a[i] as u16 * (256 - part) + b[i] as u16 * part) / 256) as u8)
}

/// A field's box: draws its frame, in the accent while its edit control
/// has the focus and in red while its value is wrong; colours the edit
/// control; and tells the window when typing in it is done.
pub(super) unsafe extern "system" fn box_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let wrong = GetWindowLongPtrW(hwnd, GWLP_USERDATA) & WRONG != 0;
        let edit = GetWindow(hwnd, GW_CHILD);
        match msg {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let (frame, fill) = box_colours(wrong, !edit.is_null() && GetFocus() == edit);
                let brush = CreateSolidBrush(colorref(frame));
                FillRect(dc, &r, brush);
                DeleteObject(brush);
                let b = scale(FRAME, part_dpi(hwnd));
                let inner = RECT { left: r.left + b, top: r.top + b, right: r.right - b, bottom: r.bottom - b };
                let brush = CreateSolidBrush(colorref(fill));
                FillRect(dc, &inner, brush);
                DeleteObject(brush);
                EndPaint(hwnd, &ps);
                0
            }
            WM_SIZE => {
                let dpi = part_dpi(hwnd);
                let (w, h) = ((lp & 0xFFFF) as i32, (lp >> 16 & 0xFFFF) as i32);
                let (inset, eh) = (scale(MARGIN - 2, dpi), scale(EDIT_H, dpi));
                MoveWindow(edit, inset, (h - eh) / 2, (w - 2 * inset).max(0), eh, 1);
                0
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
                let dc = wp as HDC;
                let c = panel::palette();
                let (_, fill) = box_colours(wrong, false);
                // Greyed, as the handle's parts are with no handle.
                let ink = if msg == WM_CTLCOLORSTATIC { mix(c.text, fill, 128) } else { c.text };
                SetTextColor(dc, colorref(ink));
                SetBkColor(dc, colorref(fill));
                SetDCBrushColor(dc, colorref(fill));
                GetStockObject(DC_BRUSH) as LRESULT
            }
            WM_COMMAND => {
                let code = (wp >> 16 & 0xFFFF) as u32;
                if code == EN_SETFOCUS || code == EN_KILLFOCUS {
                    InvalidateRect(hwnd, null(), 1);
                }
                // Come to by Tab: what is there selected, to type over.
                if code == EN_SETFOCUS {
                    SendMessageW(edit, EM_SETSEL, 0, -1);
                }
                // Left: what was typed is entered, once the window is free.
                if code == EN_KILLFOCUS {
                    PostMessageW(GetParent(hwnd), WM_COMMAND, GetDlgCtrlID(hwnd) as usize, 0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// Draw choice `hwnd` into `dc`: a segment for each option, the one chosen
/// in the accent, a frame round it all while it has the focus (red while
/// its value is wrong).
pub(super) unsafe fn draw_choice(hwnd: HWND, dc: HDC, r: RECT, wrong: bool) {
    unsafe {
        let c = panel::palette();
        let options = options(GetDlgCtrlID(hwnd) as usize);
        let now = text_of(hwnd);
        let gap = scale(2, part_dpi(hwnd));
        let fill = |r: &RECT, colour: Rgb| {
            let brush = CreateSolidBrush(colorref(colour));
            FillRect(dc, r, brush);
            DeleteObject(brush);
        };
        let focused = GetFocus() == hwnd;
        let frame = if wrong {
            [0xD0, 0x30, 0x30]
        } else if focused {
            c.text
        } else {
            c.background
        };
        fill(&r, frame);
        let inner = RECT { left: r.left + gap, top: r.top + gap, right: r.right - gap, bottom: r.bottom - gap };
        fill(&inner, c.background);
        let n = options.len().max(1) as i32;
        let w = inner.right - inner.left;
        SetBkMode(dc, TRANSPARENT as i32);
        for (i, &(value, label)) in options.iter().enumerate() {
            let i = i as i32;
            let mut seg = RECT {
                left: inner.left + w * i / n + if i > 0 { gap / 2 } else { 0 },
                top: inner.top,
                right: inner.left + w * (i + 1) / n - if i < n - 1 { gap / 2 } else { 0 },
                bottom: inner.bottom,
            };
            let chosen = value.eq_ignore_ascii_case(&now);
            let (face, ink) = if chosen { (c.held, c.held_text) } else { (c.face, c.text) };
            fill(&seg, face);
            SetTextColor(dc, colorref(ink));
            let label = wide(label);
            DrawTextW(dc, label.as_ptr(), -1, &mut seg, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        }
    }
}

/// The option of choice `hwnd` at screen x `x`.
pub(super) unsafe fn option_at(hwnd: HWND, x: i32) -> Option<&'static str> {
    unsafe {
        let options = options(GetDlgCtrlID(hwnd) as usize);
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        let w = (r.right - r.left).max(1);
        let i = ((x - r.left) * options.len() as i32 / w).clamp(0, options.len() as i32 - 1);
        options.get(i as usize).map(|&(v, _)| v)
    }
}

/// Choose `value` in choice `hwnd`, by a click, and tell the window.
pub(super) unsafe fn choose(hwnd: HWND, value: &str) {
    if text_of(hwnd) != value {
        set_text(hwnd, value);
        unsafe { PostMessageW(GetParent(hwnd), WM_COMMAND, GetDlgCtrlID(hwnd) as usize, 0) };
    }
}

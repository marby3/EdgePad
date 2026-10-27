//! The アプリ別 page (spec 7.1): the profiles, [profile.*], which key set
//! each edge shows while an app is in front.
//!
//! On the left, the apps: 既定（ほかのアプリ） first, the default profile,
//! then each profile's by its process, the one in effect marked ▶ (as the
//! running EdgePad says, asked with `profile::active_message`). Under them,
//! 開いているアプリから追加, which gathers the apps with a window shown, once,
//! into a list one can also type a process name in, and 削除. On the right,
//! the chosen app's process, and a figure of the screen with, on each edge,
//! a drop-down list of the key set it shows, or なし.
//!
//! A change rewrites its line alone in the window's copy of the file, as on
//! the other pages; a new profile is named after its process
//! (CLIPStudioPaint.exe → clipstudiopaint).

use std::cell::Cell;
use std::mem::zeroed;
use std::ptr::{null, null_mut};

use edgepad_core::config;
use edgepad_core::document::Document;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus, VK_DOWN, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use super::fields::{EM_GETMODIFY, EM_SETMODIFY};
use super::{Editor, MARGIN, PAINT, mark_unsaved, scale, tell, wide, window_dpi, with_editor};
use crate::colors::colorref;
use crate::profile;

/// The page, of `super::PAGES`.
pub(super) const PAGE: usize = 1;

/// Control ids, which the tests find the parts by: the apps and their
/// buttons; the process to add, typed or chosen, with 追加 and やめる; the
/// chosen app's process (or, for the default, what it is for); the figure,
/// and the key set of each of its edges, left, right, top and bottom.
const APPS: usize = 2000;
const ADD_APP: usize = 2001;
const REMOVE_APP: usize = 2002;
const CANDIDATES: usize = 2003;
const ADD: usize = 2004;
const CANCEL_ADD: usize = 2005;
const TITLE_PROCESS: usize = 2010;
const PROCESS: usize = 2011;
const NOTE: usize = 2012;
const FIGURE: usize = 2013;
const EDGES: usize = 2020;

pub(super) const FIGURE_CLASS: *const u16 = w!("EdgePadSettingsApps");

const DEFAULT: &str = "default";
const DEFAULT_LABEL: &str = "既定（ほかのアプリ）";
const MARK: &str = "▶ ";
const NONE: &str = "なし";
const EDGE_NAMES: [&str; 4] = ["left", "right", "top", "bottom"];

/// Sizes in logical pixels (96 DPI).
const ROW_H: i32 = 26;
const LIST_W: i32 = 220;
const LIST_H: i32 = 200;
const FIGURE_W: i32 = 440;
const FIGURE_H: i32 = 280;
const EDGE_W: i32 = 120;
/// The figure's screen, inset from its edges; and each edge's band.
const INSET: i32 = 8;
const BAND: i32 = 12;

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Text,
    Edit,
    Button,
    List,
    /// A drop-down list one can type in.
    Typed,
    /// A drop-down list, out of the Tab key's way: the figure leads to it.
    Edge,
    Figure,
}

use Class::*;

/// Every part, in the order the Tab key follows: the figure before the
/// lists on it, which stand over it.
const PARTS: &[(usize, Class, &str)] = &[
    (APPS, List, ""),
    (ADD_APP, Button, "開いているアプリから追加"),
    (REMOVE_APP, Button, "削除"),
    (CANDIDATES, Typed, ""),
    (ADD, Button, "追加"),
    (CANCEL_ADD, Button, "やめる"),
    (TITLE_PROCESS, Text, "プロセス名"),
    (PROCESS, Edit, ""),
    (NOTE, Text, "ほかのどのプロファイルにも当たらないアプリ"),
    (FIGURE, Figure, ""),
    (EDGES, Edge, ""),
    (EDGES + 1, Edge, ""),
    (EDGES + 2, Edge, ""),
    (EDGES + 3, Edge, ""),
];

/// The page's state, in the window's.
#[derive(Default)]
pub(super) struct Apps {
    /// The chosen profile, by its lower-case name; none for the default.
    profile: Option<String>,
    /// Whether the process to add is being chosen.
    adding: bool,
    /// The profile in effect in the running EdgePad, by its name.
    active: Option<String>,
    /// The field whose value was turned away, and why, while it stands.
    wrong: Option<(usize, String)>,
}

thread_local! {
    // The figure's edge chosen by the keyboard, of EDGE_NAMES.
    static EDGE: Cell<usize> = const { Cell::new(0) };
}

fn part_of(e: &Editor, id: usize) -> HWND {
    unsafe { GetDlgItem(e.hwnd, id as i32) }
}

fn text_of(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

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
                let mut buf = [0u16; 512];
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

/// The profiles of `doc`, by their lower-case names: the default first,
/// then the others as written.
fn profiles(doc: &Document) -> Vec<String> {
    let others = doc.sections().into_iter().filter_map(|s| s.strip_prefix("profile.").map(String::from)).filter(|p| p != DEFAULT);
    std::iter::once(DEFAULT.to_string()).chain(others).collect()
}

fn sets(doc: &Document) -> Vec<String> {
    doc.sections().into_iter().filter_map(|s| s.strip_prefix("set.").map(String::from)).collect()
}

fn process_of(doc: &Document, profile: &str) -> Option<String> {
    doc.get(&format!("profile.{profile}"), "process").map(|p| p.trim().to_string()).filter(|p| !p.is_empty())
}

/// The chosen profile's name.
fn chosen(e: &Editor) -> String {
    e.apps.profile.clone().unwrap_or_else(|| DEFAULT.into())
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
            List => (WS_EX_CLIENTEDGE, w!("ListBox"), WS_TABSTOP | WS_VSCROLL | (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32),
            Typed => (0, w!("ComboBox"), WS_TABSTOP | WS_VSCROLL | (CBS_DROPDOWN | CBS_AUTOHSCROLL) as u32),
            Edge => (0, w!("ComboBox"), WS_VSCROLL | CBS_DROPDOWNLIST as u32),
            // Drawn around the lists on it.
            Figure => (0, FIGURE_CLASS, WS_TABSTOP | WS_CLIPSIBLINGS),
        };
        let text = wide(text);
        let hwnd =
            unsafe { CreateWindowExW(ex, name, text.as_ptr(), WS_CHILD | style, 0, 0, 1, 1, parent, id as HMENU, instance, null()) };
        if hwnd.is_null() {
            return false;
        }
        // The lists stand over the figure; out of the Tab key's way, their
        // place in its order does not matter.
        if class == Edge {
            unsafe { SetWindowPos(hwnd, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
        }
    }
    true
}

/// Every part of the page, for fonts.
pub(super) fn parts(e: &Editor) -> Vec<HWND> {
    PARTS.iter().map(|&(id, ..)| part_of(e, id)).collect()
}

/// The parts to theme as edit controls and drop-downs, and as buttons.
pub(super) fn themed(e: &Editor) -> (Vec<HWND>, Vec<HWND>) {
    let (mut typed, mut buttons) = (Vec::new(), Vec::new());
    for &(id, class, _) in PARTS {
        match class {
            Edit | Typed | Edge => typed.push(part_of(e, id)),
            Button | List => buttons.push(part_of(e, id)),
            _ => {}
        }
    }
    (typed, buttons)
}

pub(super) fn is_part(id: usize) -> bool {
    PARTS.iter().any(|&(p, ..)| p == id)
}

/// Whether `hwnd` is the process field. The process to add has keys of
/// its own (`candidate_key`), and its list's arrows.
pub(super) fn is_typed(e: &Editor, hwnd: HWND) -> bool {
    !hwnd.is_null() && hwnd == part_of(e, PROCESS)
}

fn is_candidate(e: &Editor, hwnd: HWND) -> bool {
    let parent = unsafe { GetParent(hwnd) };
    parent == part_of(e, CANDIDATES)
}

/// Whether `hwnd` shows a value turned away: a field, or the edit of the
/// drop-down list typed in.
pub(super) fn is_wrong(hwnd: HWND) -> bool {
    let parent = unsafe { GetParent(hwnd) };
    super::EDITOR.with(|e| {
        e.try_borrow().ok().is_some_and(|e| {
            e.as_ref().is_some_and(|e| e.apps.wrong.as_ref().is_some_and(|(id, _)| [hwnd, parent].contains(&part_of(e, *id))))
        })
    })
}

pub(super) fn standing_reason(e: &Editor) -> Option<String> {
    e.apps.wrong.as_ref().map(|(_, why)| why.clone())
}

/// Show the page's parts that go with the chosen app, if the page is shown.
pub(super) unsafe fn show(e: &Editor) {
    let default = e.apps.profile.is_none();
    for &(id, ..) in PARTS {
        let on = e.page == PAGE
            && match id {
                CANDIDATES | ADD | CANCEL_ADD => e.apps.adding,
                TITLE_PROCESS | PROCESS => !default,
                NOTE => default,
                _ => true,
            };
        unsafe { ShowWindow(part_of(e, id), if on { SW_SHOWNA } else { SW_HIDE }) };
    }
}

/// Place the parts in `left`, `top`, `width` across, down to `bottom`: the
/// apps down the left, their buttons and the process to add under them;
/// the chosen app's process and the figure on the right.
pub(super) unsafe fn place(e: &Editor, left: i32, top: i32, width: i32, bottom: i32) {
    let s = |v| scale(v, e.dpi);
    let (h, gap) = (s(ROW_H), s(MARGIN) / 2);
    let at = |id: usize, x: i32, y: i32, w: i32, height: i32| unsafe {
        MoveWindow(part_of(e, id), x, y, w.max(0), height.max(0), 1);
    };
    let rows = bottom - 3 * h - 2 * gap;
    at(APPS, left, top, s(LIST_W), rows - gap - top);
    at(ADD_APP, left, rows, s(LIST_W) - s(56) - gap, h);
    at(REMOVE_APP, left + s(LIST_W) - s(56), rows, s(56), h);
    at(CANDIDATES, left, rows + h + gap, s(LIST_W), s(LIST_H));
    at(ADD, left, rows + 2 * (h + gap), s(80), h);
    at(CANCEL_ADD, left + s(80) + gap, rows + 2 * (h + gap), s(80), h);

    let rx = left + s(LIST_W) + s(MARGIN);
    let rw = (left + width - rx).max(0);
    at(TITLE_PROCESS, rx, top, s(88), h);
    at(PROCESS, rx + s(92), top, s(240), h);
    at(NOTE, rx, top, rw, h);
    let (fx, fy) = (rx, top + h + s(MARGIN));
    let (fw, fh) = (s(FIGURE_W).min(rw), s(FIGURE_H).min((bottom - fy).max(0)));
    at(FIGURE, fx, fy, fw, fh);
    let inner = s(INSET + BAND + 4);
    // The side edges' lists no wider than keeps them apart, on a figure
    // the window has no room to grow as far (Issue #99).
    let ew = s(EDGE_W).min((fw - 2 * inner - gap) / 2).max(0);
    let (mid_x, mid_y) = (fx + fw / 2, fy + fh / 2);
    at(EDGES, fx + inner, mid_y - h / 2, ew, s(LIST_H));
    at(EDGES + 1, fx + fw - inner - ew, mid_y - h / 2, ew, s(LIST_H));
    at(EDGES + 2, mid_x - ew / 2, fy + inner, ew, s(LIST_H));
    at(EDGES + 3, mid_x - ew / 2, fy + fh - inner - h, ew, s(LIST_H));
}

/// Show the page as the window's copy of the file has it, the chosen app
/// kept if it is still there.
pub(super) unsafe fn refresh(e: &mut Editor) {
    let names = profiles(&e.doc);
    if e.apps.profile.as_ref().is_some_and(|p| !names.contains(p)) {
        e.apps.profile = None;
    }
    let chosen_name = chosen(e);
    let items: Vec<String> = names
        .iter()
        .map(|name| {
            let label = if name == DEFAULT { DEFAULT_LABEL.into() } else { process_of(&e.doc, name).unwrap_or_else(|| format!("[{name}]")) };
            let mark = if e.apps.active.as_deref() == Some(name.as_str()) { MARK } else { "" };
            format!("{mark}{label}")
        })
        .collect();
    let section = format!("profile.{chosen_name}");
    let sets = sets(&e.doc);
    let choices: Vec<String> = std::iter::once(NONE.to_string()).chain(sets.iter().cloned()).collect();
    unsafe {
        let list = part_of(e, APPS);
        fill(list, &items, true);
        SendMessageW(list, LB_SETCURSEL, names.iter().position(|n| *n == chosen_name).unwrap_or(0), 0);
        EnableWindow(part_of(e, REMOVE_APP), e.apps.profile.is_some().into());
        let process = part_of(e, PROCESS);
        set_text(process, &process_of(&e.doc, &chosen_name).unwrap_or_default());
        SendMessageW(process, EM_SETMODIFY, 0, 0);
        for (k, edge) in EDGE_NAMES.iter().enumerate() {
            let combo = part_of(e, EDGES + k);
            fill(combo, &choices, false);
            let value = e.doc.get(&section, edge).map(|v| v.trim().to_ascii_lowercase());
            let at = match value.as_deref() {
                None | Some("none") => Some(0),
                Some(set) => sets.iter().position(|s| s == set).map(|i| i + 1),
            };
            SendMessageW(combo, CB_SETCURSEL, at.unwrap_or(usize::MAX), 0);
        }
        InvalidateRect(part_of(e, FIGURE), null(), 0);
        show(e);
    }
}

/// Ask the running EdgePad which profile is in effect, and mark its app.
/// Its answer counts the profiles of the file it read, which it reads again
/// whenever the file is saved: the file as it is now.
pub(super) unsafe fn ask_active(e: &mut Editor) {
    let index = unsafe { resident_tray() }.map_or(0, |tray| {
        let mut answer = 0usize;
        let sent = unsafe { SendMessageTimeoutW(tray, profile::active_message(), 0, 0, SMTO_ABORTIFHUNG, 500, &mut answer) };
        if sent == 0 { 0 } else { answer }
    });
    let file = super::read_file(&e.path).ok();
    let profiles = file.and_then(|f| config::parse(&f).ok()).map(|c| c.profiles);
    let active = index.checked_sub(1).and_then(|i| profiles?.get(i).map(|p| p.name.clone()));
    if active != e.apps.active {
        e.apps.active = active;
        unsafe { refresh(e) };
    }
}

/// The tray window of the running EdgePad that started this process: of a
/// process of the same exe.
unsafe fn resident_tray() -> Option<HWND> {
    let own = std::env::current_exe().ok()?;
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, w!("EdgePadTray"), null()) };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if unsafe { exe_of(pid) }.is_some_and(|exe| exe.eq_ignore_ascii_case(&own.to_string_lossy())) {
            return Some(hwnd);
        }
    }
}

/// The full path of process `pid`'s exe, if it can be read.
unsafe fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        ok.then(|| String::from_utf16_lossy(&path[..len as usize]))
    }
}

/// The apps with a window shown now, by their exe's file name, EdgePad's
/// own aside, each once, in order.
unsafe fn apps_open() -> Vec<String> {
    unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> i32 {
        unsafe {
            let found = &mut *(found as *mut Vec<u32>);
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            if IsWindowVisible(hwnd) != 0 && ex & WS_EX_TOOLWINDOW == 0 && GetWindow(hwnd, GW_OWNER).is_null() {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                found.push(pid);
            }
        }
        1
    }
    let mut pids: Vec<u32> = Vec::new();
    unsafe { EnumWindows(Some(each), &mut pids as *mut Vec<u32> as LPARAM) };
    let own = std::env::current_exe().map(|p| p.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let me = unsafe { GetCurrentProcessId() };
    let mut names: Vec<String> = Vec::new();
    for pid in pids.into_iter().filter(|&p| p != me) {
        let Some(exe) = (unsafe { exe_of(pid) }) else { continue };
        if exe.to_ascii_lowercase() == own {
            continue;
        }
        let name = exe.rsplit('\\').next().unwrap_or(&exe).to_string();
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            names.push(name);
        }
    }
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names
}

/// The profile with process `process`, if there is one, other than `but`.
fn profile_of(doc: &Document, process: &str, but: Option<&str>) -> Option<String> {
    profiles(doc).into_iter().filter(|p| Some(p.as_str()) != but).find(|p| process_of(doc, p).is_some_and(|q| q.eq_ignore_ascii_case(process)))
}

/// A profile's name made from `process`: its exe's name, without `.exe`,
/// in lower case, other than the default and the profiles there.
fn name_for(doc: &Document, process: &str) -> String {
    let stem = process.rsplit(['\\', '/']).next().unwrap_or(process);
    let stem = if stem.to_ascii_lowercase().ends_with(".exe") { &stem[..stem.len() - 4] } else { stem };
    let base: String =
        stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' }).collect();
    let base = if base.is_empty() { "app".to_string() } else { base };
    let taken = profiles(doc);
    (1..)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}{n}") })
        .find(|n| !taken.contains(n) && config::valid_name(n))
        .unwrap_or(base)
}

/// Add a profile for the process typed or chosen; or say why not.
unsafe fn add(e: &mut Editor) -> bool {
    // The app is matched by its exe's name: a path typed is its name.
    let typed = text_of(part_of(e, CANDIDATES));
    let process = typed.trim().rsplit(['\\', '/']).next().unwrap_or("").trim().to_string();
    let added = if process.is_empty() {
        Err("プロセス名を選ぶか入れてください".to_string())
    } else if profile_of(&e.doc, &process, None).is_some() {
        Err(format!("「{process}」のプロファイルはもうあります"))
    } else {
        let name = name_for(&e.doc, &process);
        // Made with its process, or not at all.
        if e.doc.set_value(&format!("profile.{name}"), "process", Some(&process)) {
            e.apps.profile = Some(name);
            e.apps.adding = false;
            Ok(())
        } else {
            Err(format!("「{process}」はファイルに書けません"))
        }
    };
    let ok = added.is_ok();
    unsafe { taken(e, CANDIDATES, added) };
    ok
}

/// What a change of `field` came to, as on the キー配置 page.
unsafe fn taken(e: &mut Editor, field: usize, result: Result<(), String>) {
    match result {
        Ok(()) => {
            let had = e.apps.wrong.take().is_some();
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
        }
        Err(why) => {
            tell(e, &why);
            e.apps.wrong = Some((field, why));
            unsafe { InvalidateRect(part_of(e, field), null(), 1) };
        }
    }
}

/// A part of the page was used: `code` is its notification.
pub(super) unsafe fn command(id: usize, code: u32) {
    if id == PROCESS && code == EN_KILLFOCUS {
        if !unsafe { commit() } {
            if let Some(hwnd) = super::EDITOR.with(|e| e.try_borrow().ok().and_then(|e| e.as_ref().map(|e| e.hwnd))) {
                unsafe { PostMessageW(hwnd, super::WM_COMMIT, PROCESS, 0) };
            }
        }
        return;
    }
    let clicked = code == BN_CLICKED;
    let focus = with_editor(|e| unsafe {
        match id {
            APPS if code == LBN_SELCHANGE => {
                let at = SendMessageW(part_of(e, APPS), LB_GETCURSEL, 0, 0);
                if let Some(name) = usize::try_from(at).ok().and_then(|i| profiles(&e.doc).get(i).cloned()) {
                    e.apps.profile = (name != DEFAULT).then_some(name);
                    forget_reason(e);
                    refresh(e);
                }
                None
            }
            ADD_APP if clicked => {
                forget_reason(e);
                let combo = part_of(e, CANDIDATES);
                fill(combo, &apps_open(), false);
                set_text(combo, "");
                e.apps.adding = true;
                show(e);
                Some(combo)
            }
            // Added, the list has the focus: the parts to add are gone.
            ADD if clicked => add(e).then(|| part_of(e, APPS)),
            CANCEL_ADD if clicked => Some(cancel_adding(e)),
            REMOVE_APP if clicked => {
                if let Some(name) = e.apps.profile.clone() {
                    e.doc.remove_section(&format!("profile.{name}"));
                    e.apps.profile = None;
                    refresh(e);
                    mark_unsaved(e);
                }
                None
            }
            _ if code == CBN_SELCHANGE && (EDGES..EDGES + 4).contains(&id) => {
                let at = SendMessageW(part_of(e, id), CB_GETCURSEL, 0, 0);
                let value = match usize::try_from(at) {
                    Ok(0) => Some("none".to_string()),
                    Ok(i) => sets(&e.doc).get(i - 1).cloned(),
                    Err(_) => None,
                };
                let section = format!("profile.{}", chosen(e));
                let edge = EDGE_NAMES[id - EDGES];
                if let Some(value) = value.filter(|v| e.doc.get(&section, edge).is_none_or(|now| !now.trim().eq_ignore_ascii_case(v))) {
                    let written = e.doc.set_value(&section, edge, Some(&value));
                    taken(e, id, if written { Ok(()) } else { Err("この値はファイルに書けない".into()) });
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

/// Forget a value turned away on the page, which is no longer in a field:
/// the status line says one still standing, or the question asked.
fn forget_reason(e: &mut Editor) {
    if e.apps.wrong.take().is_some() {
        match super::standing_reason(e) {
            Some(reason) => tell(e, &reason),
            None => super::ask_reason(e),
        }
    }
}

/// Stop choosing a process to add.
fn cancel_adding(e: &mut Editor) -> HWND {
    e.apps.adding = false;
    if e.apps.wrong.as_ref().is_some_and(|(id, _)| *id == CANDIDATES) {
        e.apps.wrong = None;
        super::ask_reason(e);
    }
    unsafe { show(e) };
    part_of(e, ADD_APP)
}

/// Enter, in the process to add: add it. Esc: stop. Whether the key was
/// the page's.
pub(super) unsafe fn candidate_key(focus: HWND, enter: bool) -> bool {
    let focus_to = with_editor(|e| {
        if !is_candidate(e, focus) {
            return None;
        }
        if enter {
            Some(unsafe { add(e) }.then(|| part_of(e, APPS)))
        } else {
            Some(Some(cancel_adding(e)))
        }
    })
    .flatten();
    match focus_to {
        Some(Some(hwnd)) => {
            unsafe { SetFocus(hwnd) };
            true
        }
        Some(None) => true,
        None => false,
    }
}

/// Take the process typed, if it changed since it was last taken. False if
/// the window was in use, and nothing was done.
pub(super) unsafe fn commit() -> bool {
    with_editor(|e| unsafe {
        let field = part_of(e, PROCESS);
        if SendMessageW(field, EM_GETMODIFY, 0, 0) == 0 {
            return;
        }
        let Some(name) = e.apps.profile.clone() else { return };
        let process = text_of(field).trim().to_string();
        let result = if process.is_empty() {
            Err("プロセス名が空です".to_string())
        } else if profile_of(&e.doc, &process, Some(&name)).is_some() {
            Err(format!("「{process}」のプロファイルはもうあります"))
        } else if process_of(&e.doc, &name).as_deref() == Some(process.as_str())
            || e.doc.set_value(&format!("profile.{name}"), "process", Some(&process))
        {
            Ok(())
        } else {
            Err("この値はファイルに書けない".into())
        };
        let refused = result.is_err();
        taken(e, PROCESS, result);
        SendMessageW(field, EM_SETMODIFY, refused as WPARAM, 0);
    })
    .is_some()
}

/// Open the list of edge `k` of the figure, to choose its key set.
pub(super) unsafe fn open_edge(k: usize) {
    let Some(combo) = with_editor(|e| part_of(e, EDGES + k)) else { return };
    unsafe {
        SetFocus(combo);
        SendMessageW(combo, CB_SHOWDROPDOWN, 1, 0);
    }
}

/// The figure: the screen, its four edges as bands along it, the one the
/// keyboard has chosen in the accent while the figure has the focus. The
/// key set of each edge is in the list on it. A band pressed opens its
/// list; ←→↑↓ choose an edge and Enter opens its list.
pub(super) unsafe extern "system" fn figure_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                if let Some(paint) = PAINT.get() {
                    draw_figure(hwnd, dc, paint.look);
                }
                EndPaint(hwnd, &ps);
                0
            }
            WM_LBUTTONDOWN => {
                SetFocus(hwnd);
                let mut r: RECT = zeroed();
                GetClientRect(hwnd, &mut r);
                let p = POINT { x: (lp & 0xFFFF) as i16 as i32, y: (lp >> 16 & 0xFFFF) as i16 as i32 };
                let away = [p.x - r.left, r.right - p.x, p.y - r.top, r.bottom - p.y];
                let near = (0..4).min_by_key(|&k| away[k]).unwrap_or(0);
                // On a band, its list opens; elsewhere the figure is chosen.
                if away[near] < scale(INSET + BAND, window_dpi(hwnd)) {
                    EDGE.set(near);
                    InvalidateRect(hwnd, null(), 0);
                    PostMessageW(GetParent(hwnd), super::WM_OPEN_EDGE, near, 0);
                }
                0
            }
            WM_KEYDOWN => {
                let k = match wp as u16 {
                    VK_LEFT => 0,
                    VK_RIGHT => 1,
                    VK_UP => 2,
                    VK_DOWN => 3,
                    VK_RETURN => {
                        PostMessageW(GetParent(hwnd), super::WM_OPEN_EDGE, EDGE.get(), 0);
                        return 0;
                    }
                    _ => return DefWindowProcW(hwnd, msg, wp, lp),
                };
                EDGE.set(k);
                InvalidateRect(hwnd, null(), 0);
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
        let s = |v| scale(v, window_dpi(hwnd));
        let fill = |r: &RECT, c: [u8; 3]| {
            let brush = CreateSolidBrush(colorref(c));
            FillRect(dc, r, brush);
            DeleteObject(brush);
        };
        let mut r: RECT = zeroed();
        GetClientRect(hwnd, &mut r);
        fill(&r, look.background);
        let screen = RECT { left: r.left + s(INSET), top: r.top + s(INSET), right: r.right - s(INSET), bottom: r.bottom - s(INSET) };
        fill(&screen, look.sidebar);
        let band = s(BAND);
        let plain = super::mix(look.text, look.background, 128);
        let focused = GetFocus() == hwnd;
        for k in 0..4 {
            let b = match k {
                0 => RECT { right: screen.left + band, ..screen },
                1 => RECT { left: screen.right - band, ..screen },
                2 => RECT { bottom: screen.top + band, ..screen },
                _ => RECT { top: screen.bottom - band, ..screen },
            };
            fill(&b, if focused && EDGE.get() == k { look.accent } else { plain });
        }
        if focused {
            SetTextColor(dc, colorref(look.text));
            SetBkColor(dc, colorref(look.background));
            let f = RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 };
            DrawFocusRect(dc, &f);
        }
    }
}

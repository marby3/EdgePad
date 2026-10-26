//! The settings window (spec 7.1): a normal window, opened from the tray,
//! that edits the settings file through a `Document` and saves it once the
//! parser reads it. The panels pick the file up as when it is saved by hand.
//!
//! Its parts are small windows of its own, drawn in the panels' colours:
//! tabs, buttons and a status line. A question (the file changed outside,
//! or changes not saved on closing) shows in the status line, its answers
//! as buttons in the row below, beside 保存.
//!
//! Everything is made when the window opens, and goes when it closes.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};

use edgepad_core::config;
use edgepad_core::document::Document;
use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, FreeLibrary, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_ESCAPE};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::colors::colorref;
use crate::log::debug_log;
use crate::panel::{self, pointer_id};
use crate::{settings, tray, watch};

const TABS: [&str; 4] = ["一般", "辺", "キーセット", "プロファイル"];

/// Control ids: the tabs from TAB on, 保存, and the answers to questions.
const TAB: usize = 100;
const SAVE: usize = 200;
const RELOAD: usize = 300;
const KEEP: usize = 301;
const SAVE_AND_CLOSE: usize = 302;
const DISCARD: usize = 303;
const CANCEL: usize = 304;

/// Sizes in logical pixels (96 DPI).
const CLIENT_W: i32 = 640;
const CLIENT_H: i32 = 420;
const MARGIN: i32 = 8;
const TAB_W: i32 = 120;
const ROW_H: i32 = 32;
const BUTTON_W: i32 = 150;
const FONT_PX: i32 = 15;

/// What a part of the window is, kept in its GWLP_USERDATA.
const BUTTON: isize = 0;
const TAB_PART: isize = 1;
const TAB_SHOWN: isize = 2;
const STATUS: isize = 3;

/// "EdgePadTestEditorSet": sets `[general] opacity` to wParam in the
/// window's copy of the file, a change for the tests to save until the tabs
/// have fields of their own. Registered by name, as the tray's test
/// messages are: the name is the contract with the tests.
#[cfg(feature = "fault-injection")]
fn test_set() -> u32 {
    thread_local! {
        static ID: u32 = unsafe { RegisterWindowMessageW(w!("EdgePadTestEditorSet")) };
    }
    ID.with(|id| *id)
}

const STYLE: WINDOW_STYLE = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
const EX_STYLE: WINDOW_EX_STYLE = WS_EX_APPWINDOW;

/// A question the window asks.
#[derive(Clone, Copy)]
enum Ask {
    /// The file changed outside: read it again, or keep the window's.
    Changed,
    /// Closing with changes not saved.
    Closing,
}

struct Editor {
    hwnd: HWND,
    path: PathBuf,
    doc: Document,
    /// What the window last read or saved: changes not saved are what the
    /// document says otherwise.
    base: Vec<u8>,
    /// The file as the window last saw it: a file still so is no change
    /// outside. It differs from `base` once a change outside is kept off.
    known: Vec<u8>,
    /// The file as changed outside, until the question is answered.
    outside: Option<Vec<u8>>,
    /// The question asked now, if any.
    asked: Option<Ask>,
    tabs: [HWND; 4],
    save: HWND,
    status: HWND,
    answers: Vec<HWND>,
    font: HFONT,
    dpi: u32,
}

thread_local! {
    static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
    // The pointer that went down on a part, and the part: only it answers
    // that pointer's lift. No part takes the capture, which the panels on
    // this thread hold for a mouse pressing a key. One at a time: a second
    // finger on another part takes over from the first.
    static PRESSED: Cell<(u32, HWND)> = const { Cell::new((0, null_mut())) };
}

/// Whether the window is open: in use further up the stack, it is.
fn is_open() -> bool {
    EDITOR.with(|e| e.try_borrow().map_or(true, |e| e.is_some()))
}

/// Run `f` on the window, if it is open and not in use further up the stack.
fn with_editor<R>(f: impl FnOnce(&mut Editor) -> R) -> Option<R> {
    EDITOR.with(|e| match e.try_borrow_mut() {
        Ok(mut e) => e.as_mut().map(f),
        Err(_) => {
            debug_log!("設定画面が使用中: 後回しにできない操作は捨てる");
            None
        }
    })
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// `v` logical pixels at `dpi`.
fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

unsafe fn font_for(dpi: u32) -> HFONT {
    let (normal, charset, quality) = (FW_NORMAL as i32, DEFAULT_CHARSET as u32, CLEARTYPE_QUALITY as u32);
    unsafe { CreateFontW(-scale(FONT_PX, dpi), 0, 0, 0, normal, 0, 0, 0, charset, 0, 0, quality, 0, w!("Segoe UI")) }
}

unsafe fn register() -> bool {
    if REGISTERED.get() {
        return true;
    }
    unsafe {
        let instance = GetModuleHandleW(null());
        for (class, proc_, cursor) in [
            (w!("EdgePadSettings"), wnd_proc as unsafe extern "system" fn(_, _, _, _) -> _, IDC_ARROW),
            (w!("EdgePadSettingsTab"), part_proc, IDC_HAND),
            (w!("EdgePadSettingsButton"), part_proc, IDC_HAND),
            (w!("EdgePadSettingsStatus"), part_proc, IDC_ARROW),
        ] {
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(proc_);
            wc.hInstance = instance;
            wc.hCursor = LoadCursorW(null_mut(), cursor);
            wc.lpszClassName = class;
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                debug_log!("設定画面の窓クラスを登録できない");
                return false;
            }
        }
    }
    REGISTERED.set(true);
    true
}

/// A part of `parent`: a tab, button or status line, of `kind`. Made
/// hidden, to be shown once it is in its place.
unsafe fn part(parent: HWND, class: *const u16, text: &str, id: usize, kind: isize) -> HWND {
    unsafe {
        let text = wide(text);
        let hwnd = CreateWindowExW(
            0,
            class,
            text.as_ptr(),
            WS_CHILD,
            0,
            0,
            1,
            1,
            parent,
            id as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, kind);
        hwnd
    }
}

/// The settings file, read no further than one byte past what the parser
/// takes: a file any bigger is turned away whole anyway.
fn read_file(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(config::MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Make the title bar dark with a dark palette, light with a light one.
///
/// dwmapi.dll is loaded here, not imported: imported, it would sit in every
/// EdgePad from the start, settings opened or not, its pages counted
/// against the idle memory target (spec 8).
unsafe fn title_bar(hwnd: HWND) {
    type SetAttribute = unsafe extern "system" fn(HWND, u32, *const core::ffi::c_void, u32) -> i32;
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    let dark: i32 = edgepad_core::theme::is_dark(panel::palette().background).into();
    unsafe {
        let dwm = LoadLibraryExW(w!("dwmapi.dll"), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if dwm.is_null() {
            return;
        }
        if let Some(f) = GetProcAddress(dwm, b"DwmSetWindowAttribute\0".as_ptr()) {
            let set: SetAttribute = std::mem::transmute(f);
            set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const i32 as _, 4);
        }
        FreeLibrary(dwm);
    }
}

/// The panels' colours changed: draw the window again in them.
pub unsafe fn restyle() {
    let Some(hwnd) = with_editor(|e| e.hwnd) else { return };
    unsafe {
        title_bar(hwnd);
        RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
    }
}

/// Open the settings window from the tray, or bring it back if it is open.
pub unsafe fn open() {
    unsafe {
        if is_open() {
            if let Some(hwnd) = with_editor(|e| e.hwnd) {
                ShowWindow(hwnd, if IsIconic(hwnd) != 0 { SW_RESTORE } else { SW_SHOW });
                SetForegroundWindow(hwnd);
            }
            return;
        }
        let Some(path) = settings::path_or_template() else {
            tray::notify("設定ファイルを作れませんでした");
            return;
        };
        // The file may be new, in a folder made just now: watched from now.
        watch::rescan();
        let read = read_file(&path).map_err(|e| e.to_string());
        let (doc, known) = match read.and_then(|b| Document::new(&b).map(|d| (d, b)).map_err(|e| e.to_string())) {
            Ok(opened) => opened,
            Err(why) => {
                tray::notify(&format!("設定ファイルを開けません: {why}"));
                return;
            }
        };
        if !register() {
            return;
        }
        let hwnd = CreateWindowExW(
            EX_STYLE,
            w!("EdgePadSettings"),
            w!("EdgePad の設定"),
            STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1,
            1,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            debug_log!("設定画面を作れない");
            tray::notify("設定画面を開けませんでした");
            return;
        }
        let tabs = TABS.map(|_| null_mut());
        let mut editor = Editor {
            hwnd,
            path,
            doc,
            base: known.clone(),
            known,
            outside: None,
            asked: None,
            tabs,
            save: null_mut(),
            status: null_mut(),
            answers: Vec::new(),
            font: null_mut(),
            dpi: GetDpiForWindow(hwnd).max(96),
        };
        for (i, name) in TABS.iter().enumerate() {
            editor.tabs[i] = part(hwnd, w!("EdgePadSettingsTab"), name, TAB + i, TAB_PART);
        }
        editor.save = part(hwnd, w!("EdgePadSettingsButton"), "保存", SAVE, BUTTON);
        editor.status = part(hwnd, w!("EdgePadSettingsStatus"), "", 0, STATUS);
        if editor.tabs.iter().chain([&editor.save, &editor.status]).any(|p| p.is_null()) {
            debug_log!("設定画面の部品を作れない");
            DestroyWindow(hwnd);
            return;
        }
        editor.font = font_for(editor.dpi);
        let dpi = editor.dpi;
        EDITOR.set(Some(editor));
        title_bar(hwnd);
        debug_log!("設定画面を開く");
        size_for(hwnd, dpi, true);
        show_tab(0);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

/// Size the window for `dpi`, its client area CLIENT_W by CLIENT_H logical
/// pixels: with `centre`, in the middle of its monitor's work area, else
/// where it is, moved only as far as keeps it on the monitor it is on.
unsafe fn size_for(hwnd: HWND, dpi: u32, centre: bool) {
    unsafe {
        let mut r = RECT { left: 0, top: 0, right: scale(CLIENT_W, dpi), bottom: scale(CLIENT_H, dpi) };
        AdjustWindowRectExForDpi(&mut r, STYLE, 0, EX_STYLE, dpi);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info);
        let a = info.rcWork;
        let (x, y) = if centre {
            (a.left + (a.right - a.left - w) / 2, a.top + (a.bottom - a.top - h) / 2)
        } else {
            let mut now: RECT = zeroed();
            GetWindowRect(hwnd, &mut now);
            (now.left.min(a.right - w).max(a.left), now.top.min(a.bottom - h).max(a.top))
        };
        SetWindowPos(hwnd, null_mut(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// Place the parts in the window as large as it is now.
unsafe fn lay_out() {
    let Some((hwnd, dpi, tabs, save, status, answers)) =
        with_editor(|e| (e.hwnd, e.dpi, e.tabs, e.save, e.status, e.answers.clone()))
    else {
        return;
    };
    unsafe {
        let mut client: RECT = zeroed();
        GetClientRect(hwnd, &mut client);
        let s = |v| scale(v, dpi);
        let (m, row, button) = (s(MARGIN), s(ROW_H), s(BUTTON_W));
        for (i, &tab) in tabs.iter().enumerate() {
            MoveWindow(tab, m + i as i32 * (s(TAB_W) + m), m, s(TAB_W), row, 1);
        }
        // The buttons along the bottom, 保存 on the right; the status line,
        // and any question, the whole width above them.
        let y = client.bottom - m - row;
        let mut x = client.right - m - button;
        MoveWindow(save, x, y, button, row, 1);
        for &answer in answers.iter().rev() {
            x -= m + button;
            MoveWindow(answer, x, y, button, row, 1);
        }
        MoveWindow(status, m, y - m - 2 * row, (client.right - 2 * m).max(0), 2 * row, 1);
        // In their places now: shown, and not before.
        for &part in tabs.iter().chain([&save, &status]).chain(&answers) {
            ShowWindow(part, SW_SHOWNA);
        }
        InvalidateRect(hwnd, null(), 1);
    }
}

/// Show tab `index`.
unsafe fn show_tab(index: usize) {
    let Some(tabs) = with_editor(|e| e.tabs) else { return };
    for (i, &tab) in tabs.iter().enumerate() {
        unsafe {
            SetWindowLongPtrW(tab, GWLP_USERDATA, if i == index { TAB_SHOWN } else { TAB_PART });
            InvalidateRect(tab, null(), 1);
        }
    }
    tray::test_event(&format!("settings: tab {}", TABS[index]));
}

fn set_status(e: &Editor, text: &str) {
    let text = wide(text);
    unsafe { SetWindowTextW(e.status, text.as_ptr()) };
}

/// Question `ask`, as the status line puts it.
fn question(e: &Editor, ask: Ask) -> &'static str {
    match ask {
        Ask::Changed => "設定ファイルが外で書き換わりました",
        Ask::Closing if e.outside.is_some() || e.known != e.base => {
            "保存していない変更があります（ファイルは外でも書き換わりました。保存すると上書きします）"
        }
        Ask::Closing => "保存していない変更があります",
    }
}

/// Say `reason`: first, the question asked now, which stays, after it on
/// the line below.
fn tell(e: &Editor, reason: &str) {
    match e.asked {
        Some(ask) => set_status(e, &format!("{reason}\n{}", question(e, ask))),
        None => set_status(e, reason),
    }
}

/// Ask `ask` in the status line, its answers as buttons; or, with none,
/// take a question away.
unsafe fn ask(ask: Option<Ask>) {
    let made = with_editor(|e| unsafe {
        for answer in e.answers.drain(..) {
            DestroyWindow(answer);
        }
        e.asked = ask;
        let answers: &[(&str, usize)] = match ask {
            Some(Ask::Changed) => &[("読み直す", RELOAD), ("画面の内容を保つ", KEEP)],
            Some(Ask::Closing) => &[("保存する", SAVE_AND_CLOSE), ("保存せずに閉じる", DISCARD), ("キャンセル", CANCEL)],
            None => &[],
        };
        set_status(e, ask.map_or("", |a| question(e, a)));
        for &(label, id) in answers {
            let answer = part(e.hwnd, w!("EdgePadSettingsButton"), label, id, BUTTON);
            if answer.is_null() {
                debug_log!("設定画面の答えのボタンを作れない: {label}");
                continue;
            }
            e.answers.push(answer);
        }
    });
    if made.is_some() {
        unsafe { lay_out() };
    }
}

/// The first of `errors` the parser found, and how many more.
fn first_error(errors: &[config::Error]) -> String {
    let first = errors.first().map_or_else(String::new, ToString::to_string);
    match errors.len() {
        0 | 1 => first,
        n => format!("{first}（ほか {} 件）", n - 1),
    }
}

/// Save the window's file if the parser reads it; else, writing nothing,
/// the reason why.
fn save(e: &mut Editor) -> Result<(), String> {
    let bytes = e.doc.to_bytes();
    if let Err(errors) = config::parse(&bytes) {
        return Err(format!("保存できません: {}", first_error(&errors)));
    }
    // Written beside it, in a file of its own made for this, and then put
    // in its place: a write that fails part way leaves the file as it was,
    // not cut short.
    let fresh = e.path.with_extension(format!("ini.{}.new", std::process::id()));
    // One left by an EdgePad that crashed, and had this process's number.
    let _ = std::fs::remove_file(&fresh);
    let written = (|| -> std::io::Result<()> {
        // Made new by this, it is this's to take away if what follows fails;
        // a file of that name already there is left alone.
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&fresh)?;
        let wrote = std::io::Write::write_all(&mut f, &bytes).and_then(|()| f.sync_all());
        drop(f);
        let moved = wrote.and_then(|()| std::fs::rename(&fresh, &e.path));
        if moved.is_err() {
            let _ = std::fs::remove_file(&fresh);
        }
        moved
    })();
    if let Err(why) = written {
        return Err(format!("保存できません: {why}"));
    }
    debug_log!("設定画面から保存");
    e.base = bytes.clone();
    e.known = bytes;
    // Saved, the window's is the file: a change outside is answered.
    e.outside = None;
    Ok(())
}

/// Whether the window holds changes it has not saved.
fn unsaved(e: &Editor) -> bool {
    e.doc.to_bytes() != e.base
}

/// The settings folder was written to and has settled: if the file is not
/// as the window last saw it, it changed outside; ask.
pub unsafe fn file_changed() {
    let changed = with_editor(|e| {
        let now = match read_file(&e.path) {
            Ok(now) => now,
            Err(why) => {
                debug_log!("設定画面: 設定ファイルを読めない: {why}");
                return false;
            }
        };
        if now == e.known || e.outside.as_ref() == Some(&now) {
            return false;
        }
        e.outside = Some(now);
        true
    });
    if changed == Some(true) {
        debug_log!("設定画面: ファイルが外で書き換わった");
        // Asked about closing already: that question stays, now telling of
        // the change outside too; else, ask about the change.
        let closing = with_editor(|e| matches!(e.asked, Some(Ask::Closing))) == Some(true);
        unsafe { ask(Some(if closing { Ask::Closing } else { Ask::Changed })) };
    }
}

/// EdgePad is to end from the tray: unless the window holds changes not
/// saved, in which case it asks about them and EdgePad goes on.
pub unsafe fn may_exit() -> bool {
    if !is_open() {
        return true;
    }
    // Open but in use: not now, rather than past changes.
    let Some((hwnd, unsaved)) = with_editor(|e| (e.hwnd, unsaved(e))) else { return false };
    if !unsaved {
        return true;
    }
    unsafe {
        ShowWindow(hwnd, if IsIconic(hwnd) != 0 { SW_RESTORE } else { SW_SHOW });
        SetForegroundWindow(hwnd);
        ask(Some(Ask::Closing));
    }
    tray::notify("設定画面に保存していない変更があります。保存するか閉じてから終了してください");
    false
}

/// A part was clicked.
unsafe fn command(id: usize) {
    unsafe {
        if (TAB..TAB + TABS.len()).contains(&id) {
            return show_tab(id - TAB);
        }
        // What is left to do once the window is let go: close it, or take
        // the question away and say what was done in its place.
        let mut close = false;
        let mut said = "";
        // Cancelling a close while a change outside is still unanswered
        // asks about the change again.
        let mut ask_again = false;
        let answered = with_editor(|e| match id {
            // Saved, nothing is left to ask about; over a change outside,
            // the window's won, which it says.
            SAVE => {
                let over = e.outside.is_some() || e.known != e.base;
                match save(e) {
                    Ok(()) => {
                        said = if over { "外の変更を上書きして保存しました" } else { "保存しました" };
                        true
                    }
                    Err(reason) => {
                        tell(e, &reason);
                        false
                    }
                }
            }
            // Read again only what the parser reads: else the question
            // stays, the reason beside it.
            RELOAD => {
                let Some(bytes) = e.outside.take() else { return true };
                let read = config::parse(&bytes)
                    .map_err(|errors| first_error(&errors))
                    .and_then(|_| Document::new(&bytes).map_err(|e| e.to_string()));
                match read {
                    Ok(doc) => {
                        e.doc = doc;
                        e.base = bytes.clone();
                        e.known = bytes;
                        said = "読み直しました";
                        true
                    }
                    Err(why) => {
                        e.outside = Some(bytes);
                        tell(e, &format!("読み直せません: {why}"));
                        false
                    }
                }
            }
            // The window's stays: what it holds is unchanged, and saving it
            // overwrites the change.
            KEEP => {
                if let Some(bytes) = e.outside.take() {
                    e.known = bytes;
                }
                said = "画面の内容を保ちました（保存すると外の変更を上書きします）";
                true
            }
            // Not saved, the question stays, the reason beside it.
            SAVE_AND_CLOSE => {
                match save(e) {
                    Ok(()) => close = true,
                    Err(reason) => tell(e, &reason),
                }
                false
            }
            DISCARD => {
                close = true;
                false
            }
            CANCEL => {
                ask_again = e.outside.is_some();
                true
            }
            _ => false,
        });
        let Some(hwnd) = with_editor(|e| e.hwnd) else { return };
        if close {
            DestroyWindow(hwnd);
        } else if answered == Some(true) {
            ask(None);
            if ask_again {
                ask(Some(Ask::Changed));
            } else {
                with_editor(|e| set_status(e, said));
            }
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => {
                let mut r: RECT = zeroed();
                GetClientRect(hwnd, &mut r);
                let brush = CreateSolidBrush(colorref(panel::palette().background));
                FillRect(wp as HDC, &r, brush);
                DeleteObject(brush);
                1
            }
            WM_SIZE => {
                lay_out();
                0
            }
            // The scale of the window's monitor changed: the window and its
            // parts grow or shrink to it, where it is. The suggested place
            // in lParam is not read: any process can post this message, and
            // a pointer from it is nothing to follow.
            WM_DPICHANGED => {
                let dpi = (wp & 0xFFFF) as u32;
                // Windows's scales run from 100% to 500%; anything else is
                // no monitor's.
                if !(96..=480).contains(&dpi) {
                    return 0;
                }
                let resized = with_editor(|e| {
                    DeleteObject(e.font);
                    e.font = font_for(dpi);
                    e.dpi = dpi;
                });
                if resized.is_some() {
                    size_for(hwnd, dpi, false);
                    lay_out();
                }
                0
            }
            WM_COMMAND if wp >> 16 & 0xFFFF == 0 => {
                command(wp & 0xFFFF);
                0
            }
            // Esc closes, Ctrl+S saves. The rest of the keyboard comes with
            // the fields of the tabs.
            WM_KEYDOWN if wp == VK_ESCAPE as usize => {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
                0
            }
            WM_KEYDOWN if wp == b'S' as usize && GetKeyState(VK_CONTROL as i32) < 0 => {
                command(SAVE);
                0
            }
            // Alt alone opens no menu: the window has none, and the
            // system menu's mode would swallow the next click on a part.
            // Alt+Space still opens the system menu.
            WM_SYSCOMMAND if wp & 0xFFF0 == SC_KEYMENU as usize && lp == 0 => 0,
            // With changes not saved, ask first (a question about a change
            // outside gives way: the answer to this one settles it too).
            // Nothing of the window's to lose, it just closes.
            WM_CLOSE => {
                match with_editor(|e| unsaved(e)) {
                    Some(true) => ask(Some(Ask::Closing)),
                    Some(false) => {
                        DestroyWindow(hwnd);
                    }
                    // In use further up the stack: try again once it is done,
                    // rather than close past changes.
                    None => {
                        PostMessageW(hwnd, WM_CLOSE, 0, 0);
                    }
                }
                0
            }
            WM_DESTROY => {
                PRESSED.set((0, null_mut()));
                if let Some(e) = EDITOR.take() {
                    DeleteObject(e.font);
                    debug_log!("設定画面を閉じる");
                }
                0
            }
            #[cfg(feature = "fault-injection")]
            _ if msg == test_set() => {
                with_editor(|e| e.doc.set_value("general", "opacity", Some(&wp.to_string())));
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// A tab, button or status line: drawn in the panels' colours, the tab
/// shown and the buttons as keys, the status line as the background. A tab
/// or button tells the window when a pointer lets go on it.
unsafe extern "system" fn part_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let kind = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        match msg {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let c = panel::palette();
                let (fill, ink) = match kind {
                    TAB_SHOWN => (c.held, c.held_text),
                    STATUS => (c.background, c.text),
                    _ => (c.face, c.text),
                };
                let mut r: RECT = zeroed();
                GetClientRect(hwnd, &mut r);
                let brush = CreateSolidBrush(colorref(fill));
                FillRect(dc, &r, brush);
                DeleteObject(brush);
                let font = EDITOR.with(|e| e.try_borrow().ok().and_then(|e| e.as_ref().map(|e| e.font)));
                let old = font.map(|f| SelectObject(dc, f));
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, colorref(ink));
                let mut text = [0u16; 256];
                let n = GetWindowTextW(hwnd, text.as_mut_ptr(), 256);
                // The status line runs to two lines: a reason, and the
                // question it is about.
                let format = match kind {
                    STATUS => DT_LEFT | DT_BOTTOM | DT_WORDBREAK | DT_END_ELLIPSIS,
                    _ => DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                };
                DrawTextW(dc, text.as_ptr(), n, &mut r, format);
                if let Some(old) = old {
                    SelectObject(dc, old);
                }
                EndPaint(hwnd, &ps);
                0
            }
            WM_SETTEXT => {
                let r = DefWindowProcW(hwnd, msg, wp, lp);
                InvalidateRect(hwnd, null(), 1);
                r
            }
            WM_POINTERDOWN if kind != STATUS => {
                PRESSED.set((pointer_id(wp), hwnd));
                0
            }
            WM_POINTERUP if kind != STATUS => {
                let pressed = PRESSED.get() == (pointer_id(wp), hwnd);
                if pressed {
                    PRESSED.set((0, null_mut()));
                }
                let mut r: RECT = zeroed();
                GetWindowRect(hwnd, &mut r);
                let (x, y) = ((lp & 0xFFFF) as i16 as i32, (lp >> 16 & 0xFFFF) as i16 as i32);
                let over = x >= r.left && x < r.right && y >= r.top && y < r.bottom;
                if pressed && over {
                    let id = GetDlgCtrlID(hwnd) as usize;
                    PostMessageW(GetParent(hwnd), WM_COMMAND, id, 0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

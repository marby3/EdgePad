//! The settings window (spec 7.1): a normal window, opened from the tray,
//! that edits the settings file through a `Document` and saves it once the
//! parser reads it. The panels pick the file up as when it is saved by hand.
//!
//! Its parts are small windows of its own, drawn in the panels' colours:
//! tabs, buttons and a status line. A question (the file changed outside,
//! or changes not saved on closing) shows in the status line, its answers
//! as buttons in the row below, beside 保存. Everything is made when the window opens and goes
//! when it closes.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};

use edgepad_core::config;
use edgepad_core::document::Document;
use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::colors::colorref;
use crate::log::debug_log;
use crate::{panel, settings, tray};

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
    unsafe { RegisterWindowMessageW(w!("EdgePadTestEditorSet")) }
}

const STYLE: WINDOW_STYLE = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
const EX_STYLE: WINDOW_EX_STYLE = WS_EX_APPWINDOW;

/// A question the window asks.
#[derive(Clone, Copy, PartialEq)]
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
    /// The file as the window last read or wrote it: unsaved changes are
    /// what the document says otherwise, and a file still so is no change
    /// outside.
    known: Vec<u8>,
    /// The file as changed outside, until the question is answered.
    outside: Option<Vec<u8>>,
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
    // The part a pointer went down on: only it answers that pointer's lift.
    static PRESSED: Cell<HWND> = const { Cell::new(null_mut()) };
}

/// Run `f` on the window, if it is open and not in use further up the stack.
fn with_editor<R>(f: impl FnOnce(&mut Editor) -> R) -> Option<R> {
    EDITOR.with(|e| e.try_borrow_mut().ok()?.as_mut().map(f))
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

/// A part of `parent`: a tab, button or status line, of `kind`.
unsafe fn part(parent: HWND, class: *const u16, text: &str, id: usize, kind: isize) -> HWND {
    unsafe {
        let text = wide(text);
        let hwnd = CreateWindowExW(
            0,
            class,
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE,
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
unsafe fn title_bar(hwnd: HWND) {
    let c = panel::palette().background;
    let dark: i32 = ((c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000 < 128).into();
    unsafe { DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &dark as *const i32 as _, 4) };
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
        if let Some(hwnd) = EDITOR.with_borrow(|e| e.as_ref().map(|e| e.hwnd)) {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd);
            return;
        }
        let Some(path) = settings::path_or_template() else {
            tray::notify("設定ファイルを作れませんでした");
            return;
        };
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
            return;
        }
        let tabs = TABS.map(|_| null_mut());
        let mut editor = Editor {
            hwnd,
            path,
            doc,
            known,
            outside: None,
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
/// pixels; with `centre`, in the middle of its monitor's work area.
unsafe fn size_for(hwnd: HWND, dpi: u32, centre: bool) {
    unsafe {
        let mut r = RECT { left: 0, top: 0, right: scale(CLIENT_W, dpi), bottom: scale(CLIENT_H, dpi) };
        AdjustWindowRectExForDpi(&mut r, STYLE, 0, EX_STYLE, dpi);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        if centre {
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY), &mut info);
            let a = info.rcWork;
            let (x, y) = (a.left + (a.right - a.left - w) / 2, a.top + (a.bottom - a.top - h) / 2);
            SetWindowPos(hwnd, null_mut(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        } else {
            SetWindowPos(hwnd, null_mut(), 0, 0, w, h, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE);
        }
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
        MoveWindow(status, m, y - m - row, (client.right - 2 * m).max(0), row, 1);
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

/// Ask `ask` in the status line, its answers as buttons; or, with none,
/// take a question away.
unsafe fn ask(ask: Option<Ask>) {
    let made = with_editor(|e| unsafe {
        let asked = !e.answers.is_empty();
        for answer in e.answers.drain(..) {
            DestroyWindow(answer);
        }
        let (text, answers): (&str, &[(&str, usize)]) = match ask {
            Some(Ask::Changed) => ("設定ファイルが外で書き換わりました", &[("読み直す", RELOAD), ("画面の内容を保つ", KEEP)]),
            Some(Ask::Closing) => (
                "保存していない変更があります",
                &[("保存する", SAVE_AND_CLOSE), ("保存せずに閉じる", DISCARD), ("キャンセル", CANCEL)],
            ),
            None => ("", &[]),
        };
        // Taken away, a question leaves whatever the status line says now
        // in place: an error from answering it, for one.
        if ask.is_some() || !asked {
            set_status(e, text);
        }
        for &(label, id) in answers {
            e.answers.push(part(e.hwnd, w!("EdgePadSettingsButton"), label, id, BUTTON));
        }
    });
    if made.is_some() {
        unsafe { lay_out() };
    }
}

/// Save the window's file if the parser reads it; else say why, writing
/// nothing. True once saved.
fn save(e: &mut Editor) -> bool {
    let bytes = e.doc.to_bytes();
    if let Err(errors) = config::parse(&bytes) {
        let first = errors.first().map_or_else(String::new, ToString::to_string);
        set_status(e, &format!("保存できません: {first}"));
        return false;
    }
    if let Err(why) = std::fs::write(&e.path, &bytes) {
        set_status(e, &format!("保存できません: {why}"));
        return false;
    }
    debug_log!("設定画面から保存");
    e.known = bytes;
    set_status(e, "保存しました");
    true
}

/// Whether the window holds changes the file does not.
fn unsaved(e: &Editor) -> bool {
    e.doc.to_bytes() != e.known
}

/// The settings folder was written to and has settled: if the file is not
/// as the window last read or wrote it, it changed outside; ask.
pub unsafe fn file_changed() {
    let changed = with_editor(|e| {
        let Ok(now) = read_file(&e.path) else { return false };
        if now == e.known || e.outside.as_ref() == Some(&now) {
            return false;
        }
        e.outside = Some(now);
        true
    });
    if changed == Some(true) {
        debug_log!("設定画面: ファイルが外で書き換わった");
        unsafe { ask(Some(Ask::Changed)) };
    }
}

/// A part was clicked.
unsafe fn command(id: usize) {
    unsafe {
        if (TAB..TAB + TABS.len()).contains(&id) {
            return show_tab(id - TAB);
        }
        // What is left to do once the window is let go: close it, or take
        // the question away.
        let mut close = false;
        let answered = with_editor(|e| match id {
            SAVE => {
                save(e);
                false
            }
            RELOAD => {
                let Some(bytes) = e.outside.take() else { return true };
                match Document::new(&bytes) {
                    Ok(doc) => {
                        e.doc = doc;
                        e.known = bytes;
                        true
                    }
                    // The question stays, the reason beside it.
                    Err(why) => {
                        set_status(e, &format!("読み直せません: {why}"));
                        e.outside = Some(bytes);
                        false
                    }
                }
            }
            KEEP => {
                // The window's stays; saving it overwrites the change.
                if let Some(bytes) = e.outside.take() {
                    e.known = bytes;
                }
                true
            }
            // Not saved, the question stays, the reason beside it.
            SAVE_AND_CLOSE => {
                close = save(e);
                false
            }
            DISCARD => {
                close = true;
                false
            }
            CANCEL => true,
            _ => false,
        });
        let Some(hwnd) = with_editor(|e| e.hwnd) else { return };
        if close {
            DestroyWindow(hwnd);
        } else if answered == Some(true) {
            ask(None);
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
            // parts grow or shrink to it. Windows sends it from this thread
            // with the place to take, as the window is dragged across; any
            // other sender's lParam is not this process's to read, and the
            // window works out its new size where it is itself.
            WM_DPICHANGED => {
                let dpi = (wp & 0xFFFF) as u32;
                if dpi == 0 {
                    return 0;
                }
                let resized = with_editor(|e| {
                    DeleteObject(e.font);
                    e.font = font_for(dpi);
                    e.dpi = dpi;
                });
                if resized.is_some() {
                    if InSendMessage() == 0 && lp != 0 {
                        let r = *(lp as *const RECT);
                        SetWindowPos(hwnd, null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                    } else {
                        size_for(hwnd, dpi, false);
                    }
                    lay_out();
                }
                0
            }
            WM_COMMAND => {
                command(wp & 0xFFFF);
                0
            }
            // With changes not saved, ask first; asked already about a
            // change outside, that answer comes first.
            WM_CLOSE => {
                match with_editor(|e| (unsaved(e), e.outside.is_some())) {
                    Some((_, true)) => {}
                    Some((true, false)) => ask(Some(Ask::Closing)),
                    _ => {
                        DestroyWindow(hwnd);
                    }
                }
                0
            }
            WM_DESTROY => {
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
                let align = if kind == STATUS { DT_LEFT } else { DT_CENTER };
                DrawTextW(dc, text.as_ptr(), n, &mut r, align | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
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
                PRESSED.set(hwnd);
                // A mouse let go off the part still tells it.
                SetCapture(hwnd);
                0
            }
            WM_POINTERUP if kind != STATUS => {
                let pressed = PRESSED.replace(null_mut()) == hwnd;
                ReleaseCapture();
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

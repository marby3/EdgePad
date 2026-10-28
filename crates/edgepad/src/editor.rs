//! The settings window (spec 7.1): a normal window, opened from the tray,
//! that edits the settings file through a `Document` and saves it once the
//! parser reads it. The panels pick the file up as when it is saved by hand.
//!
//! Its parts are Windows's own controls, of the common controls version 6:
//! a sidebar of the seven pages (a list box), the page shown on its right
//! under its heading, a status line, and a row along the bottom with the
//! file's path, テキストで開く and 保存. A question (the file changed outside,
//! or changes not saved on closing) shows in the status line, its answers
//! as buttons in the row below it. Changes not saved show as a ● before the
//! title, and 保存 as the default button.
//!
//! It is light or dark as Windows's apps are, whatever the panels' theme.
//!
//! The keyboard reaches it through `take`, before a key is dispatched: Tab
//! and Shift+Tab move along the parts, Enter presses a button (Space and
//! the arrows are the controls' own), Esc closes and Ctrl+S saves. So does
//! a finger, while a panel holds the mouse for a key it presses.
//!
//! It runs in a process of its own, started by the running EdgePad
//! (`editor_process`), which ends when the window closes: what the window
//! loaded, comctl32 and Windows's text input among it, goes with it
//! (spec 8).

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};

use edgepad_core::config::{self, Accent, Theme};
use edgepad_core::document::Document;
use edgepad_core::theme::Rgb;
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, FreeLibrary, GetLastError, HANDLE, HMODULE, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, POINT, RECT,
    WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
    ACTCTXW, ActivateActCtx, CreateActCtxW, DeactivateActCtx, ReleaseActCtx,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, SystemParametersInfoForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetFocus, GetKeyState, IsWindowEnabled, SetFocus, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LBUTTON, VK_RETURN, VK_SHIFT,
    VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::Input::Pointer::GetPointerType;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::colors::{self, colorref};
use crate::log::debug_log;
use crate::panel::{pointer_at, pointer_id};
use crate::{settings, tray, watch};

mod apps;
mod fields;
mod keymap;

/// The window's class: also how the running EdgePad and the tests find it.
pub const CLASS: *const u16 = w!("EdgePadSettings");

/// The pages, by use (Issue #107).
const PAGES: [&str; 7] = ["キー配置", "アプリ別", "辺（共通）", "辺ごとの設定", "見た目", "タッチと修飾キー", "表示と非表示"];

/// Control ids: the sidebar, the page's heading, the status line, the
/// file's path, テキストで開く, 保存, and the answers to questions. Clear of
/// IDOK and IDCANCEL. The tests find the parts by these.
const SIDEBAR: usize = 10;
const HEADING: usize = 11;
const STATUS: usize = 12;
const PATH: usize = 13;
const OPEN_TEXT: usize = 14;
/// The band that says what was done, for a while.
const NOTICE: usize = 15;
/// How long the band shows, and its timer.
const NOTICE_MS: u32 = 4000;
const NOTICE_TIMER: usize = 1;
const SAVE: usize = 200;
const RELOAD: usize = 300;
const KEEP: usize = 301;
const SAVE_AND_CLOSE: usize = 302;
const DISCARD: usize = 303;
const CANCEL: usize = 304;

/// Sizes in logical pixels (96 DPI).
const CLIENT_W: i32 = 900;
const CLIENT_H: i32 = 660;
/// The least the client area may be: the sidebar, and room beside it for
/// the rows of the pages, a label, a name and a field each.
const MIN_W: i32 = 900;
const MIN_H: i32 = 640;
const MARGIN: i32 = 12;
const SIDEBAR_W: i32 = 168;
const ITEM_H: i32 = 36;
/// The space before a sidebar item's text.
const ITEM_PAD: i32 = 12;
const HEADING_H: i32 = 36;
/// The status line: two lines of text.
const STATUS_H: i32 = 40;
const BUTTON_H: i32 = 30;
const BUTTON_W: i32 = 120;
const ANSWER_W: i32 = 136;

/// The figure asks for edge wParam to be chosen; a field, id wParam, to be
/// taken once the window is free; the sample, for key wParam to be chosen;
/// the parts are to be placed again, once the window is free; and the
/// アプリ別 figure asks for the list of its edge wParam to open.
const WM_CHOOSE_EDGE: u32 = WM_APP + 1;
const WM_COMMIT: u32 = WM_APP + 2;
const WM_CHOOSE_KEY: u32 = WM_APP + 3;
const WM_LAY_OUT: u32 = WM_APP + 4;
const WM_OPEN_EDGE: u32 = WM_APP + 5;

/// Styles, messages and notices of Win32_System_SystemServices and
/// Win32_UI_Controls, features taken for these alone otherwise.
const SS_NOPREFIX: u32 = 0x80;
const SS_CENTERIMAGE: u32 = 0x200;
const SS_CENTER: u32 = 0x1;
const SS_PATHELLIPSIS: u32 = 0x8000;
const ODS_SELECTED: u32 = 0x1;
const ODS_FOCUS: u32 = 0x10;
const ODS_NOFOCUSRECT: u32 = 0x200;
const NM_CLICK: u32 = -2i32 as u32;
const NM_RETURN: u32 = -4i32 as u32;

/// NMHDR, of Win32_UI_Controls.
#[repr(C)]
struct NotifyHeader {
    from: HWND,
    id: usize,
    code: u32,
}

/// DRAWITEMSTRUCT, of Win32_UI_Controls.
#[repr(C)]
struct DrawItem {
    kind: u32,
    id: u32,
    item: u32,
    action: u32,
    state: u32,
    hwnd: HWND,
    dc: HDC,
    rect: RECT,
    data: usize,
}

/// "EdgePadTestEditorSet": sets `[general] opacity` to wParam in the
/// window's copy of the file, past any check: a change for the tests to
/// save, which can be one the parser turns away. Registered by name, as the
/// tray's test messages are: the name is the contract with the tests.
#[cfg(feature = "fault-injection")]
fn test_set() -> u32 {
    thread_local! {
        static ID: u32 = unsafe { RegisterWindowMessageW(w!("EdgePadTestEditorSet")) };
    }
    ID.with(|id| *id)
}

const STYLE: WINDOW_STYLE = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
const EX_STYLE: WINDOW_EX_STYLE = WS_EX_APPWINDOW;
const TITLE: &str = "EdgePad の設定";

/// A question the window asks.
#[derive(Clone, Copy)]
enum Ask {
    /// The file changed outside: read it again, or keep the window's.
    Changed,
    /// Closing with changes not saved.
    Closing,
}

/// The colours the window is drawn in, light or dark as Windows's apps.
#[derive(Clone, Copy, PartialEq)]
struct Look {
    dark: bool,
    background: Rgb,
    sidebar: Rgb,
    text: Rgb,
    /// The path's text: quieter than the rest.
    dim: Rgb,
    /// The page chosen in the sidebar: Windows's accent.
    accent: Rgb,
    accent_text: Rgb,
    /// Inside a field, and inside one showing a value turned away.
    field: Rgb,
    wrong: Rgb,
}

fn look() -> Look {
    let dark = colors::apps_dark();
    let (background, sidebar, text, field, wrong) = match dark {
        true => ([0x20, 0x20, 0x20], [0x2B, 0x2B, 0x2B], [0xF0, 0xF0, 0xF0], [0x2D, 0x2D, 0x2D], [0x5C, 0x24, 0x24]),
        false => ([0xF3, 0xF3, 0xF3], [0xE6, 0xE6, 0xE6], [0x1A, 0x1A, 0x1A], [0xFF, 0xFF, 0xFF], [0xF8, 0xC8, 0xC8]),
    };
    let accent = colors::palette(Theme::Auto, Accent::Auto);
    let dim = mix(text, background, 96);
    Look { dark, background, sidebar, text, dim, accent: accent.held, accent_text: accent.held_text, field, wrong }
}

/// `a` mixed with `b`, `part` of 256 of `b`.
fn mix(a: Rgb, b: Rgb, part: u16) -> Rgb {
    std::array::from_fn(|i| ((a[i] as u16 * (256 - part) + b[i] as u16 * part) / 256) as u8)
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
    sidebar: HWND,
    heading: HWND,
    status: HWND,
    path_part: HWND,
    open_text: HWND,
    save: HWND,
    answers: Vec<HWND>,
    /// The page shown, of PAGES.
    page: usize,
    rows: Vec<fields::Row>,
    figure: HWND,
    /// The edge the figure has chosen, of `fields::EDGES`.
    edge: usize,
    keymap: keymap::Keymap,
    apps: apps::Apps,
    /// The line of the file the last save failed on, while the status line
    /// leads to it.
    fault: Cell<Option<usize>>,
    /// The part that had the focus when the window was last left, to have
    /// it again on coming back.
    focus: HWND,
    /// Whether the title and 保存 show changes not saved.
    marked: bool,
    font: HFONT,
    heading_font: HFONT,
    dpi: u32,
    /// The activation context of the manifest asking for comctl32 v6, made
    /// active while parts are made; invalid if it could not be made.
    context: HANDLE,
}

/// What the window is painted with, and the path's part, drawn quieter.
#[derive(Clone, Copy)]
struct Paint {
    look: Look,
    background: HBRUSH,
    sidebar: HBRUSH,
    field: HBRUSH,
    wrong: HBRUSH,
    accent: HBRUSH,
    font: HFONT,
    path: HWND,
}

impl Paint {
    fn new(look: Look, font: HFONT, path: HWND) -> Paint {
        let brush = |c| unsafe { CreateSolidBrush(colorref(c)) };
        Paint {
            look,
            background: brush(look.background),
            sidebar: brush(look.sidebar),
            field: brush(look.field),
            wrong: brush(look.wrong),
            accent: brush(look.accent),
            font,
            path,
        }
    }

    fn delete(&self) {
        for b in [self.background, self.sidebar, self.field, self.wrong, self.accent] {
            unsafe { DeleteObject(b) };
        }
    }
}

thread_local! {
    static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) };
    // Kept apart from the window's state, which is in use when a part
    // repaints at once (a status line given text, the sidebar a choice);
    // None while the window is closed.
    static PAINT: Cell<Option<Paint>> = const { Cell::new(None) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
    // The window's DPI, as `Editor::dpi`; 0 while it is closed.
    static DPI: Cell<u32> = const { Cell::new(0) };
    // The finger that went down on a part while a panel held the mouse,
    // and the part: only it answers that finger's lift (`finger`).
    static TAPPED: Cell<(u32, HWND)> = const { Cell::new((0, null_mut())) };
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

/// The DPI the window is drawn at: its own, as it last heard it in
/// WM_DPICHANGED. Kept apart from the window's state, which is in use when
/// its parts are moved.
fn window_dpi(hwnd: HWND) -> u32 {
    match DPI.get() {
        0 => unsafe { GetDpiForWindow(hwnd) }.max(96),
        dpi => dpi,
    }
}

/// `v` logical pixels at `dpi`.
fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

/// Windows's font for dialogs at `dpi`, and the same larger and bolder
/// for a heading.
unsafe fn fonts_for(dpi: u32) -> (HFONT, HFONT) {
    unsafe {
        let mut m: NONCLIENTMETRICSW = zeroed();
        m.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        let got = SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS, m.cbSize, &mut m as *mut _ as _, 0, dpi);
        let mut f = m.lfMessageFont;
        if got == 0 {
            f = zeroed();
            f.lfHeight = -scale(12, dpi);
            f.lfCharSet = DEFAULT_CHARSET as u8;
            f.lfQuality = CLEARTYPE_QUALITY as u8;
        }
        let font = CreateFontIndirectW(&f);
        f.lfHeight = f.lfHeight * 3 / 2;
        f.lfWeight = FW_SEMIBOLD as i32;
        (font, CreateFontIndirectW(&f))
    }
}

/// A function of a system DLL, by name: loaded here, not imported, so the
/// DLL is in EdgePad only once the window is open (spec 8). With the DLL,
/// to free once done with the function.
pub(super) unsafe fn system_proc(dll: *const u16, name: &[u8]) -> Option<(HMODULE, unsafe extern "system" fn() -> isize)> {
    unsafe {
        let module = LoadLibraryExW(dll, null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if module.is_null() {
            return None;
        }
        match GetProcAddress(module, name.as_ptr()) {
            Some(f) => Some((module, f)),
            None => {
                FreeLibrary(module);
                None
            }
        }
    }
}

/// Make the title bar dark or light.
unsafe fn title_bar(hwnd: HWND, dark: bool) {
    type SetAttribute = unsafe extern "system" fn(HWND, u32, *const core::ffi::c_void, u32) -> i32;
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    let dark: i32 = dark.into();
    unsafe {
        let Some((dwm, f)) = system_proc(w!("dwmapi.dll"), b"DwmSetWindowAttribute\0") else { return };
        let set: SetAttribute = std::mem::transmute(f);
        set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const i32 as _, 4);
        FreeLibrary(dwm);
    }
}

/// Draw `buttons` dark or light. The dark look of Windows's own controls
/// is the theme File Explorer uses, "DarkMode_Explorer", which Windows
/// does not document; light, they keep their own theme, the default, which
/// costs no more memory to load.
unsafe fn button_theme(buttons: &[HWND], dark: bool) {
    type SetTheme = unsafe extern "system" fn(HWND, *const u16, *const u16) -> i32;
    unsafe {
        let Some((uxtheme, f)) = system_proc(w!("uxtheme.dll"), b"SetWindowTheme\0") else { return };
        let set: SetTheme = std::mem::transmute(f);
        let name = if dark { w!("DarkMode_Explorer") } else { null() };
        for &b in buttons {
            set(b, name, null());
        }
        FreeLibrary(uxtheme);
    }
}

/// Draw edit controls and drop-down lists dark or light: dark, in the
/// theme of the common file dialog's ("DarkMode_CFD", not documented).
unsafe fn field_theme(fields: &[HWND], dark: bool) {
    type SetTheme = unsafe extern "system" fn(HWND, *const u16, *const u16) -> i32;
    unsafe {
        let Some((uxtheme, f)) = system_proc(w!("uxtheme.dll"), b"SetWindowTheme\0") else { return };
        let set: SetTheme = std::mem::transmute(f);
        let name = if dark { w!("DarkMode_CFD") } else { null() };
        for &field in fields {
            set(field, name, null());
        }
        FreeLibrary(uxtheme);
    }
}

/// The activation context of the manifest embedded as resource 2 (see
/// build.rs), which asks for comctl32 v6.
unsafe fn make_context() -> HANDLE {
    const ACTCTX_FLAG_RESOURCE_NAME_VALID: u32 = 0x8;
    const ACTCTX_FLAG_HMODULE_VALID: u32 = 0x80;
    unsafe {
        let mut c: ACTCTXW = zeroed();
        c.cbSize = size_of::<ACTCTXW>() as u32;
        c.dwFlags = ACTCTX_FLAG_RESOURCE_NAME_VALID | ACTCTX_FLAG_HMODULE_VALID;
        c.hModule = GetModuleHandleW(null());
        c.lpResourceName = 2 as *const u16;
        let context = CreateActCtxW(&c);
        if context == INVALID_HANDLE_VALUE {
            debug_log!("設定画面: comctl32 v6 を使えない（旧来の見た目で開く）");
        }
        context
    }
}

/// Run `f` with `context` active: the parts it makes are comctl32 v6's.
unsafe fn in_context<R>(context: HANDLE, f: impl FnOnce() -> R) -> R {
    unsafe {
        let mut cookie = 0;
        let on = context != INVALID_HANDLE_VALUE && ActivateActCtx(context, &mut cookie) != 0;
        let r = f();
        if on {
            DeactivateActCtx(0, cookie);
        }
        r
    }
}

unsafe fn register() -> bool {
    if REGISTERED.get() {
        return true;
    }
    unsafe {
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = GetModuleHandleW(null());
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.lpszClassName = CLASS;
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            debug_log!("設定画面の窓クラスを登録できない");
            return false;
        }
        wc.lpfnWndProc = Some(fields::figure_proc);
        wc.hCursor = LoadCursorW(null_mut(), IDC_HAND);
        wc.lpszClassName = w!("EdgePadSettingsEdges");
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            debug_log!("設定画面の図の窓クラスを登録できない");
            return false;
        }
        wc.lpfnWndProc = Some(keymap::sample_proc);
        wc.lpszClassName = keymap::SAMPLE_CLASS;
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            debug_log!("設定画面の見本の窓クラスを登録できない");
            return false;
        }
        wc.lpfnWndProc = Some(apps::figure_proc);
        wc.lpszClassName = apps::FIGURE_CLASS;
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            debug_log!("設定画面のアプリ別の図の窓クラスを登録できない");
            return false;
        }
        wc.lpfnWndProc = Some(keymap::pie_proc);
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.lpszClassName = keymap::PIE_CLASS;
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            debug_log!("設定画面のパイの図の窓クラスを登録できない");
            return false;
        }
    }
    REGISTERED.set(true);
    true
}

/// A part of `parent` of `class` and `style`, with `text` and `id`; made
/// hidden, to be shown once it is in its place.
unsafe fn part(parent: HWND, class: *const u16, text: &str, id: usize, style: u32) -> HWND {
    unsafe {
        let text = wide(text);
        CreateWindowExW(0, class, text.as_ptr(), WS_CHILD | style, 0, 0, 1, 1, parent, id as HMENU, GetModuleHandleW(null()), null())
    }
}

unsafe fn button(parent: HWND, text: &str, id: usize) -> HWND {
    unsafe { part(parent, w!("Button"), text, id, WS_TABSTOP | BS_PUSHBUTTON as u32) }
}

/// The settings file, read no further than one byte past what the parser
/// takes: a file any bigger is turned away whole anyway.
fn read_file(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(config::MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Windows's colours may have changed: draw the window again in them.
unsafe fn restyle() {
    let now = look();
    let Some(old) = PAINT.get().filter(|p| p.look != now) else { return };
    let Some((hwnd, buttons, typed)) = with_editor(|e| {
        let (mut typed, theirs) = fields::themed(e);
        let (keymap_typed, keymap_buttons) = keymap::themed(e);
        let (apps_typed, apps_buttons) = apps::themed(e);
        typed.extend(keymap_typed.into_iter().chain(apps_typed));
        let buttons: Vec<HWND> = [e.open_text, e.save]
            .into_iter()
            .chain(e.answers.iter().copied())
            .chain(theirs)
            .chain(keymap_buttons)
            .chain(apps_buttons)
            .collect();
        (e.hwnd, buttons, typed)
    }) else {
        return;
    };
    debug_log!("設定画面: 配色の変更");
    unsafe {
        PAINT.set(Some(Paint::new(now, old.font, old.path)));
        old.delete();
        title_bar(hwnd, now.dark);
        button_theme(&buttons, now.dark);
        field_theme(&typed, now.dark);
        for part in typed {
            RedrawWindow(part, null(), null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_FRAME);
        }
        // The sample's panel colours, of theme = auto, follow Windows too.
        with_editor(|e| keymap::refresh(e));
        RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN);
    }
}

/// The settings file, made from the template if there is none, and as a
/// document; else why the window cannot open it, as the tray tells it.
fn load() -> Result<(PathBuf, Document, Vec<u8>), String> {
    let path = settings::path_or_template().ok_or("設定ファイルを作れませんでした")?;
    let read = read_file(&path).map_err(|e| e.to_string());
    match read.and_then(|b| Document::new(&b).map(|d| (d, b)).map_err(|e| e.to_string())) {
        Ok((doc, bytes)) => Ok((path, doc, bytes)),
        Err(why) => Err(format!("設定ファイルを開けません: {why}")),
    }
}

/// Whether the settings window can open the settings file: asked by the
/// running EdgePad before it starts the window's process, as only it can
/// tell why not.
pub fn check_file() -> Result<(), String> {
    load().map(|_| ())
}

/// "EdgePadSettingsMayExit", sent by the running EdgePad when it is to end:
/// the window answers whether it may (`may_exit`). Registered by name: the
/// name is the contract between the two processes.
pub fn may_exit_message() -> u32 {
    thread_local! {
        static ID: u32 = unsafe { RegisterWindowMessageW(w!("EdgePadSettingsMayExit")) };
    }
    ID.with(|id| *id)
}

/// The settings window's process (`editor_process`): open the window, and
/// run until it closes. False if it could not open.
pub unsafe fn run() -> bool {
    unsafe { watch::run_settings(|| open()) }
}

/// Open the settings window. False if it could not be.
unsafe fn open() -> bool {
    unsafe {
        let (path, doc, known) = match load() {
            Ok(loaded) => loaded,
            Err(why) => {
                debug_log!("設定画面を開けない: {why}");
                return false;
            }
        };
        if !register() {
            return false;
        }
        let title = wide(TITLE);
        let hwnd = CreateWindowExW(
            EX_STYLE,
            CLASS,
            title.as_ptr(),
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
            return false;
        }
        let dpi = GetDpiForWindow(hwnd).max(96);
        DPI.set(dpi);
        let context = make_context();
        let parts = in_context(context, || {
            let list = LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT;
            let sidebar = part(hwnd, w!("ListBox"), "", SIDEBAR, WS_TABSTOP | list as u32);
            let heading = part(hwnd, w!("Static"), PAGES[0], HEADING, SS_NOPREFIX);
            // A link, of comctl32 v6; plain text without it.
            let mut status = part(hwnd, w!("SysLink"), "", STATUS, 0);
            if status.is_null() {
                status = part(hwnd, w!("Static"), "", STATUS, SS_NOPREFIX);
            }
            let shown = path.display().to_string();
            let path_part = part(hwnd, w!("Static"), &shown, PATH, SS_NOPREFIX | SS_CENTERIMAGE | SS_PATHELLIPSIS);
            // Hidden till there is something to say.
            part(hwnd, w!("Static"), "", NOTICE, SS_NOPREFIX | SS_CENTERIMAGE | SS_CENTER);
            // The pages' parts, between the sidebar and the bottom row, for
            // the Tab key's order.
            let keymap = keymap::make(hwnd);
            let apps = apps::make(hwnd);
            let rows = fields::make(hwnd).filter(|_| keymap && apps);
            let open_text = button(hwnd, "テキストで開く", OPEN_TEXT);
            let save = button(hwnd, "保存", SAVE);
            ([sidebar, heading, status, path_part, open_text, save], rows)
        });
        let (parts, rows) = parts;
        let Some((rows, figure)) = rows.filter(|_| !parts.iter().any(|p| p.is_null())) else {
            debug_log!("設定画面の部品を作れない");
            DestroyWindow(hwnd);
            if context != INVALID_HANDLE_VALUE {
                ReleaseActCtx(context);
            }
            DPI.set(0);
            return false;
        };
        let [sidebar, heading, status, path_part, open_text, save] = parts;
        for page in PAGES {
            let text = wide(page);
            SendMessageW(sidebar, LB_ADDSTRING, 0, text.as_ptr() as LPARAM);
        }
        SendMessageW(sidebar, LB_SETCURSEL, 0, 0);
        let (font, heading_font) = fonts_for(dpi);
        let look = look();
        let editor = Editor {
            hwnd,
            path,
            doc,
            base: known.clone(),
            known,
            outside: None,
            asked: None,
            sidebar,
            heading,
            status,
            path_part,
            open_text,
            save,
            answers: Vec::new(),
            page: 0,
            rows,
            figure,
            edge: 0,
            keymap: keymap::Keymap::default(),
            apps: apps::Apps::default(),
            fault: Cell::new(None),
            focus: sidebar,
            marked: false,
            font,
            heading_font,
            dpi,
            context,
        };
        PAINT.set(Some(Paint::new(look, font, path_part)));
        set_fonts(&editor);
        let (mut typed, mut buttons) = fields::themed(&editor);
        let (keymap_typed, keymap_buttons) = keymap::themed(&editor);
        let (apps_typed, apps_buttons) = apps::themed(&editor);
        typed.extend(keymap_typed.into_iter().chain(apps_typed));
        buttons.extend(keymap_buttons.into_iter().chain(apps_buttons));
        EDITOR.set(Some(editor));
        with_editor(|e| {
            fields::refresh(e);
            keymap::refresh(e);
            apps::refresh(e);
        });
        title_bar(hwnd, look.dark);
        button_theme(&[open_text, save].into_iter().chain(buttons).collect::<Vec<_>>(), look.dark);
        field_theme(&typed, look.dark);
        // The focus shows from the start, not only once a key is pressed.
        SendMessageW(hwnd, WM_CHANGEUISTATE, (UIS_CLEAR | UISF_HIDEFOCUS << 16) as usize, 0);
        debug_log!("設定画面を開く");
        size_for(hwnd, dpi, true);
        show_page(0);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(sidebar);
    }
    true
}

/// Give the parts the window's fonts, and the sidebar's items their height.
unsafe fn set_fonts(e: &Editor) {
    unsafe {
        let all = [e.sidebar, e.status, e.path_part, e.open_text, e.save].into_iter().chain(e.answers.iter().copied());
        for p in all.chain(fields::parts(e)).chain(keymap::parts(e)).chain(apps::parts(e)) {
            SendMessageW(p, WM_SETFONT, e.font as WPARAM, 1);
        }
        SendMessageW(e.heading, WM_SETFONT, e.heading_font as WPARAM, 1);
        SendMessageW(GetDlgItem(e.hwnd, NOTICE as i32), WM_SETFONT, e.font as WPARAM, 1);
        fields::set_name_font(e, e.font);
        SendMessageW(e.sidebar, LB_SETITEMHEIGHT, 0, scale(ITEM_H, e.dpi) as LPARAM);
    }
}

/// Size the window for `dpi`, its client area CLIENT_W by CLIENT_H logical
/// pixels, or as much of that as its monitor's work area has room for:
/// with `centre`, in the middle of the work area, else where it is, moved
/// only as far as keeps it on the monitor it is on.
unsafe fn size_for(hwnd: HWND, dpi: u32, centre: bool) {
    unsafe {
        let mut r = RECT { left: 0, top: 0, right: scale(CLIENT_W, dpi), bottom: scale(CLIENT_H, dpi) };
        AdjustWindowRectExForDpi(&mut r, STYLE, 0, EX_STYLE, dpi);
        let a = work_area(hwnd);
        let (w, h) = ((r.right - r.left).min(a.right - a.left), (r.bottom - r.top).min(a.bottom - a.top));
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

/// The work area of the monitor `hwnd` is on.
unsafe fn work_area(hwnd: HWND) -> RECT {
    unsafe {
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info);
        info.rcWork
    }
}

/// Place the parts in the window as large as it is now: the sidebar down
/// the left; the heading top right; the status line, and any answers in a
/// row under it, above the bottom row; along the bottom, the path, then
/// テキストで開く and 保存 in the right corner.
unsafe fn lay_out() {
    let Some((hwnd, dpi, parts, answers)) = with_editor(|e| {
        (e.hwnd, e.dpi, [e.sidebar, e.heading, e.status, e.path_part, e.open_text, e.save], e.answers.clone())
    }) else {
        return;
    };
    let [sidebar, heading, status, path_part, open_text, save] = parts;
    unsafe {
        let mut client: RECT = zeroed();
        GetClientRect(hwnd, &mut client);
        let s = |v| scale(v, dpi);
        let (m, bh) = (s(MARGIN), s(BUTTON_H));
        let bottom = client.bottom - m - bh;
        let mut x = client.right - m - s(BUTTON_W);
        MoveWindow(save, x, bottom, s(BUTTON_W), bh, 1);
        x -= m + s(BUTTON_W);
        MoveWindow(open_text, x, bottom, s(BUTTON_W), bh, 1);
        MoveWindow(path_part, m, bottom, (x - 2 * m).max(0), bh, 1);

        let side_bottom = bottom - m;
        MoveWindow(sidebar, m, m, s(SIDEBAR_W), (side_bottom - m).max(0), 1);
        let left = m + s(SIDEBAR_W) + 2 * m;
        let width = (client.right - m - left).max(0);
        MoveWindow(heading, left, m, width, s(HEADING_H), 1);
        MoveWindow(GetDlgItem(hwnd, NOTICE as i32), left + width / 3, m, width - width / 3, s(HEADING_H), 1);
        // The rows, down to the status line, a row of answers left room for.
        let rows_bottom = side_bottom - bh - m - s(STATUS_H) - m;
        with_editor(|e| {
            let top = m + s(HEADING_H) + m;
            fields::place(e, left, top, rows_bottom);
            keymap::place(e, left, top, width, rows_bottom);
            apps::place(e, left, top, width, rows_bottom);
            fields::show(e);
            keymap::show(e);
            apps::show(e);
        });

        // The answers right aligned in their row; the status line above.
        let mut above = side_bottom;
        if !answers.is_empty() {
            above -= bh;
            let mut x = client.right - m;
            for &answer in answers.iter().rev() {
                x -= s(ANSWER_W);
                MoveWindow(answer, x, above, s(ANSWER_W), bh, 1);
                x -= m;
            }
            above -= m;
        }
        MoveWindow(status, left, above - s(STATUS_H), width, s(STATUS_H), 1);
        // In their places now: shown, and not before.
        for &part in parts.iter().chain(&answers) {
            ShowWindow(part, SW_SHOWNA);
        }
        InvalidateRect(hwnd, null(), 1);
    }
}

/// Show page `index`, under its heading, and choose it in the sidebar.
unsafe fn show_page(index: usize) {
    let Some((sidebar, heading)) = with_editor(|e| {
        e.page = index;
        keymap::cancel_capture(e);
        unsafe {
            fields::refresh(e);
            fields::show(e);
            if index == keymap::PAGE {
                keymap::refresh(e);
            }
            keymap::show(e);
            if index == apps::PAGE {
                apps::ask_active(e);
                apps::refresh(e);
            }
            apps::show(e);
        }
        (e.sidebar, e.heading)
    }) else {
        return;
    };
    unsafe {
        if SendMessageW(sidebar, LB_GETCURSEL, 0, 0) != index as LRESULT {
            SendMessageW(sidebar, LB_SETCURSEL, index, 0);
        }
        let text = wide(PAGES[index]);
        SetWindowTextW(heading, text.as_ptr());
    }
    tray::test_event(&format!("settings: page {}", PAGES[index]));
}

/// Say what was done, `text`, in the band at the top, for a while: apart
/// from the status line, which says what to fix (Issue #107).
fn notify(e: &Editor, text: &str) {
    unsafe {
        let band = GetDlgItem(e.hwnd, NOTICE as i32);
        let text = wide(text);
        SetWindowTextW(band, text.as_ptr());
        ShowWindow(band, SW_SHOWNA);
        SetTimer(e.hwnd, NOTICE_TIMER, NOTICE_MS, None);
    }
}

/// Say `text` in the status line.
fn set_status(e: &Editor, text: &str) {
    e.fault.set(None);
    show_status(e, text, None);
}

/// Put `text` in the status line, `link` (its start) as a link: one the
/// Tab key stops at then.
fn show_status(e: &Editor, text: &str, link: Option<&str>) {
    // Text from the file must not read as the link's markup.
    let plain = |t: &str| t.replace('<', "＜").replace('>', "＞");
    let text = match link {
        Some(link) => format!("<a>{}</a>{}", plain(link), plain(text)),
        None => plain(text),
    };
    let text = wide(&text);
    unsafe {
        SetWindowTextW(e.status, text.as_ptr());
        let style = GetWindowLongW(e.status, GWL_STYLE) as u32;
        let style = if link.is_some() { style | WS_TABSTOP } else { style & !WS_TABSTOP };
        SetWindowLongW(e.status, GWL_STYLE, style as i32);
    }
}

/// The status line says only the question asked now, if any.
fn ask_reason(e: &Editor) {
    set_status(e, e.asked.map_or("", |a| question(e, a)));
}

/// Say why a save failed on line `line`: as a link to it, the question
/// asked now, if any, on the line below.
fn tell_fault(e: &Editor, reason: &str, line: usize) {
    let rest = e.asked.map_or_else(String::new, |a| format!("\n{}", question(e, a)));
    show_status(e, &rest, Some(reason));
    e.fault.set(Some(line));
}

/// The status line leading to the line a save failed on was pressed: show
/// the page of that line, and the field of its setting, if it has one.
unsafe fn go_to_fault() {
    let Some(((page, row, edge), section)) = with_editor(|e| {
        let (section, name) = e.doc.place(e.fault.get()?);
        Some((fields::locate(section.as_deref(), name.as_deref()), section))
    })
    .flatten() else {
        return;
    };
    // Showing the edge's rows may tell of a value they cannot read: the
    // status line still leads to the line at fault, as it did.
    let Some((status, kept, line)) = with_editor(|e| {
        let mut text = [0u16; 1024];
        let n = unsafe { GetWindowTextW(e.status, text.as_mut_ptr(), 1024) };
        (e.status, text[..n.max(0) as usize].to_vec(), e.fault.get())
    }) else {
        return;
    };
    unsafe {
        show_page(page);
        if let Some(edge) = edge.filter(|_| row.is_some()) {
            fields::choose_edge(edge);
        }
        if let Some(set) = section.as_deref().and_then(|s| s.strip_prefix("set.")) {
            keymap::choose_set(set);
        }
        if let Some(field) = row.and_then(|i| with_editor(|e| fields::field(e, i))) {
            SetFocus(field);
        }
        let kept: Vec<u16> = kept.into_iter().chain([0]).collect();
        SetWindowTextW(status, kept.as_ptr());
        SetWindowLongW(status, GWL_STYLE, (GetWindowLongW(status, GWL_STYLE) as u32 | WS_TABSTOP) as i32);
        with_editor(|e| e.fault.set(line));
    }
}

/// Enter what is typed in the field with the focus, if any: before
/// saving, or closing.
unsafe fn enter_focused() {
    unsafe {
        let focus = GetFocus();
        let id = GetDlgCtrlID(focus) as usize;
        match with_editor(|e| (fields::is_typed(e, focus), keymap::is_typed(e, focus), apps::is_typed(e, focus))) {
            Some((true, ..)) => {
                fields::commit(id, false);
            }
            Some((_, true, _)) => {
                keymap::commit(id);
            }
            Some((.., true)) => {
                apps::commit();
            }
            _ => {}
        }
    }
}

/// Show whether the window holds changes it has not saved: a ● before the
/// title, and 保存 the default button.
unsafe fn mark_unsaved(e: &mut Editor) {
    let now = unsaved(e);
    if now == e.marked {
        return;
    }
    e.marked = now;
    let title = wide(&if now { format!("● {TITLE}") } else { TITLE.into() });
    let style = if now { BS_DEFPUSHBUTTON } else { BS_PUSHBUTTON };
    unsafe {
        SetWindowTextW(e.hwnd, title.as_ptr());
        SendMessageW(e.save, BM_SETSTYLE, style as WPARAM, 1);
    }
}

/// The reason a field's value was turned away, of one still standing, of
/// any page.
fn standing_reason(e: &Editor) -> Option<String> {
    fields::standing_reason(e).or_else(|| keymap::standing_reason(e)).or_else(|| apps::standing_reason(e))
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
            let answer = in_context(e.context, || button(e.hwnd, label, id));
            if answer.is_null() {
                debug_log!("設定画面の答えのボタンを作れない: {label}");
                continue;
            }
            SendMessageW(answer, WM_SETFONT, e.font as WPARAM, 1);
            e.answers.push(answer);
        }
        button_theme(&e.answers, PAINT.get().is_some_and(|p| p.look.dark));
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
/// the reason why, and the line at fault if the parser named one.
fn save(e: &mut Editor) -> Result<(), (String, Option<usize>)> {
    let bytes = e.doc.to_bytes();
    if let Err(errors) = settings::parse(&e.path, &bytes).0 {
        let line = errors.first().map(|e| e.line).filter(|&l| l > 0);
        return Err((format!("保存できません: {}", first_error(&errors)), line));
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
        return Err((format!("保存できません: {why}"), None));
    }
    debug_log!("設定画面から保存");
    e.base = bytes.clone();
    e.known = bytes;
    // Saved, the window's is the file: a change outside is answered.
    e.outside = None;
    unsafe { mark_unsaved(e) };
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

/// EdgePad is to end (`may_exit_message`): unless the window holds
/// changes not saved, in which case it asks about them and EdgePad goes on.
unsafe fn may_exit() -> bool {
    if !is_open() {
        return true;
    }
    // What is typed and not yet entered counts too.
    unsafe { enter_focused() };
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
    false
}

/// Say why a save failed: leading to its line, if it has one.
fn tell_failed(e: &Editor, reason: &str, line: Option<usize>) {
    match line {
        Some(line) => tell_fault(e, reason, line),
        None => tell(e, reason),
    }
}

/// A button was pressed.
unsafe fn command(id: usize) {
    unsafe {
        if matches!(id, SAVE | SAVE_AND_CLOSE) {
            enter_focused();
        }
        if id == OPEN_TEXT {
            if let Some((hwnd, path)) = with_editor(|e| (e.hwnd, e.path.clone())) {
                tray::open_file(hwnd, &path);
            }
            return;
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
                    Err((reason, line)) => {
                        tell_failed(e, &reason, line);
                        false
                    }
                }
            }
            // An answer counts only to the question asked now: one posted
            // by anything else changes nothing.
            RELOAD | KEEP if !matches!(e.asked, Some(Ask::Changed)) => false,
            SAVE_AND_CLOSE | DISCARD | CANCEL if !matches!(e.asked, Some(Ask::Closing)) => false,
            // Read again only what the parser reads: else the question
            // stays, the reason beside it.
            RELOAD => {
                let Some(bytes) = e.outside.take() else { return true };
                let read = settings::parse(&e.path, &bytes)
                    .0
                    .map_err(|errors| first_error(&errors))
                    .and_then(|_| Document::new(&bytes).map_err(|e| e.to_string()));
                match read {
                    Ok(doc) => {
                        e.doc = doc;
                        e.base = bytes.clone();
                        e.known = bytes;
                        mark_unsaved(e);
                        fields::refresh(e);
                        keymap::refresh(e);
                        apps::refresh(e);
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
                    Err((reason, line)) => tell_failed(e, &reason, line),
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
                // Saved with a field's value turned away: that value is not
                // in the file, which the status line goes on saying.
                with_editor(|e| match () {
                    // Kept with the change outside: a warning, which stays.
                    _ if id == KEEP => set_status(e, said),
                    _ => {
                        if !said.is_empty() {
                            notify(e, said);
                        }
                        match standing_reason(e).filter(|_| id == SAVE) {
                            Some(reason) => set_status(e, &format!("保存していない値があります: {reason}")),
                            None => set_status(e, ""),
                        }
                    }
                });
            }
        }
    }
}

/// Paint sidebar item `d`: on the sidebar's colour, or the accent when
/// chosen; its text inset from the left; framed while it has the focus,
/// as the list box shows it.
unsafe fn draw_item(d: &DrawItem) {
    let Some(Paint { look, font, .. }) = PAINT.get() else { return };
    let Some(&page) = PAGES.get(d.item as usize) else { return };
    unsafe {
        let chosen = d.state & ODS_SELECTED != 0;
        let (fill, ink) = if chosen { (look.accent, look.accent_text) } else { (look.sidebar, look.text) };
        let brush = CreateSolidBrush(colorref(fill));
        FillRect(d.dc, &d.rect, brush);
        DeleteObject(brush);
        let old = SelectObject(d.dc, font);
        SetBkMode(d.dc, TRANSPARENT as i32);
        SetTextColor(d.dc, colorref(ink));
        let pad = scale(ITEM_PAD, window_dpi(d.hwnd));
        let mut r = RECT { left: d.rect.left + pad, ..d.rect };
        let text = wide(page);
        DrawTextW(d.dc, text.as_ptr(), -1, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
        SelectObject(d.dc, old);
        if d.state & ODS_FOCUS != 0 && d.state & ODS_NOFOCUSRECT == 0 {
            let b = scale(2, window_dpi(d.hwnd));
            let r = RECT { left: d.rect.left + b, top: d.rect.top + b, right: d.rect.right - b, bottom: d.rect.bottom - b };
            SetTextColor(d.dc, colorref(ink));
            SetBkColor(d.dc, colorref(fill));
            DrawFocusRect(d.dc, &r);
        }
    }
}

/// The colours of a part, for WM_CTLCOLOR*: its text, and its background
/// as a brush.
unsafe fn part_colours(dc: HDC, part: HWND) -> LRESULT {
    let Some(Paint { look, background: brush, sidebar: side, path, .. }) = PAINT.get() else { return 0 };
    let class = unsafe {
        let mut name = [0u16; 16];
        let n = GetClassNameW(part, name.as_mut_ptr(), 16);
        String::from_utf16_lossy(&name[..n.max(0) as usize])
    };
    let paint = PAINT.get();
    let wrong = fields::is_wrong(part) || keymap::is_wrong(part) || apps::is_wrong(part);
    let sidebar = unsafe { GetDlgCtrlID(part) } as usize == SIDEBAR;
    let (ink, fill, brush) = match class.as_str() {
        "Edit" | "ComboLBox" if wrong => (look.text, look.wrong, paint.map_or(brush, |p| p.wrong)),
        "Edit" | "ComboLBox" => (look.text, look.field, paint.map_or(brush, |p| p.field)),
        "ListBox" if sidebar => (look.text, look.sidebar, side),
        "ListBox" => (look.text, look.field, paint.map_or(brush, |p| p.field)),
        _ if part == path || fields::is_name(part) => (look.dim, look.background, brush),
        _ if unsafe { GetDlgCtrlID(part) } as usize == NOTICE => (look.accent_text, look.accent, paint.map_or(brush, |p| p.accent)),
        _ => (look.text, look.background, brush),
    };
    unsafe {
        SetTextColor(dc, colorref(ink));
        SetBkColor(dc, colorref(fill));
    }
    brush as LRESULT
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => {
                let mut r: RECT = zeroed();
                GetClientRect(hwnd, &mut r);
                match PAINT.get().map(|p| p.background) {
                    Some(brush) => {
                        FillRect(wp as HDC, &r, brush);
                    }
                    None => {
                        FillRect(wp as HDC, &r, GetSysColorBrush(COLOR_WINDOW));
                    }
                }
                1
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT => part_colours(wp as HDC, lp as HWND),
            WM_TIMER if wp == NOTICE_TIMER => {
                KillTimer(hwnd, NOTICE_TIMER);
                ShowWindow(GetDlgItem(hwnd, NOTICE as i32), SW_HIDE);
                0
            }
            // The mouse over a row: its note shows.
            WM_SETCURSOR => {
                if with_editor(|e| fields::hover(e, wp as HWND)) == Some(true) {
                    with_editor(|e| fields::show(e));
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_NOTIFY => {
                let header = &*(lp as *const NotifyHeader);
                if header.id == STATUS && matches!(header.code, NM_CLICK | NM_RETURN) {
                    go_to_fault();
                }
                0
            }
            WM_CHOOSE_EDGE => {
                if wp < 4 {
                    fields::choose_edge(wp);
                }
                0
            }
            WM_COMMIT => {
                if fields::is_field(wp) {
                    fields::commit(wp, false);
                } else if keymap::is_typed_id(wp) {
                    keymap::commit(wp);
                } else if apps::is_part(wp) {
                    apps::commit();
                }
                0
            }
            WM_OPEN_EDGE => {
                if wp < 4 {
                    apps::open_edge(wp);
                }
                0
            }
            WM_CHOOSE_KEY => {
                keymap::choose_key(wp);
                0
            }
            WM_LAY_OUT => {
                lay_out();
                0
            }
            WM_DRAWITEM if wp == SIDEBAR => {
                draw_item(&*(lp as *const DrawItem));
                1
            }
            WM_SIZE => {
                lay_out();
                0
            }
            // No smaller than MIN_W by MIN_H, or as much of that as the
            // work area has room for.
            WM_GETMINMAXINFO => {
                let dpi = window_dpi(hwnd);
                let mut r = RECT { left: 0, top: 0, right: scale(MIN_W, dpi), bottom: scale(MIN_H, dpi) };
                AdjustWindowRectExForDpi(&mut r, STYLE, 0, EX_STYLE, dpi);
                let a = work_area(hwnd);
                let m = &mut *(lp as *mut MINMAXINFO);
                m.ptMinTrackSize.x = (r.right - r.left).min(a.right - a.left);
                m.ptMinTrackSize.y = (r.bottom - r.top).min(a.bottom - a.top);
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
                    let old = (e.font, e.heading_font);
                    (e.font, e.heading_font) = fonts_for(dpi);
                    if let Some(p) = PAINT.get() {
                        PAINT.set(Some(Paint { font: e.font, ..p }));
                    }
                    e.dpi = dpi;
                    DPI.set(dpi);
                    set_fonts(e);
                    DeleteObject(old.0);
                    DeleteObject(old.1);
                });
                if resized.is_some() {
                    size_for(hwnd, dpi, false);
                    lay_out();
                }
                0
            }
            // Windows's light or dark, or its accent, may have changed.
            WM_SETTINGCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED => {
                restyle();
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_COMMAND if wp & 0xFFFF == SIDEBAR => {
                if (wp >> 16 & 0xFFFF) as u32 == LBN_SELCHANGE {
                    let chosen = SendMessageW(lp as HWND, LB_GETCURSEL, 0, 0);
                    if (0..PAGES.len() as LRESULT).contains(&chosen) {
                        show_page(chosen as usize);
                    }
                }
                0
            }
            WM_COMMAND if fields::is_field(wp & 0xFFFF) => {
                let id = wp & 0xFFFF;
                match (wp >> 16 & 0xFFFF) as u32 {
                    // Left: what was typed is entered, once the window is
                    // free if it is not now.
                    EN_KILLFOCUS => {
                        if !fields::commit(id, false) {
                            PostMessageW(hwnd, WM_COMMIT, id, 0);
                        }
                        with_editor(|e| fields::show(e));
                    }
                    // A note shows while its field has the focus.
                    EN_SETFOCUS | CBN_SETFOCUS | CBN_KILLFOCUS => {
                        with_editor(|e| fields::show(e));
                    }
                    CBN_SELCHANGE | BN_CLICKED => {
                        fields::commit(id, true);
                    }
                    _ => {}
                }
                0
            }
            WM_COMMAND if keymap::is_part(wp & 0xFFFF) => {
                keymap::command(wp & 0xFFFF, (wp >> 16 & 0xFFFF) as u32);
                0
            }
            WM_COMMAND if apps::is_part(wp & 0xFFFF) => {
                apps::command(wp & 0xFFFF, (wp >> 16 & 0xFFFF) as u32);
                0
            }
            WM_COMMAND if wp & 0xFFFF == fields::CAPTURE => {
                if (wp >> 16 & 0xFFFF) as u32 == BN_CLICKED {
                    keymap::capture_hotkey();
                }
                0
            }
            WM_COMMAND if fields::is_reset(wp & 0xFFFF) => {
                if (wp >> 16 & 0xFFFF) as u32 == BN_CLICKED {
                    fields::reset(wp & 0xFFFF);
                }
                0
            }
            WM_COMMAND if (wp >> 16 & 0xFFFF) as u32 == BN_CLICKED => {
                command(wp & 0xFFFF);
                0
            }
            // Left, the part with the focus is kept, to have it again on
            // coming back.
            WM_ACTIVATE => {
                // Back in front: the profile in effect may be another's now.
                if wp & 0xFFFF != WA_INACTIVE as usize {
                    with_editor(|e| {
                        if e.page == apps::PAGE {
                            apps::ask_active(e);
                        }
                    });
                }
                if wp & 0xFFFF == WA_INACTIVE as usize {
                    // Another window in front: the mouse is over none of
                    // the rows.
                    if fields::unhover() {
                        with_editor(|e| fields::show(e));
                    }
                    let focus = GetFocus();
                    if IsChild(hwnd, focus) != 0 {
                        with_editor(|e| e.focus = focus);
                    }
                } else if let Some(focus) =
                    with_editor(|e| e.focus).filter(|&f| IsWindowVisible(f) != 0 && IsWindowEnabled(f) != 0)
                {
                    SetFocus(focus);
                    return 0;
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            // Alt alone opens no menu: the window has none, and the system
            // menu's mode would hold the thread in its own loop, the panels'
            // settings file unwatched and the next click on a part swallowed.
            // Alt+Space still opens the system menu.
            WM_SYSCOMMAND if wp & 0xFFF0 == SC_KEYMENU as usize && lp == 0 => 0,
            // With changes not saved, ask first (a question about a change
            // outside gives way: the answer to this one settles it too).
            // Nothing of the window's to lose, it just closes.
            WM_CLOSE => {
                enter_focused();
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
            _ if msg == may_exit_message() => may_exit() as LRESULT,
            WM_DESTROY => {
                // Its process has nothing else to do.
                PostQuitMessage(0);
                DPI.set(0);
                if let Some(p) = PAINT.take() {
                    p.delete();
                }
                TAPPED.set((0, null_mut()));
                fields::drop_name_font();
                if let Some(e) = EDITOR.take() {
                    for object in [e.font, e.heading_font] {
                        DeleteObject(object);
                    }
                    if e.context != INVALID_HANDLE_VALUE {
                        ReleaseActCtx(e.context);
                    }
                    debug_log!("設定画面を閉じる");
                }
                0
            }
            #[cfg(feature = "fault-injection")]
            _ if msg == test_set() => {
                with_editor(|e| {
                    e.doc.set_value("general", "opacity", Some(&wp.to_string()));
                    fields::refresh(e);
                    mark_unsaved(e);
                });
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// A message for the settings window, before it is dispatched: a key, or a
/// finger while the mouse is held down. Whether it took the message.
pub unsafe fn take(msg: &MSG) -> bool {
    let keyed = (WM_KEYFIRST..=WM_KEYLAST).contains(&msg.message);
    let pointed = matches!(msg.message, WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP);
    if !keyed && !pointed && msg.message != WM_MOUSEWHEEL {
        return false;
    }
    let Some(hwnd) = EDITOR.with(|e| e.try_borrow().ok().and_then(|e| e.as_ref().map(|e| e.hwnd))) else {
        return false;
    };
    unsafe {
        if msg.hwnd != hwnd && IsChild(hwnd, msg.hwnd) == 0 {
            return false;
        }
        if msg.message == WM_MOUSEWHEEL {
            return wheel(hwnd, msg);
        }
        if keyed { key(hwnd, msg) } else { finger(hwnd, msg) }
    }
}

/// The wheel turned: it steps the number in the field under the cursor,
/// focused or not. Whether it did.
unsafe fn wheel(hwnd: HWND, msg: &MSG) -> bool {
    unsafe {
        let at = POINT { x: (msg.lParam & 0xFFFF) as i16 as i32, y: (msg.lParam >> 16 & 0xFFFF) as i16 as i32 };
        let under = WindowFromPoint(at);
        if IsChild(hwnd, under) == 0 {
            return false;
        }
        // Over the window, the wheel steps only a number under the cursor:
        // nothing else, the focused part's choice not either.
        let notches = (msg.wParam >> 16 & 0xFFFF) as i16 as i32 / WHEEL_DELTA as i32;
        if notches != 0 && !fields::step(under, notches, shift()) {
            keymap::step(under, notches, shift());
        }
        true
    }
}

/// A finger on a part of the settings window while the mouse is held down
/// (on a panel, for a key it presses, or anywhere else): taken as a tap
/// here, not passed on.
/// Passed on, Windows would make mouse input of it, and the mouse's own
/// press would end there, the panel's key left held with no lift to come.
unsafe fn finger(hwnd: HWND, msg: &MSG) -> bool {
    let id = pointer_id(msg.wParam);
    unsafe {
        match msg.message {
            WM_POINTERDOWN => {
                let mut kind = 0;
                let mouse = GetPointerType(id, &mut kind) == 0 || kind == PT_MOUSE;
                if mouse || GetAsyncKeyState(VK_LBUTTON as i32) >= 0 {
                    return false;
                }
                TAPPED.set((id, msg.hwnd));
                SetFocus(msg.hwnd);
                true
            }
            WM_POINTERUPDATE => TAPPED.get().0 == id && id != 0,
            _ => {
                let (tapped, part) = TAPPED.get();
                if tapped != id || id == 0 {
                    return false;
                }
                TAPPED.set((0, null_mut()));
                let (x, y) = pointer_at(msg.lParam);
                if WindowFromPoint(POINT { x, y }) == part {
                    tap(hwnd, part, x, y);
                }
                true
            }
        }
    }
}

/// A finger tapped `part` at screen `x`, `y`: choose the page there, or
/// press the button.
unsafe fn tap(hwnd: HWND, part: HWND, x: i32, y: i32) {
    unsafe {
        let id = GetDlgCtrlID(part) as usize;
        if id == SIDEBAR {
            let mut p = POINT { x, y };
            ScreenToClient(part, &mut p);
            let at = SendMessageW(part, LB_ITEMFROMPOINT, 0, (p.x & 0xFFFF | p.y << 16) as LPARAM);
            // The high word says the point is past the last item.
            let item = (at & 0xFFFF) as usize;
            if at >> 16 & 0xFFFF == 0 && item < PAGES.len() {
                show_page(item);
            }
        } else if is_button(part) {
            PostMessageW(hwnd, WM_COMMAND, id, part as LPARAM);
        }
    }
}

/// A key for the settings window: Tab and Shift+Tab move the focus along
/// the parts, Enter presses the button with the focus, Esc closes and
/// Ctrl+S saves; on キー配置, Alt+↑ and Alt+↓ move the chosen key. While
/// keys are captured, they all go to the capture. Whether it took the key;
/// a key it did not take has its characters made, for the parts that take
/// text.
unsafe fn key(hwnd: HWND, msg: &MSG) -> bool {
    unsafe {
        if keymap::capturing() {
            return keymap::capture_key(msg);
        }
        if msg.message == WM_SYSKEYDOWN && matches!(msg.wParam as u16, VK_UP | VK_DOWN) {
            // An open drop-down's own, as is Alt+↓ opening one.
            let combo = SendMessageW(GetFocus(), CB_GETDROPPEDSTATE, 0, 0) != 0 || is_combo(GetFocus());
            if !combo && with_editor(|e| e.page == keymap::PAGE) == Some(true) {
                keymap::move_key(if msg.wParam as u16 == VK_UP { -1 } else { 1 });
                return true;
            }
        }
        if msg.message == WM_KEYDOWN {
            let focus = GetFocus();
            let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
            let typed =
                with_editor(|e| fields::is_typed(e, focus) || keymap::is_typed(e, focus) || apps::is_typed(e, focus)) == Some(true);
            // The process to add: Enter adds it, Esc stops (its list open,
            // Esc is the list's).
            let vk = msg.wParam as u16;
            let closed = SendMessageW(GetParent(focus), CB_GETDROPPEDSTATE, 0, 0) == 0;
            if (vk == VK_RETURN || vk == VK_ESCAPE && closed) && apps::candidate_key(focus, vk == VK_RETURN) {
                return true;
            }
            let took = match msg.wParam as u16 {
                VK_TAB if !ctrl => {
                    let from = if IsChild(hwnd, focus) != 0 { focus } else { null_mut() };
                    let next = GetNextDlgTabItem(hwnd, from, (shift()).into());
                    if !next.is_null() {
                        SetFocus(next);
                    }
                    true
                }
                // Naming a key set, Esc leaves the name as it was.
                VK_ESCAPE if keymap::naming(focus) => keymap::cancel_naming(),
                // An open drop-down closes on Esc, as its own.
                // A drop-down's edit has the focus where one is typed in.
                VK_ESCAPE if SendMessageW(focus, CB_GETDROPPEDSTATE, 0, 0) == 0 && closed => {
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                    true
                }
                k if k == b'S' as u16 && ctrl => {
                    enter_focused();
                    PostMessageW(hwnd, WM_COMMAND, SAVE, 0);
                    true
                }
                k if k == b'A' as u16 && ctrl && typed => {
                    SendMessageW(focus, fields::EM_SETSEL, 0, -1);
                    true
                }
                VK_RETURN if typed => {
                    let id = GetDlgCtrlID(focus) as usize;
                    if fields::is_field(id) {
                        fields::commit(id, false);
                    } else if keymap::is_typed_id(id) {
                        keymap::commit(id);
                    } else {
                        apps::commit();
                    }
                    true
                }
                VK_UP | VK_DOWN if typed => {
                    let delta = if msg.wParam as u16 == VK_UP { 1 } else { -1 };
                    if !fields::step(focus, delta, shift()) {
                        keymap::step(focus, delta, shift());
                    }
                    true
                }
                VK_RETURN if GetParent(focus) == hwnd && is_button(focus) => {
                    PostMessageW(hwnd, WM_COMMAND, GetDlgCtrlID(focus) as usize, focus as LPARAM);
                    true
                }
                _ => false,
            };
            if took {
                return true;
            }
        }
        TranslateMessage(msg);
    }
    false
}

/// Whether Shift is held, as the message being handled saw it.
fn shift() -> bool {
    unsafe { GetKeyState(VK_SHIFT as i32) < 0 }
}

/// Whether `hwnd` is a drop-down list.
unsafe fn is_combo(hwnd: HWND) -> bool {
    let mut name = [0u16; 16];
    let n = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), 16) };
    String::from_utf16_lossy(&name[..n.max(0) as usize]) == "ComboBox"
}

/// Whether `hwnd` is a button.
unsafe fn is_button(hwnd: HWND) -> bool {
    let mut name = [0u16; 8];
    let n = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), 8) };
    String::from_utf16_lossy(&name[..n.max(0) as usize]) == "Button"
}

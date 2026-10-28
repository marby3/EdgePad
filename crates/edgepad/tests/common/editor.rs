//! Helpers for the settings window's tests: open it from the tray, find
//! its parts, change its copy of the file, read what it says.
//!
//! Its parts are Windows's own controls ("Button", "ListBox", "Static"),
//! found by class and text, or by the control ids below: those, and the
//! window's class, are the contract with crates/edgepad/src/editor.rs.

use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::w;

use super::*;

pub const EDITOR_CLASS: &str = "EdgePadSettings";
/// The sidebar (a ListBox), the page's heading, the status line, and the
/// settings file's path (Statics).
pub const SIDEBAR: i32 = 10;
pub const HEADING: i32 = 11;
pub const STATUS: i32 = 12;
pub const PATH: i32 = 13;
pub const PAGES: [&str; 7] = ["キー配置", "アプリ別", "辺（共通）", "辺ごとの設定", "見た目", "タッチと修飾キー", "表示と非表示"];

/// Sets `[general] opacity` to wParam in the window's copy of the file
/// (test build only).
pub fn test_set() -> u32 {
    unsafe { RegisterWindowMessageW(w!("EdgePadTestEditorSet")) }
}

/// Wait up to 3 s for `done`.
pub fn within_3s(mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while !done() {
        if Instant::now() > end {
            return false;
        }
        pump(20);
    }
    true
}

/// The settings windows of `app` now: in a process of its own (Issue
/// #90), started from the same exe as `app`.
pub fn editors(app: &Running) -> Vec<HWND> {
    let name = wide(EDITOR_CLASS);
    let exe = exe_dir(app).join("edgepad.exe");
    let mut found = Vec::new();
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), null()) };
        if hwnd.is_null() {
            return found;
        }
        if process_exe(pid_of(hwnd)).is_some_and(|p| same_file(&p, &exe)) {
            found.push(hwnd);
        }
    }
}

/// The process owning `hwnd`.
pub fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

/// The exe process `pid` runs, if it can be read.
pub fn process_exe(pid: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut n);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..n as usize]).into())
    }
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

pub fn open_id(app: &Running) -> u32 {
    menu_item(app, "設定…").id
}

/// Choose 設定… (command `id`) in the tray without opening its menu, and
/// return the window once it is shown.
pub fn open(app: &Running, id: u32) -> HWND {
    let tray = find_window(app, TRAY_CLASS, false);
    unsafe { PostMessageW(tray, WM_COMMAND, id as usize, 0) };
    let mut shown = null_mut();
    assert!(
        within_3s(|| {
            shown = editors(app).into_iter().find(|&w| unsafe { IsWindowVisible(w) } != 0).unwrap_or(null_mut());
            !shown.is_null()
        }),
        "the settings window opens"
    );
    shown
}

/// The shown child of `parent` of `class` whose text is `text`, if any.
pub fn child(parent: HWND, class: &str, text: &str) -> Option<HWND> {
    let (class, text) = (wide(class), wide(text));
    let hwnd = unsafe { FindWindowExW(parent, null_mut(), class.as_ptr(), text.as_ptr()) };
    (!hwnd.is_null() && unsafe { IsWindowVisible(hwnd) } != 0).then_some(hwnd)
}

/// The part of `editor` with control id `id`.
pub fn part(editor: HWND, id: i32) -> HWND {
    let hwnd = unsafe { GetDlgItem(editor, id) };
    assert!(!hwnd.is_null(), "a part with id {id}");
    hwnd
}

pub fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

/// Click the button of `editor` labelled `text`, in front first: the
/// window opens behind whatever is in front when the tray did not have the
/// focus to give it, as the posted command does not on CI.
pub fn click_button(editor: HWND, text: &str) {
    let c = child(editor, "Button", text).unwrap_or_else(|| panic!("no button 「{text}」"));
    wait_in_front(editor);
    click(centre(window_rect(c)));
    pump(300);
}

/// The text of `hwnd`, a window of EdgePad's: asked with WM_GETTEXT, as
/// GetWindowText does not ask a control of another process.
pub fn text_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { SendMessageW(hwnd, WM_GETTEXT, 512, buf.as_mut_ptr() as isize) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// What the window's status line says.
pub fn status(editor: HWND) -> String {
    text_of(part(editor, STATUS))
}

/// The heading of the page shown.
pub fn heading(editor: HWND) -> String {
    text_of(part(editor, HEADING))
}

pub fn file(app: &Running) -> String {
    std::fs::read_to_string(exe_dir(app).join("edgepad.ini")).expect("edgepad.ini")
}

/// Set `[general] opacity` in the window's copy of the file.
pub fn change(editor: HWND, opacity: usize) {
    unsafe { PostMessageW(editor, test_set(), opacity, 0) };
    pump(200);
}

/// The sidebar item `i`'s rectangle on the screen.
pub fn sidebar_item(editor: HWND, i: usize) -> RECT {
    let list = part(editor, SIDEBAR);
    let mut r: RECT = unsafe { std::mem::zeroed() };
    let found = unsafe { SendMessageW(list, LB_GETITEMRECT, i, &mut r as *mut RECT as isize) };
    assert!(found != LB_ERR as isize, "sidebar item {i}");
    let mut p = [POINT { x: r.left, y: r.top }, POINT { x: r.right, y: r.bottom }];
    unsafe { windows_sys::Win32::Graphics::Gdi::MapWindowPoints(list, null_mut(), p.as_mut_ptr(), 2) };
    RECT { left: p[0].x, top: p[0].y, right: p[1].x, bottom: p[1].y }
}

/// The window with the keyboard focus on `editor`'s thread.
pub fn focus_of(editor: HWND) -> HWND {
    let thread = unsafe { GetWindowThreadProcessId(editor, null_mut()) };
    let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    unsafe { GetGUIThreadInfo(thread, &mut info) };
    info.hwndFocus
}

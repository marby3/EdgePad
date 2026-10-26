//! Acceptance tests for Issue #90: the settings window in a process of its
//! own, started from the same exe (spec 7.1), so that closing it gives all
//! its memory back; the running EdgePad does without the IME. Its memory is
//! checked on the release build, by ci/test_measure.py (AC7).
//!
//! The settings window's process is told from the running EdgePad by its
//! process id: the tests find the window by its class and its exe
//! (common::editor). A debug build is slow to draw and to act on a click:
//! what a test checks, it waits for.

mod common;

use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, POINT, RECT, WAIT_OBJECT_0};
use windows_sys::Win32::System::ProcessStatus::{EnumProcessModulesEx, GetModuleFileNameExW, LIST_MODULES_ALL};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_VM_READ, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::editor::*;
use common::*;

/// Two hold keys and Ctrl on the left, a key on the right.
fn settings(general: &str) -> String {
    format!(
        "[general]\nopacity = 100\n{general}\n[set.pad]\nkey = A | hold | A\nkey = Ctrl | modifier | Ctrl\nkey = B | hold | B\n\
         [set.right]\nkey = C | hold | C\n[profile.default]\nleft = pad\nright = right\n"
    )
}

/// The windows of class `class` that process `pid` owns.
fn windows_of(pid: u32, class: &str) -> Vec<windows_sys::Win32::Foundation::HWND> {
    let name = wide(class);
    let mut found = Vec::new();
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), std::ptr::null()) };
        if hwnd.is_null() {
            return found;
        }
        if pid_of(hwnd) == pid {
            found.push(hwnd);
        }
    }
}

/// Whether process `pid` has ended within 3 s.
fn ends(pid: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return true;
        }
        let ended = WaitForSingleObject(process, 3000) == WAIT_OBJECT_0;
        CloseHandle(process);
        ended
    }
}

// AC1
#[test]
fn the_tray_opens_the_window_in_a_second_process_of_the_same_exe_and_only_once() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    let panels = panels_now(&app).len();
    let editor = open(&app, id);
    let pid = pid_of(editor);
    assert_ne!(pid, app.0.id(), "not in the running EdgePad's process");
    let exe = process_exe(pid).expect("its exe");
    assert!(exe.to_string_lossy().eq_ignore_ascii_case(&exe_dir(&app).join("edgepad.exe").to_string_lossy()), "the same exe: {exe:?}");
    assert!(windows_of(app.0.id(), EDITOR_CLASS).is_empty(), "no settings window in the running EdgePad");
    for class in [PANEL_CLASS, TRAY_CLASS] {
        assert!(windows_of(pid, class).is_empty(), "the settings window's process has no {class}");
    }
    pump(500);
    assert_eq!(panels_now(&app).len(), panels, "it toggled none of the running EdgePad's panels, as a second launch does");

    unsafe { ShowWindow(editor, SW_MINIMIZE) };
    pump(300);
    let again = open(&app, id);
    assert_eq!(again, editor, "the same window, brought back");
    assert_eq!(editors(&app).len(), 1, "one window only");
    assert!(within_3s(|| unsafe { IsIconic(editor) } == 0), "brought back from the taskbar");
}

// AC2
#[test]
fn closing_the_window_ends_its_process() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    for _ in 0..2 {
        let editor = open(&app, id);
        let pid = pid_of(editor);
        unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
        assert!(ends(pid), "its process ends with it");
        assert!(app_alive(&app), "the running EdgePad goes on");
    }
}

fn app_alive(app: &Running) -> bool {
    (unsafe { WaitForSingleObject(app.0.as_raw_handle(), 0) }) != WAIT_OBJECT_0
}

/// The file names of the modules process `pid` has loaded, lower case.
fn modules(pid: u32) -> Vec<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        assert!(!process.is_null(), "open process {pid}");
        let mut handles = vec![null_mut(); 1024];
        let mut needed = 0;
        let size = (handles.len() * size_of::<usize>()) as u32;
        let listed = EnumProcessModulesEx(process, handles.as_mut_ptr(), size, &mut needed, LIST_MODULES_ALL);
        assert!(listed != 0, "list the modules of process {pid}");
        handles.truncate(needed as usize / size_of::<usize>());
        let names = handles
            .iter()
            .map(|&m| {
                let mut buf = [0u16; 520];
                let n = GetModuleFileNameExW(process, m, buf.as_mut_ptr(), 520);
                let path = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase();
                path.rsplit('\\').next().unwrap_or_default().to_string()
            })
            .collect();
        CloseHandle(process);
        names
    }
}

/// What Windows loads into a process for its text input once a window of
/// it takes the focus with the IME in use.
const TEXT_INPUT: &str = "textinputframework.dll";

// AC3
#[test]
fn the_running_edgepad_does_without_the_ime_and_the_settings_window_has_it() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    // Its menu takes the focus, as a window of it would.
    let _ = tray_menu(&app);
    let editor = open(&app, id);
    wait_in_front(editor);
    click(centre(sidebar_item(editor, 1)));
    let pid = pid_of(editor);
    assert!(within_3s(|| modules(pid).iter().any(|m| m == TEXT_INPUT)), "the settings window has the IME: {:?}", modules(pid));
    assert!(!modules(app.0.id()).iter().any(|m| m == TEXT_INPUT), "the running EdgePad has not: {:?}", modules(app.0.id()));
}

fn press(vk: u16) {
    keyboard(vk, true);
    keyboard(vk, false);
}

/// Open the settings window, bring it to the front, and close it again.
fn open_and_close(app: &Running, id: u32) {
    let editor = open(app, id);
    wait_in_front(editor);
    press(VK_DOWN);
    let pid = pid_of(editor);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(ends(pid), "closed");
    pump(300);
}

// AC4
#[test]
fn opening_and_closing_the_window_leaves_the_running_edgepad_as_it_was() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let left = left_panel(&app);
    // Ctrl latched by a tap, A held by a finger, the profile pinned, the
    // right edge hidden.
    touch.down(1, key_centre(left, 1));
    touch.up(1);
    touch.down(2, key_centre(left, 0));
    choose(&app, "固定");
    choose(&app, "右辺");
    let before = log.keys();
    assert_eq!(before, [(VK_LCONTROL, false), (b'A' as u16, false)], "precondition: Ctrl latched, A held");
    let item = |text: &str| menu_item(&app, text);
    assert!(item("固定").checked && !item("右辺").checked, "precondition: pinned, right hidden");

    open_and_close(&app, id);
    assert_eq!(log.keys(), before, "nothing let go: Ctrl still latched, A still held");
    assert!(item("固定").checked, "still pinned");
    assert!(!item("右辺").checked, "the right edge still hidden");
    touch.up(2);
    touch.down(1, key_centre(left, 1));
    touch.up(1);
    let _ = within_3s(|| log.keys().len() >= 4);
    assert_eq!(log.keys()[2..], [(b'A' as u16, true), (VK_LCONTROL, true)], "A comes up with its finger, Ctrl with a second tap");

    choose(&app, "一時停止");
    assert!(item("一時停止").checked, "precondition: paused");
    open_and_close(&app, id);
    assert!(item("一時停止").checked, "still paused");
}

// AC5
#[test]
fn exit_asks_in_the_window_about_changes_and_otherwise_ends_both() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings("")), None);
    let id = open_id(&app);
    let exit = menu_item(&app, "終了").id;
    let tray = find_window(&app, TRAY_CLASS, false);

    let editor = open(&app, id);
    change(editor, 50);
    unsafe { PostMessageW(tray, WM_COMMAND, exit as usize, 0) };
    assert!(within_3s(|| child(editor, "Button", "保存せずに閉じる").is_some()), "the window asks about its changes");
    pump(1000);
    assert!(app_alive(&app), "EdgePad goes on");
    assert!(test_events(&app).iter().any(|e| e.contains("保存していない変更")), "and says why: {:?}", test_events(&app));
    click_button(editor, "保存せずに閉じる");
    assert!(ends(pid_of(editor)), "closed without saving");

    let editor = open(&app, id);
    let pid = pid_of(editor);
    unsafe { PostMessageW(tray, WM_COMMAND, exit as usize, 0) };
    assert!(ends(pid), "nothing to save: the window goes");
    assert!(within_3s(|| !app_alive(&app)), "and EdgePad ends");
}

/// A topmost backdrop down the left of the primary monitor, under the
/// panel: made before EdgePad starts, so the panel goes above it.
fn backdrop() -> TestWindow {
    let m = primary_monitor();
    let under = RECT { left: m.left, top: m.top, right: m.left + key_px() * 3, bottom: m.bottom };
    test_window("EdgePadTestProcessBackdrop", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, under)
}

fn brightness(p: POINT) -> i32 {
    let c = channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0]);
    (c[0] + c[1] + c[2]) / 3
}

// AC6: the window's own behaviour is checked by editor.rs and
// settings_window.rs, run against this process; here, that a save reaches
// the running EdgePad.
#[test]
fn a_save_in_the_window_reaches_the_running_edgepad() {
    let _desktop = desktop();
    let back = backdrop();
    set_backdrop(back.0, 0x0000_0000);
    let app = launch_with(Some(&settings("theme = light")), None);
    let editor = open(&app, open_id(&app));
    let corner = || {
        let r = left_panel(&app);
        POINT { x: r.left + 1, y: r.top + 1 }
    };
    let opaque = brightness(corner());
    assert!(opaque > 150, "precondition: a light panel, fully opaque, {opaque}");
    change(editor, 30);
    click_button(editor, "保存");
    let deadline = Instant::now() + Duration::from_secs(3);
    while brightness(corner()) > opaque / 2 && Instant::now() < deadline {
        pump(50);
    }
    let now = brightness(corner());
    assert!(now < opaque / 2, "the panel 30% opaque over black: {opaque} -> {now}");
}

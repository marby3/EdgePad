//! Acceptance tests for Issue #86: the settings window rebuilt on Windows's
//! own controls (spec 7.1): a sidebar of four pages, the bottom row with the
//! file's path, unsaved changes shown, the theme of Windows, the DPI, and
//! the keyboard. Its memory is checked on the release build, by
//! ci/test_measure.py (AC8).
//!
//! Following Windows's theme is checked by writing the user's own setting
//! in the registry and announcing the change as Windows does
//! (WM_SETTINGCHANGE, "ImmersiveColorSet"); it is put back afterwards, even
//! when the test fails. A debug build is slow to draw and to act on a
//! click: what a test checks, it waits for.

mod common;

use std::os::windows::io::AsRawHandle;
use std::mem::size_of;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::ProcessStatus::{EnumProcessModulesEx, GetModuleFileNameExW, LIST_MODULES_ALL};
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::editor::*;
use common::*;

/// The panels light: the window, following Windows, need not be.
fn settings() -> String {
    "; 手で書いた設定\n[general]\nopacity = 90   ; 通常表示\ntheme = light\n\
     [set.two]\nkey = A | hold | A\nkey = B | hold | B\n[profile.default]\nleft = two\n"
        .into()
}

/// Static styles, of Win32_System_SystemServices, a feature taken for
/// these alone otherwise.
const SS_ELLIPSISMASK: u32 = 0xC000;
const SS_PATHELLIPSIS: u32 = 0x8000;

/// `r` for a message: RECT has no Debug.
fn at(r: RECT) -> String {
    format!("({}, {})-({}, {})", r.left, r.top, r.right, r.bottom)
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 128) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Every child window of `parent`, shown or not.
fn children(parent: HWND) -> Vec<HWND> {
    let mut found = Vec::new();
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(parent, hwnd, std::ptr::null(), std::ptr::null()) };
        if hwnd.is_null() {
            return found;
        }
        found.push(hwnd);
    }
}

fn sidebar_texts(editor: HWND) -> Vec<String> {
    let list = part(editor, SIDEBAR);
    let n = unsafe { SendMessageW(list, LB_GETCOUNT, 0, 0) };
    (0..n.max(0) as usize)
        .map(|i| {
            let mut buf = [0u16; 64];
            let len = unsafe { SendMessageW(list, LB_GETTEXT, i, buf.as_mut_ptr() as isize) };
            String::from_utf16_lossy(&buf[..len.max(0) as usize])
        })
        .collect()
}

// AC1
#[test]
fn a_sidebar_of_four_pages_switches_the_page_and_opens_on_key_layout() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open(&app, open_id(&app));
    let list = part(editor, SIDEBAR);
    assert_eq!(class_of(list), "ListBox", "the sidebar is a list box");
    assert!(unsafe { IsWindowVisible(list) } != 0, "shown");
    assert_eq!(sidebar_texts(editor), PAGES, "four pages, in this order");
    assert_eq!(heading(editor), "キー配置", "opened on キー配置");
    assert_eq!(unsafe { SendMessageW(list, LB_GETCURSEL, 0, 0) }, 0, "キー配置 chosen in the sidebar");

    // The sidebar on the left, the page to its right.
    let (l, h) = (window_rect(list), window_rect(part(editor, HEADING)));
    assert!(l.right <= h.left, "the heading is right of the sidebar: {} {}", at(l), at(h));

    for i in [2, 1, 3, 0] {
        wait_in_front(editor);
        click(centre(sidebar_item(editor, i)));
        assert!(within_3s(|| heading(editor) == PAGES[i]), "{}: the page shown, not {:?}", PAGES[i], heading(editor));
        let want = format!("settings: page {}", PAGES[i]);
        assert!(within_3s(|| test_events(&app).last() == Some(&want)), "{:?}", test_events(&app));
    }
}

/// The files of the modules loaded in `app`.
fn modules(app: &Running) -> Vec<String> {
    let process = app.0.as_raw_handle();
    let mut handles = vec![null_mut(); 1024];
    let mut needed = 0;
    let size = (handles.len() * size_of::<usize>()) as u32;
    let ok = unsafe { EnumProcessModulesEx(process, handles.as_mut_ptr(), size, &mut needed, LIST_MODULES_ALL) };
    assert!(ok != 0, "EnumProcessModulesEx");
    handles.truncate(needed as usize / size_of::<usize>());
    handles
        .iter()
        .map(|&m| {
            let mut buf = [0u16; 520];
            let n = unsafe { GetModuleFileNameExW(process, m, buf.as_mut_ptr(), 520) };
            String::from_utf16_lossy(&buf[..n as usize]).to_lowercase()
        })
        .collect()
}

fn comctl32_v6(app: &Running) -> bool {
    modules(app).iter().any(|m| m.contains("microsoft.windows.common-controls") && m.ends_with("comctl32.dll"))
}

/// The files of the modules loaded in process `pid`.
fn modules_of(pid: u32) -> Vec<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
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
                String::from_utf16_lossy(&buf[..n as usize]).to_lowercase()
            })
            .collect();
        CloseHandle(process);
        names
    }
}

// AC2
#[test]
fn its_parts_are_windows_own_controls_of_comctl32_v6_asking_as_before() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let id = open_id(&app);
    assert!(!comctl32_v6(&app), "precondition: not loaded before the window opens");
    let editor = open(&app, id);
    // In the settings window's own process since Issue #90.
    let theirs = modules_of(pid_of(editor));
    assert!(
        theirs.iter().any(|m| m.contains("microsoft.windows.common-controls") && m.ends_with("comctl32.dll")),
        "loaded from WinSxS once it opens: {theirs:?}"
    );
    for c in children(editor) {
        let class = class_of(c);
        // Since Issue #83 the fields too, and the figure of the edges, drawn
        // as the spec has the window's own drawings; since Issue #87 the
        // sample of the panel and the pie's figure.
        let own = ["Button", "ListBox", "Static", "SysLink", "Edit", "ComboBox"].contains(&class.as_str());
        let drawn = ["EdgePadSettingsEdges", "EdgePadSettingsSample", "EdgePadSettingsPie"].contains(&class.as_str());
        assert!(own || drawn, "a part of Windows's own, not {class:?}");
    }

    // The questions, with Windows's buttons, as before.
    change(editor, 50);
    unsafe { PostMessageW(editor, WM_CLOSE, 0, 0) };
    assert!(within_3s(|| child(editor, "Button", "キャンセル").is_some()), "asked about closing");
    assert!(child(editor, "Button", "保存する").is_some() && child(editor, "Button", "保存せずに閉じる").is_some());
    assert!(status(editor).contains("保存していない変更"), "{:?}", status(editor));
    click_button(editor, "キャンセル");
    assert!(within_3s(|| child(editor, "Button", "キャンセル").is_none()), "answered");

    std::fs::write(exe_dir(&app).join("edgepad.ini"), settings().replace("opacity = 90", "opacity = 80")).unwrap();
    assert!(within_3s(|| child(editor, "Button", "読み直す").is_some()), "told of the change outside");
    assert!(child(editor, "Button", "画面の内容を保つ").is_some());
    assert!(status(editor).contains("外で書き換わりました"), "{:?}", status(editor));
}

// AC3
#[test]
fn the_bottom_row_shows_the_path_and_opens_the_file_as_text() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open(&app, open_id(&app));
    let path = exe_dir(&app).join("edgepad.ini");

    let shown = part(editor, PATH);
    assert_eq!(class_of(shown), "Static");
    assert_eq!(text_of(shown), path.display().to_string(), "the full path");
    let style = unsafe { GetWindowLongW(shown, GWL_STYLE) } as u32;
    assert_eq!(style & SS_ELLIPSISMASK, SS_PATHELLIPSIS, "a long path loses its middle");

    let open_text = child(editor, "Button", "テキストで開く").expect("テキストで開く");
    let save = child(editor, "Button", "保存").expect("保存");
    let (p, o, s) = (window_rect(shown), window_rect(open_text), window_rect(save));
    let row = |r: RECT| (r.top + r.bottom) / 2;
    assert!((row(p) - row(s)).abs() <= 4 && (row(o) - row(s)).abs() <= 4, "one row: {} {} {}", at(p), at(o), at(s));
    assert!(p.right <= o.left && o.right <= s.left, "path, テキストで開く, 保存 from left to right");
    let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(editor, &mut client) };
    let mut bottom = POINT { x: 0, y: client.bottom };
    unsafe { ClientToScreen(editor, &mut bottom) };
    assert!(bottom.y - s.bottom < s.bottom - s.top, "along the bottom of the window");

    click_button(editor, "テキストで開く");
    let want = format!("open: {}", path.display());
    assert!(within_3s(|| test_events(&app).contains(&want)), "opened as the tray's 設定ファイルを開く does: {:?}", test_events(&app));
}

fn title(editor: HWND) -> String {
    text_of(editor)
}

fn emphasised(editor: HWND) -> bool {
    let save = child(editor, "Button", "保存").expect("保存");
    let style = unsafe { GetWindowLongW(save, GWL_STYLE) } as u32;
    style & BS_TYPEMASK as u32 == BS_DEFPUSHBUTTON as u32
}

// AC4
#[test]
fn unsaved_changes_show_in_the_title_and_on_save_and_ctrl_s_saves() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open(&app, open_id(&app));
    assert!(!title(editor).starts_with('●') && !emphasised(editor), "nothing changed yet: {:?}", title(editor));

    change(editor, 50);
    assert!(within_3s(|| title(editor).starts_with('●')), "changed: {:?}", title(editor));
    assert!(emphasised(editor), "保存 stands out");

    // Back to the value in the file: nothing left to save.
    change(editor, 90);
    assert!(within_3s(|| !title(editor).starts_with('●')), "changed back: {:?}", title(editor));
    assert!(!emphasised(editor));

    change(editor, 50);
    click_button(editor, "保存");
    assert!(within_3s(|| file(&app).contains("opacity = 50")), "{}", file(&app));
    assert!(within_3s(|| !title(editor).starts_with('●')), "saved: {:?}", title(editor));
    assert!(!emphasised(editor));

    // Ctrl+S, as 保存.
    change(editor, 40);
    assert!(within_3s(|| title(editor).starts_with('●')));
    wait_in_front(editor);
    keyboard(VK_CONTROL, true);
    keyboard(b'S' as u16, true);
    keyboard(b'S' as u16, false);
    keyboard(VK_CONTROL, false);
    assert!(within_3s(|| file(&app).contains("opacity = 40")), "Ctrl+S saved: {}", file(&app));
    assert!(within_3s(|| !title(editor).starts_with('●')), "{:?}", title(editor));
}

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Windows's app mode, set for the test and put back on drop.
struct AppMode(Option<u32>);

fn read_mode() -> Option<u32> {
    let (key, name) = (wide(PERSONALIZE), wide("AppsUseLightTheme"));
    let (mut value, mut size) = (0u32, 4u32);
    let found = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, null_mut(), &mut value as *mut u32 as _, &mut size)
    };
    (found == 0).then_some(value)
}

fn write_mode(light: u32) {
    let (key, name) = (wide(PERSONALIZE), wide("AppsUseLightTheme"));
    let done = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, &light as *const u32 as _, 4) };
    assert_eq!(done, 0, "write the registry value");
    announce();
}

/// Tell every top-level window the colour settings changed.
fn announce() {
    let what = wide("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, what.as_ptr() as isize, SMTO_ABORTIFHUNG, 2000, null_mut());
    }
    pump(300);
}

impl AppMode {
    fn set(light: u32) -> AppMode {
        let was = read_mode();
        write_mode(light);
        AppMode(was)
    }
}

impl Drop for AppMode {
    fn drop(&mut self) {
        let (key, name) = (wide(PERSONALIZE), wide("AppsUseLightTheme"));
        unsafe {
            match self.0 {
                Some(v) => {
                    RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, &v as *const u32 as _, 4);
                }
                None => {
                    RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
                }
            }
        }
        announce();
    }
}

fn brightness(p: POINT) -> i32 {
    unsafe { DwmFlush() };
    let c = channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0]);
    (c[0] + c[1] + c[2]) / 3
}

/// Points of `editor` to read the theme off: the background of the page,
/// the sidebar below its items, the face of 保存 clear of its label, and
/// the middle of the title bar.
fn spots(editor: HWND) -> [(&'static str, POINT); 4] {
    let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(editor, &mut client) };
    let mut page = POINT { x: client.right * 3 / 4, y: client.bottom / 2 };
    unsafe { ClientToScreen(editor, &mut page) };
    let list = window_rect(part(editor, SIDEBAR));
    let sidebar = POINT { x: (list.left + list.right) / 2, y: list.bottom - 6 };
    let save = window_rect(child(editor, "Button", "保存").expect("保存"));
    let dpi = unsafe { GetDpiForWindow(editor) };
    let button = POINT { x: save.left + 6 * dpi as i32 / 96, y: (save.top + save.bottom) / 2 };
    let w = window_rect(editor);
    let caption = unsafe { GetSystemMetricsForDpi(SM_CYCAPTION, dpi) };
    let bar = POINT { x: (w.left + w.right) / 2, y: w.top + unsafe { GetSystemMetricsForDpi(SM_CYFRAME, dpi) } + caption / 2 };
    [("the page", page), ("the sidebar", sidebar), ("保存", button), ("the title bar", bar)]
}

// AC5
#[test]
fn it_follows_windows_light_or_dark_not_the_panels_and_switches_while_open() {
    let _desktop = desktop();
    let mode = AppMode::set(0);
    let app = launch_with(Some(&settings()), None);
    let editor = open(&app, open_id(&app));
    wait_in_front(editor);
    for (what, p) in spots(editor) {
        let _ = within_3s(|| brightness(p) < 90);
        assert!(brightness(p) < 90, "Windows dark, the panels light: {what} is dark, not {}", brightness(p));
    }

    write_mode(1);
    for (what, p) in spots(editor) {
        let _ = within_3s(|| brightness(p) > 170);
        assert!(brightness(p) > 170, "Windows turned light while open: {what} is light, not {}", brightness(p));
    }
    drop(mode);
}

// AC6
#[test]
fn it_follows_the_dpi_lays_out_again_when_resized_and_keeps_its_least_size() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open(&app, open_id(&app));
    let list = part(editor, SIDEBAR);
    let save = child(editor, "Button", "保存").expect("保存");
    let height = |w: HWND| window_rect(w).bottom - window_rect(w).top;
    let item = || unsafe { SendMessageW(list, LB_GETITEMHEIGHT, 0, 0) } as i32;
    let (item0, save0) = (item(), height(save));

    let dpi = unsafe { GetDpiForWindow(editor) } as i32;
    let new = dpi * 3 / 2;
    let suggested = window_rect(editor);
    unsafe { SendMessageW(editor, WM_DPICHANGED, (new << 16 | new) as usize, &suggested as *const _ as isize) };
    pump(300);
    assert!((item() - item0 * 3 / 2).abs() <= 2, "the sidebar's items grow with the DPI: {item0} -> {}", item());
    assert!((height(save) - save0 * 3 / 2).abs() <= 2, "and the buttons: {save0} -> {}", height(save));
    unsafe { SendMessageW(editor, WM_DPICHANGED, (dpi << 16 | dpi) as usize, &suggested as *const _ as isize) };
    pump(300);

    // Larger: 保存 keeps to the bottom right corner, the sidebar to the
    // left, down to the bottom row.
    let corner = |w: HWND| {
        let (r, s) = (window_rect(editor), window_rect(w));
        (r.right - s.right, r.bottom - s.bottom)
    };
    let before = corner(save);
    let list0 = height(list);
    let r = window_rect(editor);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    unsafe { SetWindowPos(editor, null_mut(), 0, 0, w + 200, h + 120, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE) };
    pump(300);
    let after = corner(save);
    assert!((after.0 - before.0).abs() <= 1 && (after.1 - before.1).abs() <= 1, "保存 stays in the corner: {before:?} -> {after:?}");
    // As much taller as the screen let it be (CI's is 1024 x 768).
    let grown = window_rect(editor).bottom - window_rect(editor).top - h;
    assert!(grown > 0 && (height(list) - list0 - grown).abs() <= 1, "the sidebar runs on down: {list0} -> {}, +{grown}", height(list));
    assert!(window_rect(list).bottom < window_rect(save).top, "above the bottom row");

    // Far too small: it stops at its least size, its parts all inside it.
    unsafe { SetWindowPos(editor, null_mut(), 0, 0, 120, 90, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE) };
    pump(300);
    let r = window_rect(editor);
    assert!(r.right - r.left > 120 * dpi / 96 * 3 && r.bottom - r.top > 90 * dpi / 96 * 3, "no smaller than its least: {}", at(r));
    let s = window_rect(save);
    assert!(s.right <= r.right && s.bottom <= r.bottom && window_rect(list).bottom < s.top, "its parts fit: {} {}", at(r), at(s));
}

fn shows_focus(hwnd: HWND) -> bool {
    let state = unsafe { SendMessageW(hwnd, WM_QUERYUISTATE, 0, 0) } as u32;
    state & UISF_HIDEFOCUS == 0
}

fn press(vk: u16) {
    keyboard(vk, true);
    keyboard(vk, false);
}

// AC7
#[test]
fn the_keyboard_alone_moves_along_its_parts_and_the_focus_shows() {
    let _desktop = desktop();
    let app = launch_with(Some(&settings()), None);
    let editor = open(&app, open_id(&app));
    wait_in_front(editor);
    let list = part(editor, SIDEBAR);
    assert!(within_3s(|| focus_of(editor) == list), "the sidebar has the focus first");
    assert!(shows_focus(list), "and shows it");

    press(VK_DOWN);
    assert!(within_3s(|| heading(editor) == "アプリ別"), "down: アプリ別, not {:?}", heading(editor));
    press(VK_DOWN);
    press(VK_DOWN);
    assert!(within_3s(|| heading(editor) == "操作"), "down twice more: 操作");
    press(VK_UP);
    assert!(within_3s(|| heading(editor) == "辺と表示"), "up: 辺と表示");
    // Back to キー配置. From the sidebar, Shift+Tab goes back round to the
    // bottom row, past the page's fields.
    press(VK_UP);
    press(VK_UP);
    assert!(within_3s(|| heading(editor) == "キー配置"), "up twice: キー配置");

    let open_text = child(editor, "Button", "テキストで開く").expect("テキストで開く");
    let save = child(editor, "Button", "保存").expect("保存");
    keyboard(VK_SHIFT, true);
    press(VK_TAB);
    press(VK_TAB);
    keyboard(VK_SHIFT, false);
    assert!(within_3s(|| focus_of(editor) == open_text), "Shift+Tab twice: テキストで開く");
    assert!(shows_focus(open_text), "its focus shows");
    press(VK_TAB);
    assert!(within_3s(|| focus_of(editor) == save), "Tab: 保存");
    press(VK_TAB);
    assert!(within_3s(|| focus_of(editor) == list), "Tab: round to the sidebar");
    keyboard(VK_SHIFT, true);
    press(VK_TAB);
    keyboard(VK_SHIFT, false);
    assert!(within_3s(|| focus_of(editor) == save), "Shift+Tab: back to 保存");
    keyboard(VK_SHIFT, true);
    press(VK_TAB);
    keyboard(VK_SHIFT, false);
    assert!(within_3s(|| focus_of(editor) == open_text), "Shift+Tab: テキストで開く");

    press(VK_RETURN);
    let want = format!("open: {}", exe_dir(&app).join("edgepad.ini").display());
    assert!(within_3s(|| test_events(&app).contains(&want)), "Enter presses it: {:?}", test_events(&app));
    press(VK_SPACE);
    assert!(within_3s(|| test_events(&app).iter().filter(|e| **e == want).count() == 2), "and Space");

    press(VK_ESCAPE);
    assert!(within_3s(|| editors(&app).is_empty()), "Esc closes it, nothing changed");
}

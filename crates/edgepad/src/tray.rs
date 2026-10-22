//! The tray icon and its menu, owned by a hidden top-level window. A hidden
//! window rather than a message-only one, because only top-level windows hear
//! the TaskbarCreated broadcast sent when Explorer restarts.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::send;

// The class name, TRAY_ID, TRAY_CALLBACK and the test panic message are the
// contract tests/tray.rs drives the tray through: change them together.
const TRAY_ID: u32 = 1;
const TRAY_CALLBACK: u32 = WM_APP + 1;
const CMD_EXIT: usize = 1001;

static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "fault-injection")]
static TEST_PANIC: AtomicU32 = AtomicU32::new(0);

/// Create the tray window and show the icon. Returns null on failure. A
/// missing taskbar is not a failure: the icon appears once it is created.
pub unsafe fn create() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = instance;
        wc.lpszClassName = w!("EdgePadTray");
        RegisterClassW(&wc);

        TASKBAR_CREATED_MSG.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        #[cfg(feature = "fault-injection")]
        TEST_PANIC.store(RegisterWindowMessageW(w!("EdgePadTestPanic")), Ordering::Relaxed);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            wc.lpszClassName,
            w!("EdgePad"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if !hwnd.is_null() {
            add_icon(hwnd);
        }
        hwnd
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    data
}

unsafe fn add_icon(hwnd: HWND) {
    unsafe {
        let mut data = icon_data(hwnd);
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY_CALLBACK;
        // Placeholder artwork until the real icon ships (v1.0).
        data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        for (dst, src) in data.szTip.iter_mut().zip("EdgePad".encode_utf16()) {
            *dst = src;
        }
        Shell_NotifyIconW(NIM_ADD, &data);
    }
}

unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, CMD_EXIT, w!("終了(&X)"));
        let mut at = POINT { x: 0, y: 0 };
        GetCursorPos(&mut at);
        // Without this the menu would not close when clicking elsewhere.
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, at.x, at.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}

/// Leave: let go of every held key first, then take the icon down and quit.
unsafe fn exit(hwnd: HWND) {
    unsafe {
        send::release_all();
        Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
        PostQuitMessage(0);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            TRAY_CALLBACK => {
                if matches!(lp as u32, WM_RBUTTONUP | WM_CONTEXTMENU) {
                    show_menu(hwnd);
                }
                0
            }
            WM_COMMAND if wp & 0xFFFF == CMD_EXIT => {
                exit(hwnd);
                0
            }
            WM_QUERYENDSESSION => 1,
            WM_ENDSESSION => {
                if wp != 0 {
                    exit(hwnd);
                }
                0
            }
            m if m == TASKBAR_CREATED_MSG.load(Ordering::Relaxed) => {
                add_icon(hwnd);
                0
            }
            #[cfg(feature = "fault-injection")]
            m if m == TEST_PANIC.load(Ordering::Relaxed) => panic!("fault injection"),
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

//! The settings window's process, as the running EdgePad sees it (spec
//! 7.1): the same exe started again with `--settings`, showing the settings
//! window and nothing else, and ending when it closes. What it loaded (the
//! common controls, Windows's text input) goes with it, so the running
//! EdgePad keeps to its idle memory (spec 8).
//!
//! It runs in a job that ends it when the running EdgePad ends, however
//! that happens: there is no settings window without an EdgePad to take up
//! what it saves.
//!
//! Where its window is, the running EdgePad follows as it moves, and hands
//! to the next one it starts (Issue #137): the window opens again where it
//! closed, until EdgePad ends. Nothing is written for it.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command};
use std::ptr::{null, null_mut};

use edgepad_core::placement::Placement;
use windows_sys::Win32::Foundation::{HANDLE, HWND, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::WaitForSingleObject;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::log::{self, debug_log};
use crate::{editor, tray, watch};

/// The argument that starts the exe as the settings window's process.
pub const ARG: &str = "--settings";

thread_local! {
    static CHILD: RefCell<Option<Child>> = const { RefCell::new(None) };
    static JOB: Cell<HANDLE> = const { Cell::new(null_mut()) };
    /// Where the window last was, and the hook that follows it.
    static PLACE: Cell<Option<Placement>> = const { Cell::new(None) };
    static FOLLOW: Cell<HWINEVENTHOOK> = const { Cell::new(null_mut()) };
}

/// The job the settings window's process runs in, made the first time: its
/// handle is closed only as this process ends, which ends that one too.
unsafe fn job() -> HANDLE {
    if JOB.get().is_null() {
        unsafe {
            let job = CreateJobObjectW(null(), null());
            if !job.is_null() {
                let mut limit: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                limit.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let size = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
                SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limit as *const _ as _, size);
            }
            JOB.set(job);
        }
    }
    JOB.get()
}

/// The settings window's process, while it runs.
fn running() -> Option<u32> {
    CHILD.with_borrow_mut(|child| {
        let alive = child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if !alive {
            *child = None;
        }
        child.as_ref().map(Child::id)
    })
}

/// Whether the settings window's process ends within 0.2 s.
fn ends_soon() -> bool {
    CHILD.with_borrow_mut(|child| {
        let Some(c) = child.as_ref() else { return true };
        let ended = unsafe { WaitForSingleObject(c.as_raw_handle(), 200) } == WAIT_OBJECT_0;
        if ended {
            *child = None;
        }
        ended
    })
}

/// Whether `pid` is the settings window's process: its windows are
/// EdgePad's own, no app in front (spec 4.4).
pub fn is_settings(pid: u32) -> bool {
    running() == Some(pid)
}

/// The settings window of process `pid`, once it has made it.
fn window(pid: u32) -> Option<HWND> {
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, editor::CLASS, null()) };
        if hwnd.is_null() {
            return None;
        }
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        if owner == pid {
            return Some(hwnd);
        }
    }
}

/// Open the settings window from the tray: start its process, or bring its
/// window back if it is open (one still starting shows it soon).
pub unsafe fn open() {
    unsafe {
        if let Some(pid) = running() {
            if let Some(hwnd) = window(pid) {
                ShowWindow(hwnd, if IsIconic(hwnd) != 0 { SW_RESTORE } else { SW_SHOW });
                SetForegroundWindow(hwnd);
                return;
            }
            // No window: still starting, or ending, its window just closed.
            // An ending one is gone in a moment; one starting shows soon.
            if !ends_soon() {
                return;
            }
        }
        // A file the window cannot open is told here, where the tray is.
        if let Err(why) = editor::check_file() {
            tray::notify(&why);
            return;
        }
        // The file may be new, in a folder made just now: watched from now.
        watch::rescan();
        let Ok(exe) = std::env::current_exe() else { return };
        let mut command = Command::new(exe);
        command.arg(ARG);
        if log::enabled() {
            command.arg("--debug");
        }
        if let Some(p) = PLACE.get() {
            command.arg(format!("{}{p}", editor::PLACE_ARG));
        }
        match command.spawn() {
            Ok(child) => {
                follow(child.id());
                let job = job();
                if job.is_null() || AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                    debug_log!("設定画面のプロセスをジョブに入れられない");
                }
                // Chosen from the tray, it may come to the front.
                AllowSetForegroundWindow(child.id());
                debug_log!("設定画面のプロセスを起動: {}", child.id());
                CHILD.set(Some(child));
            }
            Err(why) => {
                debug_log!("設定画面のプロセスを起動できない: {why}");
                tray::notify("設定画面を開けませんでした");
            }
        }
    }
}

/// Follow where the window of process `pid` goes, in place of the last
/// one's: its moves, sizes, maximizing and minimizing.
unsafe fn follow(pid: u32) {
    unsafe {
        let last = FOLLOW.replace(null_mut());
        if !last.is_null() {
            UnhookWinEvent(last);
        }
        let flags = WINEVENT_OUTOFCONTEXT;
        FOLLOW.set(SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE, null_mut(), Some(moved), pid, 0, flags));
    }
}

/// A window of the settings window's process moved: if it is the settings
/// window itself, where it is now, as Windows keeps it.
unsafe extern "system" fn moved(_: HWINEVENTHOOK, _: u32, hwnd: HWND, object: i32, _: i32, _: u32, _: u32) {
    if object != OBJID_WINDOW || hwnd.is_null() || running().and_then(window) != Some(hwnd) {
        return;
    }
    unsafe {
        let mut wp: WINDOWPLACEMENT = zeroed();
        wp.length = size_of::<WINDOWPLACEMENT>() as u32;
        if GetWindowPlacement(hwnd, &mut wp) == 0 {
            return;
        }
        // Minimized, it opens again as it was before.
        let maximized = match wp.showCmd as i32 {
            SW_SHOWMAXIMIZED => true,
            SW_SHOWMINIMIZED => wp.flags & WPF_RESTORETOMAXIMIZED != 0,
            _ => false,
        };
        let r = wp.rcNormalPosition;
        PLACE.set(Some(Placement { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top, maximized }));
    }
}

/// EdgePad is to end from the tray: unless the settings window holds
/// changes not saved, in which case it asks about them there and EdgePad
/// goes on. A window that gives no answer may still hold changes: EdgePad
/// goes on then too; one gone meanwhile has none.
pub unsafe fn may_exit() -> bool {
    let Some(hwnd) = running().and_then(window) else { return true };
    let mut answer = 0;
    unsafe {
        // This process, chosen from the tray, may bring it to the front,
        // where its question is seen; the window's own process may not.
        SetForegroundWindow(hwnd);
        let sent = SendMessageTimeoutW(hwnd, editor::may_exit_message(), 0, 0, SMTO_ABORTIFHUNG, 3000, &mut answer);
        if sent == 0 {
            return IsWindow(hwnd) == 0;
        }
    }
    answer != 0
}

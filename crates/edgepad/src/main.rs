//! EdgePad for Windows: the Win32 platform layer around `edgepad_core`.

#![windows_subsystem = "windows"]

mod panel;
mod send;
mod settings;
mod tray;

use std::mem::zeroed;
use std::process::ExitCode;
use std::ptr::null_mut;

use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::Pointer::EnableMouseInPointer;
use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG};

fn main() -> ExitCode {
    // A panic must not leave keys held system-wide: release them, then let
    // the default hook report it. Runs even with panic = "abort".
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        unsafe { send::release_all() };
        report(info);
    }));
    unsafe {
        // Each panel follows the DPI of its own monitor (WM_DPICHANGED).
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Mouse arrives as WM_POINTER* too, so one code path serves both.
        EnableMouseInPointer(1);
        // An edge set to none shows no panel; the tray runs regardless.
        let panels = settings::load();
        for p in &panels.edges {
            if !panel::create(p.edge, p.lines, panels.key_size, &p.keys) {
                return ExitCode::FAILURE;
            }
        }
        if tray::create().is_null() {
            return ExitCode::FAILURE;
        }
        // Everything is message driven: no timers, no polling.
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
    }
    ExitCode::SUCCESS
}

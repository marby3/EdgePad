//! EdgePad for Windows: the Win32 platform layer around `edgepad_core`.

#![windows_subsystem = "windows"]

mod panel;

use std::mem::zeroed;
use std::ptr::null_mut;

use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_SYSTEM_AWARE, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG};

fn main() {
    unsafe {
        // Per-monitor awareness arrives with multi-monitor support (v0.2).
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        if panel::create_left().is_null() {
            return;
        }
        // Everything is message driven: no timers, no polling.
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
    }
}

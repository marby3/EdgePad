//! Starting what a run key names: a program, a document, a folder or a URL,
//! through the shell, as opening it from Explorer would.

use edgepad_core::config;

use crate::log::debug_log;
use crate::tray;

/// Start `command` from a run key: its program and arguments as
/// `config::run_target` splits them. A failure is reported in the tray.
pub unsafe fn run(command: &str) {
    let (file, params) = config::run_target(command);
    debug_log!("起動 {file}");
    tray::test_event(&format!("run: {file} | {params}"));
    // Test builds stop short of starting anything.
    #[cfg(not(feature = "fault-injection"))]
    unsafe {
        use std::mem::{size_of, zeroed};
        use std::ptr::null;
        use windows_sys::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
        };
        use windows_sys::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW};
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (file_w, params_w) = (wide(file), wide(params));
        let mut info: SHELLEXECUTEINFOW = zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        // The tray says what went wrong, not a dialog of the shell's.
        info.fMask = SEE_MASK_FLAG_NO_UI;
        info.lpFile = file_w.as_ptr();
        info.lpParameters = if params.is_empty() { null() } else { params_w.as_ptr() };
        info.nShow = SW_SHOWNORMAL;
        // The shell may hand the work to COM objects. Already initialised on
        // this thread is fine (S_FALSE); either way it is balanced below.
        let initialised = CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) >= 0;
        let started = ShellExecuteExW(&mut info) != 0;
        if initialised {
            CoUninitialize();
        }
        if !started {
            debug_log!("起動できない {file}: {}", std::io::Error::last_os_error());
            tray::notify(&format!("{file} を起動できませんでした"));
        }
    }
}

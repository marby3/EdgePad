//! EdgePad for Windows: the Win32 platform layer around `edgepad_core`.

#![windows_subsystem = "windows"]

mod autostart;
mod colors;
mod editor;
mod editor_process;
mod icon;
mod instance;
mod launch;
mod log;
mod panel;
mod pie_menu;
mod profile;
mod send;
mod settings;
mod tray;
mod watch;

use std::process::ExitCode;

use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::Ime::ImmDisableIME;
use windows_sys::Win32::UI::Input::Pointer::EnableMouseInPointer;

fn main() -> ExitCode {
    // A panic must not leave keys held system-wide: release them, then let
    // the default hook report it. Runs even with panic = "abort".
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        unsafe { send::release_all() };
        report(info);
    }));
    let debug = std::env::args_os().skip(1).any(|a| a == "--debug");
    unsafe {
        // Each panel follows the DPI of its own monitor (WM_DPICHANGED).
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Started from the tray to show the settings window, and nothing
        // else (editor_process).
        if std::env::args_os().skip(1).any(|a| a == editor_process::ARG) {
            log::start(debug, log::SETTINGS_FILE_NAME);
            return if editor::run() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        }
        // Nothing here takes typing: without the IME, Windows loads none of
        // its text input into EdgePad when the tray menu takes the focus
        // (spec 8). The settings window, which does take typing, has it in
        // its own process.
        ImmDisableIME(u32::MAX);
        // Mouse arrives as WM_POINTER* too, so one code path serves both.
        EnableMouseInPointer(1);
        // An edge set to none shows no panel; the tray runs regardless.
        // A second launch toggles the running EdgePad's panels and leaves.
        let Some(toggle) = instance::claim() else { return ExitCode::SUCCESS };
        // After the claim: a second launch must not wipe the running one's log.
        log::start(debug, log::FILE_NAME);
        let path = settings::path();
        let shown = path.as_ref().map_or("なし".into(), |p| p.display().to_string());
        log::debug_log!("起動 EdgePad {} 設定: {shown}", env!("CARGO_PKG_VERSION"));
        let (config, seen, notice) = settings::load();
        let panels = profile::use_config(config);
        panel::hide_at_start(profile::hidden_at_start());
        match (&notice, path) {
            (Some(notice), _) => log::debug_log!("設定を読めない（固定のキーで動く）: {notice}"),
            (None, Some(_)) => log::debug_log!("設定を読み込んだ: パネル {} 枚", panels.edges.len()),
            (None, None) => log::debug_log!("設定ファイルなし: 固定のキーで動く"),
        }
        panel::set_style(&panels);
        for p in &panels.edges {
            if !panel::create(p, &panels) {
                log::debug_log!("終了（パネルを作れない）");
                // Panels made before this one may have taken room.
                panel::undock_all();
                return ExitCode::FAILURE;
            }
        }
        if tray::create().is_null() {
            log::debug_log!("終了（トレイを作れない）");
            panel::undock_all();
            return ExitCode::FAILURE;
        }
        // Everything is message driven: no timers, no polling.
        if let Some(notice) = &notice {
            tray::notify(notice);
        }
        if !profile::watch() {
            log::debug_log!("前面のアプリを追えない");
            tray::notify("前面のアプリを追えません。プロファイルは自動で切り替わりません");
        }
        tray::set_hotkey(panels.hotkey);
        let unreadable = if seen.is_none() { notice } else { None };
        // Started: what starting touched (the shell's DLLs above all, for the
        // tray) leaves the working set. Pages used again come back as used
        // (spec 8, Issue #10).
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        watch::run(seen, unreadable, toggle);
    }
    ExitCode::SUCCESS
}

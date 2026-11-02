//! The monitors connected, with the numbers and names Windows gives them
//! (Issue #149): the numbers place the panels, the names label the settings
//! window's list.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use edgepad_core::layout::Rect;
use edgepad_core::monitor::{Display, display_number};
use windows_sys::Win32::Devices::Display::*;
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

unsafe extern "system" fn gather(monitor: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
    unsafe { (*(out as *mut Vec<HMONITOR>)).push(monitor) };
    1
}

/// The text of a NUL-ended UTF-16 buffer.
fn from_wide(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}

/// Every monitor with its rectangle and number (0 for a device name with
/// none), and the index of the primary one.
pub unsafe fn monitors() -> (Vec<(HMONITOR, Rect, u8)>, usize) {
    unsafe {
        let mut handles: Vec<HMONITOR> = Vec::new();
        EnumDisplayMonitors(null_mut(), null(), Some(gather), &mut handles as *mut _ as LPARAM);
        let mut primary = 0;
        let mut found = Vec::new();
        for handle in handles {
            let mut info: MONITORINFOEXW = zeroed();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(handle, &mut info as *mut _ as *mut MONITORINFO) == 0 {
                continue;
            }
            if info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0 {
                primary = found.len();
            }
            let m = info.monitorInfo.rcMonitor;
            let number = display_number(&from_wide(&info.szDevice)).unwrap_or(0);
            found.push((handle, Rect { x: m.left, y: m.top, w: m.right - m.left, h: m.bottom - m.top }, number));
        }
        (found, primary)
    }
}

/// The connected monitors as the settings window lists them, and the
/// primary one's number (0 when it has none).
pub fn displays() -> (Vec<Display>, u8) {
    let (monitors, primary) = unsafe { monitors() };
    let names = names();
    let displays = monitors
        .iter()
        .filter(|&&(_, _, n)| n > 0)
        .map(|&(_, _, number)| {
            let (name, internal) = names.iter().find(|(n, ..)| *n == number).map_or((String::new(), false), |(_, name, i)| (name.clone(), *i));
            Display { number, name, internal }
        })
        .collect();
    (displays, monitors.get(primary).map_or(0, |&(_, _, n)| n))
}

/// What each active display path reports: its source's number, the monitor's
/// name, and whether it is built in. A mirrored source has a path per
/// monitor; the first with a name wins.
fn names() -> Vec<(u8, String, bool)> {
    unsafe {
        let (mut paths, mut modes) = (0, 0);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes) != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut path: Vec<DISPLAYCONFIG_PATH_INFO> = vec![zeroed(); paths as usize];
        let mut mode: Vec<DISPLAYCONFIG_MODE_INFO> = vec![zeroed(); modes as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut paths, path.as_mut_ptr(), &mut modes, mode.as_mut_ptr(), null_mut()) != ERROR_SUCCESS {
            return Vec::new();
        }
        path.truncate(paths as usize);
        let mut found: Vec<(u8, String, bool)> = Vec::new();
        for p in &path {
            let mut source: DISPLAYCONFIG_SOURCE_DEVICE_NAME = zeroed();
            source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: p.sourceInfo.adapterId,
                id: p.sourceInfo.id,
            };
            let mut target: DISPLAYCONFIG_TARGET_DEVICE_NAME = zeroed();
            target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                adapterId: p.targetInfo.adapterId,
                id: p.targetInfo.id,
            };
            if DisplayConfigGetDeviceInfo(&mut source.header) != 0 || DisplayConfigGetDeviceInfo(&mut target.header) != 0 {
                continue;
            }
            let Some(number) = display_number(&from_wide(&source.viewGdiDeviceName)) else { continue };
            let name = from_wide(&target.monitorFriendlyDeviceName);
            let internal = matches!(
                target.outputTechnology,
                DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
            );
            match found.iter_mut().find(|(n, ..)| *n == number) {
                Some(seen) if seen.1.is_empty() => *seen = (number, name, internal),
                Some(_) => {}
                None => found.push((number, name, internal)),
            }
        }
        found
    }
}

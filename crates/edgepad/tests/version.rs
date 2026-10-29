//! Acceptance test for Issue #79, AC2: the exe carries a version resource,
//! named EdgePad, at the version of Cargo.toml. Its size limit is
//! ci/test_build.py's.

use std::ptr::null_mut;

use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// The string `name` of the exe's version resource, in its first language.
fn field(info: &[u8], name: &str) -> Option<String> {
    unsafe {
        let (mut at, mut len) = (null_mut(), 0u32);
        let translation = wide("\\VarFileInfo\\Translation");
        if VerQueryValueW(info.as_ptr() as *const _, translation.as_ptr(), &mut at, &mut len) == 0 || len < 4 {
            return None;
        }
        let pair = *(at as *const [u16; 2]);
        let path = wide(&format!("\\StringFileInfo\\{:04x}{:04x}\\{name}", pair[0], pair[1]));
        if VerQueryValueW(info.as_ptr() as *const _, path.as_ptr(), &mut at, &mut len) == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(at as *const u16, len as usize);
        Some(String::from_utf16_lossy(text).trim_end_matches('\0').to_string())
    }
}

#[test]
fn the_exe_has_a_version_resource_named_edgepad_at_the_manifests_version() {
    let exe = wide(env!("CARGO_BIN_EXE_edgepad"));
    let size = unsafe { GetFileVersionInfoSizeW(exe.as_ptr(), null_mut()) };
    assert!(size > 0, "the exe has a version resource");
    let mut info = vec![0u8; size as usize];
    assert!(unsafe { GetFileVersionInfoW(exe.as_ptr(), 0, size, info.as_mut_ptr() as *mut _) } != 0);
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(field(&info, "ProductName").as_deref(), Some("EdgePad"));
    assert_eq!(field(&info, "FileDescription").as_deref(), Some("EdgePad"));
    assert_eq!(field(&info, "FileVersion").as_deref(), Some(version));
    assert_eq!(field(&info, "ProductVersion").as_deref(), Some(version));
}

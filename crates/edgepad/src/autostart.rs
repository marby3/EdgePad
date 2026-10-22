//! Starting with Windows: a shortcut to this exe in the startup folder.
//!
//! The shortcut is made through IShellLinkW and IPersistFile. windows-sys
//! has the COM functions but not the interfaces, so the two vtables are laid
//! out here, as far as the methods used.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::ShellLink;
use windows_sys::core::{GUID, HRESULT};

const IID_ISHELLLINKW: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
const IID_IPERSISTFILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);

type SetString = unsafe extern "system" fn(*mut c_void, *const u16) -> HRESULT;

#[repr(C)]
struct Unknown {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// IShellLinkW: IUnknown, then GetPath .. SetWorkingDirectory is the 7th
/// method and SetPath the 18th.
#[repr(C)]
struct ShellLinkVtbl {
    unknown: Unknown,
    _before_working_directory: [usize; 6],
    set_working_directory: SetString,
    _before_path: [usize; 10],
    set_path: SetString,
}

/// IPersistFile: IUnknown, GetClassID, IsDirty, Load, then Save.
#[repr(C)]
struct PersistFileVtbl {
    unknown: Unknown,
    _before_save: [usize; 3],
    save: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> HRESULT,
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

/// %APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\EdgePad.lnk
fn link() -> Option<PathBuf> {
    let appdata = PathBuf::from(std::env::var_os("APPDATA")?);
    Some(appdata.join(r"Microsoft\Windows\Start Menu\Programs\Startup\EdgePad.lnk"))
}

pub fn enabled() -> bool {
    link().is_some_and(|l| l.is_file())
}

/// Add the shortcut, or remove it if it is there. Returns false on failure.
pub unsafe fn toggle() -> bool {
    let Some(link) = link() else { return false };
    if link.is_file() {
        return std::fs::remove_file(&link).is_ok();
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let made = link.parent().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok());
    made && unsafe { create(&link, &exe) }
}

unsafe fn create(link: &Path, exe: &Path) -> bool {
    unsafe {
        // Already initialised on this thread is fine (S_FALSE); either way
        // it is balanced by CoUninitialize below.
        let initialised = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) >= 0;
        let made = with_link(link, exe);
        if initialised {
            CoUninitialize();
        }
        made
    }
}

unsafe fn with_link(link: &Path, exe: &Path) -> bool {
    unsafe {
        let mut shell_link: *mut c_void = null_mut();
        if CoCreateInstance(&ShellLink, null_mut(), CLSCTX_INPROC_SERVER, &IID_ISHELLLINKW, &mut shell_link) < 0 {
            return false;
        }
        let vtbl = &**(shell_link as *mut *const ShellLinkVtbl);
        let mut ok = (vtbl.set_path)(shell_link, wide(exe).as_ptr()) >= 0;
        if let Some(dir) = exe.parent() {
            ok &= (vtbl.set_working_directory)(shell_link, wide(dir).as_ptr()) >= 0;
        }
        let mut persist: *mut c_void = null_mut();
        if ok && (vtbl.unknown.query_interface)(shell_link, &IID_IPERSISTFILE, &mut persist) >= 0 {
            let file = &**(persist as *mut *const PersistFileVtbl);
            ok = (file.save)(persist, wide(link).as_ptr(), 1) >= 0;
            (file.unknown.release)(persist);
        } else {
            ok = false;
        }
        (vtbl.unknown.release)(shell_link);
        ok
    }
}

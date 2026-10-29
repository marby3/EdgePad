//! Embeds the manifest that asks for the common controls version 6 (the
//! settings window's buttons, lists and fields in Windows's own look), and
//! EdgePad's icon.
//!
//! The manifest goes in as resource 2, not 1: Windows applies resource 1 to
//! the whole process at its start, which would load comctl32 into every
//! EdgePad, settings opened or not. The settings window applies resource 2
//! itself, when it opens (spec 7.1, 8).
//!
//! The icon, icon/edgepad.ico (made by icon/make_ico.py), goes in as icon
//! group 1, in a .res file built here and handed to the linker: no rc.exe,
//! no crate (Issue #116).

use std::path::Path;

/// Icon group 1's resource id: the contract with src/icon.rs and the tests.
const GROUP: u16 = 1;
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED,ID=2");
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
             version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
        println!("cargo:rerun-if-changed=icon/edgepad.ico");
        let ico = std::fs::read("icon/edgepad.ico").expect("icon/edgepad.ico");
        let res = Path::new(&std::env::var("OUT_DIR").unwrap()).join("edgepad.res");
        std::fs::write(&res, icon_res(&ico)).expect("write edgepad.res");
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
}

/// A .res file holding the images of `ico`, one icon resource each from 1
/// on, and icon group `GROUP` listing them.
fn icon_res(ico: &[u8]) -> Vec<u8> {
    let u16_at = |at: usize| u16::from_le_bytes([ico[at], ico[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(ico[at..at + 4].try_into().unwrap());
    let count = u16_at(4);
    // A .res file starts with an empty resource.
    let mut res = Vec::new();
    entry(&mut res, 0, 0, 0, &[]);
    let mut group = vec![0, 0, 1, 0];
    group.extend(count.to_le_bytes());
    for i in 0..count {
        // An .ico's 16-byte entry; a group's is the same but for its last
        // field, the image's resource id in place of its offset in the file.
        let at = 6 + i as usize * 16;
        let (len, offset) = (u32_at(at + 8) as usize, u32_at(at + 12) as usize);
        entry(&mut res, RT_ICON, i + 1, 0x1010, &ico[offset..offset + len]);
        group.extend(&ico[at..at + 12]);
        group.extend((i + 1).to_le_bytes());
    }
    entry(&mut res, RT_GROUP_ICON, GROUP, 0x1030, &group);
    res
}

/// Append a resource of type `kind` and id `name`, language neutral, to
/// `res`: a 32-byte header, then `data`, padded to 4 bytes.
fn entry(res: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    res.extend((data.len() as u32).to_le_bytes());
    res.extend(32u32.to_le_bytes());
    for id in [kind, name] {
        res.extend(0xFFFFu16.to_le_bytes());
        res.extend(id.to_le_bytes());
    }
    res.extend(0u32.to_le_bytes()); // data version
    res.extend(flags.to_le_bytes());
    res.extend(0u16.to_le_bytes()); // language
    res.extend(0u32.to_le_bytes()); // version
    res.extend(0u32.to_le_bytes()); // characteristics
    res.extend(data);
    res.resize(res.len().next_multiple_of(4), 0);
}

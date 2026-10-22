//! Key output with SendInput, tagged so EdgePad's own input is recognisable.
//!
//! Every key this module presses is remembered until it is released, so
//! `release_all` can let go of exactly EdgePad's keys on any way out (exit,
//! logoff, panic) and never touches keys held on the physical keyboard.

use std::mem::{size_of, zeroed};
use std::sync::atomic::{AtomicU64, Ordering};

use edgepad_core::keys::SEND_TAG;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
};

/// Bit set of the virtual keys EdgePad currently holds down (virtual keys are
/// 1..=254, so four words cover them). Atomic so the panic hook can read it
/// whatever state the UI thread was left in.
static HELD: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// Press (`down`) or release a virtual key. The scan code goes along for
/// applications that read it instead of the virtual key.
///
/// Only an event that was actually injected changes what is recorded as held:
/// a key-down that failed must not be "released" later over a key the user
/// holds on the keyboard, and a key-up that failed stays recorded for retry.
pub unsafe fn key(vk: u16, down: bool) {
    debug_assert!(vk < 256, "not a virtual key: {vk}");
    let sent = unsafe {
        let mut input: INPUT = zeroed();
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.wScan = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as u16;
        input.Anonymous.ki.dwFlags = if down { 0 } else { KEYEVENTF_KEYUP };
        input.Anonymous.ki.dwExtraInfo = SEND_TAG;
        SendInput(1, &input, size_of::<INPUT>() as i32) == 1
    };
    diag(format_args!("SEND vk={vk:#x} down={down} sent={sent}"));
    if !sent {
        return;
    }
    let (word, bit) = (&HELD[(vk as usize >> 6) & 3], 1u64 << (vk & 63));
    if down {
        word.fetch_or(bit, Ordering::Relaxed);
    } else {
        word.fetch_and(!bit, Ordering::Relaxed);
    }
}

/// TEMPORARY diagnostics for #12 (removed before merge).
pub fn diag(line: std::fmt::Arguments) {
    use std::io::Write;
    if let Some(dir) = std::env::var_os("EDGEPAD_DIAG") {
        let path = std::path::Path::new(&dir).join(format!("edgepad-diag-{}.log", std::process::id()));
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 1_000_000;
            let _ = writeln!(f, "{t} {line}");
        }
    }
}

/// Release every key EdgePad still holds.
pub unsafe fn release_all() {
    for (i, word) in HELD.iter().enumerate() {
        let mut bits = word.load(Ordering::Relaxed);
        while bits != 0 {
            let vk = (i * 64) as u16 + bits.trailing_zeros() as u16;
            bits &= bits - 1;
            unsafe { key(vk, false) };
        }
    }
}

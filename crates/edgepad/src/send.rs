//! Key output with SendInput, tagged so EdgePad's own input is recognisable.
//!
//! Every key this module presses is remembered until it is released, so
//! `release_all` can let go of exactly EdgePad's keys on any way out (exit,
//! logoff, panic) and never touches keys held on the physical keyboard.

use std::mem::{size_of, zeroed};
use std::sync::atomic::{AtomicU64, Ordering};

use edgepad_core::keys::{Key, SEND_TAG};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

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

/// The virtual key of `key`. Modifiers are the left-hand ones.
pub fn vk(key: Key) -> u16 {
    match key {
        Key::Ctrl => VK_LCONTROL,
        Key::Shift => VK_LSHIFT,
        Key::Alt => VK_LMENU,
        Key::Win => VK_LWIN,
        // Letters and digits are their own virtual keys.
        Key::Char(c @ (b'A'..=b'Z' | b'0'..=b'9')) => c as u16,
        Key::Char(b'[') => VK_OEM_4,
        Key::Char(b']') => VK_OEM_6,
        Key::Char(b'-') => VK_OEM_MINUS,
        Key::Char(b'=') => VK_OEM_PLUS,
        Key::Char(b';') => VK_OEM_1,
        Key::Char(b'\'') => VK_OEM_7,
        Key::Char(b',') => VK_OEM_COMMA,
        Key::Char(b'.') => VK_OEM_PERIOD,
        Key::Char(b'/') => VK_OEM_2,
        Key::Char(b'\\') => VK_OEM_5,
        Key::Char(b'`') => VK_OEM_3,
        // Key::parse makes no other Char (keys.rs, SYMBOLS).
        Key::Char(c) => unreachable!("the parser admits no key {c}"),
        Key::F(n) => VK_F1 + n as u16 - 1,
        Key::Space => VK_SPACE,
        Key::Enter => VK_RETURN,
        Key::Esc => VK_ESCAPE,
        Key::Tab => VK_TAB,
        Key::Backspace => VK_BACK,
        Key::Delete => VK_DELETE,
        Key::Insert => VK_INSERT,
        Key::Home => VK_HOME,
        Key::End => VK_END,
        Key::PgUp => VK_PRIOR,
        Key::PgDn => VK_NEXT,
        Key::Left => VK_LEFT,
        Key::Right => VK_RIGHT,
        Key::Up => VK_UP,
        Key::Down => VK_DOWN,
    }
}

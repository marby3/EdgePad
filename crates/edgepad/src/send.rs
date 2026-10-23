//! Key output with SendInput, tagged so EdgePad's own input is recognisable.
//!
//! Every key this module presses is remembered until it is released, so
//! `release_all` can let go of exactly EdgePad's keys on any way out (exit,
//! logoff, panic) and never touches keys held on the physical keyboard.

use std::mem::{size_of, zeroed};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use edgepad_core::keys::{Key, SEND_TAG};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};

/// Bit set of the virtual keys EdgePad currently holds down (virtual keys are
/// 1..=254, so four words cover them). Atomic so the panic hook can read it
/// whatever state the UI thread was left in.
static HELD: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// Whether a failed send has been reported and no send has succeeded since:
/// one notice per run of failures, not one per key.
static FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);

/// Whether the app in front runs out of EdgePad's reach (as administrator:
/// Windows drops the keys sent to it, often without SendInput failing), and
/// whether that has been told since it came to the front.
static OUT_OF_REACH: AtomicBool = AtomicBool::new(false);
static OUT_OF_REACH_REPORTED: AtomicBool = AtomicBool::new(false);

/// Test builds only: make every send fail, as sending to an elevated app does.
#[cfg(feature = "fault-injection")]
pub static FAIL: AtomicBool = AtomicBool::new(false);

fn failing() -> bool {
    #[cfg(feature = "fault-injection")]
    return FAIL.load(Ordering::Relaxed);
    #[cfg(not(feature = "fault-injection"))]
    false
}

/// Most events one SendInput carries: four modifiers down and up, and a key.
const MAX_EVENTS: usize = 10;

/// Press (`down`) or release a virtual key.
pub unsafe fn key(vk: u16, down: bool) {
    unsafe { send(&[(vk, down)]) };
}

/// Send a shortcut: press `mods` in order, tap `key`, release `mods` in
/// reverse, all in one SendInput so nothing can come in between. A modifier
/// that is already down, held on the panel or on the physical keyboard, is
/// left alone: pressing it again changes nothing, and releasing it would cut
/// it from under the finger holding it.
pub unsafe fn shortcut(mods: &[u16], key: Option<u16>) {
    // Four modifiers at most (Ctrl, Shift, Alt, Win), so 4 + 2 + 4 events.
    assert!(mods.len() <= 4, "more modifiers than there are: {}", mods.len());
    let mut up = [0u16; 4];
    let mut events = [(0u16, false); MAX_EVENTS];
    let mut n = 0;
    for &m in mods {
        if !unsafe { is_down(m) } {
            up[n] = m;
            events[n] = (m, true);
            n += 1;
        }
    }
    let downs = n;
    if let Some(k) = key {
        events[n] = (k, true);
        events[n + 1] = (k, false);
        n += 2;
    }
    for &m in up[..downs].iter().rev() {
        events[n] = (m, false);
        n += 1;
    }
    unsafe { send(&events[..n]) };
}

/// Whether modifier `vk` is down already, either side of the keyboard.
unsafe fn is_down(vk: u16) -> bool {
    let either = match vk {
        VK_LCONTROL => VK_CONTROL,
        VK_LSHIFT => VK_SHIFT,
        VK_LMENU => VK_MENU,
        other => other,
    };
    let down = |v: u16| unsafe { GetAsyncKeyState(v as i32) } as u16 & 0x8000 != 0;
    held(vk) || down(either) || (vk == VK_LWIN && down(VK_RWIN))
}

fn held(vk: u16) -> bool {
    HELD[(vk as usize >> 6) & 3].load(Ordering::Relaxed) & 1u64 << (vk & 63) != 0
}

/// Keys whose scan code Windows prefixes with E0: without the extended flag
/// they arrive as their number-pad twins.
fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_INSERT | VK_DELETE | VK_HOME | VK_END | VK_PRIOR | VK_NEXT
            | VK_LWIN | VK_RWIN
    )
}

/// Inject `events` (virtual key, is down) in one SendInput. The scan code
/// goes along for applications that read it instead of the virtual key.
///
/// Only an event that was actually injected changes what is recorded as held:
/// a key-down that failed must not be "released" later over a key the user
/// holds on the keyboard, and a key-up that failed stays recorded for retry.
///
/// Builds the input on the stack: `release_all` runs from the panic hook,
/// where allocating is best avoided. A shortcut is at most 10 events.
unsafe fn send(events: &[(u16, bool)]) {
    let events = &events[..events.len().min(MAX_EVENTS)];
    let mut inputs: [INPUT; MAX_EVENTS] = unsafe { zeroed() };
    for (input, &(vk, down)) in inputs.iter_mut().zip(events) {
        debug_assert!(vk < 256, "not a virtual key: {vk}");
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.wScan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
        let extended = if is_extended(vk) { KEYEVENTF_EXTENDEDKEY } else { 0 };
        input.Anonymous.ki.dwFlags = extended | if down { 0 } else { KEYEVENTF_KEYUP };
        input.Anonymous.ki.dwExtraInfo = SEND_TAG;
    }
    let sent = match failing() {
        true => 0,
        false => (unsafe { SendInput(events.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) }) as usize,
    };
    for (i, &(vk, down)) in events.iter().enumerate() {
        let what = if down { "押す" } else { "離す" };
        match i < sent {
            true => crate::log::debug_log!("送信 0x{vk:02X} {what}"),
            false => crate::log::debug_log!("送信できない 0x{vk:02X} {what}"),
        }
    }
    if !events.is_empty() {
        report(sent == events.len());
    }
    for &(vk, down) in &events[..sent.min(events.len())] {
        let (word, bit) = (&HELD[(vk as usize >> 6) & 3], 1u64 << (vk & 63));
        if down {
            word.fetch_or(bit, Ordering::Relaxed);
        } else {
            word.fetch_and(!bit, Ordering::Relaxed);
        }
    }
}

/// Move the mouse to where it is, so Windows works out afresh which window
/// it is over: a relative move of nothing is dropped, an absolute one is not.
/// Rounding to the 0..=65535 scale may shift it by a pixel. Tagged as
/// EdgePad's, like the keys. Returns false if it could not be sent.
pub unsafe fn recheck_mouse() -> bool {
    unsafe {
        let mut at = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut at) == 0 {
            return false;
        }
        // Absolute moves run 0..=65535 over the whole virtual desktop.
        let (left, top) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let (w, h) = (GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2), GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2));
        let mut input: INPUT = zeroed();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.dx = ((at.x - left) * 65535 + (w - 1) / 2) / (w - 1);
        input.Anonymous.mi.dy = ((at.y - top) * 65535 + (h - 1) / 2) / (h - 1);
        input.Anonymous.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        input.Anonymous.mi.dwExtraInfo = SEND_TAG;
        SendInput(1, &input, size_of::<INPUT>() as i32) == 1
    }
}

/// An app came to the front: say whether it is `out` of reach.
pub fn set_out_of_reach(out: bool) {
    OUT_OF_REACH.store(out, Ordering::Relaxed);
    OUT_OF_REACH_REPORTED.store(false, Ordering::Relaxed);
}

/// Note whether a SendInput got everything through: one notice per run of
/// failures. With an app out of reach in front, one notice saying so
/// instead, the first time a key goes to it.
fn report(all_sent: bool) {
    if OUT_OF_REACH.load(Ordering::Relaxed) {
        // Posted, not shown here: this may run from the panic hook.
        if !OUT_OF_REACH_REPORTED.swap(true, Ordering::Relaxed) {
            unsafe { crate::tray::post_out_of_reach() };
        }
        return;
    }
    if all_sent {
        FAILURE_REPORTED.store(false, Ordering::Relaxed);
    } else if !FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
        // UIPI blocks input to an app running elevated. Posted, not shown
        // here: this may run from the panic hook.
        unsafe { crate::tray::post_send_failed() };
    }
}

/// Type `text` as Unicode characters: a key-down and a key-up for each UTF-16
/// unit, all in one SendInput. Nothing is left down, so nothing is recorded
/// as held.
pub unsafe fn text(text: &str) {
    let inputs: Vec<INPUT> = text
        .encode_utf16()
        .flat_map(|unit| [0, KEYEVENTF_KEYUP].map(move |up| (unit, up)))
        .map(|(unit, up)| {
            let mut input: INPUT = unsafe { zeroed() };
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wScan = unit;
            input.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE | up;
            input.Anonymous.ki.dwExtraInfo = SEND_TAG;
            input
        })
        .collect();
    if inputs.is_empty() {
        return;
    }
    let sent = match failing() {
        true => 0,
        false => (unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) }) as usize,
    };
    // The count, not the text: the log should not keep what was typed.
    crate::log::debug_log!("送信 文字 {} 単位中 {} 単位", inputs.len() / 2, sent / 2);
    report(sent == inputs.len());
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

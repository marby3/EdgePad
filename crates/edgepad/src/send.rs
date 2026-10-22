//! Key output with SendInput, tagged so EdgePad's own input is recognisable.

use std::mem::{size_of, zeroed};

use edgepad_core::keys::SEND_TAG;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
};

/// Press (`down`) or release a virtual key. The scan code goes along for
/// applications that read it instead of the virtual key.
pub unsafe fn key(vk: u16, down: bool) {
    unsafe {
        let mut input: INPUT = zeroed();
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.wScan = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as u16;
        input.Anonymous.ki.dwFlags = if down { 0 } else { KEYEVENTF_KEYUP };
        input.Anonymous.ki.dwExtraInfo = SEND_TAG;
        SendInput(1, &input, size_of::<INPUT>() as i32);
    }
}

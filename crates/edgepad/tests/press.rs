//! Acceptance tests for Issue #5: keys are held down while a finger or the
//! mouse presses them.
//!
//! Touch and pen are injected with InjectTouchInput / InjectSyntheticPointerInput.
//! A low-level keyboard hook records every injected key event and swallows it,
//! so nothing the panel sends reaches other applications during the tests.

mod common;

use std::mem::zeroed;
use std::ptr::null_mut;
use std::sync::{Mutex, Once};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Controls::{
    CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, POINTER_FEEDBACK_NONE,
    POINTER_TYPE_INFO,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Input::Pointer::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const VK: [u16; 5] = [VK_ESCAPE, VK_LCONTROL, VK_LSHIFT, VK_LMENU, VK_SPACE];
const ESC: usize = 0;
const CTRL: usize = 1;
const SHIFT: usize = 2;
const ALT: usize = 3;
const SPACE: usize = 4;

/// (virtual key, is key-up, dwExtraInfo) of one injected key event.
type KeyEvent = (u16, bool, usize);

static EVENTS: Mutex<Vec<KeyEvent>> = Mutex::new(Vec::new());

unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let k = &*(lp as *const KBDLLHOOKSTRUCT);
            if k.flags & LLKHF_INJECTED != 0 {
                EVENTS.lock().unwrap().push((k.vkCode as u16, k.flags & LLKHF_UP != 0, k.dwExtraInfo));
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, wp, lp)
    }
}

/// Records and swallows injected key events while alive.
struct KeyLog(HHOOK);

impl KeyLog {
    fn start() -> KeyLog {
        EVENTS.lock().unwrap().clear();
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), null_mut(), 0) };
        assert!(!hook.is_null(), "install keyboard hook");
        KeyLog(hook)
    }

    /// Key events so far as (virtual key, is key-up).
    fn keys(&self) -> Vec<(u16, bool)> {
        EVENTS.lock().unwrap().iter().map(|&(vk, up, _)| (vk, up)).collect()
    }

    fn all(&self) -> Vec<KeyEvent> {
        EVENTS.lock().unwrap().clone()
    }
}

impl Drop for KeyLog {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

fn down(key: usize) -> (u16, bool) {
    (VK[key], false)
}

fn up(key: usize) -> (u16, bool) {
    (VK[key], true)
}

/// Centre of key `i` of the panel.
fn key_centre(panel: RECT, i: usize) -> POINT {
    let size = panel.right - panel.left;
    POINT { x: panel.left + size / 2, y: panel.top + size * i as i32 + size / 2 }
}

/// A point well outside the panel, level with key `i`.
fn outside(panel: RECT, i: usize) -> POINT {
    let p = key_centre(panel, i);
    POINT { x: panel.right + 300, y: p.y }
}

fn mouse_down_at(p: POINT) {
    mouse_at(p, MOUSEEVENTF_LEFTDOWN);
    pump(100);
}

fn mouse_move_to(p: POINT) {
    mouse_at(p, 0);
    pump(100);
}

fn mouse_up_at(p: POINT) {
    mouse_at(p, MOUSEEVENTF_LEFTUP);
    pump(150);
}

/// Injected touch contacts. Every frame carries every active contact, and
/// `hold` keeps them alive with update frames.
struct Touch {
    contacts: Vec<(u32, POINT)>,
}

static TOUCH_INIT: Once = Once::new();

impl Touch {
    fn new() -> Touch {
        TOUCH_INIT.call_once(|| {
            let ok = unsafe { InitializeTouchInjection(10, TOUCH_FEEDBACK_NONE) };
            assert!(ok != 0, "InitializeTouchInjection");
        });
        Touch { contacts: Vec::new() }
    }

    fn info(id: u32, p: POINT, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO {
        let mut t: POINTER_TOUCH_INFO = unsafe { zeroed() };
        t.pointerInfo.pointerType = PT_TOUCH;
        t.pointerInfo.pointerId = id;
        t.pointerInfo.ptPixelLocation = p;
        t.pointerInfo.pointerFlags = flags;
        t.touchMask = TOUCH_MASK_NONE;
        t
    }

    fn inject(&self, frame: &[POINTER_TOUCH_INFO]) {
        let ok = unsafe { InjectTouchInput(frame.len() as u32, frame.as_ptr()) };
        assert!(ok != 0, "InjectTouchInput: {}", std::io::Error::last_os_error());
    }

    fn moving(&self) -> Vec<POINTER_TOUCH_INFO> {
        let flags = POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT;
        self.contacts.iter().map(|&(id, p)| Touch::info(id, p, flags)).collect()
    }

    fn down(&mut self, id: u32, p: POINT) {
        let mut frame = self.moving();
        frame.push(Touch::info(id, p, POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT));
        self.inject(&frame);
        self.contacts.push((id, p));
        self.hold(100);
    }

    fn move_to(&mut self, id: u32, p: POINT) {
        for c in self.contacts.iter_mut().filter(|c| c.0 == id) {
            c.1 = p;
        }
        self.inject(&self.moving());
        self.hold(100);
    }

    fn up(&mut self, id: u32) {
        let mut frame = self.moving();
        for f in frame.iter_mut().filter(|f| f.pointerInfo.pointerId == id) {
            f.pointerInfo.pointerFlags = POINTER_FLAG_UP;
        }
        self.inject(&frame);
        self.contacts.retain(|c| c.0 != id);
        self.hold(150);
    }

    fn hold(&self, ms: u64) {
        let mut left = ms;
        while left > 0 {
            if !self.contacts.is_empty() {
                self.inject(&self.moving());
            }
            let step = left.min(30);
            pump(step);
            left -= step;
        }
    }
}

impl Drop for Touch {
    fn drop(&mut self) {
        while let Some(&(id, _)) = self.contacts.first() {
            self.up(id);
        }
    }
}

fn pen_frame(p: POINT, flags: POINTER_FLAGS) -> POINTER_TYPE_INFO {
    let mut info: POINTER_TYPE_INFO = unsafe { zeroed() };
    info.r#type = PT_PEN;
    let mut pen: POINTER_PEN_INFO = unsafe { zeroed() };
    pen.pointerInfo.pointerType = PT_PEN;
    pen.pointerInfo.ptPixelLocation = p;
    pen.pointerInfo.pointerFlags = flags;
    pen.penMask = PEN_MASK_NONE;
    info.Anonymous.penInfo = pen;
    info
}

/// Press a key with an injected pen: hover in, touch down, hold, lift.
fn pen_tap(p: POINT) {
    unsafe {
        let pen = CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_NONE);
        assert!(!pen.is_null(), "CreateSyntheticPointerDevice");
        let steps = [
            POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE,
            POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
            POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
            POINTER_FLAG_UP | POINTER_FLAG_INRANGE,
            POINTER_FLAG_UPDATE,
        ];
        for flags in steps {
            let frame = pen_frame(p, flags);
            let ok = InjectSyntheticPointerInput(pen, &frame, 1);
            assert!(ok != 0, "InjectSyntheticPointerInput: {}", std::io::Error::last_os_error());
            pump(100);
        }
        DestroySyntheticPointerDevice(pen);
    }
}

fn panel_rect(app: &Running) -> (HWND, RECT) {
    let hwnd = find_panel(app);
    let mut r: RECT = unsafe { zeroed() };
    unsafe { GetWindowRect(hwnd, &mut r) };
    (hwnd, r)
}

// AC1
#[test]
fn pressing_a_key_with_touch_or_mouse_holds_its_virtual_key_until_release() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    // Mouse, every key: one key-down while held, one key-up on release.
    for key in [ESC, CTRL, SHIFT, ALT, SPACE] {
        let before = log.keys().len();
        mouse_down_at(key_centre(panel, key));
        pump(400);
        assert_eq!(log.keys()[before..], [down(key)], "mouse held on key {key}");
        mouse_up_at(key_centre(panel, key));
        assert_eq!(log.keys()[before..], [down(key), up(key)], "mouse released key {key}");
    }

    // Touch.
    let before = log.keys().len();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    touch.hold(400);
    assert_eq!(log.keys()[before..], [down(CTRL)], "finger held on Ctrl");
    touch.up(1);
    assert_eq!(log.keys()[before..], [down(CTRL), up(CTRL)], "finger lifted from Ctrl");
}

// AC2
#[test]
fn two_fingers_hold_two_keys_and_lifting_one_releases_only_its_key() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, CTRL));
    touch.down(2, key_centre(panel, SHIFT));
    touch.hold(200);
    assert_eq!(log.keys(), [down(CTRL), down(SHIFT)]);

    touch.up(1);
    assert_eq!(log.keys(), [down(CTRL), down(SHIFT), up(CTRL)], "only Ctrl released");

    touch.up(2);
    assert_eq!(log.keys(), [down(CTRL), down(SHIFT), up(CTRL), up(SHIFT)]);
}

// AC3
#[test]
fn sliding_off_the_key_keeps_it_down_until_release() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, ALT));
    touch.move_to(1, outside(panel, ALT));
    touch.hold(300);
    assert_eq!(log.keys(), [down(ALT)], "finger slid off Alt");
    touch.up(1);
    assert_eq!(log.keys(), [down(ALT), up(ALT)], "finger lifted outside the panel");

    let before = log.keys().len();
    mouse_down_at(key_centre(panel, SHIFT));
    let away = outside(panel, SHIFT);
    mouse_move_to(away);
    pump(200);
    assert_eq!(log.keys()[before..], [down(SHIFT)], "cursor dragged off Shift");
    mouse_up_at(away);
    assert_eq!(log.keys()[before..], [down(SHIFT), up(SHIFT)], "button released outside the panel");
}

// AC4
#[test]
fn pen_presses_nothing() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    pen_tap(key_centre(panel, CTRL));
    pump(200);
    assert_eq!(log.keys(), [], "pen must not press keys");
}

// AC5
#[test]
fn a_held_key_changes_colour_and_returns_on_release() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let _log = KeyLog::start();

    // A spot of the Shift key's face, clear of its label.
    let size = panel.right - panel.left;
    let top = panel.top + size * SHIFT as i32;
    let spot = RECT {
        left: panel.left + size / 4,
        top: top + size / 6,
        right: panel.left + size / 4 + 2,
        bottom: top + size / 6 + 2,
    };
    let colour = || channels(capture(spot)[0]);
    let differs = |a: [i32; 3], b: [i32; 3]| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 40);

    let idle = colour();
    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, SHIFT));
    touch.hold(200);
    let pressed = colour();
    touch.up(1);
    pump(200);
    let released = colour();

    assert!(differs(idle, pressed), "pressed colour {pressed:?} looks like idle {idle:?}");
    assert!(!differs(idle, released), "released colour {released:?} did not return to {idle:?}");
}

// AC6
#[test]
fn every_sent_key_event_carries_the_edgepad_tag() {
    let _desktop = desktop();
    let app = launch();
    let (_, panel) = panel_rect(&app);
    let log = KeyLog::start();

    let mut touch = Touch::new();
    touch.down(1, key_centre(panel, SPACE));
    touch.up(1);
    mouse_down_at(key_centre(panel, ESC));
    mouse_up_at(key_centre(panel, ESC));

    let events = log.all();
    assert_eq!(events.len(), 4, "events: {events:?}");
    // One shared, non-zero tag: physical keys carry zero.
    let tag = events[0].2;
    assert_ne!(tag, 0, "EdgePad's key events carry no tag");
    for (vk, is_up, extra) in events {
        assert_eq!(extra, tag, "vk {vk:#x} up={is_up} carried {extra:#x}, others {tag:#x}");
    }
}

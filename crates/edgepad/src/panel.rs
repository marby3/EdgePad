//! The edge panel window: a layered, topmost, never-activated popup, and the
//! handle left on its edge while it is hidden.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use edgepad_core::config::{Action, Edge, Handle, Monitor};
use edgepad_core::gesture::{self, Swipe};
use edgepad_core::keys::{Key, Mods};
use edgepad_core::layout::{self, Rect};
use edgepad_core::press::Presses;
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::Input::Pointer::GetPointerType;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::send;
use crate::settings::{EdgePanel, Panels};

/// Opacity of the shown panel: 90%.
const OPACITY: u8 = 230;
const PANEL_BG: u32 = 0x0020_2020;
const KEY_FACE: u32 = 0x003A_3A3A;
/// Face of a held key: accent blue, RGB(0, 120, 215).
const KEY_HELD: u32 = 0x00D7_7800;
const KEY_TEXT: u32 = 0x00F0_F0F0;
/// The handle: light grey, 30% opaque. A placeholder until themes (v0.4)
/// give it a colour of their own.
const HANDLE_COLOR: u32 = 0x00C8_C8C8;
const HANDLE_OPACITY: u8 = 77;

/// One key as the panel needs it.
struct PanelKey {
    label: Vec<u16>,
    /// In keys along the edge.
    width: usize,
    /// Where it sits inside the panel; none while it does not fit.
    rect: Option<Rect>,
    sends: Sends,
}

/// What a key sends, in virtual keys.
enum Sends {
    /// Down while pressed, up on release.
    Hold(u16),
    /// The whole combination on release: modifiers, then the key.
    Shortcut { mods: Vec<u16>, key: Option<u16> },
    /// Kinds that send nothing yet.
    Nothing,
}

impl Sends {
    fn of(action: &Action) -> Sends {
        match action {
            Action::Modifier(key) | Action::Hold(key) => Sends::Hold(send::vk(*key)),
            Action::Shortcut(combo) => {
                let order = [(Mods::CTRL, Key::Ctrl), (Mods::SHIFT, Key::Shift), (Mods::ALT, Key::Alt), (Mods::WIN, Key::Win)];
                let mods = order.iter().filter(|(m, _)| combo.mods.contains(*m)).map(|&(_, k)| send::vk(k)).collect();
                Sends::Shortcut { mods, key: combo.key.map(send::vk) }
            }
            _ => Sends::Nothing,
        }
    }
}

struct Panel {
    hwnd: HWND,
    edge: Edge,
    monitor: Monitor,
    lines: usize,
    /// Side of a key in logical pixels.
    key_size: u32,
    /// The panel on screen, and the side of a key, in pixels at the current DPI.
    rect: Rect,
    key: i32,
    keys: Vec<PanelKey>,
    presses: Presses,
    /// The mouse pointer that holds the window's capture, if any.
    captured: Option<u32>,
    /// Where each pointer down on the panel started.
    starts: Vec<Start>,
    handle: Option<Handle>,
    /// The handle's window; null with `handle = none`.
    handle_hwnd: HWND,
}

/// Where and when a pointer went down, in screen pixels.
#[derive(Clone, Copy)]
struct Start {
    hwnd: HWND,
    id: u32,
    x: i32,
    y: i32,
    at: Instant,
}

/// A flick that hides a panel is over within this time. A finger that moves
/// more slowly is a hand drifting while the pen draws (spec 6.1): the keys it
/// holds stay held.
const FLICK_TIME: Duration = Duration::from_millis(300);

impl Panel {
    /// How far a pointer on the handle goes before it is a swipe: half a key.
    fn swipe_distance(&self) -> i32 {
        (self.key / 2).max(1)
    }

    /// How far a flick on the panel goes before it hides it: a whole key.
    fn flick_distance(&self) -> i32 {
        self.key.max(1)
    }
}

thread_local! {
    // The UI thread owns the panels; the window procedure reaches them here.
    static PANELS: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
    // How many keys, over all panels, hold each virtual key down. The same
    // key may sit on several edges; it goes down with the first and up with
    // the last.
    static HOLDS: RefCell<[u8; 256]> = const { RefCell::new([0; 256]) };
    // The edges hidden from the tray, and whether EdgePad is paused. Kept
    // across rebuilds, never written to disk.
    static HIDDEN: Cell<[bool; 4]> = const { Cell::new([false; 4]) };
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    // Each pointer down on a handle, with that handle's window.
    static HANDLE_DRAGS: RefCell<Vec<Start>> = const { RefCell::new(Vec::new()) };
}

/// Whether the panel on `edge` should be on screen.
fn visible(edge: Edge) -> bool {
    !PAUSED.get() && !HIDDEN.get()[edge as usize]
}

/// Whether the handle on `edge` should be on screen: while its panel is
/// hidden, but not while paused.
fn handle_visible(edge: Edge) -> bool {
    !PAUSED.get() && HIDDEN.get()[edge as usize]
}

/// The handle's window: plain colour, see-through by its layered alpha.
unsafe fn create_handle() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(handle_proc);
        wc.hInstance = instance;
        wc.hCursor = LoadCursorW(null_mut(), IDC_HAND);
        wc.hbrBackground = CreateSolidBrush(HANDLE_COLOR);
        wc.lpszClassName = w!("EdgePadHandle");
        // Every handle after the first finds the class already there; that
        // brush is then never used. Any other failure leaves no handle.
        if RegisterClassW(&wc) == 0 {
            DeleteObject(wc.hbrBackground);
            if GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return null_mut();
            }
        }
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            w!("EdgePadHandle"),
            w!("EdgePad"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if !hwnd.is_null() {
            SetLayeredWindowAttributes(hwnd, 0, HANDLE_OPACITY, LWA_ALPHA);
        }
        hwnd
    }
}

/// Create and show the panel `p`, keys `key_size` logical pixels square at
/// the DPI of the monitor it goes on. Keys that do not fit are left out; with
/// none left, no panel is made. Returns false if the window could not be
/// created.
pub unsafe fn create(p: &EdgePanel, key_size: u32) -> bool {
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = instance;
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.lpszClassName = w!("EdgePadPanel");
        // Fails harmlessly for every panel after the first.
        RegisterClassW(&wc);

        // The first draw gives the window its real place and size.
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            wc.lpszClassName,
            w!("EdgePad"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return false;
        }
        let keys: Vec<PanelKey> = p
            .keys
            .iter()
            .map(|k| PanelKey {
                label: k.label.encode_utf16().collect(),
                width: k.width as usize,
                rect: None,
                sends: Sends::of(&k.action),
            })
            .collect();
        let presses = Presses::new(keys.len());
        let rect = Rect { x: 0, y: 0, w: 0, h: 0 };
        let (edge, monitor, lines) = (p.edge, p.monitor, p.lines);
        let handle_hwnd = if p.handle.is_some() { create_handle() } else { null_mut() };
        let mut panel = Panel {
            hwnd,
            edge,
            monitor,
            lines,
            key_size,
            rect,
            key: 0,
            keys,
            presses,
            captured: None,
            starts: Vec::new(),
            handle: p.handle,
            handle_hwnd,
        };
        lay_out(&mut panel, None);
        let fits = panel.keys.iter().any(|k| k.rect.is_some());
        // A handle that could not be made leaves the edge without one; the
        // panel itself still goes up.
        if handle_hwnd.is_null() {
            panel.handle = None;
        }
        if !fits || !draw(&panel) {
            DestroyWindow(hwnd);
            if !handle_hwnd.is_null() {
                DestroyWindow(handle_hwnd);
            }
            // Nothing fitting is no failure: there is just no panel.
            return !fits;
        }
        let (show, show_handle) = (visible(edge), handle_visible(edge));
        PANELS.with_borrow_mut(|p| p.push(panel));
        if show {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        if show_handle && !handle_hwnd.is_null() {
            ShowWindow(handle_hwnd, SW_SHOWNOACTIVATE);
        }
        true
    }
}

/// Replace every panel with those of `panels`. Keys held on the old panels
/// are released first, so none is left down with no panel to lift it.
/// Returns false if a window could not be created.
pub unsafe fn rebuild(panels: &Panels) -> bool {
    unsafe {
        // Taken out first: messages sent while a window is destroyed (capture
        // lost, for one) then find no panel and change nothing.
        for panel in PANELS.take() {
            DestroyWindow(panel.hwnd);
            if !panel.handle_hwnd.is_null() {
                DestroyWindow(panel.handle_hwnd);
            }
        }
        let held = HOLDS.replace([0; 256]);
        for (vk, &n) in held.iter().enumerate() {
            if n > 0 {
                send::key(vk as u16, false);
            }
        }
        // Every edge is tried, even after one fails.
        panels.edges.iter().fold(true, |ok, p| create(p, panels.key_size) && ok)
    }
}

/// For each edge (Edge as index): `None` without a panel, else whether it is
/// not hidden from the tray.
pub fn edges() -> [Option<bool>; 4] {
    let mut edges = [None; 4];
    let hidden = HIDDEN.get();
    PANELS.with_borrow(|panels| {
        for p in panels {
            edges[p.edge as usize] = Some(!hidden[p.edge as usize]);
        }
    });
    edges
}

pub fn is_paused() -> bool {
    PAUSED.get()
}

/// Hide the panel on `edge`, or show it again.
pub unsafe fn toggle_edge(edge: Edge) {
    let mut hidden = HIDDEN.get();
    hidden[edge as usize] ^= true;
    HIDDEN.set(hidden);
    unsafe { apply_visibility() };
}

/// Hide the panel on `edge` (true) or show it (false).
unsafe fn set_hidden(edge: Edge, hide: bool) {
    let mut hidden = HIDDEN.get();
    hidden[edge as usize] = hide;
    HIDDEN.set(hidden);
    unsafe { apply_visibility() };
}

/// Pause (hide every panel) or resume, back to the edges shown before.
pub unsafe fn toggle_pause() {
    PAUSED.set(!PAUSED.get());
    unsafe { apply_visibility() };
}

/// A second launch: hide every edge if any is shown, else show them all.
/// Paused, nothing changes.
pub unsafe fn toggle_all() {
    if PAUSED.get() {
        return;
    }
    let any = PANELS.with_borrow(|panels| panels.iter().any(|p| visible(p.edge)));
    HIDDEN.set([any; 4]);
    unsafe { apply_visibility() };
}

/// Show or hide each panel and handle as `visible` and `handle_visible`
/// say. A panel going away lets go of the keys held on it first.
unsafe fn apply_visibility() {
    PANELS.with_borrow_mut(|panels| unsafe {
        for panel in panels {
            let show = visible(panel.edge);
            if show != (IsWindowVisible(panel.hwnd) != 0) {
                if show {
                    ShowWindow(panel.hwnd, SW_SHOWNOACTIVATE);
                } else {
                    let_go(panel);
                    ShowWindow(panel.hwnd, SW_HIDE);
                }
            }
            let handle = handle_visible(panel.edge);
            if !panel.handle_hwnd.is_null() && handle != (IsWindowVisible(panel.handle_hwnd) != 0) {
                ShowWindow(panel.handle_hwnd, if handle { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
    });
}

/// Release the keys held on `panel` and forget its pointers. A shortcut
/// still under a finger is dropped, not sent.
unsafe fn let_go(panel: &mut Panel) {
    unsafe {
        for (i, key) in panel.keys.iter().enumerate() {
            if let (true, Sends::Hold(vk)) = (panel.presses.is_down(i), &key.sends) {
                hold(*vk, false);
            }
        }
        panel.presses = Presses::new(panel.keys.len());
        panel.starts.clear();
        if panel.captured.take().is_some() {
            ReleaseCapture();
        }
        draw(panel);
    }
}

/// Let go of every key EdgePad holds, from any panel (the tray's 全キー解放).
pub unsafe fn release_all() {
    unsafe {
        PANELS.with_borrow_mut(|panels| {
            for panel in panels {
                let_go(panel);
            }
        });
        // Anything still recorded (a key-up that failed) goes too.
        HOLDS.replace([0; 256]);
        send::release_all();
    }
}

/// Place the panel and its keys: pick its monitor afresh and size the keys
/// for `dpi`, or for that monitor's DPI when none is given. What is held
/// stays held.
unsafe fn lay_out(panel: &mut Panel, dpi: Option<u32>) {
    unsafe {
        let (monitors, primary) = monitors();
        if monitors.is_empty() {
            return;
        }
        let mut cursor = POINT { x: 0, y: 0 };
        if panel.monitor == Monitor::Cursor {
            GetCursorPos(&mut cursor);
        }
        let rects: Vec<Rect> = monitors.iter().map(|&(_, r)| r).collect();
        let (handle, screen) = monitors[layout::pick_monitor(&rects, primary, (cursor.x, cursor.y), panel.monitor)];
        let dpi = dpi.unwrap_or_else(|| {
            let (mut x, mut y) = (0, 0);
            match GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut x, &mut y) {
                0 => x,
                _ => 96,
            }
        });
        panel.key = layout::scale(panel.key_size as i32, dpi);
        let widths: Vec<usize> = panel.keys.iter().map(|k| k.width).collect();
        let (rect, rects) = layout::edge_panel(screen, panel.edge, panel.key, panel.lines, &widths);
        panel.rect = rect;
        let mut rects = rects.into_iter();
        for key in &mut panel.keys {
            key.rect = rects.next();
        }
        if let (Some(h), false) = (panel.handle, panel.handle_hwnd.is_null()) {
            let r = layout::handle(screen, panel.edge, layout::scale(h.width as i32, dpi), h.length, h.position);
            SetWindowPos(panel.handle_hwnd, HWND_TOPMOST, r.x, r.y, r.w, r.h, SWP_NOACTIVATE);
        }
    }
}

unsafe extern "system" fn gather(monitor: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
    unsafe { (*(out as *mut Vec<HMONITOR>)).push(monitor) };
    1
}

/// Every monitor with its rectangle, and the index of the primary one.
unsafe fn monitors() -> (Vec<(HMONITOR, Rect)>, usize) {
    unsafe {
        let mut handles: Vec<HMONITOR> = Vec::new();
        EnumDisplayMonitors(null_mut(), null(), Some(gather), &mut handles as *mut _ as LPARAM);
        let mut primary = 0;
        let mut found = Vec::new();
        for handle in handles {
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(handle, &mut info) == 0 {
                continue;
            }
            if info.dwFlags & MONITORINFOF_PRIMARY != 0 {
                primary = found.len();
            }
            let m = info.rcMonitor;
            found.push((handle, Rect { x: m.left, y: m.top, w: m.right - m.left, h: m.bottom - m.top }));
        }
        (found, primary)
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            // Pointer messages are handled here and never passed on, so touch
            // is not promoted to mouse input.
            WM_POINTERDOWN => {
                pointer_down(hwnd, wp, lp);
                0
            }
            WM_POINTERUP | WM_POINTERCAPTURECHANGED => {
                pointer_up(hwnd, wp);
                0
            }
            WM_POINTERUPDATE => {
                pointer_update(hwnd, wp, lp);
                0
            }
            // The scale of the panel's monitor changed. The suggested
            // rectangle in lParam is ignored: a panel keeps to its edge, so
            // its place is worked out again from scratch.
            WM_DPICHANGED => {
                let dpi = (wp >> 16 & 0xFFFF) as u32;
                // Windows never sends 0, but any process can send this
                // message, and a zero key size would divide by zero.
                if dpi == 0 {
                    return 0;
                }
                relay_out(hwnd, msg, wp, Some(dpi));
                0
            }
            // Monitors were added, removed, moved or resized: every panel
            // picks its monitor again, at that monitor's DPI.
            WM_DISPLAYCHANGE => {
                relay_out(hwnd, msg, wp, None);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn pointer_id(wp: WPARAM) -> u32 {
    (wp & 0xFFFF) as u32
}

/// The screen position a pointer message carries.
fn pointer_at(lp: LPARAM) -> (i32, i32) {
    ((lp & 0xFFFF) as i16 as i32, (lp >> 16 & 0xFFFF) as i16 as i32)
}

/// Touch or mouse, not pen (pens press nothing), as the pointer's kind.
unsafe fn finger_or_mouse(id: u32) -> Option<POINTER_INPUT_TYPE> {
    let mut kind = 0;
    match unsafe { GetPointerType(id, &mut kind) } {
        0 => None,
        _ if kind == PT_PEN => None,
        _ => Some(kind),
    }
}

/// A pointer moved on a panel: a quick flick toward the screen edge hides the
/// panel, letting go of the key it pressed.
unsafe fn pointer_update(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
    let (id, (x, y)) = (pointer_id(wp), pointer_at(lp));
    let mut hide = None;
    with_panel(hwnd, |panel| {
        if let Some(s) = panel.starts.iter().find(|s| s.id == id && s.at.elapsed() <= FLICK_TIME) {
            if gesture::swipe(panel.edge, x - s.x, y - s.y, panel.flick_distance()) == Some(Swipe::Outward) {
                hide = Some(panel.edge);
            }
        }
    });
    // Outside the borrow: hiding goes over every panel.
    if let Some(edge) = hide {
        unsafe { set_hidden(edge, true) };
    }
}

unsafe extern "system" fn handle_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            WM_POINTERDOWN => {
                let id = pointer_id(wp);
                if let Some(kind) = finger_or_mouse(id) {
                    let (x, y) = pointer_at(lp);
                    HANDLE_DRAGS.with_borrow_mut(|drags| {
                        drags.retain(|d| d.id != id);
                        drags.push(Start { hwnd, id, x, y, at: Instant::now() });
                    });
                    if kind == PT_MOUSE {
                        // The handle is thin: keep the mouse once it leaves.
                        SetCapture(hwnd);
                    }
                }
                0
            }
            WM_POINTERUPDATE | WM_POINTERUP | WM_POINTERCAPTURECHANGED => {
                handle_pointer(hwnd, msg, pointer_id(wp), pointer_at(lp));
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// A pointer that went down on handle `hwnd` moved, lifted or was lost. A
/// swipe inward, or a tap, shows the panel.
unsafe fn handle_pointer(hwnd: HWND, msg: u32, id: u32, (x, y): (i32, i32)) {
    let Some(Start { x: x0, y: y0, .. }) =
        HANDLE_DRAGS.with_borrow(|drags| drags.iter().find(|d| d.hwnd == hwnd && d.id == id).copied())
    else {
        return;
    };
    let Some((edge, distance)) = PANELS.with(|panels| {
        let panels = panels.try_borrow().ok()?;
        panels.iter().find(|p| p.handle_hwnd == hwnd).map(|p| (p.edge, p.swipe_distance()))
    }) else {
        return;
    };
    let show = match msg {
        WM_POINTERUPDATE => gesture::swipe(edge, x - x0, y - y0, distance) == Some(Swipe::Inward),
        WM_POINTERUP => gesture::is_tap(x - x0, y - y0, distance),
        // Capture lost: the gesture ends with nothing.
        _ => false,
    };
    if msg != WM_POINTERUPDATE || show {
        HANDLE_DRAGS.with_borrow_mut(|drags| drags.retain(|d| d.id != id));
        if unsafe { GetCapture() } == hwnd {
            unsafe { ReleaseCapture() };
        }
    }
    if show {
        unsafe { set_hidden(edge, false) };
    }
}

/// Run `f` on the panel of window `hwnd`, if it is one of ours. Returns false
/// when the panels are already in use further up the stack, and `f` did not
/// run: moving a panel onto a monitor of another DPI makes Windows send
/// WM_DPICHANGED from inside the draw that moved it.
fn with_panel(hwnd: HWND, f: impl FnOnce(&mut Panel)) -> bool {
    PANELS.with(|panels| {
        let Ok(mut panels) = panels.try_borrow_mut() else { return false };
        if let Some(panel) = panels.iter_mut().find(|p| p.hwnd == hwnd) {
            f(panel);
        }
        true
    })
}

/// Lay the panel of `hwnd` out again and redraw it. When that cannot happen
/// now (see `with_panel`), `msg` is queued again to be handled once the
/// current work is done, so no change of monitors or scale is lost.
unsafe fn relay_out(hwnd: HWND, msg: u32, wp: WPARAM, dpi: Option<u32>) {
    let done = with_panel(hwnd, |panel| unsafe {
        lay_out(panel, dpi);
        draw(panel);
    });
    if !done {
        // lParam is not read by either message's handler.
        unsafe { PostMessageW(hwnd, msg, wp, 0) };
    }
}

/// Press or release `vk` for one key. Only the first press and the last
/// release over all panels reach SendInput.
unsafe fn hold(vk: u16, down: bool) {
    let send = HOLDS.with_borrow_mut(|holds| {
        let n = &mut holds[vk as usize & 0xFF];
        if down {
            *n += 1;
            *n == 1
        } else {
            *n = n.saturating_sub(1);
            *n == 0
        }
    });
    if send {
        unsafe { send::key(vk, down) };
    }
}

/// A pointer went down: press the key under it, unless it is a pen. The key
/// stays held by this pointer until it lifts, wherever it moves.
unsafe fn pointer_down(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
    unsafe {
        let id = pointer_id(wp);
        let Some(kind) = finger_or_mouse(id) else { return };
        let (sx, sy) = pointer_at(lp);
        with_panel(hwnd, |panel| {
            panel.starts.retain(|s| s.id != id);
            panel.starts.push(Start { hwnd, id, x: sx, y: sy, at: Instant::now() });
            if kind == PT_MOUSE {
                // Keep receiving moves and the button-up after leaving the
                // panel, key or not: a swipe to hide runs off it.
                SetCapture(hwnd);
                panel.captured = Some(id);
            }
            let (x, y) = (sx - panel.rect.x, sy - panel.rect.y);
            let Some(key) = panel.keys.iter().position(|k| k.rect.is_some_and(|r| r.contains(x, y))) else {
                return;
            };
            if let Some(key) = panel.presses.down(id, key) {
                // Send first, draw after: the key reaches the app sooner.
                if let Sends::Hold(vk) = panel.keys[key].sends {
                    hold(vk, true);
                }
                draw(panel);
            }
        });
    }
}

/// A pointer lifted or was cancelled: release its key if nobody else holds it.
unsafe fn pointer_up(hwnd: HWND, wp: WPARAM) {
    unsafe {
        let id = pointer_id(wp);
        with_panel(hwnd, |panel| {
            panel.starts.retain(|s| s.id != id);
            if let Some(key) = panel.presses.up(id) {
                match &panel.keys[key].sends {
                    Sends::Hold(vk) => hold(*vk, false),
                    Sends::Shortcut { mods, key } => send::shortcut(mods, *key),
                    Sends::Nothing => {}
                }
                draw(panel);
            }
            // Only the mouse that took the capture gives it back; a finger
            // lifting meanwhile must not cut the mouse off from its button-up.
            if panel.captured == Some(id) {
                panel.captured = None;
                ReleaseCapture();
            }
        });
    }
}

/// Paint the keys into a 32-bit DIB and hand it to the window with
/// UpdateLayeredWindow. The DIB is freed right after; the window keeps its
/// own copy, so nothing is held between redraws. Returns false on failure.
unsafe fn draw(panel: &Panel) -> bool {
    let (hwnd, r, key) = (panel.hwnd, panel.rect, panel.key);
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = r.w;
        bmi.bmiHeader.biHeight = -r.h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        let mut bits = null_mut();
        let dib = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if dib.is_null() || bits.is_null() {
            DeleteDC(mem);
            ReleaseDC(null_mut(), screen);
            return false;
        }
        let old_bitmap = SelectObject(mem, dib);

        let background = CreateSolidBrush(PANEL_BG);
        FillRect(mem, &RECT { left: 0, top: 0, right: r.w, bottom: r.h }, background);
        DeleteObject(background);

        let face = CreateSolidBrush(KEY_FACE);
        let held = CreateSolidBrush(KEY_HELD);
        let font = CreateFontW(
            -(key * 2 / 7),
            0,
            0,
            0,
            FW_SEMIBOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            w!("Segoe UI"),
        );
        let old_brush = SelectObject(mem, face);
        let old_pen = SelectObject(mem, GetStockObject(NULL_PEN));
        let old_font = SelectObject(mem, font);
        SetBkMode(mem, TRANSPARENT as i32);
        SetTextColor(mem, KEY_TEXT);

        let gap = (key / 16).max(2);
        let corner = key / 6;
        for (i, k) in panel.keys.iter().enumerate() {
            let Some(at) = k.rect else { continue };
            let mut cell = RECT { left: at.x + gap, top: at.y + gap, right: at.x + at.w - gap, bottom: at.y + at.h - gap };
            SelectObject(mem, if panel.presses.is_down(i) { held } else { face });
            RoundRect(mem, cell.left, cell.top, cell.right + 1, cell.bottom + 1, corner, corner);
            DrawTextW(mem, k.label.as_ptr(), k.label.len() as i32, &mut cell, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }

        SelectObject(mem, old_font);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        DeleteObject(font);
        DeleteObject(face);
        DeleteObject(held);

        // GDI leaves alpha at 0; make every pixel opaque and let the constant
        // alpha below give the panel its 90%.
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (r.w * r.h) as usize);
        for p in pixels {
            *p |= 0xFF00_0000;
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: OPACITY,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let shown = UpdateLayeredWindow(
            hwnd,
            screen,
            &POINT { x: r.x, y: r.y },
            &SIZE { cx: r.w, cy: r.h },
            mem,
            &POINT { x: 0, y: 0 },
            0,
            &blend,
            ULW_ALPHA,
        ) != 0;

        SelectObject(mem, old_bitmap);
        DeleteObject(dib);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        shown
    }
}

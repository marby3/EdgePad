//! The edge panel window: a layered, topmost, never-activated popup.

use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use edgepad_core::config::{Action, Edge, Monitor};
use edgepad_core::keys::{Key, Mods};
use edgepad_core::layout::{self, Rect};
use edgepad_core::press::Presses;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
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
}

thread_local! {
    // The UI thread owns the panels; the window procedure reaches them here.
    static PANELS: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
    // How many keys, over all panels, hold each virtual key down. The same
    // key may sit on several edges; it goes down with the first and up with
    // the last.
    static HOLDS: RefCell<[u8; 256]> = const { RefCell::new([0; 256]) };
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
        let mut panel =
            Panel { hwnd, edge, monitor, lines, key_size, rect, key: 0, keys, presses, captured: None };
        lay_out(&mut panel, None);
        if panel.keys.iter().all(|k| k.rect.is_none()) {
            DestroyWindow(hwnd);
            return true;
        }
        if !draw(&panel) {
            DestroyWindow(hwnd);
            return false;
        }
        PANELS.with_borrow_mut(|p| p.push(panel));
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
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
            WM_POINTERUPDATE => 0,
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
        let mut kind = 0;
        if GetPointerType(id, &mut kind) == 0 || kind == PT_PEN {
            return;
        }
        let (x, y) = ((lp & 0xFFFF) as i16 as i32, (lp >> 16 & 0xFFFF) as i16 as i32);
        with_panel(hwnd, |panel| {
            let (x, y) = (x - panel.rect.x, y - panel.rect.y);
            let Some(key) = panel.keys.iter().position(|k| k.rect.is_some_and(|r| r.contains(x, y))) else {
                return;
            };
            if kind == PT_MOUSE {
                // Keep receiving the button-up even after leaving the panel.
                SetCapture(hwnd);
                panel.captured = Some(id);
            }
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

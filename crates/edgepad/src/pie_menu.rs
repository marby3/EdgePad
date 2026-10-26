//! The pie menu on screen (spec 4.6, 5.3 and 5.6): a layered, topmost,
//! never-activated window around where a pie key was pressed: a half ring
//! of sectors, one per item, with the middle left open. It is made when the
//! key goes down and destroyed once it closes: when the key comes up, or,
//! after a tap left it open, when an item or its middle is tapped. Nothing
//! of it is kept while it is closed.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use edgepad_core::config::{Edge, PIE_ITEMS};
use edgepad_core::pie;
use edgepad_core::theme::Palette;
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::colors::colorref;
use crate::log::debug_log;
use crate::panel::{self, finger_or_mouse, key_font, key_gap, pointer_at, pointer_id};
use crate::tray;

/// The menu open now.
struct Menu {
    hwnd: HWND,
    /// The panel whose pie key opened it, and the pointer holding that key:
    /// none once a tap has left the menu open.
    panel: HWND,
    pointer: Option<u32>,
    /// The key set whose keys are its items.
    set: String,
    /// Whether the pointer holding the key has been out of the middle: let
    /// go without, it was a tap.
    left_middle: bool,
    /// A pointer down on the menu left open, which runs the item it lifts on.
    tap: Option<u32>,
    edge: Edge,
    /// Where the key was pressed, in screen pixels: the middle of the menu.
    at: (i32, i32),
    key: i32,
    labels: Vec<Vec<u16>>,
    ring: pie::Ring,
    pointing: Option<usize>,
    palette: Palette,
    alpha: u8,
}

thread_local! {
    // One finger opens one menu; the UI thread owns it.
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
    // A menu closed by a tap on it, still on screen until the finger that
    // tapped is done with it (see `linger`), and that pointer.
    static LINGERING: Cell<Option<(HWND, u32)>> = const { Cell::new(None) };
}

/// The longest a closed menu lingers for a pointer that never says it left.
const LINGER_MS: u32 = 300;
const LINGER_TIMER: usize = 1;

/// What a pie key opens its menu with.
pub struct Opening<'a> {
    pub panel: HWND,
    pub pointer: u32,
    pub edge: Edge,
    pub at: (i32, i32),
    /// Side of a key in pixels at the panel's DPI.
    pub key: i32,
    pub set: &'a str,
    pub labels: &'a [&'a str],
    pub palette: Palette,
    /// The constant alpha the panel is drawn with now.
    pub alpha: u8,
}

/// Open the menu of a pie key just pressed, in place of any other.
pub unsafe fn open(o: Opening) {
    unsafe {
        close_all();
        // The parser allows nothing else; drawn, any other count would not
        // make a menu.
        if !PIE_ITEMS.contains(&o.labels.len()) {
            debug_log!("パイメニューの項目が {} 個: 開かない", o.labels.len());
            return;
        }
        let instance = GetModuleHandleW(null());
        if !REGISTERED.get() {
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(wnd_proc);
            wc.hInstance = instance;
            wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
            wc.lpszClassName = w!("EdgePadPie");
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                debug_log!("パイメニューの窓クラスを登録できない");
                return;
            }
            REGISTERED.set(true);
        }
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            w!("EdgePadPie"),
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
            debug_log!("パイメニューの窓を作れない");
            return;
        }
        let menu = Menu {
            hwnd,
            panel: o.panel,
            pointer: Some(o.pointer),
            set: o.set.into(),
            left_middle: false,
            tap: None,
            edge: o.edge,
            at: o.at,
            key: o.key,
            labels: o.labels.iter().map(|l| l.encode_utf16().collect()).collect(),
            ring: pie::Ring::new(o.labels.len(), o.key),
            pointing: None,
            palette: o.palette,
            alpha: o.alpha,
        };
        if !draw(&menu) {
            DestroyWindow(hwnd);
            return;
        }
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        debug_log!("パイメニューを開く: {} 項目", menu.labels.len());
        let panel = menu.panel;
        MENU.set(Some(menu));
        redraw_panels(panel);
    }
}

impl Menu {
    /// The item `(x, y)` on screen points at.
    fn item_at(&self, (x, y): (i32, i32)) -> Option<usize> {
        pie::item(self.edge, x - self.at.0, y - self.at.1, self.key, self.labels.len())
    }

    /// Whether `(x, y)` on screen is in the middle, where letting go cancels.
    fn in_middle(&self, (x, y): (i32, i32)) -> bool {
        pie::in_middle(x - self.at.0, y - self.at.1, self.key)
    }

    /// Light `pointing` instead of the item lit now.
    unsafe fn light(&mut self, pointing: Option<usize>) {
        if pointing == self.pointing {
            return;
        }
        let was = std::mem::replace(&mut self.pointing, pointing);
        // Not drawn, it stays as it was: the next move tries again.
        if !unsafe { draw(self) } {
            debug_log!("パイメニューを描けない");
            self.pointing = was;
            return;
        }
        tray::test_event(&match pointing {
            Some(i) => format!("pie: point {i}"),
            None => "pie: point none".into(),
        });
    }
}

/// `pointer` moved to `(x, y)`: light the item it points at now.
pub unsafe fn point(pointer: u32, at: (i32, i32)) {
    MENU.with_borrow_mut(|menu| {
        let Some(m) = menu.as_mut().filter(|m| m.pointer == Some(pointer)) else { return };
        if !m.in_middle(at) {
            m.left_middle = true;
        }
        let pointing = m.item_at(at);
        unsafe { m.light(pointing) };
    });
}

/// Whether a menu is open: the panels are dimmed behind it.
pub fn is_open() -> bool {
    MENU.with_borrow(|menu| menu.is_some())
}

/// A menu opened or closed from `panel`: the panels are drawn again, dimmed
/// or not (`panel::REDRAW`).
fn redraw_panels(panel: HWND) {
    unsafe { PostMessageW(panel, panel::REDRAW, 0, 0) };
}

/// Whether `pointer` holds the menu open.
pub fn is_held_by(pointer: u32) -> bool {
    MENU.with_borrow(|menu| menu.as_ref().is_some_and(|m| m.pointer == Some(pointer)))
}

/// Whether a tap left a menu from `panel` open.
pub fn is_left_open(panel: HWND) -> bool {
    MENU.with_borrow(|menu| menu.as_ref().is_some_and(|m| m.panel == panel && m.pointer.is_none()))
}

/// Whether `(x, y)` on screen is in the middle of a menu from `panel`.
pub fn in_middle(panel: HWND, at: (i32, i32)) -> bool {
    MENU.with_borrow(|menu| menu.as_ref().is_some_and(|m| m.panel == panel && m.in_middle(at)))
}

/// What letting go of a pie key comes to.
pub enum Release {
    /// Run item `item` of key set `set`.
    Run { set: String, item: usize },
    Nothing,
}

/// `pointer` holding the pie key of `panel` lifted, or was taken away
/// (`cancelled`). On an item, it runs; in the middle or toward the edge,
/// nothing does; either way the menu closes. A tap, never out of the
/// middle, leaves it open. None when `pointer` held no menu.
pub unsafe fn release(panel: HWND, pointer: u32, cancelled: bool) -> Option<Release> {
    let tapped = MENU.with_borrow_mut(|menu| {
        let m = menu.as_mut().filter(|m| m.panel == panel && m.pointer == Some(pointer))?;
        let tapped = !cancelled && !m.left_middle;
        if tapped {
            m.pointer = None;
        }
        Some(tapped)
    })?;
    if tapped {
        debug_log!("パイメニューを開いたままにする");
        return Some(Release::Nothing);
    }
    let m = MENU.take()?;
    debug_log!("パイメニューを閉じる");
    unsafe { DestroyWindow(m.hwnd) };
    redraw_panels(m.panel);
    Some(match m.pointing.filter(|_| !cancelled) {
        Some(item) => Release::Run { set: m.set, item },
        None => Release::Nothing,
    })
}

/// Close the menu opened from `panel` by `pointer`, or whatever menu from
/// `panel` is open when `pointer` is none.
pub unsafe fn close(panel: HWND, pointer: Option<u32>) {
    let menu = MENU.with_borrow_mut(|menu| menu.take_if(|m| m.panel == panel && pointer.is_none_or(|p| m.pointer == Some(p))));
    if let Some(m) = menu {
        debug_log!("パイメニューを閉じる");
        unsafe { DestroyWindow(m.hwnd) };
        redraw_panels(m.panel);
    }
}

/// Close whatever menu is open.
pub unsafe fn close_all() {
    if let Some(m) = MENU.take() {
        unsafe { DestroyWindow(m.hwnd) };
        redraw_panels(m.panel);
    }
    unsafe { end_linger() };
}

/// Keep the window of a menu just closed by a tap on it until that pointer
/// has left it. Gone at once, the rest of the tap (its leaving, or the mouse
/// input Windows makes of it) would land on whatever window is under the
/// menu: a click on the app below.
unsafe fn linger(hwnd: HWND, pointer: u32) {
    unsafe {
        end_linger();
        LINGERING.set(Some((hwnd, pointer)));
        SetTimer(hwnd, LINGER_TIMER, LINGER_MS, None);
    }
}

/// Destroy the window of a menu closed by a tap, if one lingers.
unsafe fn end_linger() {
    if let Some((hwnd, _)) = LINGERING.take() {
        unsafe { DestroyWindow(hwnd) };
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            // Pointer messages are handled here and never passed on, so touch
            // is not promoted to mouse input. Only a menu left open by a tap
            // gets any: the finger holding a pie key is the panel's.
            WM_POINTERDOWN => {
                tap_down(pointer_id(wp), pointer_at(lp));
                0
            }
            WM_POINTERUP => {
                tap_up(pointer_id(wp), pointer_at(lp), (wp >> 16) as u32 & POINTER_MESSAGE_FLAG_CANCELED != 0);
                0
            }
            WM_POINTERCAPTURECHANGED => {
                tap_up(pointer_id(wp), (0, 0), true);
                0
            }
            // The tap that closed it is over.
            WM_POINTERLEAVE if LINGERING.get() == Some((hwnd, pointer_id(wp))) => {
                end_linger();
                0
            }
            WM_TIMER if wp == LINGER_TIMER => {
                KillTimer(hwnd, LINGER_TIMER);
                if LINGERING.get().is_some_and(|(h, _)| h == hwnd) {
                    end_linger();
                }
                0
            }
            WM_POINTERUPDATE | WM_POINTERENTER | WM_POINTERLEAVE => 0,
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// A finger or the mouse went down on a menu left open: light the item it
/// is on. Pens tap nothing.
unsafe fn tap_down(id: u32, at: (i32, i32)) {
    debug_log!("パイメニューをポインタ {id} {at:?} が押す");
    if unsafe { finger_or_mouse(id) }.is_none() {
        return;
    }
    MENU.with_borrow_mut(|menu| {
        let Some(m) = menu.as_mut().filter(|m| m.pointer.is_none()) else { return };
        m.tap = Some(id);
        let pointing = m.item_at(at);
        unsafe { m.light(pointing) };
    });
}

/// The pointer that went down on a menu left open lifted at `at`, or was
/// taken away (`cancelled`): on the item it went down on, run it and close
/// the menu.
unsafe fn tap_up(id: u32, at: (i32, i32), cancelled: bool) {
    debug_log!("パイメニューからポインタ {id} {at:?} が離れる（取消 {cancelled}）");
    let item = MENU.with_borrow_mut(|menu| {
        let m = menu.as_mut().filter(|m| m.tap == Some(id))?;
        m.tap = None;
        let item = m.item_at(at).filter(|&i| !cancelled && m.pointing == Some(i));
        if item.is_none() {
            unsafe { m.light(None) };
        }
        item
    });
    let Some(item) = item else { return };
    let Some(m) = MENU.take() else { return };
    debug_log!("パイメニューを閉じる");
    redraw_panels(m.panel);
    unsafe {
        linger(m.hwnd, id);
        panel::run_pie_item(m.panel, &m.set, item);
    }
}

/// Paint the ring into a 32-bit DIB just big enough for it and hand it to
/// the window with UpdateLayeredWindow, then free it. The ring is worked out
/// pixel by pixel, smooth at its edges (GDI's own pies are not); GDI writes
/// the labels on it. Outside the ring the menu is see-through. Returns false
/// on failure.
unsafe fn draw(m: &Menu) -> bool {
    let n = m.labels.len();
    let r = m.ring.outer.ceil() as i32 + 1;
    // The half the ring is in, from its middle.
    let (left, top, right, bottom) = match m.edge {
        Edge::Left => (-1, -r, r, r),
        Edge::Right => (-r, -r, 1, r),
        Edge::Top => (-r, -1, r, r),
        Edge::Bottom => (-r, -r, r, 1),
    };
    let (w, h) = (right - left, bottom - top);
    // As far apart as the keys of a panel.
    let gap = f64::from(key_gap(m.key));
    let cover = |x: i32, y: i32| {
        let (dx, dy) = (f64::from(x + left) + 0.5, f64::from(y + top) + 0.5);
        m.ring.cover(m.edge, n, gap, dx, dy)
    };
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h;
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
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        // A DIB pixel is 0xAARRGGBB.
        let rgb = |[r, g, b]: [u8; 3]| (r as u32) << 16 | (g as u32) << 8 | b as u32;

        // Each sector in its colour, edges and all; the coverage comes last.
        let c = m.palette;
        for y in 0..h {
            for x in 0..w {
                let (_, sector) = cover(x, y);
                let lit = m.pointing == Some(sector);
                pixels[(y * w + x) as usize] = rgb(if lit { c.held } else { c.face });
            }
        }

        // The labels, as the panel writes its keys'.
        let font = key_font(m.key);
        let old_font = SelectObject(mem, font);
        SetBkMode(mem, TRANSPARENT as i32);
        let half_width = ((m.ring.label_width(n) - 2.0 * gap) / 2.0).max(f64::from(m.key) / 2.0) as i32;
        let half_height = m.key / 2;
        for (i, (&(lx, ly), label)) in m.ring.labels(m.edge, n).iter().zip(&m.labels).enumerate() {
            let (x, y) = (lx - left, ly - top);
            let mut cell = RECT { left: x - half_width, top: y - half_height, right: x + half_width, bottom: y + half_height };
            SetTextColor(mem, colorref(if m.pointing == Some(i) { c.held_text } else { c.text }));
            DrawTextW(mem, label.as_ptr(), label.len() as i32, &mut cell, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
        SelectObject(mem, old_font);
        DeleteObject(font);
        GdiFlush();

        // Premultiplied by how much of each pixel the ring covers. The
        // constant alpha below gives the menu the panel's opacity.
        for y in 0..h {
            for x in 0..w {
                let p = &mut pixels[(y * w + x) as usize];
                let a = (cover(x, y).0 * 255.0).round() as u32;
                let scale = |shift: u32| ((*p >> shift & 0xFF) * a / 255) << shift;
                *p = a << 24 | scale(16) | scale(8) | scale(0);
            }
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: m.alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let at = POINT { x: m.at.0 + left, y: m.at.1 + top };
        let size = SIZE { cx: w, cy: h };
        let from = POINT { x: 0, y: 0 };
        let shown = UpdateLayeredWindow(m.hwnd, screen, &at, &size, mem, &from, 0, &blend, ULW_ALPHA) != 0;

        SelectObject(mem, old_bitmap);
        DeleteObject(dib);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        shown
    }
}

//! The pie menu on screen (spec 4.6, 5.3 and 5.6): a layered, topmost,
//! never-activated window of rounded buttons around where a pie key was
//! pressed. It is made when the key goes down and destroyed when it comes
//! up, so nothing of it is kept while it is closed.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use edgepad_core::config::Edge;
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
use crate::tray;

/// The menu open now.
struct Menu {
    hwnd: HWND,
    /// The panel whose pie key opened it, and the pointer holding that key.
    panel: HWND,
    pointer: u32,
    edge: Edge,
    /// Where the key was pressed, in screen pixels: the middle of the menu.
    at: (i32, i32),
    key: i32,
    labels: Vec<Vec<u16>>,
    /// The middle of each button, from `at`.
    buttons: Vec<(i32, i32)>,
    pointing: Option<usize>,
    palette: Palette,
    alpha: u8,
}

thread_local! {
    // One finger opens one menu; the UI thread owns it.
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
}

/// What a pie key opens its menu with.
pub struct Opening<'a> {
    pub panel: HWND,
    pub pointer: u32,
    pub edge: Edge,
    pub at: (i32, i32),
    /// Side of a key in pixels at the panel's DPI.
    pub key: i32,
    pub labels: &'a [&'a str],
    pub palette: Palette,
    /// The constant alpha the panel is drawn with now.
    pub alpha: u8,
}

/// Open the menu of a pie key just pressed, in place of any other.
pub unsafe fn open(o: Opening) {
    unsafe {
        close_all();
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
            pointer: o.pointer,
            edge: o.edge,
            at: o.at,
            key: o.key,
            labels: o.labels.iter().map(|l| l.encode_utf16().collect()).collect(),
            buttons: pie::buttons(o.edge, o.labels.len(), o.key),
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
        MENU.set(Some(menu));
    }
}

/// `pointer` moved to `(x, y)`: light the item it points at now.
pub unsafe fn point(pointer: u32, (x, y): (i32, i32)) {
    MENU.with_borrow_mut(|menu| {
        let Some(m) = menu.as_mut().filter(|m| m.pointer == pointer) else { return };
        let pointing = pie::item(m.edge, x - m.at.0, y - m.at.1, m.key, m.labels.len());
        if pointing == m.pointing {
            return;
        }
        let was = std::mem::replace(&mut m.pointing, pointing);
        // Not drawn, it stays as it was: the next move tries again.
        if !unsafe { draw(m) } {
            debug_log!("パイメニューを描けない");
            m.pointing = was;
            return;
        }
        tray::test_event(&match pointing {
            Some(i) => format!("pie: point {i}"),
            None => "pie: point none".into(),
        });
    });
}

/// Whether `pointer` holds the menu open.
pub fn is_held_by(pointer: u32) -> bool {
    MENU.with_borrow(|menu| menu.as_ref().is_some_and(|m| m.pointer == pointer))
}

/// Close the menu opened from `panel` by `pointer`, or by any pointer when
/// `pointer` is none.
pub unsafe fn close(panel: HWND, pointer: Option<u32>) {
    let menu = MENU.with_borrow_mut(|menu| menu.take_if(|m| m.panel == panel && pointer.is_none_or(|p| p == m.pointer)));
    if let Some(m) = menu {
        debug_log!("パイメニューを閉じる");
        unsafe { DestroyWindow(m.hwnd) };
    }
}

/// Close whatever menu is open.
pub unsafe fn close_all() {
    if let Some(m) = MENU.take() {
        unsafe { DestroyWindow(m.hwnd) };
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// Paint the buttons into a 32-bit DIB just big enough for them and hand it
/// to the window with UpdateLayeredWindow, then free it. Between and around
/// the buttons the menu is see-through. Returns false on failure.
unsafe fn draw(m: &Menu) -> bool {
    let half = m.key / 2;
    let left = m.buttons.iter().map(|b| b.0).min().unwrap_or(0) - half;
    let top = m.buttons.iter().map(|b| b.1).min().unwrap_or(0) - half;
    let right = m.buttons.iter().map(|b| b.0).max().unwrap_or(0) + half;
    let bottom = m.buttons.iter().map(|b| b.1).max().unwrap_or(0) + half;
    let (w, h) = (right - left, bottom - top);
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

        // Filled with a colour no button is drawn in, which then turns
        // see-through: GDI draws with no alpha of its own.
        let c = m.palette;
        let clear = [[0x01, 0x02, 0x03], [0x03, 0x02, 0x01]]
            .into_iter()
            .find(|k| ![c.face, c.text, c.held, c.held_text].contains(k))
            .unwrap_or([0x01, 0x02, 0x03]);
        let clear = colorref(clear);
        let backdrop = CreateSolidBrush(clear);
        FillRect(mem, &RECT { left: 0, top: 0, right: w, bottom: h }, backdrop);
        DeleteObject(backdrop);

        let face = CreateSolidBrush(colorref(c.face));
        let held = CreateSolidBrush(colorref(c.held));
        let font = CreateFontW(
            -(m.key * 2 / 7),
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

        // As the panel draws its keys.
        let gap = (m.key / 16).max(2);
        let corner = m.key / 6;
        for (i, (&(bx, by), label)) in m.buttons.iter().zip(&m.labels).enumerate() {
            let (x, y) = (bx - left - half, by - top - half);
            let mut cell = RECT { left: x + gap, top: y + gap, right: x + m.key - gap, bottom: y + m.key - gap };
            let (brush, text) = if m.pointing == Some(i) { (held, c.held_text) } else { (face, c.text) };
            SelectObject(mem, brush);
            SetTextColor(mem, colorref(text));
            RoundRect(mem, cell.left, cell.top, cell.right + 1, cell.bottom + 1, corner, corner);
            DrawTextW(mem, label.as_ptr(), label.len() as i32, &mut cell, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }

        SelectObject(mem, old_font);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        DeleteObject(font);
        DeleteObject(face);
        DeleteObject(held);

        // The buttons opaque, the rest clear (premultiplied: all zero). The
        // constant alpha below gives the menu the panel's opacity.
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        // The DIB holds 0x00RRGGBB; a COLORREF is 0x00BBGGRR.
        let clear = (clear & 0xFF) << 16 | (clear & 0xFF00) | (clear >> 16 & 0xFF);
        for p in pixels {
            *p = if *p & 0x00FF_FFFF == clear { 0 } else { *p | 0xFF00_0000 };
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

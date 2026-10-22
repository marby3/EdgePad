//! The edge panel window: a layered, topmost, never-activated popup.

use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use edgepad_core::keys::M0_LABELS;
use edgepad_core::layout::{self, Rect};
use edgepad_core::press::Presses;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_SPACE,
};
use windows_sys::Win32::UI::Input::Pointer::GetPointerType;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::send;

/// Opacity of the shown panel: 90%.
const OPACITY: u8 = 230;
const PANEL_BG: u32 = 0x0020_2020;
const KEY_FACE: u32 = 0x003A_3A3A;
/// Face of a held key: accent blue, RGB(0, 120, 215).
const KEY_HELD: u32 = 0x00D7_7800;
const KEY_TEXT: u32 = 0x00F0_F0F0;

/// Virtual keys of `M0_LABELS`, in the same order.
const M0_VKS: [u16; 5] = [VK_ESCAPE, VK_LCONTROL, VK_LSHIFT, VK_LMENU, VK_SPACE];

const KEYS: usize = M0_LABELS.len();

struct Panel {
    rect: Rect,
    key: i32,
    presses: Presses<KEYS>,
}

thread_local! {
    // The UI thread owns the only panel; the window procedure reaches it here.
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
}

/// Create and show the fixed M0 key column on the left edge of the primary
/// monitor. Returns null if the window could not be created.
pub unsafe fn create_left() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = instance;
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.lpszClassName = w!("EdgePadPanel");
        RegisterClassW(&wc);

        let key = layout::scale(layout::KEY_SIZE, GetDpiForSystem());
        let r = layout::left_column(primary_monitor(), key, M0_LABELS.len() as i32);
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            wc.lpszClassName,
            w!("EdgePad"),
            WS_POPUP,
            r.x,
            r.y,
            r.w,
            r.h,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return hwnd;
        }
        let panel = Panel { rect: r, key, presses: Presses::default() };
        if !draw(hwnd, &panel) {
            DestroyWindow(hwnd);
            return null_mut();
        }
        PANEL.with_borrow_mut(|p| *p = Some(panel));
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        hwnd
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
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn pointer_id(wp: WPARAM) -> u32 {
    (wp & 0xFFFF) as u32
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
        PANEL.with_borrow_mut(|panel| {
            let Some(panel) = panel else { return };
            let Some(key) = layout::key_at(panel.rect, panel.key, x, y) else { return };
            if kind == PT_MOUSE {
                // Keep receiving the button-up even after leaving the panel.
                SetCapture(hwnd);
            }
            if let Some(key) = panel.presses.down(id, key) {
                // Send first, draw after: the key reaches the app sooner.
                send::key(M0_VKS[key], true);
                draw(hwnd, panel);
            }
        });
    }
}

/// A pointer lifted or was cancelled: release its key if nobody else holds it.
unsafe fn pointer_up(hwnd: HWND, wp: WPARAM) {
    unsafe {
        let id = pointer_id(wp);
        PANEL.with_borrow_mut(|panel| {
            let Some(panel) = panel else { return };
            if let Some(key) = panel.presses.up(id) {
                send::key(M0_VKS[key], false);
                draw(hwnd, panel);
            }
        });
        if GetCapture() == hwnd {
            ReleaseCapture();
        }
    }
}

unsafe fn primary_monitor() -> Rect {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        let m = info.rcMonitor;
        Rect { x: m.left, y: m.top, w: m.right - m.left, h: m.bottom - m.top }
    }
}

/// Paint the keys into a 32-bit DIB and hand it to the window with
/// UpdateLayeredWindow. The DIB is freed right after; the window keeps its
/// own copy, so nothing is held between redraws. Returns false on failure.
unsafe fn draw(hwnd: HWND, panel: &Panel) -> bool {
    let (r, key) = (panel.rect, panel.key);
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
        for (i, label) in M0_LABELS.iter().enumerate() {
            let top = key * i as i32;
            let mut cell = RECT { left: gap, top: top + gap, right: key - gap, bottom: top + key - gap };
            SelectObject(mem, if panel.presses.is_down(i) { held } else { face });
            RoundRect(mem, cell.left, cell.top, cell.right + 1, cell.bottom + 1, corner, corner);
            let text: Vec<u16> = label.encode_utf16().collect();
            DrawTextW(mem, text.as_ptr(), text.len() as i32, &mut cell, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
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

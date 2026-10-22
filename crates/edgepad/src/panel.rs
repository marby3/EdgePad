//! The edge panel window: a layered, topmost, never-activated popup.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use edgepad_core::keys::M0_LABELS;
use edgepad_core::layout::{self, Rect};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

/// Opacity of the shown panel: 90%.
const OPACITY: u8 = 230;
const PANEL_BG: u32 = 0x0020_2020;
const KEY_FACE: u32 = 0x003A_3A3A;
const KEY_TEXT: u32 = 0x00F0_F0F0;

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
        draw(hwnd, r, key);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        hwnd
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
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
/// own copy, so nothing is held between redraws.
unsafe fn draw(hwnd: HWND, r: Rect, key: i32) {
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
        let old_bitmap = SelectObject(mem, dib);

        let background = CreateSolidBrush(PANEL_BG);
        FillRect(mem, &RECT { left: 0, top: 0, right: r.w, bottom: r.h }, background);
        DeleteObject(background);

        let face = CreateSolidBrush(KEY_FACE);
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
            RoundRect(mem, cell.left, cell.top, cell.right + 1, cell.bottom + 1, corner, corner);
            let text: Vec<u16> = label.encode_utf16().collect();
            DrawTextW(mem, text.as_ptr(), text.len() as i32, &mut cell, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }

        SelectObject(mem, old_font);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        DeleteObject(font);
        DeleteObject(face);

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
        UpdateLayeredWindow(
            hwnd,
            screen,
            &POINT { x: r.x, y: r.y },
            &SIZE { cx: r.w, cy: r.h },
            mem,
            &POINT { x: 0, y: 0 },
            0,
            &blend,
            ULW_ALPHA,
        );

        SelectObject(mem, old_bitmap);
        DeleteObject(dib);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
    }
}

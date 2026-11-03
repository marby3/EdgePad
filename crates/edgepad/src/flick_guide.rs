//! The flick guide (Issue #159, spec 4.7): a small cross around a key with
//! flicks, its ways' labels a key above, below, left and right of it, the
//! way pointed at lit. A layered, topmost, never-activated window that takes
//! no input, made once the finger has started to slide and destroyed as it
//! lifts. Nothing of it is kept in between.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use edgepad_core::flick::Dir;
use edgepad_core::theme::Palette;
use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::colors::colorref;
use crate::log::debug_log;
use crate::panel::{draw_label, key_font, key_gap};
use crate::tray;

/// The guide on screen now.
struct Guide {
    hwnd: HWND,
    /// The panel whose key it is, and the pointer holding that key.
    panel: HWND,
    pointer: u32,
    /// The middle of the key, in screen pixels.
    at: (i32, i32),
    key: i32,
    /// Each way's label, by `Dir as usize`; none where the key has nothing.
    labels: [Option<Vec<u16>>; 4],
    pointing: Option<Dir>,
    palette: Palette,
    alpha: u8,
}

thread_local! {
    // One at a time: the latest finger to slide a key with flicks.
    static GUIDE: RefCell<Option<Guide>> = const { RefCell::new(None) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
}

/// What a guide is shown with.
pub struct Showing<'a> {
    pub panel: HWND,
    pub pointer: u32,
    pub at: (i32, i32),
    /// Side of a key in pixels at the panel's DPI.
    pub key: i32,
    pub labels: [Option<&'a [u16]>; 4],
    pub palette: Palette,
    pub alpha: u8,
}

/// Show the guide of `s.pointer`'s key, in place of any other, pointing at
/// `pointing`; or, if it is up already, light `pointing` instead.
pub unsafe fn show(s: Showing, pointing: Option<Dir>) {
    let lit = GUIDE.with_borrow_mut(|guide| {
        let g = guide.as_mut().filter(|g| g.panel == s.panel && g.pointer == s.pointer)?;
        if g.pointing != pointing {
            let was = std::mem::replace(&mut g.pointing, pointing);
            // Not drawn, it stays as it was: the next move tries again.
            if !unsafe { draw(g) } {
                g.pointing = was;
                return Some(());
            }
            point_event(pointing);
        }
        Some(())
    });
    if lit.is_some() {
        return;
    }
    unsafe {
        close_all();
        let instance = GetModuleHandleW(null());
        if !REGISTERED.get() {
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.hInstance = instance;
            wc.lpszClassName = w!("EdgePadFlick");
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                debug_log!("フリックのガイドの窓クラスを登録できない");
                return;
            }
            REGISTERED.set(true);
        }
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            w!("EdgePadFlick"),
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
            debug_log!("フリックのガイドの窓を作れない");
            return;
        }
        let guide = Guide {
            hwnd,
            panel: s.panel,
            pointer: s.pointer,
            at: s.at,
            key: s.key,
            labels: s.labels.map(|l| l.map(<[u16]>::to_vec)),
            pointing,
            palette: s.palette,
            alpha: s.alpha,
        };
        if !draw(&guide) {
            DestroyWindow(hwnd);
            return;
        }
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        debug_log!("フリックのガイドを出す");
        point_event(pointing);
        GUIDE.set(Some(guide));
    }
}

fn point_event(pointing: Option<Dir>) {
    tray::test_event(&format!("flick: point {}", pointing.map_or("none", Dir::name)));
}

/// Take away the guide of `pointer` on `panel`, or whatever guide of
/// `panel` is up when `pointer` is none.
pub unsafe fn close(panel: HWND, pointer: Option<u32>) {
    let guide = GUIDE.with_borrow_mut(|guide| guide.take_if(|g| g.panel == panel && pointer.is_none_or(|p| g.pointer == p)));
    if let Some(g) = guide {
        debug_log!("フリックのガイドを消す");
        unsafe { DestroyWindow(g.hwnd) };
    }
}

/// Take away whatever guide is up.
pub unsafe fn close_all() {
    if let Some(g) = GUIDE.take() {
        unsafe { DestroyWindow(g.hwnd) };
    }
}

/// Where way `d`'s cell is, in keys from the key.
fn offset(d: Dir) -> (i32, i32) {
    match d {
        Dir::Up => (0, -1),
        Dir::Down => (0, 1),
        Dir::Left => (-1, 0),
        Dir::Right => (1, 0),
    }
}

/// Paint the cross into a 32-bit DIB three keys square, the key in its
/// middle, and hand it to the window with UpdateLayeredWindow, then free
/// it. Each way with an action is a key face, as the panel draws them, with
/// its label; the rest is see-through. Returns false on failure.
unsafe fn draw(g: &Guide) -> bool {
    let k = g.key;
    let (w, h) = (3 * k, 3 * k);
    let gap = key_gap(k);
    // As the panel rounds its keys: corners key / 6 across.
    let radius = k / 12;
    // The faces, by way, in the DIB's pixels.
    let cells: Vec<(Dir, RECT)> = Dir::ALL
        .into_iter()
        .filter(|&d| g.labels[d as usize].is_some())
        .map(|d| {
            let (cx, cy) = offset(d);
            let (x, y) = ((cx + 1) * k, (cy + 1) * k);
            (d, RECT { left: x + gap, top: y + gap, right: x + k - gap, bottom: y + k - gap })
        })
        .collect();
    let inside = |x: i32, y: i32| {
        cells.iter().find(|(_, r)| {
            if x < r.left || x >= r.right || y < r.top || y >= r.bottom {
                return false;
            }
            // Off a corner: outside the circle that rounds it.
            let nx = (r.left + radius - x).max(x - (r.right - 1 - radius)).max(0);
            let ny = (r.top + radius - y).max(y - (r.bottom - 1 - radius)).max(0);
            nx * nx + ny * ny <= radius * radius
        })
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
        let c = g.palette;
        for y in 0..h {
            for x in 0..w {
                if let Some(&(d, _)) = inside(x, y) {
                    pixels[(y * w + x) as usize] = rgb(if g.pointing == Some(d) { c.held } else { c.face });
                }
            }
        }

        // The labels, as the panel writes its keys'.
        let font = key_font(k);
        let old_font = SelectObject(mem, font);
        SetBkMode(mem, TRANSPARENT as i32);
        for (d, cell) in &cells {
            let Some(label) = &g.labels[*d as usize] else { continue };
            let mut cell = *cell;
            SetTextColor(mem, colorref(if g.pointing == Some(*d) { c.held_text } else { c.text }));
            draw_label(mem, label, &mut cell);
        }
        SelectObject(mem, old_font);
        DeleteObject(font);
        GdiFlush();

        // Opaque on the faces, see-through elsewhere; the constant alpha
        // below gives it the panel's opacity.
        for y in 0..h {
            for x in 0..w {
                let p = &mut pixels[(y * w + x) as usize];
                *p = if inside(x, y).is_some() { *p | 0xFF00_0000 } else { 0 };
            }
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: g.alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let at = POINT { x: g.at.0 - w / 2, y: g.at.1 - h / 2 };
        let size = SIZE { cx: w, cy: h };
        let from = POINT { x: 0, y: 0 };
        let shown = UpdateLayeredWindow(g.hwnd, screen, &at, &size, mem, &from, 0, &blend, ULW_ALPHA) != 0;

        SelectObject(mem, old_bitmap);
        DeleteObject(dib);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        shown
    }
}

//! Acceptance tests for Issue #85: panels opaque by default, shown at
//! `dim_opacity` only while a pie menu is open or a pen hovers over them;
//! the short swipe from the handle no longer dims (it did since Issue #54).
//!
//! Opacity is read off the screen: topmost backdrops made before EdgePad
//! sit right under the left and right panels, and a panel's corner is
//! captured once over white and once over black. How far the two differ is
//! how much of the backdrop shows through.

mod common;

use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Controls::{
    CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, HSYNTHETICPOINTERDEVICE, POINTER_FEEDBACK_NONE,
};
use windows_sys::Win32::UI::Input::Pointer::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use common::*;

const A: u16 = b'A' as u16;

/// Two keys side by side on the left, a pie key among them, and a key on
/// the right; at 70% shown and 40% dimmed: neither is a default, so the
/// panels can only be following the settings.
fn settings(general: &str) -> String {
    format!(
        "[general]\n{general}\n[edge.left]\ncolumns = 2\n\
         [set.pad]\nkey = A | hold | A\nkey = 変形 | pie | menu\n\
         [set.menu]\nkey = 一 | shortcut | F1\nkey = 二 | shortcut | F2\n\
         [set.right]\nkey = C | hold | C\n\
         [profile.default]\nleft = pad\nright = right\n"
    )
}

const DIMMING: &str = "opacity = 70\ndim_opacity = 40";

/// Topmost backdrops down the left and right of the primary monitor, five
/// keys wide: under a pie menu too. Made before EdgePad starts, so the panels go above them.
fn backdrops() -> [TestWindow; 2] {
    let m = primary_monitor();
    let w = key_px() * 5;
    [
        test_window("EdgePadTestDimLeft", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, RECT { left: m.left, top: m.top, right: m.left + w, bottom: m.bottom }),
        test_window("EdgePadTestDimRight", WS_EX_TOPMOST | WS_EX_TOOLWINDOW, RECT { left: m.right - w, top: m.top, right: m.right, bottom: m.bottom }),
    ]
}

/// The opacity in percent of `panel`, from how much of the backdrops shows
/// through its top-left corner.
fn opacity(backdrops: &[TestWindow; 2], panel: RECT) -> i32 {
    let corner = RECT { left: panel.left, top: panel.top, right: panel.left + 3, bottom: panel.top + 3 };
    let average = |pixels: Vec<u32>| {
        let n = pixels.len() as i32;
        let sum = pixels.iter().map(|&p| channels(p)).fold([0; 3], |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2]]);
        sum.map(|v| v / n)
    };
    let set = |colour| backdrops.iter().for_each(|b| set_backdrop(b.0, colour));
    set(0x00FF_FFFF);
    let on_white = average(capture(corner));
    set(0x0000_0000);
    let on_black = average(capture(corner));
    let through = (0..3).map(|c| on_white[c] - on_black[c]).sum::<i32>() / 3;
    100 - through * 100 / 255
}

/// Whether `actual` is `want` percent, give or take rounding on the way.
fn near(actual: i32, want: i32) -> bool {
    (actual - want).abs() <= 4
}

/// Wait up to 2 s for the opacity of the panel `panel` gives to be `want`.
fn becomes(backdrops: &[TestWindow; 2], panel: impl Fn() -> RECT, want: i32) -> i32 {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let o = opacity(backdrops, panel());
        if near(o, want) || std::time::Instant::now() > end {
            return o;
        }
        pump(50);
    }
}

fn right_panel(app: &Running) -> RECT {
    let m = primary_monitor();
    panels(app).into_iter().find(|r| r.right == m.right).expect("a panel on the right")
}

fn centre(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

/// The centre of the key at `column`, `row` of the left panel.
fn cell(panel: RECT, column: i32, row: i32) -> POINT {
    let key = key_px();
    POINT { x: panel.left + key * column + key / 2, y: panel.top + key * row + key / 2 }
}

// AC1
#[test]
fn written_without_opacity_a_panel_is_fully_opaque() {
    let _desktop = desktop();
    let backdrops = backdrops();
    let app = launch_with(Some(&settings("")), None);
    let o = becomes(&backdrops, || left_panel(&app), 100);
    assert!(near(o, 100), "opaque by default, not {o}%");
}

// AC2
#[test]
fn a_short_swipe_shows_the_panel_normally_and_an_inward_flick_lets_no_key_go() {
    let _desktop = desktop();
    let backdrops = backdrops();
    let app = launch_with(Some(&settings(DIMMING)), None);
    find_panel(&app);
    choose(&app, "左辺");
    assert!(panels_now(&app).iter().all(|r| r.left != primary_monitor().left), "precondition: hidden");
    // Half a key in from the handle, with the mouse (on CI a touch from the
    // screen edge never reaches the handle), and let go there.
    let start = centre(left_handle(&app));
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(start);
    mouse_down_at(start);
    let end = POINT { x: start.x + key_px() * 3 / 4, y: start.y };
    mouse_move_to(end);
    pump(100);
    mouse_up_at(end);
    unsafe { SetCursorPos(saved.x, saved.y) };
    pump(300);
    let o = becomes(&backdrops, || left_panel(&app), 70);
    assert!(near(o, 70), "shown at opacity 70%, not dimmed: {o}%");

    // A quick flick inward on the panel: A stays pressed until the lift.
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let a = cell(left_panel(&app), 0, 0);
    touch.down(1, a);
    touch.move_to(1, POINT { x: a.x + key_px() * 2, y: a.y });
    pump(300);
    assert_eq!(log.keys(), [(A, false)], "A held through the flick");
    let o = opacity(&backdrops, left_panel(&app));
    assert!(near(o, 70), "and the panel as it was, at 70%, not {o}%");
    touch.up(1);
    log.wait_for(2);
    assert_eq!(log.keys(), [(A, false), (A, true)], "A comes up with the finger");
}

/// The pie menu window of `app`, if one is shown.
fn pie_window(app: &Running) -> Option<windows_sys::Win32::Foundation::HWND> {
    let name = wide("EdgePadPie");
    let mut hwnd = null_mut();
    loop {
        hwnd = unsafe { FindWindowExW(null_mut(), hwnd, name.as_ptr(), std::ptr::null()) };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == app.0.id() && unsafe { IsWindowVisible(hwnd) } != 0 {
            return Some(hwnd);
        }
    }
}

fn wait_pie(app: &Running, open: bool) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while pie_window(app).is_some() != open && std::time::Instant::now() < end {
        pump(20);
    }
    assert_eq!(pie_window(app).is_some(), open, "the menu {}", if open { "opens" } else { "closes" });
}

// AC3
#[test]
fn while_a_pie_menu_is_open_the_panels_are_dimmed_and_the_menu_is_not() {
    let _desktop = desktop();
    let backdrops = backdrops();
    let app = launch_with(Some(&settings(DIMMING)), None);
    find_panel(&app);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    let pie = cell(left_panel(&app), 1, 0);

    // Held open, slid to an item and let go.
    touch.down(1, pie);
    wait_pie(&app, true);
    let o = becomes(&backdrops, || left_panel(&app), 40);
    assert!(near(o, 40), "the left panel at dim_opacity 40%, not {o}%");
    let o = becomes(&backdrops, || right_panel(&app), 40);
    assert!(near(o, 40), "and the right one, not {o}%");
    let menu = window_rect(pie_window(&app).expect("the menu"));
    // Inside the second of the two items (down and to the right, at 45°),
    // out past its label (the ring runs from half a key to two keys out),
    // clear of the panel.
    let r = key_px() as f64 * 1.625 / std::f64::consts::SQRT_2;
    let item = POINT { x: pie.x + r as i32, y: pie.y + r as i32 };
    let o = opacity(&backdrops, RECT { left: item.x, top: item.y, right: item.x + 1, bottom: item.y + 1 });
    assert!(menu.right > item.x && near(o, 70), "the menu itself at opacity 70%, not {o}%");
    touch.move_to(1, item);
    touch.up(1);
    wait_pie(&app, false);
    let o = becomes(&backdrops, || left_panel(&app), 70);
    assert!(near(o, 70), "closed: back to 70%, not {o}%");
    let o = becomes(&backdrops, || right_panel(&app), 70);
    assert!(near(o, 70), "the right one too, not {o}%");

    // Left open by a tap, then closed by a tap in its middle.
    touch.down(1, pie);
    touch.up(1);
    wait_pie(&app, true);
    let o = becomes(&backdrops, || left_panel(&app), 40);
    assert!(near(o, 40), "left open: dimmed, {o}%");
    touch.down(2, pie);
    touch.up(2);
    wait_pie(&app, false);
    let o = becomes(&backdrops, || left_panel(&app), 70);
    assert!(near(o, 70), "closed by a tap: back to 70%, not {o}%");
    let _ = log.keys();
}

/// An injected pen, hovering in range without touching. A real pen keeps
/// reporting while in range; an injected one says nothing between frames,
/// and Windows takes it out of range after a while: a keeper thread says
/// the last frame again, as the touch helper does for its contacts.
struct Pen {
    device: usize,
    last: Arc<Mutex<Option<(POINT, POINTER_FLAGS)>>>,
    stop: Arc<AtomicBool>,
    keeper: Option<std::thread::JoinHandle<()>>,
}

fn inject_pen(device: usize, p: POINT, flags: POINTER_FLAGS) -> bool {
    let frame = pen_frame(p, flags);
    unsafe { InjectSyntheticPointerInput(device as HSYNTHETICPOINTERDEVICE, &frame, 1) != 0 }
}

impl Pen {
    fn new() -> Pen {
        let device = unsafe { CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_NONE) };
        assert!(!device.is_null(), "CreateSyntheticPointerDevice");
        let device = device as usize;
        let last: Arc<Mutex<Option<(POINT, POINTER_FLAGS)>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let keeper = {
            let (last, stop) = (last.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Some((p, flags)) = *last.lock().unwrap() {
                        inject_pen(device, p, flags);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            })
        };
        Pen { device, last, stop, keeper: Some(keeper) }
    }

    fn frame(&self, p: POINT, flags: POINTER_FLAGS) {
        {
            let mut last = self.last.lock().unwrap();
            assert!(inject_pen(self.device, p, flags), "InjectSyntheticPointerInput: {}", std::io::Error::last_os_error());
            // In range, kept so; out of range, said once.
            *last = (flags & POINTER_FLAG_INRANGE != 0).then_some((p, flags));
        }
        pump(100);
    }

    /// Hover over `p`.
    fn hover(&self, p: POINT) {
        self.frame(p, POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE);
    }

    /// Out of range, at `p`.
    fn away(&self, p: POINT) {
        self.frame(p, POINTER_FLAG_UPDATE);
    }
}

impl Drop for Pen {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(keeper) = self.keeper.take() {
            let _ = keeper.join();
        }
        unsafe { DestroySyntheticPointerDevice(self.device as HSYNTHETICPOINTERDEVICE) };
    }
}

// AC4
#[test]
fn a_pen_over_a_panel_dims_that_panel_alone_and_presses_nothing() {
    let _desktop = desktop();
    let backdrops = backdrops();
    let app = launch_with(Some(&settings(DIMMING)), None);
    find_panel(&app);
    let log = KeyLog::start();
    let a = cell(left_panel(&app), 0, 0);
    let middle = centre(primary_monitor());

    let pen = Pen::new();
    pen.hover(middle);
    pen.hover(a);
    pen.hover(POINT { x: a.x, y: a.y + 2 });
    let o = becomes(&backdrops, || left_panel(&app), 40);
    assert!(near(o, 40), "a pen over the left panel dims it: {o}%");
    let o = opacity(&backdrops, right_panel(&app));
    assert!(near(o, 70), "the right one stays at 70%: {o}%");

    // Off the panel: back.
    pen.hover(middle);
    let o = becomes(&backdrops, || left_panel(&app), 70);
    assert!(near(o, 70), "the pen gone off the panel: 70%, not {o}%");

    // Out of range over it: back too.
    pen.hover(a);
    let o = becomes(&backdrops, || left_panel(&app), 40);
    assert!(near(o, 40), "over it again: {o}%");
    pen.away(a);
    let o = becomes(&backdrops, || left_panel(&app), 70);
    assert!(near(o, 70), "out of range: 70%, not {o}%");
    drop(pen);
    assert_eq!(log.keys(), [], "a pen presses nothing");

    // The mouse over the panel dims nothing.
    let mut saved = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut saved) };
    mouse_move_to(a);
    pump(300);
    let o = opacity(&backdrops, left_panel(&app));
    unsafe { SetCursorPos(saved.x, saved.y) };
    assert!(near(o, 70), "the mouse over it: still 70%, not {o}%");
}

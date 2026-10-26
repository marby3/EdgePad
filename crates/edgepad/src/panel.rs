//! The edge panel window: a layered, topmost, never-activated popup, and the
//! handle left on its edge while it is hidden.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::time::{Duration, Instant};

use edgepad_core::config::{Accent, Action, Edge, Handle, KeyDef, KeySet, Monitor, Page, Step, Theme};
use edgepad_core::gesture::{self, Swipe};
use edgepad_core::keys::{Combo, Key, Mods};
use edgepad_core::layout::{self, Rect};
use edgepad_core::modifier::{LatchMode, Modifier};
use edgepad_core::press::Presses;
use edgepad_core::theme::Palette;
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::Input::Pointer::GetPointerType;
use windows_sys::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABN_POSCHANGED, APPBARDATA,
    SHAppBarMessage,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

use crate::colors::{self, colorref};
use crate::log::debug_log;
use crate::{launch, pie_menu, send};
use crate::settings::{EdgePanel, Panels};

/// The handle: the accent, 30% opaque.
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

impl PanelKey {
    fn of(k: &KeyDef) -> PanelKey {
        PanelKey { label: k.label.encode_utf16().collect(), width: k.width as usize, rect: None, sends: Sends::of(&k.action) }
    }

    /// Whether it is on every page: it turns them.
    fn turns_pages(&self) -> bool {
        matches!(self.sends, Sends::Page(Page::Next | Page::Prev))
    }
}

/// What a key sends, in virtual keys.
#[derive(Clone)]
enum Sends {
    /// Down while pressed, up on release.
    Hold(u16),
    /// As Hold, but a short tap latches it down (spec 4.3).
    Modifier { vk: u16, state: Modifier },
    /// The whole combination on release: modifiers, then the key.
    Shortcut { mods: Vec<u16>, key: Option<u16> },
    /// The whole combination as pressed, then every `interval` ms once held
    /// for REPEAT_DELAY.
    Repeat { mods: Vec<u16>, key: Option<u16>, interval: u32 },
    /// Steps sent in order on release, waiting where they say.
    Macro(Rc<[MacroStep]>),
    /// Typed as Unicode characters on release.
    Text(String),
    /// A program or URL started on release.
    Run(String),
    /// Turns the page, or switches to another key set, on release.
    Page(Page),
    /// Opens a pie menu of the key set it names while pressed (spec 4.6).
    /// Nothing is sent yet.
    Pie(String),
}

impl Sends {
    fn of(action: &Action) -> Sends {
        match action {
            Action::Modifier(key) => Sends::Modifier { vk: send::vk(*key), state: Modifier::Up },
            Action::Hold(key) => Sends::Hold(send::vk(*key)),
            Action::Shortcut(combo) => {
                let (mods, key) = vks(combo);
                Sends::Shortcut { mods, key }
            }
            &Action::Repeat { ref combo, interval_ms } => {
                let (mods, key) = vks(combo);
                Sends::Repeat { mods, key, interval: interval_ms }
            }
            Action::Macro(steps) => Sends::Macro(
                steps
                    .iter()
                    .map(|step| match step {
                        Step::Keys(combo) => {
                            let (mods, key) = vks(combo);
                            MacroStep::Keys { mods, key }
                        }
                        &Step::Wait(ms) => MacroStep::Wait(ms),
                    })
                    .collect::<Rc<[_]>>(),
            ),
            Action::Text(text) => Sends::Text(text.clone()),
            Action::Run(command) => Sends::Run(command.clone()),
            Action::Page(page) => Sends::Page(page.clone()),
            Action::Pie(set) => Sends::Pie(set.clone()),
        }
    }
}

enum MacroStep {
    /// A combination, as a shortcut sends it.
    Keys { mods: Vec<u16>, key: Option<u16> },
    /// Milliseconds before the next step.
    Wait(u32),
}

/// The virtual keys of `combo`: its modifiers in Ctrl, Shift, Alt, Win
/// order, and its key.
fn vks(combo: &Combo) -> (Vec<u16>, Option<u16>) {
    let order = [(Mods::CTRL, Key::Ctrl), (Mods::SHIFT, Key::Shift), (Mods::ALT, Key::Alt), (Mods::WIN, Key::Win)];
    let mods = order.iter().filter(|(m, _)| combo.mods.contains(*m)).map(|&(_, k)| send::vk(k)).collect();
    (mods, combo.key.map(send::vk))
}

/// How long a repeat key is held before it starts repeating: a tap sends
/// once.
const REPEAT_DELAY: u32 = 300;

/// The timer of the repeat key at `index` on a panel. Timer ids are nonzero.
fn repeat_timer(index: usize) -> usize {
    index + 1
}

/// The timer ending a macro's wait: past any repeat key's.
const MACRO_TIMER: usize = 0x1_0000;

/// The timers that hide a panel: `auto_hide_ms` after it was last used, and
/// `hover_ms` after the mouse left a panel it showed by hovering.
const AUTO_HIDE_TIMER: usize = 0x1_0001;
const LEAVE_TIMER: usize = 0x1_0002;

/// The timer moving a sliding panel, every SLIDE_STEP ms while it slides.
const SLIDE_TIMER: usize = 0x1_0003;
const SLIDE_STEP: u32 = 10;

/// The message the shell notifies a docked panel with; ABN_POSCHANGED in
/// wParam says another AppBar or the taskbar moved. A panel window's own,
/// apart from the tray's messages.
const APPBAR_CALLBACK: u32 = WM_APP + 1;

/// The timer on a handle that shows its panel once the mouse has rested on
/// it for `hover_ms`. A handle window's own, apart from the panel's timers.
const HOVER_TIMER: usize = 1;

struct Panel {
    hwnd: HWND,
    edge: Edge,
    monitor: Monitor,
    lines: usize,
    /// Side of a key in logical pixels.
    key_size: u32,
    /// A modifier tapped for less than this latches.
    latch_time: Duration,
    latch_mode: LatchMode,
    /// Constant alpha of the panel shown normally.
    alpha: u8,
    /// Constant alpha of the panel shown dimmed.
    dim_alpha: u8,
    /// The panel on screen, and the side of a key, in pixels at the current DPI.
    rect: Rect,
    key: i32,
    /// The monitor the panel is on.
    screen: Rect,
    keys: Vec<PanelKey>,
    /// The page shown, of how many.
    page: usize,
    pages: usize,
    /// Every key set, for a page key that switches to another.
    sets: Rc<[KeySet]>,
    presses: Presses,
    /// The macro running on this panel: its steps, and the one to send next.
    running: Option<(Rc<[MacroStep]>, usize)>,
    /// The mouse pointer that holds the window's capture, if any.
    captured: Option<u32>,
    /// Where each pointer down on the panel started.
    starts: Vec<Start>,
    handle: Option<Handle>,
    /// The handle's window; null with `handle = none`.
    handle_hwnd: HWND,
    /// From the settings; 0 is off.
    hover_ms: u32,
    auto_hide_ms: u32,
    /// Shown by hovering (see HOVERED): whether the mouse is outside both
    /// the panel and its handle now.
    mouse_out: bool,
    /// To be an AppBar while shown, and whether it is one now.
    dock: bool,
    docked: bool,
    /// How long a slide takes (0: none), and the slide going on, if any.
    slide_ms: u32,
    slide: Option<Slide>,
}

/// A panel sliding in or out: how far out of its place it was when the
/// slide began, in pixels, and when that was.
#[derive(Clone, Copy)]
struct Slide {
    inward: bool,
    from: i32,
    at: Instant,
    /// How long it takes: `slide_ms` for the whole way, less for the rest of
    /// a slide turned back.
    ms: u32,
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

    /// How far a swipe from the handle goes before it shows the panel
    /// normally, not dimmed: two keys.
    fn long_swipe_distance(&self) -> i32 {
        (self.key * 2).max(1)
    }

    /// How far a flick on the panel goes before it undims it: a whole key.
    fn flick_distance(&self) -> i32 {
        self.key.max(1)
    }

    /// How far a flick from `(x, y)` toward the screen edge goes before it
    /// hides the panel: three quarters of a key, or two thirds of the way to
    /// the edge where that is less. A one-column panel is only a key thick:
    /// from the middle of a key there is half a key to go, and a finger stops
    /// short of the very edge. A flick of a whole key, as before, clears it.
    fn hide_distance(&self, (x, y): (i32, i32)) -> i32 {
        let s = self.screen;
        let room = match self.edge {
            Edge::Left => x - s.x,
            Edge::Right => s.x + s.w - x,
            Edge::Top => y - s.y,
            Edge::Bottom => s.y + s.h - y,
        };
        (self.key * 3 / 4).min(room * 2 / 3).max(1)
    }

    /// The constant alpha it is drawn with now.
    fn current_alpha(&self) -> u8 {
        if DIMMED.get()[self.edge as usize] { self.dim_alpha } else { self.alpha }
    }
}

/// The colours panels and handles are drawn in now.
pub fn palette() -> Palette {
    PALETTE.get().unwrap_or_else(|| {
        let (theme, accent) = STYLE.get();
        let p = colors::palette(theme, accent);
        PALETTE.set(Some(p));
        p
    })
}

/// Windows' colour settings may have changed: if the colours come to
/// something else now, redraw every panel and handle in them. Returns false
/// when the panels are in use further up the stack and nothing changed: the
/// caller tries again later.
pub unsafe fn restyle() -> bool {
    let (theme, accent) = STYLE.get();
    let p = colors::palette(theme, accent);
    if PALETTE.get() == Some(p) {
        return true;
    }
    PANELS.with(|panels| {
        let Ok(panels) = panels.try_borrow() else { return false };
        PALETTE.set(Some(p));
        debug_log!("配色の変更");
        for panel in panels.iter() {
            unsafe {
                draw(panel);
                if !panel.handle_hwnd.is_null() {
                    InvalidateRect(panel.handle_hwnd, null(), 1);
                }
            }
        }
        true
    })
}

/// Take the theme and accent of `panels`: the colours the panels and
/// handles are drawn in.
pub fn set_style(panels: &Panels) {
    STYLE.set((panels.theme, panels.accent));
    PALETTE.set(Some(colors::palette(panels.theme, panels.accent)));
}

/// `percent` opacity as a constant alpha.
fn alpha(percent: u8) -> u8 {
    ((percent.min(100) as u32 * 255 + 50) / 100) as u8
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
    // The edges shown dimmed by a short swipe from their handle (spec 4.5).
    // Hiding an edge ends it, so every other way of showing it is normal.
    static DIMMED: Cell<[bool; 4]> = const { Cell::new([false; 4]) };
    // The theme and accent of the settings, and the colours they come to.
    // Every panel shares them; Windows changing its own makes them anew.
    static STYLE: Cell<(Theme, Accent)> = const { Cell::new((Theme::Auto, Accent::Auto)) };
    static PALETTE: Cell<Option<Palette>> = const { Cell::new(None) };
    // The handle the mouse is resting on, its hover timer running.
    static HOVERING: Cell<HWND> = const { Cell::new(null_mut()) };
    // The edges shown by the mouse resting on their handle, to hide once it
    // goes. Their handle stays up meanwhile, beside or under the panel: the
    // mouse is still over it, and it tells when the mouse goes.
    static HOVERED: Cell<[bool; 4]> = const { Cell::new([false; 4]) };
    // Each pointer down on a handle, with that handle's window.
    static HANDLE_DRAGS: RefCell<Vec<Start>> = const { RefCell::new(Vec::new()) };
}

/// The edge as the tray and the log name it.
fn edge_name(edge: Edge) -> &'static str {
    match edge {
        Edge::Left => "左辺",
        Edge::Right => "右辺",
        Edge::Top => "上辺",
        Edge::Bottom => "下辺",
    }
}

/// Whether the panel on `edge` should be on screen.
fn visible(edge: Edge) -> bool {
    !PAUSED.get() && !HIDDEN.get()[edge as usize]
}

/// Whether the handle on `edge` should be on screen: while its panel is
/// hidden or shown by hovering, but not while paused.
fn handle_visible(edge: Edge) -> bool {
    !PAUSED.get() && (HIDDEN.get()[edge as usize] || hovered(edge))
}

fn hovered(edge: Edge) -> bool {
    HOVERED.get()[edge as usize]
}

fn set_hovered(edge: Edge, on: bool) {
    let mut hovered = HOVERED.get();
    hovered[edge as usize] = on;
    HOVERED.set(hovered);
}

/// The handle's window: plain accent colour (see `handle_proc`), see-through
/// by its layered alpha.
unsafe fn create_handle() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(handle_proc);
        wc.hInstance = instance;
        wc.hCursor = LoadCursorW(null_mut(), IDC_HAND);
        wc.lpszClassName = w!("EdgePadHandle");
        // Every handle after the first finds the class already there. Any
        // other failure leaves no handle.
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return null_mut();
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

/// Create and show the panel `p` of `panels`, keys as many logical pixels
/// square as they say at the DPI of the monitor it goes on. Keys that do not
/// fit are left out; with none left, no panel is made. Returns false if the
/// window could not be created.
pub unsafe fn create(p: &EdgePanel, panels: &Panels) -> bool {
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
        let keys: Vec<PanelKey> = p.keys.iter().map(PanelKey::of).collect();
        let presses = Presses::new(keys.len());
        let rect = Rect { x: 0, y: 0, w: 0, h: 0 };
        let (edge, monitor, lines) = (p.edge, p.monitor, p.lines);
        let handle_hwnd = if p.handle.is_some() { create_handle() } else { null_mut() };
        let mut panel = Panel {
            hwnd,
            edge,
            monitor,
            lines,
            key_size: panels.key_size,
            latch_time: Duration::from_millis(panels.latch_ms as u64),
            latch_mode: panels.latch,
            alpha: alpha(panels.opacity),
            dim_alpha: alpha(panels.dim_opacity),
            rect,
            key: 0,
            screen: rect,
            keys,
            page: 0,
            pages: 1,
            sets: panels.sets.clone(),
            presses,
            running: None,
            captured: None,
            starts: Vec::new(),
            handle: p.handle,
            handle_hwnd,
            hover_ms: panels.hover_ms,
            auto_hide_ms: panels.auto_hide_ms,
            mouse_out: false,
            dock: p.dock,
            docked: false,
            slide_ms: panels.slide_ms,
            slide: None,
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
        // At once, slide or not: panels are made anew on every profile
        // switch, and would slide each time another app came to the front.
        if show {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        if show_handle && !handle_hwnd.is_null() {
            ShowWindow(handle_hwnd, SW_SHOWNOACTIVATE);
        }
        if show {
            with_panel(hwnd, |panel| {
                // Rebuilt while shown by hovering: still so, over its handle.
                if hovered(edge) {
                    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    panel.mouse_out = !in_reach(panel, cursor());
                }
                // At launch and on every rebuild: it was shown just now.
                count_idle(panel);
                dock(panel);
            });
        }
        true
    }
}

/// Replace every panel with those of `panels`. Keys held on the old panels
/// are released first, so none is left down with no panel to lift it.
/// Returns false if a window could not be created.
pub unsafe fn rebuild(panels: &Panels) -> bool {
    set_style(panels);
    unsafe {
        // The handles go, and their hover timers with them. An edge shown by
        // hovering stays so, if it still has a panel.
        HOVERING.set(null_mut());
        let mut hovered = HOVERED.get();
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            hovered[edge as usize] &= panels.edges.iter().any(|p| p.edge == edge);
        }
        HOVERED.set(hovered);
        // Taken out first: messages sent while a window is destroyed (capture
        // lost, for one) then find no panel and change nothing.
        pie_menu::close_all();
        for mut panel in PANELS.take() {
            undock(&mut panel);
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
        // An edge left without a panel is not dimmed when it comes back.
        let mut dimmed = DIMMED.get();
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            dimmed[edge as usize] &= panels.edges.iter().any(|p| p.edge == edge);
        }
        DIMMED.set(dimmed);
        // Every edge is tried, even after one fails.
        panels.edges.iter().fold(true, |ok, p| create(p, panels) && ok)
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

/// How a panel is shown.
#[derive(Clone, Copy, PartialEq)]
enum Look {
    Normal,
    Dimmed,
}

/// Show the panel on `edge` with `look`, or change how it is shown. It is
/// redrawn at its new alpha before it goes up.
unsafe fn show(edge: Edge, look: Look) {
    let dim = look == Look::Dimmed;
    let mut dimmed = DIMMED.get();
    if dimmed[edge as usize] != dim {
        dimmed[edge as usize] = dim;
        DIMMED.set(dimmed);
        debug_log!("パネル {}: {}", edge_name(edge), if dim { "半透明" } else { "通常表示" });
        PANELS.with_borrow(|panels| {
            for panel in panels.iter().filter(|p| p.edge == edge) {
                unsafe { draw(panel) };
            }
        });
    }
    unsafe { set_hidden(edge, false) };
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
/// say. A panel going away lets go of the keys held on it first, and is no
/// longer dimmed: shown again, it is normal unless shown dimmed.
unsafe fn apply_visibility() {
    PANELS.with_borrow_mut(|panels| unsafe {
        for panel in panels {
            let show = visible(panel.edge);
            // Sliding out, it is as good as hidden already.
            let shown_now = IsWindowVisible(panel.hwnd) != 0 && !panel.slide.is_some_and(|s| !s.inward);
            if show != shown_now {
                debug_log!("パネル {}: {}", edge_name(panel.edge), if show { "表示" } else { "非表示" });
                if show {
                    // Room first: no maximised window is under it as it shows.
                    dock(panel);
                    slide(panel, true);
                    ShowWindow(panel.hwnd, SW_SHOWNOACTIVATE);
                    count_idle(panel);
                } else {
                    let mut dimmed = DIMMED.get();
                    dimmed[panel.edge as usize] = false;
                    DIMMED.set(dimmed);
                    // Draws it, now at its normal alpha. Its keys go before
                    // it slides away, as when it goes at once.
                    let_go(panel);
                    if !slide(panel, false) {
                        ShowWindow(panel.hwnd, SW_HIDE);
                        // The room goes back while hidden.
                        undock(panel);
                    }
                    KillTimer(panel.hwnd, AUTO_HIDE_TIMER);
                    KillTimer(panel.hwnd, LEAVE_TIMER);
                    set_hovered(panel.edge, false);
                    panel.mouse_out = false;
                }
            }
            let handle = handle_visible(panel.edge);
            if !panel.handle_hwnd.is_null() && handle != (IsWindowVisible(panel.handle_hwnd) != 0) {
                ShowWindow(panel.handle_hwnd, if handle { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
    });
}

/// Release the keys held or latched on `panel` and forget its pointers. A
/// shortcut still under a finger is dropped, not sent.
unsafe fn let_go(panel: &mut Panel) {
    unsafe {
        for (i, key) in panel.keys.iter_mut().enumerate() {
            let up = match &mut key.sends {
                Sends::Hold(vk) => panel.presses.is_down(i).then_some(*vk),
                Sends::Modifier { vk, state } => state.let_go().then_some(*vk),
                Sends::Repeat { .. } => {
                    KillTimer(panel.hwnd, repeat_timer(i));
                    None
                }
                _ => None,
            };
            if let Some(vk) = up {
                hold(vk, false);
            }
        }
        panel.presses = Presses::new(panel.keys.len());
        // A macro stops between steps, so it leaves nothing down.
        if panel.running.take().is_some() {
            KillTimer(panel.hwnd, MACRO_TIMER);
        }
        panel.starts.clear();
        if panel.captured.take().is_some() {
            ReleaseCapture();
        }
        pie_menu::close(panel.hwnd, None);
        draw(panel);
    }
}

/// Let go of every key EdgePad holds, from any panel (the tray's 全キー解放).
pub unsafe fn release_all() {
    unsafe {
        PANELS.with_borrow_mut(|panels| {
            for panel in panels {
                let_go(panel);
                count_idle(panel);
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
        panel.screen = screen;
        place_keys(panel);
        if let (Some(h), false) = (panel.handle, panel.handle_hwnd.is_null()) {
            let r = layout::handle(screen, panel.edge, layout::scale(h.width as i32, dpi), h.length, h.position);
            SetWindowPos(panel.handle_hwnd, HWND_TOPMOST, r.x, r.y, r.w, r.h, SWP_NOACTIVATE);
        }
        // Moved or resized: a docked panel takes the room it needs now.
        if panel.docked {
            dock(panel);
        }
    }
}

/// Put the keys of the page shown in place on the panel's monitor; the keys
/// of other pages get no place. The panel takes the size of the page.
fn place_keys(panel: &mut Panel) {
    let widths: Vec<usize> = panel.keys.iter().map(|k| k.width).collect();
    let sticky: Vec<bool> = panel.keys.iter().map(PanelKey::turns_pages).collect();
    let pages = layout::pages(panel.screen, panel.edge, panel.key, panel.lines, &widths, &sticky);
    panel.pages = pages.len().max(1);
    panel.page = panel.page.min(panel.pages - 1);
    let shown = pages.get(panel.page).map_or(&[][..], Vec::as_slice);
    let shown_widths: Vec<usize> = shown.iter().map(|&i| widths[i]).collect();
    let (rect, rects) = layout::edge_panel(panel.screen, panel.edge, panel.key, panel.lines, &shown_widths);
    panel.rect = rect;
    for key in &mut panel.keys {
        key.rect = None;
    }
    for (&i, r) in shown.iter().zip(rects) {
        panel.keys[i].rect = Some(r);
    }
}

/// Turn the page of `panel`, or switch it to another key set, as a page key
/// says. Keys held or latched on other pages stay so; switching sets lets go
/// of the panel's keys first, as the keys under them change.
unsafe fn turn(panel: &mut Panel, page: &Page) {
    match page {
        Page::Next => panel.page = (panel.page + 1) % panel.pages,
        Page::Prev => panel.page = (panel.page + panel.pages - 1) % panel.pages,
        Page::Set(name) => {
            // The parser makes sure the set is there.
            let Some(set) = panel.sets.iter().find(|s| s.name.eq_ignore_ascii_case(name)) else { return };
            let keys: Vec<PanelKey> = set.keys.iter().map(PanelKey::of).collect();
            unsafe { let_go(panel) };
            panel.presses = Presses::new(keys.len());
            panel.keys = keys;
            panel.page = 0;
        }
    }
    debug_log!("パネル {}: ページ {}/{}", edge_name(panel.edge), panel.page + 1, panel.pages);
    place_keys(panel);
    unsafe { draw(panel) };
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
            WM_POINTERUP => {
                pie_menu::point(pointer_id(wp), pointer_at(lp));
                pointer_up(hwnd, wp, (wp >> 16) as u32 & POINTER_MESSAGE_FLAG_CANCELED != 0);
                0
            }
            WM_POINTERCAPTURECHANGED => {
                pointer_up(hwnd, wp, true);
                0
            }
            WM_POINTERUPDATE => {
                mouse_moved(hwnd, wp, Some(pointer_at(lp)));
                pointer_update(hwnd, wp, lp);
                0
            }
            WM_POINTERENTER => {
                mouse_moved(hwnd, wp, Some(pointer_at(lp)));
                0
            }
            // Gone from the panel, though perhaps onto its handle.
            WM_POINTERLEAVE => {
                mouse_moved(hwnd, wp, None);
                0
            }
            APPBAR_CALLBACK if wp as u32 == ABN_POSCHANGED => {
                // A notice that comes after the panel was hidden registers
                // nothing again.
                let done = with_panel(hwnd, |panel| {
                    if panel.docked {
                        dock(panel);
                    }
                });
                if !done {
                    PostMessageW(hwnd, msg, wp, lp);
                }
                0
            }
            WM_TIMER => {
                match wp {
                    MACRO_TIMER => macro_wait_over(hwnd),
                    AUTO_HIDE_TIMER | LEAVE_TIMER => idle_over(hwnd, wp),
                    SLIDE_TIMER => {
                        with_panel(hwnd, |panel| slide_step(panel));
                    }
                    _ => repeat(hwnd, wp),
                }
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

pub fn pointer_id(wp: WPARAM) -> u32 {
    (wp & 0xFFFF) as u32
}

/// The screen position a pointer message carries.
pub fn pointer_at(lp: LPARAM) -> (i32, i32) {
    ((lp & 0xFFFF) as i16 as i32, (lp >> 16 & 0xFFFF) as i16 as i32)
}

/// Touch or mouse, not pen (pens press nothing), as the pointer's kind.
pub unsafe fn finger_or_mouse(id: u32) -> Option<POINTER_INPUT_TYPE> {
    let mut kind = 0;
    match unsafe { GetPointerType(id, &mut kind) } {
        0 => None,
        _ if kind == PT_PEN => None,
        _ => Some(kind),
    }
}

/// A pointer moved on a panel: a quick flick toward the screen edge hides the
/// panel, letting go of the key it pressed. On a dimmed panel, a quick flick
/// the other way shows it normally, and the key it pressed comes up as for a
/// contact taken away: a gesture, not a key press. A finger holding a pie
/// menu open slides to its items instead, and flicks nothing.
unsafe fn pointer_update(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
    let (id, (x, y)) = (pointer_id(wp), pointer_at(lp));
    unsafe { pie_menu::point(id, (x, y)) };
    if pie_menu::is_held_by(id) {
        return;
    }
    let mut flick = None;
    with_panel(hwnd, |panel| {
        if let Some(s) = panel.starts.iter().find(|s| s.id == id && s.at.elapsed() <= FLICK_TIME) {
            let (dx, dy) = (x - s.x, y - s.y);
            let hide = panel.hide_distance((s.x, s.y));
            flick = match gesture::swipe(panel.edge, dx, dy, hide) {
                Some(Swipe::Outward) => Some((panel.edge, Swipe::Outward)),
                _ => gesture::swipe(panel.edge, dx, dy, panel.flick_distance()).map(|f| (panel.edge, f)),
            };
        }
    });
    // Outside the borrow: hiding and undimming go over every panel.
    match flick {
        Some((edge, Swipe::Outward)) => unsafe { set_hidden(edge, true) },
        Some((edge, Swipe::Inward)) if DIMMED.get()[edge as usize] => unsafe {
            pointer_up(hwnd, wp, true);
            show(edge, Look::Normal);
        },
        _ => {}
    }
}

unsafe extern "system" fn handle_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            WM_POINTERDOWN => {
                let id = pointer_id(wp);
                // Pressed, not resting: no hover.
                hover_off(hwnd);
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
                if msg == WM_POINTERUPDATE && hovering_mouse(wp) {
                    hover_on(hwnd);
                    mouse_on_handle(hwnd, true);
                }
                handle_pointer(hwnd, msg, pointer_id(wp), pointer_at(lp));
                0
            }
            WM_POINTERLEAVE => {
                hover_off(hwnd);
                mouse_on_handle(hwnd, false);
                0
            }
            WM_TIMER if wp == HOVER_TIMER => {
                hover_over(hwnd);
                0
            }
            WM_ERASEBKGND => {
                let mut r: RECT = zeroed();
                GetClientRect(hwnd, &mut r);
                // Held keys are the accent, as plain as it comes.
                let brush = CreateSolidBrush(colorref(palette().held));
                FillRect(wp as HDC, &r, brush);
                DeleteObject(brush);
                1
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// A pointer that went down on handle `hwnd` moved, lifted or was lost. A
/// short swipe inward shows the panel dimmed; carried on to a long one, the
/// panel turns normal. A tap shows it normally.
unsafe fn handle_pointer(hwnd: HWND, msg: u32, id: u32, (x, y): (i32, i32)) {
    let Some(Start { x: x0, y: y0, .. }) =
        HANDLE_DRAGS.with_borrow(|drags| drags.iter().find(|d| d.hwnd == hwnd && d.id == id).copied())
    else {
        return;
    };
    let Some((edge, short, long)) = PANELS.with(|panels| {
        let panels = panels.try_borrow().ok()?;
        let p = panels.iter().find(|p| p.handle_hwnd == hwnd)?;
        Some((p.edge, p.swipe_distance(), p.long_swipe_distance()))
    }) else {
        return;
    };
    let (dx, dy) = (x - x0, y - y0);
    let inward = |distance| gesture::swipe(edge, dx, dy, distance) == Some(Swipe::Inward);
    // How it shows the panel, if it does, and whether the gesture is over.
    let (shows, over) = match msg {
        WM_POINTERUPDATE if inward(long) => (Some(Look::Normal), true),
        // Once: the swipe goes on, and may yet turn the panel normal.
        WM_POINTERUPDATE if inward(short) && HIDDEN.get()[edge as usize] => (Some(Look::Dimmed), false),
        WM_POINTERUPDATE => (None, false),
        WM_POINTERUP => (gesture::is_tap(dx, dy, short).then_some(Look::Normal), true),
        // Capture lost: the gesture ends with nothing more.
        _ => (None, true),
    };
    if over {
        HANDLE_DRAGS.with_borrow_mut(|drags| drags.retain(|d| d.id != id));
        if unsafe { GetCapture() } == hwnd {
            unsafe { ReleaseCapture() };
        }
    }
    if let Some(look) = shows {
        // Shown on purpose, though it was up by hovering: it stays.
        set_hovered(edge, false);
        unsafe { show(edge, look) };
    }
}

/// Run `f` on the panel of window `hwnd`, if it is one of ours. Returns false
/// when the panels are already in use further up the stack, and `f` did not
/// run: moving a panel onto a monitor of another DPI makes Windows send
/// WM_DPICHANGED from inside the draw that moved it.
fn with_panel(hwnd: HWND, f: impl FnOnce(&mut Panel)) -> bool {
    with_panels(hwnd, |panels, i| f(&mut panels[i]))
}

/// As `with_panel`, giving `f` every panel and the index of `hwnd`'s.
fn with_panels(hwnd: HWND, f: impl FnOnce(&mut [Panel], usize)) -> bool {
    PANELS.with(|panels| {
        let Ok(mut panels) = panels.try_borrow_mut() else { return false };
        if let Some(i) = panels.iter().position(|p| p.hwnd == hwnd) {
            f(&mut panels, i);
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
/// stays held by this pointer until it lifts, wherever it moves. Modifiers
/// held meanwhile, on any panel, no longer latch.
unsafe fn pointer_down(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
    unsafe {
        let id = pointer_id(wp);
        let (sx, sy) = pointer_at(lp);
        let Some(kind) = finger_or_mouse(id) else {
            debug_log!("ポインタ {id} ({sx}, {sy}): ペンか種類が分からない");
            return;
        };
        with_panels(hwnd, |panels, i| {
            let panel = &mut panels[i];
            // Its keys are on the move, not where they will be.
            if panel.slide.is_some() {
                return;
            }
            // In use: nothing counts toward hiding it until it is let go.
            KillTimer(hwnd, AUTO_HIDE_TIMER);
            KillTimer(hwnd, LEAVE_TIMER);
            panel.starts.retain(|s| s.id != id);
            panel.starts.push(Start { hwnd, id, x: sx, y: sy, at: Instant::now() });
            if kind == PT_MOUSE {
                // Keep receiving moves and the button-up after leaving the
                // panel, key or not: a swipe to hide runs off it.
                SetCapture(hwnd);
                panel.captured = Some(id);
            }
            let (x, y) = (sx - panel.rect.x, sy - panel.rect.y);
            let key = panel.keys.iter().position(|k| k.rect.is_some_and(|r| r.contains(x, y)));
            debug_log!("ポインタ {id} ({sx}, {sy}) が押す: キー {key:?}");
            // A menu left open by a tap closes on a tap in its middle or on
            // a pie key, and that press goes no further.
            if pie_menu::is_left_open(hwnd) {
                let on_pie = key.is_some_and(|k| matches!(panel.keys[k].sends, Sends::Pie(_)));
                if on_pie || pie_menu::in_middle(hwnd, (sx, sy)) {
                    pie_menu::close(hwnd, None);
                    return;
                }
            }
            let Some(key) = key else { return };
            // A modifier pressed while another key is held is part of a
            // chord, as much as one held while another is pressed.
            let chord = panels.iter().any(|p| (0..p.keys.len()).any(|k| p.presses.is_down(k)));
            let panel = &mut panels[i];
            let Some(key) = panel.presses.down(id, key) else { return };
            // The key just pressed is up or latched, which this leaves alone.
            for k in panels.iter_mut().flat_map(|p| &mut p.keys) {
                if let Sends::Modifier { state, .. } = &mut k.sends {
                    state.other_key();
                }
            }
            let panel = &mut panels[i];
            // Send first, draw after: the key reaches the app sooner.
            let down = match &mut panel.keys[key].sends {
                Sends::Hold(vk) => Some(*vk),
                Sends::Modifier { vk, state } => {
                    let down = state.press(Instant::now());
                    if chord {
                        state.other_key();
                    }
                    down.then_some(*vk)
                }
                Sends::Repeat { mods, key: k, .. } => {
                    send::shortcut(mods, *k);
                    SetTimer(hwnd, repeat_timer(key), REPEAT_DELAY, None);
                    None
                }
                _ => None,
            };
            if let Some(vk) = down {
                hold(vk, true);
            }
            draw(panel);
            // Over the panel, drawn with the key held.
            if let Sends::Pie(name) = &panel.keys[key].sends {
                // The parser made sure the set is there.
                let set = panel.sets.iter().find(|s| s.name.eq_ignore_ascii_case(name));
                let labels: Vec<&str> = set.map_or(Vec::new(), |s| s.keys.iter().map(|k| k.label.as_str()).collect());
                pie_menu::open(pie_menu::Opening {
                    panel: hwnd,
                    pointer: id,
                    edge: panel.edge,
                    at: (sx, sy),
                    key: panel.key,
                    set: name,
                    labels: &labels,
                    palette: palette(),
                    alpha: panel.current_alpha(),
                });
            }
        });
    }
}

/// A pointer lifted, or was taken away (`cancelled`): release its key if
/// nobody else holds it. A modifier tapped briefly latches instead. A key
/// that acts once acts now (see `act`), and so does the pie item a finger
/// holding a pie key lets go on.
unsafe fn pointer_up(hwnd: HWND, wp: WPARAM, cancelled: bool) {
    unsafe {
        let id = pointer_id(wp);
        let pie = pie_menu::release(hwnd, id, cancelled);
        with_panels(hwnd, |panels, i| {
            let panel = &mut panels[i];
            panel.starts.retain(|s| s.id != id);
            let lifted = panel.presses.up(id);
            let mut released = lifted;
            // What a key does on release is for a finger that lifted: a
            // contact the system took away starts no program, and sends
            // nothing more. Held keys still come up.
            if let Some(key) = released.filter(|_| cancelled) {
                if !matches!(panel.keys[key].sends, Sends::Hold(_) | Sends::Modifier { .. } | Sends::Repeat { .. }) {
                    debug_log!("取り消された接触: キー {key} は送らない");
                    released = None;
                }
            }
            let mut sent = false;
            let once = released.and_then(|key| match &mut panel.keys[key].sends {
                Sends::Hold(vk) => {
                    hold(*vk, false);
                    None
                }
                Sends::Modifier { vk, state } => {
                    match state.release(Instant::now(), panel.latch_time, cancelled) {
                        true => hold(*vk, false),
                        false => debug_log!("ラッチ 0x{vk:02X}"),
                    }
                    None
                }
                // Its sends were combined with the latches until now.
                Sends::Repeat { .. } => {
                    KillTimer(hwnd, repeat_timer(key));
                    sent = true;
                    None
                }
                once => Some(once.clone()),
            });
            if let Some(once) = once {
                act(panels, i, &once);
            }
            if let Some(pie_menu::Release::Run { set, item }) = &pie {
                if let Some(item) = pie_item(&panels[i], set, *item) {
                    act(panels, i, &item);
                }
            }
            let panel = &mut panels[i];
            if lifted.is_some() {
                draw(panel);
            }
            // Only the mouse that took the capture gives it back; a finger
            // lifting meanwhile must not cut the mouse off from its button-up.
            if panel.captured == Some(id) {
                panel.captured = None;
                ReleaseCapture();
            }
            if sent {
                let mode = panel.latch_mode;
                end_one_shot_latches(panels, mode);
            }
            // A mouse let go outside a panel it had captured: no leave
            // follows, so look where it is.
            let panel = &mut panels[i];
            if hovered(panel.edge) {
                panel.mouse_out = !in_reach(panel, cursor());
            }
            count_idle(panel);
        });
    }
}

/// Do what a key that acts once does, on panel `i` of `panels`: send its
/// shortcut, run its macro, type its text, start its program or turn its
/// page. A shortcut ends the one-shot latches once sent; a macro or text
/// ends them first. Held keys and pie keys do nothing here.
unsafe fn act(panels: &mut [Panel], i: usize, sends: &Sends) {
    unsafe {
        let mode = panels[i].latch_mode;
        // A whole sequence under a latched Ctrl would go wrong: Esc would
        // open Start, and typed text would turn into shortcuts.
        if matches!(sends, Sends::Macro(_) | Sends::Text(_)) {
            end_one_shot_latches(panels, mode);
        }
        match sends {
            Sends::Shortcut { mods, key } => {
                send::shortcut(mods, *key);
                end_one_shot_latches(panels, mode);
            }
            // One macro at a time on a panel: their steps would mix.
            Sends::Macro(_) if panels[i].running.is_some() => debug_log!("マクロの実行中: 押されたマクロは送らない"),
            Sends::Macro(steps) => {
                panels[i].running = Some((steps.clone(), 0));
                run_macro(&mut panels[i]);
            }
            Sends::Text(text) => send::text(text),
            Sends::Run(command) => launch::run(command),
            Sends::Page(page) => turn(&mut panels[i], page),
            Sends::Hold(_) | Sends::Modifier { .. } | Sends::Repeat { .. } | Sends::Pie(_) => {}
        }
    }
}

/// What item `item` of key set `set` of `panel`'s settings sends.
fn pie_item(panel: &Panel, set: &str, item: usize) -> Option<Sends> {
    let set = panel.sets.iter().find(|s| s.name.eq_ignore_ascii_case(set))?;
    Some(Sends::of(&set.keys.get(item)?.action))
}

/// A tap on a menu left open lands on item `item` of key set `set`: run it
/// on the panel of `hwnd`, which opened the menu.
pub unsafe fn run_pie_item(hwnd: HWND, set: &str, item: usize) {
    let done = with_panels(hwnd, |panels, i| unsafe {
        if let Some(item) = pie_item(&panels[i], set, item) {
            act(panels, i, &item);
        }
        count_idle(&panels[i]);
    });
    if !done {
        debug_log!("パネルが使用中: パイの項目 {item} は送らない");
    }
}

/// A repeat key's timer fired: send its combination again, every interval
/// from now on. A timer whose key is no longer held stops.
unsafe fn repeat(hwnd: HWND, timer: WPARAM) {
    with_panel(hwnd, |panel| unsafe {
        let index = timer.wrapping_sub(1);
        match panel.keys.get(index).map(|k| &k.sends) {
            Some(Sends::Repeat { mods, key, interval }) if panel.presses.is_down(index) => {
                send::shortcut(mods, *key);
                // Replaces the delay the timer started with.
                SetTimer(hwnd, timer, *interval, None);
            }
            _ => {
                KillTimer(hwnd, timer);
            }
        }
    });
}

/// Send the running macro's steps from the next one on, up to a wait, which
/// a timer ends, or to the last step.
unsafe fn run_macro(panel: &mut Panel) {
    while let Some((steps, step)) = panel.running.take() {
        let Some(next) = steps.get(step) else { break };
        panel.running = Some((steps.clone(), step + 1));
        match *next {
            MacroStep::Keys { ref mods, key } => unsafe { send::shortcut(mods, key) },
            MacroStep::Wait(0) => {}
            MacroStep::Wait(ms) => {
                unsafe { SetTimer(panel.hwnd, MACRO_TIMER, ms, None) };
                return;
            }
        }
    }
    panel.running = None;
    unsafe { count_idle(panel) };
}

/// A macro's wait is over: carry on with its steps. While the panels are in
/// use the timer stays, and fires again.
unsafe fn macro_wait_over(hwnd: HWND) {
    with_panel(hwnd, |panel| unsafe {
        KillTimer(hwnd, MACRO_TIMER);
        run_macro(panel);
    });
}

/// A shortcut or a repeat key went out, or a macro or text is about to:
/// under one-shot latches, let go of every latched modifier on every panel.
unsafe fn end_one_shot_latches(panels: &mut [Panel], mode: LatchMode) {
    for panel in panels {
        let mut changed = false;
        for key in &mut panel.keys {
            if let Sends::Modifier { vk, state } = &mut key.sends {
                if state.shortcut_sent(mode) {
                    unsafe { hold(*vk, false) };
                    changed = true;
                }
            }
        }
        if changed {
            unsafe {
                draw(panel);
                count_idle(panel);
            }
        }
    }
}

/// Whether anything is held or latched on `panel`, or a macro of it is
/// running: while so, nothing counts toward hiding it, which would cut the
/// macro short.
fn busy(panel: &Panel) -> bool {
    panel.running.is_some()
        || panel.keys.iter().enumerate().any(|(i, k)| {
            panel.presses.is_down(i) || matches!(k.sends, Sends::Modifier { state, .. } if state.is_latched())
        })
}

/// Start both counts toward hiding `panel` afresh, `auto_hide_ms` and (for a
/// panel shown by hovering) `hover_ms` from the mouse leaving: it was shown,
/// used or let go of. Neither runs while it is hidden or busy.
unsafe fn count_idle(panel: &Panel) {
    unsafe {
        KillTimer(panel.hwnd, AUTO_HIDE_TIMER);
        if panel.auto_hide_ms > 0 && IsWindowVisible(panel.hwnd) != 0 && !busy(panel) {
            SetTimer(panel.hwnd, AUTO_HIDE_TIMER, panel.auto_hide_ms, None);
        }
        count_leave(panel);
    }
}

/// Start or stop counting toward hiding a panel shown by hovering, as the
/// mouse is outside it or not.
unsafe fn count_leave(panel: &Panel) {
    unsafe {
        KillTimer(panel.hwnd, LEAVE_TIMER);
        let counts = hovered(panel.edge) && panel.hover_ms > 0 && panel.mouse_out;
        if counts && IsWindowVisible(panel.hwnd) != 0 && !busy(panel) {
            SetTimer(panel.hwnd, LEAVE_TIMER, panel.hover_ms, None);
        }
    }
}

/// A timer hiding the panel of `hwnd` ran out. Should anything be held or
/// latched by now, the count starts again once it is let go of.
unsafe fn idle_over(hwnd: HWND, timer: WPARAM) {
    let mut hide = None;
    with_panel(hwnd, |panel| unsafe {
        KillTimer(hwnd, timer);
        let gone = timer == AUTO_HIDE_TIMER || hovered(panel.edge);
        if gone && !busy(panel) {
            debug_log!("パネル {}: {}", edge_name(panel.edge), if timer == AUTO_HIDE_TIMER { "自動非表示" } else { "マウスが離れた" });
            hide = Some(panel.edge);
        }
    });
    if let Some(edge) = hide {
        unsafe { set_hidden(edge, true) };
    }
}

/// The mouse moved to `at` over or around the panel of `hwnd`, or left it
/// (none: where it is now counts). A mouse the panel has captured gets here
/// from outside it, too.
unsafe fn mouse_moved(hwnd: HWND, wp: WPARAM, at: Option<(i32, i32)>) {
    let mut kind = 0;
    if unsafe { GetPointerType(pointer_id(wp), &mut kind) } == 0 || kind != PT_MOUSE {
        return;
    }
    let at = at.unwrap_or_else(cursor);
    with_panel(hwnd, |panel| unsafe {
        let out = !in_reach(panel, at);
        set_mouse_out(panel, out);
    });
}

/// The mouse is over handle `hwnd` (`on`) or has left it: for a panel shown
/// by hovering, it has gone once it is off both the handle and the panel.
unsafe fn mouse_on_handle(hwnd: HWND, on: bool) {
    PANELS.with(|panels| {
        let Ok(mut panels) = panels.try_borrow_mut() else { return };
        if let Some(panel) = panels.iter_mut().find(|p| p.handle_hwnd == hwnd) {
            let out = !on && !in_reach(panel, cursor());
            unsafe { set_mouse_out(panel, out) };
        }
    });
}

/// Note whether the mouse is `out` of a panel shown by hovering, and count
/// toward hiding it from when it went out.
unsafe fn set_mouse_out(panel: &mut Panel, out: bool) {
    if hovered(panel.edge) && panel.mouse_out != out {
        panel.mouse_out = out;
        unsafe { count_leave(panel) };
    }
}

/// Where the mouse is now.
fn cursor() -> (i32, i32) {
    let mut at = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut at) };
    (at.x, at.y)
}

/// Whether `(x, y)` is on `panel` or on its handle while that is up.
fn in_reach(panel: &Panel, (x, y): (i32, i32)) -> bool {
    if panel.rect.contains(x, y) {
        return true;
    }
    let mut r: RECT = unsafe { zeroed() };
    let up = !panel.handle_hwnd.is_null() && unsafe { IsWindowVisible(panel.handle_hwnd) != 0 && GetWindowRect(panel.handle_hwnd, &mut r) != 0 };
    up && x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

/// Whether a pointer message is the mouse, button up: hovering.
unsafe fn hovering_mouse(wp: WPARAM) -> bool {
    let mut kind = 0;
    let found = unsafe { GetPointerType(pointer_id(wp), &mut kind) } != 0;
    found && kind == PT_MOUSE && ((wp >> 16) as u32 & POINTER_MESSAGE_FLAG_INCONTACT) == 0
}

/// The edge and `hover_ms` of the panel whose handle is `hwnd`.
fn handle_panel(hwnd: HWND) -> Option<(Edge, u32)> {
    PANELS.with(|panels| {
        let panels = panels.try_borrow().ok()?;
        panels.iter().find(|p| p.handle_hwnd == hwnd).map(|p| (p.edge, p.hover_ms))
    })
}

/// The mouse is on handle `hwnd`: once it has rested there for `hover_ms`,
/// the panel shows. Moving on the handle does not start the count again.
unsafe fn hover_on(hwnd: HWND) {
    let Some((edge, ms)) = handle_panel(hwnd) else { return };
    if ms > 0 && HIDDEN.get()[edge as usize] && HOVERING.get() != hwnd {
        HOVERING.set(hwnd);
        if unsafe { SetTimer(hwnd, HOVER_TIMER, ms, None) } == 0 {
            HOVERING.set(null_mut());
        }
    }
}

/// The mouse left handle `hwnd` before its panel showed.
unsafe fn hover_off(hwnd: HWND) {
    if HOVERING.get() == hwnd {
        HOVERING.set(null_mut());
        unsafe { KillTimer(hwnd, HOVER_TIMER) };
    }
}

/// The mouse rested on handle `hwnd` for `hover_ms`: show its panel, to hide
/// again once the mouse has left it.
unsafe fn hover_over(hwnd: HWND) {
    unsafe { hover_off(hwnd) };
    let Some((edge, _)) = handle_panel(hwnd) else { return };
    if PAUSED.get() || !HIDDEN.get()[edge as usize] {
        return;
    }
    debug_log!("パネル {}: マウスのホバー", edge_name(edge));
    // Before showing: the handle stays up (see HOVERED).
    set_hovered(edge, true);
    unsafe { show(edge, Look::Normal) };
    PANELS.with_borrow_mut(|panels| {
        for panel in panels.iter_mut().filter(|p| p.edge == edge) {
            unsafe {
                // Over its handle, which keeps the mouse it has.
                SetWindowPos(panel.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                panel.mouse_out = !in_reach(panel, cursor());
                count_leave(panel);
            }
        }
    });
}

/// How far a panel is a key thick across its edge: how far it slides.
fn thickness(panel: &Panel) -> i32 {
    if panel.edge.is_side() { panel.rect.w } else { panel.rect.h }
}

/// How far out of its place, toward the screen edge, the panel is now.
fn slid_out(panel: &Panel) -> i32 {
    let Some(s) = panel.slide else { return 0 };
    let to = if s.inward { 0 } else { thickness(panel) };
    let done = (s.at.elapsed().as_millis() as f64 / s.ms.max(1) as f64).min(1.0);
    s.from + ((to - s.from) as f64 * done).round() as i32
}

/// The part of the panel in the screen now: where on the screen, how big,
/// and where in the panel's picture. Nothing goes past the edge, so a panel
/// sliding on an edge another monitor adjoins never shows on that monitor.
/// At least a pixel is in: a layered window cannot be empty.
fn in_part(panel: &Panel) -> (POINT, SIZE, POINT) {
    let r = panel.rect;
    let out = slid_out(panel).clamp(0, thickness(panel) - 1);
    match panel.edge {
        Edge::Left => (POINT { x: r.x, y: r.y }, SIZE { cx: r.w - out, cy: r.h }, POINT { x: out, y: 0 }),
        Edge::Right => (POINT { x: r.x + out, y: r.y }, SIZE { cx: r.w - out, cy: r.h }, POINT { x: 0, y: 0 }),
        Edge::Top => (POINT { x: r.x, y: r.y }, SIZE { cx: r.w, cy: r.h - out }, POINT { x: 0, y: out }),
        Edge::Bottom => (POINT { x: r.x, y: r.y + out }, SIZE { cx: r.w, cy: r.h - out }, POINT { x: 0, y: 0 }),
    }
}

/// Start `panel` sliding in (`inward`) or out from where it is now. A panel
/// coming in from hidden starts all the way out. Returns false, and does
/// nothing, with slides off or no timer to drive them: it then comes or
/// goes at once.
unsafe fn slide(panel: &mut Panel, inward: bool) -> bool {
    if panel.slide_ms == 0 {
        return false;
    }
    let from = match panel.slide {
        Some(_) => slid_out(panel),
        None if inward => thickness(panel),
        None => 0,
    };
    // Turned back part way, it takes the share of the time the rest needs.
    let way = if inward { from } else { thickness(panel) - from };
    let ms = (panel.slide_ms as i64 * way as i64 / thickness(panel).max(1) as i64).max(1) as u32;
    panel.slide = Some(Slide { inward, from, at: Instant::now(), ms });
    unsafe {
        if SetTimer(panel.hwnd, SLIDE_TIMER, SLIDE_STEP, None) == 0 {
            debug_log!("パネル {}: スライドのタイマーを作れない", edge_name(panel.edge));
            panel.slide = None;
            draw(panel);
            return false;
        }
        draw(panel);
    }
    true
}

/// The slide timer: move the panel on, and once the slide is over, stop the
/// timer and leave it in place, or hidden.
unsafe fn slide_step(panel: &mut Panel) {
    let Some(s) = panel.slide else {
        unsafe { KillTimer(panel.hwnd, SLIDE_TIMER) };
        return;
    };
    unsafe {
        let over = s.at.elapsed().as_millis() >= s.ms as u128;
        if !over {
            draw(panel);
            return;
        }
        KillTimer(panel.hwnd, SLIDE_TIMER);
        if !s.inward {
            // Gone first, then whole again for the next time it shows.
            ShowWindow(panel.hwnd, SW_HIDE);
            // The room goes back once it is gone.
            undock(panel);
        }
        panel.slide = None;
        draw(panel);
        if s.inward {
            // Whole and in place now: it counts from here toward hiding.
            count_idle(panel);
        }
    }
}

fn appbar_data(hwnd: HWND) -> APPBARDATA {
    let mut data: APPBARDATA = unsafe { zeroed() };
    data.cbSize = size_of::<APPBARDATA>() as u32;
    data.hWnd = hwnd;
    data
}

/// Register `panel` as an AppBar if it docks, and reserve a strip of its
/// thickness along the whole of its edge: the shell keeps maximised windows
/// out of it. Asked again, it takes the room it needs now. The panel itself
/// stays where its layout put it.
unsafe fn dock(panel: &mut Panel) {
    if !panel.dock {
        return;
    }
    let mut data = appbar_data(panel.hwnd);
    unsafe {
        if !panel.docked {
            data.uCallbackMessage = APPBAR_CALLBACK;
            if SHAppBarMessage(ABM_NEW, &mut data) == 0 {
                debug_log!("パネル {}: AppBar に登録できない", edge_name(panel.edge));
                return;
            }
            panel.docked = true;
            debug_log!("パネル {}: ドッキング", edge_name(panel.edge));
        }
        let m = panel.screen;
        data.rc = RECT { left: m.x, top: m.y, right: m.x + m.w, bottom: m.y + m.h };
        let thick = if panel.edge.is_side() { panel.rect.w } else { panel.rect.h };
        data.uEdge = match panel.edge {
            Edge::Left => ABE_LEFT,
            Edge::Right => ABE_RIGHT,
            Edge::Top => ABE_TOP,
            Edge::Bottom => ABE_BOTTOM,
        };
        // The shell may move the strip off the very edge (another AppBar,
        // the taskbar); it keeps its thickness from wherever it lands.
        let fit = |rc: &mut RECT| match panel.edge {
            Edge::Left => rc.right = rc.left + thick,
            Edge::Right => rc.left = rc.right - thick,
            Edge::Top => rc.bottom = rc.top + thick,
            Edge::Bottom => rc.top = rc.bottom - thick,
        };
        fit(&mut data.rc);
        SHAppBarMessage(ABM_QUERYPOS, &mut data);
        fit(&mut data.rc);
        SHAppBarMessage(ABM_SETPOS, &mut data);
    }
}

/// Give the room of a docked `panel` back.
unsafe fn undock(panel: &mut Panel) {
    if panel.docked {
        let mut data = appbar_data(panel.hwnd);
        unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
        panel.docked = false;
        debug_log!("パネル {}: ドッキング解除", edge_name(panel.edge));
    }
}

/// EdgePad is ending: every docked panel gives its room back.
pub unsafe fn undock_all() {
    PANELS.with(|panels| match panels.try_borrow_mut() {
        Ok(mut panels) => {
            for panel in panels.iter_mut() {
                unsafe { undock(panel) };
            }
        }
        Err(_) => debug_log!("パネルが使用中: ドッキングを解除できない"),
    });
}

/// Explorer started again and forgot every AppBar: the panels shown and
/// docked register afresh.
pub unsafe fn redock_all() {
    PANELS.with(|panels| {
        let Ok(mut panels) = panels.try_borrow_mut() else { return };
        for panel in panels.iter_mut().filter(|p| p.docked) {
            panel.docked = false;
            unsafe { dock(panel) };
        }
    });
}

/// The space between a key's face and its cell, keys `key` pixels across:
/// half the gap between neighbouring keys.
pub fn key_gap(key: i32) -> i32 {
    (key / 16).max(2)
}

/// The font key labels are written in, keys `key` pixels across. The
/// caller deletes it.
pub unsafe fn key_font(key: i32) -> HFONT {
    let (bold, charset, quality) = (FW_SEMIBOLD as i32, DEFAULT_CHARSET as u32, CLEARTYPE_QUALITY as u32);
    unsafe { CreateFontW(-(key * 2 / 7), 0, 0, 0, bold, 0, 0, 0, charset, 0, 0, quality, 0, w!("Segoe UI")) }
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

        let colours = palette();
        let background = CreateSolidBrush(colorref(colours.background));
        FillRect(mem, &RECT { left: 0, top: 0, right: r.w, bottom: r.h }, background);
        DeleteObject(background);

        let face = CreateSolidBrush(colorref(colours.face));
        let held = CreateSolidBrush(colorref(colours.held));
        let latched_face = CreateSolidBrush(colorref(colours.latched));
        let font = key_font(key);
        let old_brush = SelectObject(mem, face);
        let old_pen = SelectObject(mem, GetStockObject(NULL_PEN));
        let old_font = SelectObject(mem, font);
        SetBkMode(mem, TRANSPARENT as i32);

        let gap = key_gap(key);
        let corner = key / 6;
        for (i, k) in panel.keys.iter().enumerate() {
            let Some(at) = k.rect else { continue };
            let mut cell = RECT { left: at.x + gap, top: at.y + gap, right: at.x + at.w - gap, bottom: at.y + at.h - gap };
            let latched = matches!(k.sends, Sends::Modifier { state, .. } if state.is_latched());
            let (brush, text) = match () {
                _ if panel.presses.is_down(i) => (held, colours.held_text),
                _ if latched => (latched_face, colours.latched_text),
                _ => (face, colours.text),
            };
            SelectObject(mem, brush);
            SetTextColor(mem, colorref(text));
            RoundRect(mem, cell.left, cell.top, cell.right + 1, cell.bottom + 1, corner, corner);
            DrawTextW(mem, k.label.as_ptr(), k.label.len() as i32, &mut cell, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }

        SelectObject(mem, old_font);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        DeleteObject(font);
        DeleteObject(face);
        DeleteObject(held);
        DeleteObject(latched_face);

        // GDI leaves alpha at 0; make every pixel opaque and let the constant
        // alpha below give the panel its opacity, shown or dimmed.
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (r.w * r.h) as usize);
        for p in pixels {
            *p |= 0xFF00_0000;
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: panel.current_alpha(),
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // Sliding, only the part already in is shown, against the edge.
        let (at, size, from) = in_part(panel);
        let shown = UpdateLayeredWindow(hwnd, screen, &at, &size, mem, &from, 0, &blend, ULW_ALPHA) != 0;

        SelectObject(mem, old_bitmap);
        DeleteObject(dib);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        shown
    }
}

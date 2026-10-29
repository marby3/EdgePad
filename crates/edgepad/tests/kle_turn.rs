//! Acceptance tests for Issue #126: KLE keys turned (r, rx, ry) are drawn
//! turned on the panel, and pressed only inside their turned shape.
//!
//! R turns 45° about its own middle: a diamond reaching past the square it
//! started as, and leaving that square's corners. N, not turned, sits to its
//! right. The set is on the top edge, as laid out.

mod common;

use windows_sys::Win32::Foundation::{POINT, RECT};
use common::*;

const KLE: &str = r#"[{r:45,rx:0.5,ry:0.5,x:-0.5,y:-0.5},"R"],[{r:0,rx:0,ry:0,x:3},"N"]"#;
const R: u16 = b'R' as u16;

fn settings() -> String {
    "[general]\ntheme = light\n[set.pad]\nlayout = kle:pad.json\nkey = R | hold | R\nkey = N | hold | N\n\
     [profile.default]\nleft = none\ntop = pad\n"
        .into()
}

/// The panel on the top edge of the primary monitor, once there is one.
fn top_panel(app: &Running) -> RECT {
    let m = primary_monitor();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(r) = panels_now(app).into_iter().find(|r| r.top == m.top) {
            return r;
        }
        assert!(std::time::Instant::now() < end, "a panel on the top edge: {} panels", panels_now(app).len());
        pump(50);
    }
}

/// EdgePad running the settings, the KLE file beside them.
fn launch_turned() -> Running {
    let app = launch_with(Some(&settings()), None);
    let dir = exe_dir(&app);
    std::fs::write(dir.join("pad.json"), KLE).expect("write the KLE file");
    std::fs::write(dir.join("edgepad.ini"), settings() + "\n").expect("save edgepad.ini");
    app
}

/// R's middle on the panel `panel`, keys `key` pixels: the layout starts a
/// quarter left of and above R's square, where R's corners reach.
fn middle(panel: RECT, key: i32) -> POINT {
    POINT { x: panel.left + key * 3 / 4, y: panel.top + key * 3 / 4 }
}

fn colour_at(p: POINT) -> [i32; 3] {
    channels(capture(RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 })[0])
}

fn near(a: [i32; 3], b: [i32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 4)
}

// AC2
#[test]
fn a_turned_key_is_drawn_turned_and_the_panel_holds_it() {
    let _desktop = desktop();
    let app = launch_turned();
    let key = key_px();
    let panel = top_panel(&app);
    // From a quarter left of R to N's right end, from a quarter above R to
    // a quarter below it: R's corners reach 0.21 of a key out.
    assert_eq!((panel.right - panel.left, panel.bottom - panel.top), ((17 * key + 2) / 4, (6 * key + 2) / 4), "panel {} x {}", panel.right - panel.left, panel.bottom - panel.top);
    let c = middle(panel, key);
    let face = colour_at(POINT { x: c.x, y: c.y + key * 3 / 10 });
    let background = colour_at(POINT { x: panel.left + 1, y: panel.top + 1 });
    assert!(!near(face, background), "precondition: a key's face and the background told apart");
    // Past the square, inside the diamond: the key.
    let tip = colour_at(POINT { x: c.x + key * 55 / 100, y: c.y });
    assert!(near(tip, face), "R's tip is drawn: {tip:?}, face {face:?}");
    // The square's corner, outside the diamond: the background.
    let corner = colour_at(POINT { x: c.x + key * 42 / 100, y: c.y + key * 42 / 100 });
    assert!(near(corner, background), "the square's corner is not: {corner:?}, background {background:?}");
}

// AC3
#[test]
fn a_turned_key_is_pressed_inside_its_shape_only() {
    let _desktop = desktop();
    let app = launch_turned();
    let key = key_px();
    let c = middle(top_panel(&app), key);
    let log = KeyLog::start();
    let mut touch = Touch::new();
    // The square's corner, outside the diamond: nothing.
    touch.down(1, POINT { x: c.x + key * 42 / 100, y: c.y + key * 42 / 100 });
    pump(100);
    touch.up(1);
    pump(300);
    assert!(log.keys().is_empty(), "the corner presses nothing: {:?}", log.keys());
    // Past the square, inside the diamond: R.
    touch.down(1, POINT { x: c.x + key * 55 / 100, y: c.y });
    pump(100);
    touch.up(1);
    log.wait_for(2);
    assert_eq!(log.keys(), [(R, false), (R, true)], "the tip presses R");
}

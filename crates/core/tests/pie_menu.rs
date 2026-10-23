//! Acceptance tests for Issue #68: which item of a pie menu a finger points
//! at, from how far it has moved since it pressed the pie key (spec 4.6).

use edgepad_core::config::Edge;
use edgepad_core::pie::item;

/// Key size in pixels: the centre is half of it across.
const KEY: i32 = 56;

/// Far enough out of the centre.
const FAR: f64 = 4.0 * KEY as f64;

/// The movement (into the screen, along the edge in reading order) toward
/// the middle of item `i` of `n`: the half circle runs from -90° (first) to
/// +90° (last).
fn middle_of(i: usize, n: usize) -> (f64, f64) {
    let angle = (-90.0 + (i as f64 + 0.5) * 180.0 / n as f64).to_radians();
    (FAR * angle.cos(), FAR * angle.sin())
}

/// (dx, dy) on screen for a movement `inward` into the screen and `along`
/// the edge in reading order: down for the side edges, right for the others.
fn on_screen(edge: Edge, inward: f64, along: f64) -> (i32, i32) {
    let (dx, dy) = match edge {
        Edge::Left => (inward, along),
        Edge::Right => (-inward, along),
        Edge::Top => (along, inward),
        Edge::Bottom => (along, -inward),
    };
    (dx.round() as i32, dy.round() as i32)
}

// AC1
#[test]
fn on_the_left_edge_the_half_circle_to_the_right_is_split_evenly_from_the_top() {
    for n in 2..=8 {
        for i in 0..n {
            let (inward, along) = middle_of(i, n);
            let (dx, dy) = on_screen(Edge::Left, inward, along);
            assert_eq!(item(Edge::Left, dx, dy, KEY, n), Some(i), "{n} items, toward the middle of item {i}: ({dx}, {dy})");
        }
    }
    // Item 0 is up, the last one down.
    assert_eq!(item(Edge::Left, 10, -FAR as i32, KEY, 4), Some(0));
    assert_eq!(item(Edge::Left, 10, FAR as i32, KEY, 4), Some(3));
}

// AC2
#[test]
fn the_other_edges_open_into_the_screen_in_reading_order() {
    for edge in [Edge::Right, Edge::Top, Edge::Bottom] {
        for n in 2..=8 {
            for i in 0..n {
                let (inward, along) = middle_of(i, n);
                let (dx, dy) = on_screen(edge, inward, along);
                assert_eq!(item(edge, dx, dy, KEY, n), Some(i), "{edge:?}, {n} items, toward item {i}: ({dx}, {dy})");
            }
        }
    }
    // Spot checks on screen: right opens left, top down, bottom up; first
    // is up on the right edge and left on the top and bottom edges.
    assert_eq!(item(Edge::Right, -10, -200, KEY, 2), Some(0));
    assert_eq!(item(Edge::Top, -200, 10, KEY, 2), Some(0));
    assert_eq!(item(Edge::Bottom, 200, -10, KEY, 2), Some(1));
}

// AC3
#[test]
fn inside_half_a_key_of_the_press_is_the_centre_and_cancels() {
    let r = KEY / 2;
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        for degrees in (0..360).step_by(15) {
            let a = (degrees as f64).to_radians();
            // Just inside the centre, wherever it points.
            let (dx, dy) = ((r as f64 - 1.0) * a.cos(), (r as f64 - 1.0) * a.sin());
            assert_eq!(item(edge, dx as i32, dy as i32, KEY, 4), None, "{edge:?} at {degrees}°");
        }
        assert_eq!(item(edge, 0, 0, KEY, 4), None, "{edge:?}: not moved");
    }
    assert!(item(Edge::Left, r, 0, KEY, 4).is_some(), "half a key out is out of the centre");
}

// AC4
#[test]
fn toward_the_edge_cancels_however_far_and_along_it_is_the_first_or_last_item() {
    for n in [2, 5, 8] {
        for (dx, dy) in [(-1, 0), (-1, -300), (-1, 300), (-300, 0), (-30, -30)] {
            assert_eq!(item(Edge::Left, dx, dy, KEY, n), None, "{n} items: ({dx}, {dy}) is off the left edge's side");
        }
        assert_eq!(item(Edge::Left, 0, -300, KEY, n), Some(0), "{n} items: straight up");
        assert_eq!(item(Edge::Left, 0, 300, KEY, n), Some(n - 1), "{n} items: straight down");
        assert_eq!(item(Edge::Right, 1, 0, KEY, n), None);
        assert_eq!(item(Edge::Top, 0, -1, KEY, n), None);
        assert_eq!(item(Edge::Bottom, 300, 1, KEY, n), None);
    }
}

//! Which item of a pie menu a pointer points at, and where its buttons go
//! (spec 4.6 and 5.6).
//!
//! The menu is a half circle around where the pie key was pressed, opening
//! into the screen and split evenly among its items. They run in the order
//! of a panel's keys: top to bottom on the side edges, left to right on the
//! top and bottom ones. The middle, within half a key of the press, and the
//! side toward the edge point at nothing.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::config::{Edge, PIE_ITEMS};

/// The item of `items` a pointer points at, `dx`, `dy` pixels from the middle
/// of a pie menu opened from `edge`, with keys `key` pixels across: where a
/// finger has slid to since pressing the pie key, or where it taps a menu
/// left open. None for the middle or the edge's side, where letting go
/// cancels. The offsets and `key` are in the same pixels, and `items` is
/// what the settings allow (2 to 8).
pub fn item(edge: Edge, dx: i32, dy: i32, key: i32, items: usize) -> Option<usize> {
    debug_assert!(PIE_ITEMS.contains(&items) && key > 0, "{items} items, key {key}");
    // Wide enough that squaring any offset cannot overflow.
    let (dx, dy, key) = (i128::from(dx), i128::from(dy), i128::from(key));
    // Within half a key: the middle.
    if 4 * (dx * dx + dy * dy) < key * key {
        return None;
    }
    // Movement into the screen, and along the edge in reading order.
    let (inward, along) = match edge {
        Edge::Left => (dx, dy),
        Edge::Right => (-dx, dy),
        Edge::Top => (dy, dx),
        Edge::Bottom => (-dy, dx),
    };
    if inward < 0 {
        return None;
    }
    // From -90° (the first item) to +90° (the last), as a share of the half.
    let share = ((along as f64).atan2(inward as f64) + FRAC_PI_2) / PI;
    Some(((share * items as f64) as usize).min(items - 1))
}

/// Where the buttons of a pie menu of `items` opened from `edge` go, keys
/// `key` pixels across: the middle of each, in order, as an offset from the
/// middle of the menu. They sit on an arc, each in the middle of the
/// direction that points at it, as close in as they go without two
/// neighbours, squares a key across, overlapping, and at least one and a
/// half keys out.
pub fn buttons(edge: Edge, items: usize, key: i32) -> Vec<(i32, i32)> {
    debug_assert!(PIE_ITEMS.contains(&items) && key > 0, "{items} items, key {key}");
    let key = f64::from(key);
    let angle = |i: usize| -FRAC_PI_2 + (i as f64 + 0.5) * PI / items as f64;
    let mut radius = 1.5 * key;
    for i in 1..items {
        let (a, b) = (angle(i - 1), angle(i));
        // Squares apart when far enough apart across or along.
        let apart = (a.cos() - b.cos()).abs().max((a.sin() - b.sin()).abs());
        radius = radius.max(key / apart);
    }
    (0..items)
        .map(|i| {
            let (inward, along) = (radius * angle(i).cos(), radius * angle(i).sin());
            let (dx, dy) = match edge {
                Edge::Left => (inward, along),
                Edge::Right => (-inward, along),
                Edge::Top => (along, inward),
                Edge::Bottom => (along, -inward),
            };
            (dx.round() as i32, dy.round() as i32)
        })
        .collect()
}

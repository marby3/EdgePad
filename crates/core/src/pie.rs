//! Which item of a pie menu a pointer points at (spec 4.6).
//!
//! The menu is a half circle around where the pie key was pressed, opening
//! into the screen and split evenly among its items. They run in the order
//! of a panel's keys: top to bottom on the side edges, left to right on the
//! top and bottom ones. The middle, within half a key of the press, and the
//! side toward the edge point at nothing.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::config::Edge;

/// The item of `items` a pointer points at after moving (`dx`, `dy`) pixels
/// from where it pressed a pie key on `edge`, with keys `key` pixels across;
/// none for the middle or the edge's side, where letting go cancels.
pub fn item(edge: Edge, dx: i32, dy: i32, key: i32, items: usize) -> Option<usize> {
    let (dx, dy, key) = (i64::from(dx), i64::from(dy), i64::from(key));
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

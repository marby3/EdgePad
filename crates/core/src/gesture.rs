//! Telling a swipe from a tap.
//!
//! A pointer that went down and has since moved by (`dx`, `dy`) pixels is a
//! swipe once it has gone at least the threshold in one direction across its
//! edge, and further across than along. Anything less, when it lifts, is a tap.

use crate::config::Edge;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swipe {
    /// Away from the screen edge, into the screen: from a handle, shows.
    Inward,
    /// Toward the screen edge: on a panel, hides.
    Outward,
}

/// The swipe a pointer has made on `edge` after moving (`dx`, `dy`), if it
/// has gone `threshold` pixels or more across the edge.
pub fn swipe(edge: Edge, dx: i32, dy: i32, threshold: i32) -> Option<Swipe> {
    // Movement into the screen, and along the edge.
    let (inward, along) = match edge {
        Edge::Left => (dx, dy),
        Edge::Right => (-dx, dy),
        Edge::Top => (dy, dx),
        Edge::Bottom => (-dy, dx),
    };
    if inward.abs() < threshold || inward.abs() <= along.abs() {
        return None;
    }
    Some(if inward > 0 { Swipe::Inward } else { Swipe::Outward })
}

/// Whether a pointer that moved (`dx`, `dy`) before lifting was a tap.
pub fn is_tap(dx: i32, dy: i32, threshold: i32) -> bool {
    dx.abs() < threshold && dy.abs() < threshold
}

//! Flicks (Issue #157): a key pressed and slid up, down, left or right does
//! another action of its own, its flick that way, when it lifts.
//!
//! The way is the screen's, whatever the panel's edge. A press that moved
//! less than half a key, as the middle of a pie menu is told
//! (`pie::in_middle`), did not move: the key does its own action. A key
//! with flicks is never a swipe that hides its panel.

use crate::config::{Edge, KeyDef};
use crate::gesture::{self, Swipe};
use crate::pie;

/// A way to flick, as the settings file names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    /// Every way, in the order a key keeps its flicks.
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    /// Its name in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Dir::Up => "up",
            Dir::Down => "down",
            Dir::Left => "left",
            Dir::Right => "right",
        }
    }

    /// The way named `name`, in any case.
    pub fn parse(name: &str) -> Option<Dir> {
        Dir::ALL.into_iter().find(|d| d.name().eq_ignore_ascii_case(name))
    }
}

/// The way a press went, (`dx`, `dy`) pixels from where it went down on a
/// key `key` pixels across: None under half a key; else up, down, left or
/// right, whichever it went most, across at 45°.
pub fn direction(dx: i32, dy: i32, key: i32) -> Option<Dir> {
    if pie::in_middle(dx, dy, key) {
        return None;
    }
    Some(match dx.unsigned_abs() >= dy.unsigned_abs() {
        true if dx < 0 => Dir::Left,
        true => Dir::Right,
        false if dy < 0 => Dir::Up,
        false => Dir::Down,
    })
}

/// What a press on a key comes to as it lifts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    /// The key's own action.
    Own,
    /// Its flick that way.
    Flick(Dir),
    /// Flicked where it has none: nothing.
    Nothing,
    /// A swipe toward the edge, on a key with no flick: the panel hides.
    Hide,
}

/// What a press on `key`, on a panel of `edge` with keys `size` pixels
/// across, comes to as it lifts after moving (`dx`, `dy`): a swipe to hide
/// is one of `threshold` pixels or more (`gesture::swipe`).
pub fn release(key: &KeyDef, edge: Edge, dx: i32, dy: i32, size: i32, threshold: i32) -> Release {
    if key.flicks.iter().all(Option::is_none) {
        return match gesture::swipe(edge, dx, dy, threshold) {
            Some(Swipe::Outward) => Release::Hide,
            _ => Release::Own,
        };
    }
    match direction(dx, dy, size) {
        None => Release::Own,
        Some(d) if key.flicks[d as usize].is_some() => Release::Flick(d),
        Some(_) => Release::Nothing,
    }
}

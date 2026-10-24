//! Which item of a pie menu a pointer points at, and the shape of the menu
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

/// Movement `dx`, `dy` as `edge` sees it: into the screen, and along the
/// edge in reading order.
fn frame(edge: Edge, dx: f64, dy: f64) -> (f64, f64) {
    match edge {
        Edge::Left => (dx, dy),
        Edge::Right => (-dx, dy),
        Edge::Top => (dy, dx),
        Edge::Bottom => (-dy, dx),
    }
}

/// The middle of item `i` of `items`, from -90° (first) to +90° (last).
fn middle(i: usize, items: usize) -> f64 {
    -FRAC_PI_2 + (i as f64 + 0.5) * PI / items as f64
}

/// The size of a pie menu: a half ring, its hole the middle where letting go
/// cancels (half a key across), its outside two keys out, or further for
/// many items, until every sector is a key wide along the arc half way out,
/// where its label goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub inner: f64,
    pub outer: f64,
}

impl Ring {
    /// The ring of `items` with keys `key` pixels across. As for `item`,
    /// `items` is what the settings allow (2 to 8) and `key` is positive.
    pub fn new(items: usize, key: i32) -> Ring {
        debug_assert!(PIE_ITEMS.contains(&items) && key > 0, "{items} items, key {key}");
        let (key, inner) = (f64::from(key), f64::from(key) / 2.0);
        // Half way out, a sector is r * pi / items wide.
        let label = key * items as f64 / PI;
        Ring { inner, outer: (2.0 * key).max(2.0 * label - inner) }
    }

    /// Where labels go: half way between the hole and the outside.
    pub fn label_radius(&self) -> f64 {
        (self.inner + self.outer) / 2.0
    }

    /// How wide a sector of `items` is along the arc where its label goes.
    pub fn label_width(&self, items: usize) -> f64 {
        self.label_radius() * PI / items as f64
    }

    /// Where the label of each of `items` goes on a menu from `edge`, as an
    /// offset from its middle.
    pub fn labels(&self, edge: Edge, items: usize) -> Vec<(i32, i32)> {
        let r = self.label_radius();
        (0..items)
            .map(|i| {
                let a = middle(i, items);
                // Back from the edge's frame to the screen.
                let (inward, along) = (r * a.cos(), r * a.sin());
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

    /// How much of the pixel whose middle is `dx`, `dy` from the middle of
    /// a menu of `items` from `edge` the menu covers, from 0 to 1, and the
    /// item whose sector it is in. Sectors are `gap` pixels short of each
    /// boundary between them, so neighbours are twice that apart, as keys
    /// on a panel are. Edges are smoothed over a pixel.
    pub fn cover(&self, edge: Edge, items: usize, gap: f64, dx: f64, dy: f64) -> (f64, usize) {
        let (inward, along) = frame(edge, dx, dy);
        let d = inward.hypot(along);
        let a = along.atan2(inward);
        let share = (a + FRAC_PI_2) / PI;
        let sector = ((share * items as f64).max(0.0) as usize).min(items - 1);
        let smooth = |x: f64| (x + 0.5).clamp(0.0, 1.0);
        let mut cover = smooth(self.outer - d) * smooth(d - self.inner) * smooth(inward);
        // The boundaries either side of the sector; the half's own ends
        // are along the edge, where `inward` ends it.
        for b in [sector, sector + 1].into_iter().filter(|&b| b > 0 && b < items) {
            let b = -FRAC_PI_2 + b as f64 * PI / items as f64;
            let off = (inward * b.sin() - along * b.cos()).abs();
            cover *= smooth(off - gap);
        }
        (cover, sector)
    }
}

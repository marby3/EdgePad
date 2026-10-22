//! Panel geometry in physical pixels.

/// Side of a key in logical pixels (96 DPI).
pub const KEY_SIZE: i32 = 56;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Logical pixels to physical pixels at `dpi`, rounded to nearest.
pub fn scale(logical: i32, dpi: u32) -> i32 {
    (logical * dpi as i32 + 48) / 96
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Index of the key under (`x`, `y`) in a column of `key`-sized keys filling
/// `panel` top to bottom.
pub fn key_at(panel: Rect, key: i32, x: i32, y: i32) -> Option<usize> {
    panel.contains(x, y).then(|| ((y - panel.y) / key) as usize)
}

/// A one-key-wide column of `count` keys, flush with the left of `monitor`
/// and centred vertically.
pub fn left_column(monitor: Rect, key: i32, count: i32) -> Rect {
    let h = key * count;
    Rect { x: monitor.x, y: monitor.y + (monitor.h - h) / 2, w: key, h }
}

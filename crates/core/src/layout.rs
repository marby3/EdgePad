//! Panel geometry in physical pixels.

use crate::config::{Edge, Monitor};

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

/// A panel on `edge` of `monitor`, with keys `key` pixels square: the
/// panel's rectangle on screen and, for each key that fits, the key's
/// rectangle inside the panel, in order.
///
/// Keys run along the edge (top to bottom on the left and right, left to
/// right on the top and bottom), filling the line against the screen edge
/// first. A key `widths[i]` cells long that the current line cannot take
/// starts the next line. The panel is `lines` keys thick, as short as holds
/// every key, and centred along the edge. Keys that do not fit within the
/// edge's length are left out, along with every key after them.
///
/// No panel shorter than all the cells spread over `lines`, or than the
/// longest key, can hold every key, so the search for the length starts
/// there. `lines` is at least 1.
pub fn edge_panel(monitor: Rect, edge: Edge, key: i32, lines: usize, widths: &[usize]) -> (Rect, Vec<Rect>) {
    debug_assert!(lines >= 1, "a panel is at least one key thick");
    let side = edge.is_side();
    let edge_length = if side { monitor.h } else { monitor.w };
    let room = (edge_length / key).max(0) as usize;
    let total: usize = widths.iter().sum();
    let widest = widths.iter().copied().max().unwrap_or(0);
    let mut length = total.div_ceil(lines).max(widest).min(room);
    let cells = loop {
        let cells = place(widths, lines, length);
        if cells.len() == widths.len() || length >= room {
            break cells;
        }
        length += 1;
    };

    let (thick, long) = (lines as i32 * key, length as i32 * key);
    let panel = match edge {
        Edge::Left => Rect { x: monitor.x, y: monitor.y + (monitor.h - long) / 2, w: thick, h: long },
        Edge::Right => Rect { x: monitor.x + monitor.w - thick, y: monitor.y + (monitor.h - long) / 2, w: thick, h: long },
        Edge::Top => Rect { x: monitor.x + (monitor.w - long) / 2, y: monitor.y, w: long, h: thick },
        Edge::Bottom => Rect { x: monitor.x + (monitor.w - long) / 2, y: monitor.y + monitor.h - thick, w: long, h: thick },
    };
    let keys = cells
        .into_iter()
        .map(|(line, at, cells)| {
            // Line 0 is against the screen edge.
            let line = match edge {
                Edge::Left | Edge::Top => line,
                Edge::Right | Edge::Bottom => lines - 1 - line,
            } as i32;
            let (across, along, cells) = (line * key, at as i32 * key, cells as i32 * key);
            match side {
                true => Rect { x: across, y: along, w: key, h: cells },
                false => Rect { x: along, y: across, w: cells, h: key },
            }
        })
        .collect();
    (panel, keys)
}

/// (line, position along it, length) of each key, in cells, until the first
/// key that does not fit into `lines` lines of `length` cells.
fn place(widths: &[usize], lines: usize, length: usize) -> Vec<(usize, usize, usize)> {
    let (mut line, mut at, mut cells) = (0, 0, Vec::new());
    for &w in widths {
        if at + w > length {
            line += 1;
            at = 0;
        }
        if w > length || line >= lines {
            break;
        }
        cells.push((line, at, w));
        at += w;
    }
    cells
}

/// Which of `monitors` an edge set to `choice` goes on, as an index into
/// `monitors`. Numbers count the monitors left to right, and top to bottom
/// where their left edges match; a number past the last one, or a cursor
/// outside every monitor, means the primary one.
pub fn pick_monitor(monitors: &[Rect], primary: usize, cursor: (i32, i32), choice: Monitor) -> usize {
    match choice {
        Monitor::Primary => primary,
        Monitor::Number(n) => {
            let mut order: Vec<usize> = (0..monitors.len()).collect();
            order.sort_by_key(|&i| (monitors[i].x, monitors[i].y));
            (n as usize).checked_sub(1).and_then(|i| order.get(i)).copied().unwrap_or(primary)
        }
        Monitor::Cursor => monitors.iter().position(|m| m.contains(cursor.0, cursor.1)).unwrap_or(primary),
    }
}

/// The handle on `edge` of `monitor`: `width` pixels thick against the screen
/// edge, `length` percent of the edge long, its centre `position` percent
/// along the edge, moved inward where it would stick out.
pub fn handle(monitor: Rect, edge: Edge, width: i32, length: u8, position: u8) -> Rect {
    let edge_length = if edge.is_side() { monitor.h } else { monitor.w };
    let long = (edge_length * length as i32 / 100).max(1);
    let start = (edge_length * position as i32 / 100 - long / 2).clamp(0, (edge_length - long).max(0));
    match edge {
        Edge::Left => Rect { x: monitor.x, y: monitor.y + start, w: width, h: long },
        Edge::Right => Rect { x: monitor.x + monitor.w - width, y: monitor.y + start, w: width, h: long },
        Edge::Top => Rect { x: monitor.x + start, y: monitor.y, w: long, h: width },
        Edge::Bottom => Rect { x: monitor.x + start, y: monitor.y + monitor.h - width, w: long, h: width },
    }
}

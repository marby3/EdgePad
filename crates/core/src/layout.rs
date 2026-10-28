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

/// Where a key of a free layout sits, in quarters of a key: its top left
/// corner, the layout's top left at 0, 0, and its width and height
/// (Issue #101).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// A panel of keys at `places`, on `edge` of `monitor`, a key `key` pixels
/// square: the panel's rectangle on screen and each key's inside it, in
/// order (spec 5.2).
///
/// The panel is as large as holds every key, and centred along the edge.
/// On the top and the bottom it is as laid out. On the left it is turned
/// clockwise, and on the right anticlockwise: its bottom row against the
/// screen edge, as on the bottom. Labels are never turned, so neither is
/// anything here.
pub fn free_panel(monitor: Rect, edge: Edge, key: i32, places: &[Place]) -> (Rect, Vec<Rect>) {
    free_panel_in(monitor, edge, key, places, frame(places))
}

/// The left, top, width and height of the layout `places` spans, in
/// quarters.
pub fn frame(places: &[Place]) -> Place {
    let left = places.iter().map(|p| p.x).min().unwrap_or(0);
    let top = places.iter().map(|p| p.y).min().unwrap_or(0);
    let wide = places.iter().map(|p| p.x + p.w).max().unwrap_or(0) - left;
    let tall = places.iter().map(|p| p.y + p.h).max().unwrap_or(0) - top;
    Place { x: left, y: top, w: wide, h: tall }
}

/// `free_panel`, the layout taken to span `frame`: the settings window's
/// sample keeps the frame a drag started in, so the keys not dragged stay
/// where they are.
pub fn free_panel_in(monitor: Rect, edge: Edge, key: i32, places: &[Place], frame: Place) -> (Rect, Vec<Rect>) {
    let Place { x: left, y: top, w: wide, h: tall } = frame;
    // Quarters to pixels, each edge on its own: keys that touch still do.
    let px = |q: i32| (q * key + 2) / 4;
    let span = |from: i32, len: i32| (px(from), px(from + len) - px(from));
    let (long, thick) = (px(wide), px(tall));
    let panel = match edge {
        Edge::Left => Rect { x: monitor.x, y: monitor.y + (monitor.h - long) / 2, w: thick, h: long },
        Edge::Right => Rect { x: monitor.x + monitor.w - thick, y: monitor.y + (monitor.h - long) / 2, w: thick, h: long },
        Edge::Top => Rect { x: monitor.x + (monitor.w - long) / 2, y: monitor.y, w: long, h: thick },
        Edge::Bottom => Rect { x: monitor.x + (monitor.w - long) / 2, y: monitor.y + monitor.h - thick, w: long, h: thick },
    };
    let keys = places
        .iter()
        .map(|p| {
            let t = turn(edge, wide, tall, Place { x: p.x - left, y: p.y - top, ..*p });
            let ((x, w), (y, h)) = (span(t.x, t.w), span(t.y, t.h));
            Rect { x, y, w, h }
        })
        .collect();
    (panel, keys)
}

/// Place `p` of a free layout `wide` by `tall` quarters, its top left at
/// 0, 0, as the panel on `edge` turns it: in quarters from the panel's top
/// left, across and down as the screen goes.
pub fn turn(edge: Edge, wide: i32, tall: i32, p: Place) -> Place {
    match edge {
        Edge::Top | Edge::Bottom => p,
        Edge::Left => Place { x: tall - p.y - p.h, y: p.x, w: p.h, h: p.w },
        Edge::Right => Place { x: p.y, y: wide - p.x - p.w, w: p.h, h: p.w },
    }
}

/// The place of the layout that `turn` turns into `q` (the settings
/// window's sample, dragged as the screen goes).
pub fn unturn(edge: Edge, wide: i32, tall: i32, q: Place) -> Place {
    match edge {
        Edge::Top | Edge::Bottom => q,
        Edge::Left => Place { x: q.y, y: tall - q.x - q.w, w: q.h, h: q.w },
        Edge::Right => Place { x: wide - q.y - q.h, y: q.x, w: q.h, h: q.w },
    }
}

/// The keys on each page of a panel, as `edge_panel` would lay them out:
/// indices into `widths`, in the order they go on the panel.
///
/// When every key fits, there is one page, in the order written. Otherwise
/// the `sticky` keys (those that turn the pages) go at the end of every page,
/// and the others fill the pages in order, each taking as many as fit beside
/// them. Should not even one fit beside them, the keys that fit in the order
/// written make the only page.
pub fn pages(monitor: Rect, edge: Edge, key: i32, lines: usize, widths: &[usize], sticky: &[bool]) -> Vec<Vec<usize>> {
    let fits = |keys: &[usize]| {
        let w: Vec<usize> = keys.iter().map(|&i| widths[i]).collect();
        edge_panel(monitor, edge, key, lines, &w).1.len() == w.len()
    };
    let all: Vec<usize> = (0..widths.len()).collect();
    if fits(&all) {
        return vec![all];
    }
    let (pinned, flow): (Vec<usize>, Vec<usize>) = all.iter().partition(|&&i| sticky.get(i) == Some(&true));
    let mut pages = Vec::new();
    let mut at = 0;
    while at < flow.len() {
        let mut n = 0;
        while at + n < flow.len() && fits(&[&flow[at..at + n + 1], &pinned[..]].concat()) {
            n += 1;
        }
        if n == 0 {
            break;
        }
        pages.push([&flow[at..at + n], &pinned[..]].concat());
        at += n;
    }
    if pages.is_empty() {
        let fitting = edge_panel(monitor, edge, key, lines, widths).1.len();
        pages.push(all[..fitting].to_vec());
    }
    pages
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

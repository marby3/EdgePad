//! Monitors by the numbers Windows gives them (Issue #149), and the list the
//! settings window chooses an edge's monitor from.
//!
//! Windows's display settings number monitors by their GDI device names,
//! `\\.\DISPLAY1` and so on, not by where they are; `monitor = 2` means the
//! one called 2 there.

use crate::config::Monitor;

/// A connected monitor, as the settings window names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub number: u8,
    /// What the monitor reports itself as; empty when it reports nothing.
    pub name: String,
    /// The screen built into a laptop or tablet.
    pub internal: bool,
}

/// The number in a GDI device name: 2 for `\\.\DISPLAY2`.
pub fn display_number(device: &str) -> Option<u8> {
    device.strip_prefix(r"\\.\DISPLAY")?.parse().ok()
}

/// The monitor list's items, each with what choosing it writes: the primary
/// monitor, saying which it is now (`primary`, a number, or 0 for none
/// found), the one under the mouse, then `displays` by number. A number
/// `chosen` that no monitor has is listed too, so the file keeps it.
pub fn choices(displays: &[Display], primary: u8, chosen: Monitor) -> Vec<(Monitor, String)> {
    let main = match primary {
        0 => "主モニター".to_string(),
        n => format!("主モニター（今はモニター {n}）"),
    };
    let mut numbered: Vec<(u8, String)> = displays
        .iter()
        .map(|d| {
            let label = match (d.name.trim(), d.internal) {
                ("", true) => format!("モニター {}（内蔵ディスプレイ）", d.number),
                ("", false) => format!("モニター {}", d.number),
                (name, _) => format!("モニター {}（{name}）", d.number),
            };
            (d.number, label)
        })
        .collect();
    if let Monitor::Number(n) = chosen
        && !displays.iter().any(|d| d.number == n)
    {
        numbered.push((n, format!("モニター {n}（つながっていません）")));
    }
    numbered.sort_by_key(|&(n, _)| n);
    numbered.dedup_by_key(|&mut (n, _)| n);
    let mut list = vec![(Monitor::Primary, main), (Monitor::Cursor, "マウスのあるモニター".to_string())];
    list.extend(numbered.into_iter().map(|(n, label)| (Monitor::Number(n), label)));
    list
}

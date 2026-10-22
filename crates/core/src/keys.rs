//! Key definitions.

/// The fixed key column of the M0 prototype, top to bottom.
pub const M0_LABELS: [&str; 5] = ["Esc", "Ctrl", "Shift", "Alt", "Space"];

/// Tag carried by every key event EdgePad sends ("EDPG"), so its own input
/// can be told apart from the physical keyboard (dwExtraInfo on Windows).
pub const SEND_TAG: usize = 0x4544_5047;

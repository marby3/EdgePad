//! OS-independent core of EdgePad.
//!
//! Holds settings, key sets, the modifier state machine, layout and gesture
//! logic. It has no dependencies and calls no OS API; each platform layer
//! (Windows today) drives it and performs the actual input and drawing.

pub mod keys;
pub mod layout;

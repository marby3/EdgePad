//! OS-independent core of EdgePad.
//!
//! Holds settings, key sets, the modifier state machine, layout, gesture and
//! pie menu logic. It has no dependencies and calls no OS API; each platform
//! layer (Windows today) drives it and performs the actual input and drawing.

pub mod config;
pub mod document;
pub mod gesture;
pub mod keys;
pub mod kle;
pub mod layout;
pub mod modifier;
pub mod pie;
pub mod press;
pub mod theme;

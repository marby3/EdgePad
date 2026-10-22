//! The modifier state machine (spec 4.3).
//!
//! A modifier key goes down when it is pressed and, held for the latch time
//! or longer, up when it is released. Tapped more briefly, with no other key
//! pressed meanwhile, it latches instead: it stays down until it is tapped
//! again or, with one-shot latches, until a shortcut has been sent with it.
//!
//! The caller says when things happen, so nothing here reads a clock.

use std::time::{Duration, Instant};

/// What ends a latch besides another tap (`latch` in `[general]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LatchMode {
    /// Sending a shortcut lets the latch go.
    #[default]
    OneShot,
    /// Only a tap lets it go.
    Lock,
}

/// One modifier key on a panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modifier {
    #[default]
    Up,
    /// Under a pointer since `at`. `latched`: it was latched when pressed, so
    /// lifting lets it go. `used`: another key was pressed meanwhile.
    Pressed { at: Instant, latched: bool, used: bool },
    /// Down with no pointer on it.
    Latched,
}

impl Modifier {
    /// The first pointer went down on it at `at`. Returns whether the key
    /// goes down now: a latched key is down already.
    pub fn press(&mut self, at: Instant) -> bool {
        let latched = match *self {
            Modifier::Up => false,
            Modifier::Latched => true,
            Modifier::Pressed { .. } => return false,
        };
        *self = Modifier::Pressed { at, latched, used: false };
        !latched
    }

    /// The last pointer lifted at `at`, or the system took it away
    /// (`cancelled`). Returns whether the key goes up now; if not, it latched.
    pub fn release(&mut self, at: Instant, latch: Duration, cancelled: bool) -> bool {
        let Modifier::Pressed { at: start, latched, used } = *self else { return false };
        let tap = !latched && !used && !cancelled && at.saturating_duration_since(start) < latch;
        *self = if tap { Modifier::Latched } else { Modifier::Up };
        !tap
    }

    /// Another key was pressed: if this one is under a pointer, it no longer
    /// latches when released.
    pub fn other_key(&mut self) {
        if let Modifier::Pressed { used, .. } = self {
            *used = true;
        }
    }

    /// A shortcut was sent while this key was down. Returns whether the key
    /// goes up now: a one-shot latch ends. A key under a pointer stays down
    /// until the pointer lifts.
    pub fn shortcut_sent(&mut self, mode: LatchMode) -> bool {
        if *self == Modifier::Latched && mode == LatchMode::OneShot {
            *self = Modifier::Up;
            return true;
        }
        false
    }

    /// Forget any pointer and any latch. Returns whether the key goes up now.
    pub fn let_go(&mut self) -> bool {
        let down = self.is_down();
        *self = Modifier::Up;
        down
    }

    pub fn is_down(&self) -> bool {
        *self != Modifier::Up
    }

    pub fn is_latched(&self) -> bool {
        *self == Modifier::Latched
    }
}

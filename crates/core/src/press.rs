//! Which pointer holds which key.
//!
//! Each pointer (finger, mouse) holds at most one key, from the moment it goes
//! down on that key until it lifts, wherever it moves in between. A key is
//! down while at least one pointer holds it, so the platform layer sends one
//! key-down when the first pointer arrives and one key-up when the last leaves.

/// Most pointers tracked at once; extra contacts are ignored.
pub const MAX_POINTERS: usize = 10;

#[derive(Debug)]
pub struct Presses<const KEYS: usize> {
    pointers: [Option<(u32, usize)>; MAX_POINTERS],
    holders: [u8; KEYS],
}

impl<const KEYS: usize> Default for Presses<KEYS> {
    fn default() -> Self {
        Presses { pointers: [None; MAX_POINTERS], holders: [0; KEYS] }
    }
}

impl<const KEYS: usize> Presses<KEYS> {
    /// `pointer` went down on `key`. Returns the key to press, if it was up.
    pub fn down(&mut self, pointer: u32, key: usize) -> Option<usize> {
        if key >= KEYS || self.pointers.iter().flatten().any(|&(p, _)| p == pointer) {
            return None;
        }
        let slot = self.pointers.iter_mut().find(|s| s.is_none())?;
        *slot = Some((pointer, key));
        self.holders[key] += 1;
        (self.holders[key] == 1).then_some(key)
    }

    /// `pointer` lifted or was cancelled. Returns the key to release, if no
    /// other pointer still holds it.
    pub fn up(&mut self, pointer: u32) -> Option<usize> {
        let slot = self.pointers.iter_mut().find(|s| matches!(s, Some((p, _)) if *p == pointer))?;
        let (_, key) = slot.take()?;
        self.holders[key] -= 1;
        (self.holders[key] == 0).then_some(key)
    }

    pub fn is_down(&self, key: usize) -> bool {
        self.holders.get(key).is_some_and(|&n| n > 0)
    }
}

//! Where the settings window was (Issue #137): its place and size when not
//! maximized, and whether it was. The window runs in a process of its own,
//! started again each time it opens; the running EdgePad keeps where it
//! was, in memory only, and hands it to the next one on its command line.

use std::fmt;

/// The window's place and size, in pixels, as Windows keeps them for it
/// while not maximized; and whether it is maximized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub maximized: bool,
}

/// No coordinate or size reads past this many pixels.
const MOST: i32 = 100_000;

impl Placement {
    /// A place as `to_string` writes it, `x,y,w,h,0` (or `,1` maximized):
    /// whole numbers, the size over 0. None if it does not read so.
    pub fn read(text: &str) -> Option<Self> {
        let number = |s: &str| {
            let digits = s.strip_prefix('-').unwrap_or(s);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            s.parse::<i32>().ok().filter(|n| n.abs() <= MOST)
        };
        let parts: Vec<&str> = text.split(',').collect();
        let &[x, y, w, h, maximized] = parts.as_slice() else { return None };
        let (w, h) = (number(w).filter(|&n| n > 0)?, number(h).filter(|&n| n > 0)?);
        let maximized = match maximized {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        Some(Placement { x: number(x)?, y: number(y)?, w, h, maximized })
    }
}

impl fmt::Display for Placement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{},{},{},{}", self.x, self.y, self.w, self.h, u8::from(self.maximized))
    }
}

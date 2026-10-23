//! The colours a panel is drawn in (spec 5.2): light or dark, and one accent.
//!
//! Colours are (red, green, blue). Whether `auto` means light or dark, and
//! which accent Windows has, is the platform layer's to find out.

pub type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Behind the keys, in the gaps between them.
    pub background: Rgb,
    pub face: Rgb,
    pub text: Rgb,
    /// A held key: the accent.
    pub held: Rgb,
    pub held_text: Rgb,
    /// A latched modifier: the accent, darkened.
    pub latched: Rgb,
    pub latched_text: Rgb,
}

const WHITE: Rgb = [0xFF, 0xFF, 0xFF];
const BLACK: Rgb = [0x00, 0x00, 0x00];

/// Darker or lighter labels, whichever reads on `face`.
fn text_on(face: Rgb) -> Rgb {
    let [r, g, b] = face.map(u32::from);
    // Perceived brightness, 0..=255.
    if (r * 299 + g * 587 + b * 114) / 1000 < 150 { WHITE } else { BLACK }
}

/// The palette for a `dark` or light panel with `accent`.
pub fn palette(dark: bool, accent: Rgb) -> Palette {
    let latched = accent.map(|c| (c as u32 * 3 / 5) as u8);
    let (background, face, text) = match dark {
        true => ([0x20, 0x20, 0x20], [0x3A, 0x3A, 0x3A], [0xF0, 0xF0, 0xF0]),
        false => ([0xE4, 0xE4, 0xE4], [0xFA, 0xFA, 0xFA], [0x20, 0x20, 0x20]),
    };
    Palette { background, face, text, held: accent, held_text: text_on(accent), latched, latched_text: text_on(latched) }
}

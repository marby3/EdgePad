//! Key definitions.

/// The fixed key column of the M0 prototype, top to bottom.
pub const M0_LABELS: [&str; 5] = ["Esc", "Ctrl", "Shift", "Alt", "Space"];

/// Tag carried by every key event EdgePad sends ("EDPG"), so its own input
/// can be told apart from the physical keyboard (dwExtraInfo on Windows).
pub const SEND_TAG: usize = 0x4544_5047;

/// A key as the settings file names it. Platform layers map it to their own
/// key codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Ctrl,
    Shift,
    Alt,
    Win,
    /// A letter (`A`..=`Z`, upper case), a digit, or a US-layout symbol key
    /// (`[ ] - = ; ' , . / \` and the backtick).
    Char(u8),
    /// F1..=F24.
    F(u8),
    Space,
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PgUp,
    PgDn,
    Left,
    Right,
    Up,
    Down,
}

const SYMBOLS: &[u8] = b"[]-=;',./\\`";

const NAMED: [(&str, Key); 20] = [
    ("ctrl", Key::Ctrl),
    ("shift", Key::Shift),
    ("alt", Key::Alt),
    ("win", Key::Win),
    ("space", Key::Space),
    ("enter", Key::Enter),
    ("esc", Key::Esc),
    ("tab", Key::Tab),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("insert", Key::Insert),
    ("home", Key::Home),
    ("end", Key::End),
    ("pgup", Key::PgUp),
    ("pgdn", Key::PgDn),
    ("left", Key::Left),
    ("right", Key::Right),
    ("up", Key::Up),
    ("down", Key::Down),
    // Inside a macro "," separates the steps, so the comma key has a name.
    ("comma", Key::Char(b',')),
];

impl Key {
    /// The key called `name` in the settings file, ignoring case.
    pub fn parse(name: &str) -> Option<Key> {
        let lower = name.to_ascii_lowercase();
        if let Some(&(_, key)) = NAMED.iter().find(|(n, _)| *n == lower) {
            return Some(key);
        }
        if let [c] = name.as_bytes() {
            let c = c.to_ascii_uppercase();
            if c.is_ascii_uppercase() || c.is_ascii_digit() || SYMBOLS.contains(&c) {
                return Some(Key::Char(c));
            }
        }
        let digits = lower.strip_prefix('f')?;
        let n: u8 = digits.parse().ok()?;
        let plain = digits.len() <= 2 && digits.bytes().all(|b| b.is_ascii_digit());
        (plain && (1..=24).contains(&n)).then_some(Key::F(n))
    }

    /// The modifier bit of Ctrl, Shift, Alt and Win; none for other keys.
    pub fn modifier(self) -> Option<Mods> {
        match self {
            Key::Ctrl => Some(Mods::CTRL),
            Key::Shift => Some(Mods::SHIFT),
            Key::Alt => Some(Mods::ALT),
            Key::Win => Some(Mods::WIN),
            _ => None,
        }
    }
}

/// A set of modifier keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const CTRL: Mods = Mods(1);
    pub const SHIFT: Mods = Mods(2);
    pub const ALT: Mods = Mods(4);
    pub const WIN: Mods = Mods(8);

    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }
}

/// Modifiers plus at most one other key, written `Ctrl+Shift+S`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combo {
    pub mods: Mods,
    pub key: Option<Key>,
}

impl Combo {
    /// Parse `text`, or say what is wrong with it.
    pub fn parse(text: &str) -> Result<Combo, String> {
        let mut combo = Combo { mods: Mods::NONE, key: None };
        for part in text.split('+').map(str::trim) {
            if part.is_empty() {
                return Err(format!("キーの組み合わせ「{text}」に空の部分がある"));
            }
            let key = Key::parse(part).ok_or_else(|| format!("知らないキー名「{part}」"))?;
            match key.modifier() {
                Some(m) => combo.mods = combo.mods | m,
                None if combo.key.is_some() => {
                    return Err(format!("「{text}」に修飾キー以外のキーが2つ以上ある"));
                }
                None => combo.key = Some(key),
            }
        }
        Ok(combo)
    }
}

impl std::fmt::Display for Key {
    /// The name as the settings file writes it.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let name = match self {
            Key::Char(c) => return write!(f, "{}", *c as char),
            Key::F(n) => return write!(f, "F{n}"),
            Key::Ctrl => "Ctrl",
            Key::Shift => "Shift",
            Key::Alt => "Alt",
            Key::Win => "Win",
            Key::Space => "Space",
            Key::Enter => "Enter",
            Key::Esc => "Esc",
            Key::Tab => "Tab",
            Key::Backspace => "Backspace",
            Key::Delete => "Delete",
            Key::Insert => "Insert",
            Key::Home => "Home",
            Key::End => "End",
            Key::PgUp => "PgUp",
            Key::PgDn => "PgDn",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::Up => "Up",
            Key::Down => "Down",
        };
        f.write_str(name)
    }
}

impl std::fmt::Display for Combo {
    /// `Ctrl+Shift+S`: modifiers in Ctrl, Shift, Alt, Win order, then the key.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mods = [(Mods::CTRL, "Ctrl"), (Mods::SHIFT, "Shift"), (Mods::ALT, "Alt"), (Mods::WIN, "Win")];
        let mut parts: Vec<String> =
            mods.iter().filter(|(m, _)| self.mods.contains(*m)).map(|(_, n)| n.to_string()).collect();
        parts.extend(self.key.map(|k| k.to_string()));
        f.write_str(&parts.join("+"))
    }
}

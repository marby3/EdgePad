//! Kana keys (Issue #163): a kana sent as the romaji keys an IME turns into
//! it, K then A for か, so that it can go on to kanji; and the mark key,
//! ゛゜小, which changes the kana sent last as a phone's does.
//!
//! The romaji are the ones Microsoft IME and Google 日本語入力 both read
//! with their defaults: si, ti, tu, hu; xa and xtu for the small ones; nn.
//! With the IME off they are typed as they are.

use crate::keys::{Combo, Key, Mods};

/// What a kana key sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kana {
    /// The kana, as its romaji.
    Char(char),
    /// The kana sent last, changed (`next`).
    Mark,
}

/// The mark key's content, as the settings file writes it.
pub const MARK: &str = "゛゜小";

impl Kana {
    /// The kana key `content` writes: one kana `romaji` knows, or `MARK`.
    pub fn parse(content: &str) -> Option<Kana> {
        if content == MARK {
            return Some(Kana::Mark);
        }
        let mut chars = content.chars();
        let c = chars.next().filter(|_| chars.next().is_none())?;
        romaji(c).map(|_| Kana::Char(c))
    }
}

/// The romaji of each kana, row by row of the gojūon.
const ROMAJI: &[(char, &str)] = &[
    ('あ', "A"), ('い', "I"), ('う', "U"), ('え', "E"), ('お', "O"),
    ('か', "KA"), ('き', "KI"), ('く', "KU"), ('け', "KE"), ('こ', "KO"),
    ('さ', "SA"), ('し', "SI"), ('す', "SU"), ('せ', "SE"), ('そ', "SO"),
    ('た', "TA"), ('ち', "TI"), ('つ', "TU"), ('て', "TE"), ('と', "TO"),
    ('な', "NA"), ('に', "NI"), ('ぬ', "NU"), ('ね', "NE"), ('の', "NO"),
    ('は', "HA"), ('ひ', "HI"), ('ふ', "HU"), ('へ', "HE"), ('ほ', "HO"),
    ('ま', "MA"), ('み', "MI"), ('む', "MU"), ('め', "ME"), ('も', "MO"),
    ('や', "YA"), ('ゆ', "YU"), ('よ', "YO"),
    ('ら', "RA"), ('り', "RI"), ('る', "RU"), ('れ', "RE"), ('ろ', "RO"),
    ('わ', "WA"), ('を', "WO"), ('ん', "NN"),
    ('が', "GA"), ('ぎ', "GI"), ('ぐ', "GU"), ('げ', "GE"), ('ご', "GO"),
    ('ざ', "ZA"), ('じ', "ZI"), ('ず', "ZU"), ('ぜ', "ZE"), ('ぞ', "ZO"),
    ('だ', "DA"), ('ぢ', "DI"), ('づ', "DU"), ('で', "DE"), ('ど', "DO"),
    ('ば', "BA"), ('び', "BI"), ('ぶ', "BU"), ('べ', "BE"), ('ぼ', "BO"),
    ('ぱ', "PA"), ('ぴ', "PI"), ('ぷ', "PU"), ('ぺ', "PE"), ('ぽ', "PO"),
    ('ゔ', "VU"),
    ('ぁ', "XA"), ('ぃ', "XI"), ('ぅ', "XU"), ('ぇ', "XE"), ('ぉ', "XO"),
    ('っ', "XTU"), ('ゃ', "XYA"), ('ゅ', "XYU"), ('ょ', "XYO"), ('ゎ', "XWA"),
];

/// The keys that type kana `c` through an IME, in order: its romaji's
/// letters, or the key of a mark. None for any other character.
pub fn romaji(c: char) -> Option<Vec<Combo>> {
    let plain = |key| Combo { mods: Mods::NONE, key: Some(key) };
    let shifted = |key| Combo { mods: Mods::SHIFT, key: Some(key) };
    let mark = match c {
        'ー' => Some(plain(Key::Char(b'-'))),
        '、' => Some(plain(Key::Char(b','))),
        '。' => Some(plain(Key::Char(b'.'))),
        '？' => Some(shifted(Key::Char(b'/'))),
        '！' => Some(shifted(Key::Char(b'1'))),
        _ => None,
    };
    if let Some(mark) = mark {
        return Some(vec![mark]);
    }
    let (_, letters) = ROMAJI.iter().find(|(k, _)| *k == c)?;
    Some(letters.bytes().map(|b| plain(Key::Char(b))).collect())
}

/// The kana each in turn becomes as the mark key is pressed, as a phone
/// has them: voiced, then half-voiced or small, then back.
const TURNS: &[&str] = &[
    "あぁ", "いぃ", "うぅゔ", "えぇ", "おぉ",
    "かが", "きぎ", "くぐ", "けげ", "こご",
    "さざ", "しじ", "すず", "せぜ", "そぞ",
    "ただ", "ちぢ", "つっづ", "てで", "とど",
    "はばぱ", "ひびぴ", "ふぶぷ", "へべぺ", "ほぼぽ",
    "やゃ", "ゆゅ", "よょ", "わゎ",
];

/// What kana `c` becomes as the mark key is pressed after it: the next in
/// its turn, round to the first. None for a kana with no other.
pub fn next(c: char) -> Option<char> {
    let turn: Vec<char> = TURNS.iter().find(|t| t.contains(c))?.chars().collect();
    let at = turn.iter().position(|&k| k == c)?;
    Some(turn[(at + 1) % turn.len()])
}

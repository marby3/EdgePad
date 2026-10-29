//! Keyboard Layout Editor's raw data: where each key of a layout sits
//! (Issue #101).
//!
//! The text of KLE's Raw data box (rows of keys, the names of properties
//! unquoted, no brackets around it all), or the JSON file KLE downloads
//! (the same in brackets, the names quoted, perhaps the layout's details
//! first). A row is an array of labels, each a key, and of objects of
//! properties for the keys after them: x and y move the next key along and
//! down (y for the rows after it too), w and h are its width and height.
//! r turns the keys after it by that many degrees clockwise, about rx, ry
//! (Issue #126); writing rx or ry starts the next key there, and each row
//! after starts again at rx, as in KLE. Properties of looks (colours,
//! fonts, profiles and the like) are passed over. A layout may reach left
//! of and above where KLE starts: it is moved to start at 0, 0.

use crate::layout::{Place, Tilt};

/// Furthest a key sits from where KLE starts, and largest it is, in keys:
/// as far as a settings file may write (config's x, y, w and h).
const MAX_PLACE: f64 = 40.0;
const MAX_SIZE: f64 = 8.0;

/// Deepest the brackets go: KLE's own files go 3 deep.
const MAX_DEPTH: usize = 16;

/// A key of the layout: where it sits, and its label (the first of its
/// labels written; KLE puts one on each line).
pub type KleKey = (Place, String);

/// The keys of the layout in `text`, from the top left: row by row from
/// the top, left to right in a row. Or why it cannot be read, in the
/// parser's words.
pub fn parse(text: &str) -> Result<Vec<KleKey>, String> {
    let mut reader = Reader { text, at: 0, depth: 0 };
    let mut items = Vec::new();
    reader.space();
    while !reader.done() {
        items.push(reader.value()?);
        reader.space();
        if !reader.done() && !reader.eat(b',') {
            return Err(reader.wrong());
        }
        reader.space();
    }
    // A file in brackets: its items are inside them.
    if let [Value::Array(inner)] = &items[..] {
        if inner.iter().any(|v| matches!(v, Value::Array(_))) {
            items = inner.clone();
        }
    }
    let mut keys = Vec::new();
    let mut y = 0.0;
    // The turn, and the point it is about.
    let (mut r, mut rx, mut ry) = (0.0, 0.0, 0.0);
    for item in items {
        match item {
            // The layout's details.
            Value::Object(_) => continue,
            Value::Array(row) => {
                let (mut x, mut w, mut h) = (rx, 1.0, 1.0);
                for entry in row {
                    match entry {
                        Value::Object(props) => {
                            let read = |name: &str| -> Result<Option<f64>, String> {
                                match props.iter().find(|(n, _)| n == name) {
                                    Some((_, Value::Number(n))) => Ok(Some(*n)),
                                    Some(_) => Err(format!("KLE の {name} が数でない")),
                                    None => Ok(None),
                                }
                            };
                            // In KLE's order, whatever the file's: the turn,
                            // then the moves from where it starts.
                            let (nr, nrx, nry) = (read("r")?, read("rx")?, read("ry")?);
                            r = nr.unwrap_or(r);
                            if nrx.is_some() || nry.is_some() {
                                (rx, ry) = (nrx.unwrap_or(rx), nry.unwrap_or(ry));
                                (x, y) = (rx, ry);
                            }
                            x += read("x")?.unwrap_or(0.0);
                            y += read("y")?.unwrap_or(0.0);
                            w = read("w")?.unwrap_or(w);
                            h = read("h")?.unwrap_or(h);
                        }
                        Value::Text(labels) => {
                            let label = labels.split('\n').find(|l| !l.is_empty()).unwrap_or("").to_string();
                            // Its middle, turned about rx, ry.
                            let (mx, my) = (x + w / 2.0, y + h / 2.0);
                            let (sin, cos) = f64::to_radians(r).sin_cos();
                            let (mx, my) = if r == 0.0 {
                                (mx, my)
                            } else {
                                (rx + (mx - rx) * cos - (my - ry) * sin, ry + (mx - rx) * sin + (my - ry) * cos)
                            };
                            let placed = mx.abs() <= MAX_PLACE && my.abs() <= MAX_PLACE;
                            if !placed || !(0.25..=MAX_SIZE).contains(&w) || !(0.25..=MAX_SIZE).contains(&h) {
                                return Err(format!("KLE のキー「{label}」の位置か大きさが大きすぎる"));
                            }
                            let q = |v: f64| (v * 4.0).round() as i32;
                            let thousandths = |v: f64| (v * 1000.0).round() as i32;
                            let tilt = (r != 0.0).then(|| Tilt { x: thousandths(mx), y: thousandths(my), angle: (r * 100.0).round() as i32 });
                            let place = Place { x: q(mx - w / 2.0), y: q(my - h / 2.0), w: q(w), h: q(h), tilt };
                            keys.push((place, label));
                            x += w;
                            (w, h) = (1.0, 1.0);
                        }
                        _ => return Err("KLE の行に、ラベルでもプロパティでもないものがある".into()),
                    }
                }
                y += 1.0;
            }
            _ => return Err("KLE の raw data ではない（行の [ ] がない）".into()),
        }
    }
    if keys.is_empty() {
        return Err("KLE の配列にキーがない".into());
    }
    let left = keys.iter().map(|(p, _)| p.x).min().unwrap_or(0);
    let top = keys.iter().map(|(p, _)| p.y).min().unwrap_or(0);
    for (p, _) in &mut keys {
        (p.x, p.y) = (p.x - left, p.y - top);
        if let Some(t) = &mut p.tilt {
            (t.x, t.y) = (t.x - left * 250, t.y - top * 250);
        }
    }
    keys.sort_by_key(|(p, _)| (p.y, p.x));
    Ok(keys)
}

#[derive(Clone, Debug)]
enum Value {
    Text(String),
    Number(f64),
    Other,
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

/// JSON, as loose as KLE writes it: names of properties may go unquoted.
struct Reader<'a> {
    text: &'a str,
    at: usize,
    /// How deep in brackets the value read is.
    depth: usize,
}

impl Reader<'_> {
    fn done(&self) -> bool {
        self.at >= self.text.len()
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        let ate = self.peek() == Some(b);
        self.at += ate as usize;
        ate
    }

    fn space(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn wrong(&self) -> String {
        match self.text[self.at..].chars().next() {
            Some(c) => format!("KLE の raw data として読めない（「{c}」のところ）"),
            None => "KLE の raw data が途中で終わっている".into(),
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.depth += 1;
        let value = self.nested();
        self.depth -= 1;
        value
    }

    fn nested(&mut self) -> Result<Value, String> {
        self.space();
        if self.depth > MAX_DEPTH {
            return Err("KLE の raw data の [ ] や { } が深すぎる".into());
        }
        match self.peek() {
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.space();
                    if self.eat(b']') {
                        return Ok(Value::Array(items));
                    }
                    items.push(self.value()?);
                    self.space();
                    if !self.eat(b',') && self.peek() != Some(b']') {
                        return Err(self.wrong());
                    }
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut props = Vec::new();
                loop {
                    self.space();
                    if self.eat(b'}') {
                        return Ok(Value::Object(props));
                    }
                    let name = match self.peek() {
                        Some(b'"') => self.string()?,
                        _ => match self.word() {
                            Some(word) => word.to_string(),
                            None => return Err(self.wrong()),
                        },
                    };
                    self.space();
                    if !self.eat(b':') {
                        return Err(self.wrong());
                    }
                    props.push((name, self.value()?));
                    self.space();
                    if !self.eat(b',') && self.peek() != Some(b'}') {
                        return Err(self.wrong());
                    }
                }
            }
            Some(b'"') => Ok(Value::Text(self.string()?)),
            Some(b'-' | b'.' | b'0'..=b'9') => {
                let start = self.at;
                while self.peek().is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.at += 1;
                }
                let number = self.text[start..self.at].parse().map_err(|_| {
                    self.at = start;
                    self.wrong()
                })?;
                Ok(Value::Number(number))
            }
            _ => match self.word() {
                Some("true" | "false" | "null") => Ok(Value::Other),
                _ => Err(self.wrong()),
            },
        }
    }

    /// A name: letters, digits and _.
    fn word(&mut self) -> Option<&str> {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_') {
            self.at += 1;
        }
        (self.at > start).then(|| &self.text[start..self.at])
    }

    /// A string in double quotes, with JSON's escapes.
    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut out = String::new();
        let mut chars = self.text[self.at..].char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => {
                    self.at += i + 1;
                    return Ok(out);
                }
                '\\' => match chars.next().map(|(_, c)| c) {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{C}'),
                    Some('u') => {
                        let unit = |chars: &mut std::str::CharIndices| {
                            let hex: String = (0..4).filter_map(|_| chars.next().map(|(_, c)| c)).collect();
                            u32::from_str_radix(&hex, 16).ok()
                        };
                        let first = unit(&mut chars);
                        // A character past U+FFFF: two \u escapes, a pair.
                        let c = match first {
                            Some(hi @ 0xD800..=0xDBFF) => match (chars.next(), chars.next(), unit(&mut chars)) {
                                (Some((_, '\\')), Some((_, 'u')), Some(lo @ 0xDC00..=0xDFFF)) => {
                                    char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))
                                }
                                _ => None,
                            },
                            other => other.and_then(char::from_u32),
                        };
                        out.push(c.unwrap_or('\u{FFFD}'));
                    }
                    Some(c) => out.push(c),
                    None => break,
                },
                c => out.push(c),
            }
        }
        self.at = self.text.len();
        Err(self.wrong())
    }
}

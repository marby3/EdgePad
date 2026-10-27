//! Keyboard Layout Editor's raw data: where each key of a layout sits
//! (Issue #101).
//!
//! The text of KLE's Raw data box (rows of keys, the names of properties
//! unquoted, no brackets around it all), or the JSON file KLE downloads
//! (the same in brackets, the names quoted, perhaps the layout's details
//! first). A row is an array of labels, each a key, and of objects of
//! properties for the keys after them: x and y move the next key along and
//! down (y for the rows after it too), w and h are its width and height.
//! Rotated keys (r, rx, ry) are not read; properties of looks (colours,
//! fonts, profiles and the like) are passed over.

use crate::layout::Place;

/// A key of the layout: where it sits, and its label (the first of its
/// labels written; KLE puts one on each line).
pub type KleKey = (Place, String);

/// The keys of the layout in `text`, from the top left: row by row from
/// the top, left to right in a row. Or why it cannot be read, in the
/// parser's words.
pub fn parse(text: &str) -> Result<Vec<KleKey>, String> {
    let mut reader = Reader { text, at: 0 };
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
    for item in items {
        match item {
            // The layout's details.
            Value::Object(_) => continue,
            Value::Array(row) => {
                let (mut x, mut w, mut h) = (0.0, 1.0, 1.0);
                for entry in row {
                    match entry {
                        Value::Object(props) => {
                            for (name, value) in props {
                                let number = || match &value {
                                    Value::Number(n) => Ok(*n),
                                    _ => Err(format!("KLE の {name} が数でない")),
                                };
                                match name.as_str() {
                                    "x" => x += number()?,
                                    "y" => y += number()?,
                                    "w" => w = number()?,
                                    "h" => h = number()?,
                                    "r" | "rx" | "ry" if number().is_ok_and(|n| n != 0.0) => {
                                        return Err("回転したキー（r, rx, ry）は読めない".into());
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Value::Text(labels) => {
                            let label = labels.split('\n').find(|l| !l.is_empty()).unwrap_or("").to_string();
                            let q = |v: f64| (v * 4.0).round() as i32;
                            let place = Place { x: q(x), y: q(y), w: q(w), h: q(h) };
                            if place.x < 0 || place.y < 0 || place.w < 1 || place.h < 1 {
                                return Err(format!("KLE のキー「{label}」の位置か大きさが読めない"));
                            }
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
        self.space();
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
                    Some('u') => {
                        let hex: String = (0..4).filter_map(|_| chars.next().map(|(_, c)| c)).collect();
                        let unit = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
                        out.push(unit.unwrap_or('\u{FFFD}'));
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

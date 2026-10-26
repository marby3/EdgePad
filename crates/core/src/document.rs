//! Writing edits back into the settings file (spec 7.1).
//!
//! A document keeps the file's text line by line, endings and all. Edits
//! touch only the lines they are about: a value changes in place, beside
//! whatever comment follows it; a key line moves with its comment; a new
//! line goes after the last setting of its section. Unedited, a document
//! writes back the very bytes it read.
//!
//! Sections are named as their headers are, without the brackets:
//! `general`, `edge.left`, `set.pad`, `profile.csp`. Names match in any
//! case, as the parser reads them; of two sections, or two settings in a
//! section, of one name, which the parser turns away, edits reach the first. Every edit says whether it was
//! made: on a section or line that is not there, or with a value the file
//! cannot hold, it does nothing and says false.
//!
//! The file has no escapes, so a value cannot hold a line break, or a `;`
//! after a space, where the parser's comments start: such values are
//! turned away. Beyond that, a document does not check what it writes: the
//! settings window reads the text back with the parser before saving (spec
//! 7.1). A key set removed while keys or profiles still name it, for one,
//! is an error there, for the user to settle, rather than something to
//! guess at here.

use crate::config::{self, Error, strip_comment};

#[derive(Clone, Debug)]
pub struct Document {
    bom: bool,
    /// Each line with its ending, `\n` or `\r\n`; the last may have none.
    lines: Vec<String>,
}

/// `line` without its ending.
fn body(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// The lower-case name of the section `line` heads, if it is a header.
fn header(line: &str) -> Option<String> {
    let content = strip_comment(body(line)).trim();
    let name = content.strip_prefix('[')?.strip_suffix(']')?;
    Some(name.trim().to_ascii_lowercase())
}

/// Whether `line` starts a section as the parser reads it: any line that
/// opens with `[`, a header left unclosed too.
fn starts_section(line: &str) -> bool {
    strip_comment(body(line)).trim().starts_with('[')
}

/// The lower-case name of the setting on `line`, if it is one.
fn setting(line: &str) -> Option<String> {
    let content = strip_comment(body(line)).trim();
    if content.starts_with('[') {
        return None;
    }
    let (name, _) = content.split_once('=')?;
    Some(name.trim().to_ascii_lowercase())
}

/// Where the value of the setting on `line` lies: from after the `=` to
/// the comment (or the end), and within that, the value without the spaces
/// around it. Byte ranges of `line`.
fn value_spans(line: &str) -> Option<((usize, usize), (usize, usize))> {
    let text = body(line);
    let end = strip_comment(text).len();
    let start = text[..end].find('=')? + 1;
    let region = &text[start..end];
    let from = start + (region.len() - region.trim_start().len());
    Some(((start, end), (from, from + region.trim().len())))
}

/// The value of the setting on `line`.
fn value(line: &str) -> Option<&str> {
    let (_, (from, to)) = value_spans(line)?;
    Some(&line[from..to])
}

/// Whether `value` can stand in the file as written: on one line, with no
/// `;` the parser would take for the start of a comment.
fn writable(value: &str) -> bool {
    !value.contains(['\r', '\n']) && strip_comment(value).len() == value.len()
}

/// Names a page key or a profile's edge reads as something else than a key
/// set: the next and previous page, and no set at all.
const RESERVED: [&str; 3] = ["next", "prev", "none"];

/// Whether `name` can head a section the document reads back as one: on
/// one line, no brackets of its own, and no key set called by a name page
/// keys or edges read as no set.
fn section_name_ok(name: &str) -> bool {
    let reserved = name
        .split_once('.')
        .is_some_and(|(kind, set)| kind.eq_ignore_ascii_case("set") && RESERVED.iter().any(|r| set.eq_ignore_ascii_case(r)));
    writable(name) && !name.contains(['[', ']']) && !reserved
}

/// Whether `name` can stand before the `=` of a setting the document reads
/// back as one.
fn setting_name_ok(name: &str) -> bool {
    writable(name) && !name.trim().is_empty() && !name.contains(['[', ']', '='])
}

/// `line` with the value of its setting replaced by `value`.
fn with_value(line: &str, value: &str) -> String {
    let Some(((start, end), (from, to))) = value_spans(line) else { return line.into() };
    if from < to {
        return format!("{}{value}{}", &line[..from], &line[to..]);
    }
    // No value yet: spaced from the `=`, and from a comment right after.
    let after = if end < body(line).len() { " " } else { "" };
    format!("{} {value}{after}{}", &line[..start], &line[end..])
}

/// `line` with the content field of its key, `label | kind | content`,
/// replaced by `content`.
fn with_content(line: &str, content: &str) -> String {
    let Some((_, (from, to))) = value_spans(line) else { return line.into() };
    let bars: Vec<usize> = line[from..to].match_indices('|').map(|(i, _)| from + i).collect();
    let Some(&start) = bars.get(1) else { return line.into() };
    let end = bars.get(2).copied().unwrap_or(to);
    let field = &line[start + 1..end];
    let lead = start + 1 + (field.len() - field.trim_start().len());
    format!("{}{content}{}", &line[..lead], &line[lead + field.trim().len()..])
}

impl Document {
    /// The document of a settings file. Bytes the parser would turn away
    /// whole (too many, or not UTF-8) make none, for the same reason.
    pub fn new(bytes: &[u8]) -> Result<Document, Error> {
        let (bom, text) = config::text_of(bytes)?;
        Ok(Document { bom, lines: text.split_inclusive('\n').map(String::from).collect() })
    }

    /// The text to write back.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = if self.bom { b"\xEF\xBB\xBF".to_vec() } else { Vec::new() };
        for line in &self.lines {
            bytes.extend_from_slice(line.as_bytes());
        }
        bytes
    }

    /// The ending new lines get: the file's own, from its first line.
    fn ending(&self) -> &'static str {
        match self.lines.first() {
            Some(line) if line.ends_with("\r\n") => "\r\n",
            _ => "\n",
        }
    }

    /// End the last line, so a line can follow it.
    fn end_last_line(&mut self) {
        let ending = self.ending();
        if let Some(last) = self.lines.last_mut().filter(|l| !l.ends_with('\n')) {
            last.push_str(ending);
        }
    }

    /// The lines of section `name`: its header's index, and where the next
    /// section starts (or the end).
    fn section(&self, name: &str) -> Option<(usize, usize)> {
        let name = name.to_ascii_lowercase();
        let start = self.lines.iter().position(|l| header(l).as_deref() == Some(&name))?;
        let end = (start + 1..self.lines.len()).find(|&i| starts_section(&self.lines[i])).unwrap_or(self.lines.len());
        Some((start, end))
    }

    /// The lines of the settings called `name` in section `section`.
    fn settings(&self, section: &str, name: &str) -> Vec<usize> {
        let Some((start, end)) = self.section(section) else { return Vec::new() };
        let name = name.to_ascii_lowercase();
        (start + 1..end).filter(|&i| setting(&self.lines[i]).as_deref() == Some(&name)).collect()
    }

    /// Put `line` (without its ending) in at `at`.
    fn insert_line(&mut self, at: usize, line: &str) {
        if at == self.lines.len() {
            self.end_last_line();
        }
        let line = format!("{line}{}", self.ending());
        self.lines.insert(at, line);
    }

    /// Where a new setting of `section` goes: after its last one, or right
    /// after its header.
    fn after_last_setting(&self, start: usize, end: usize) -> usize {
        (start + 1..end).rev().find(|&i| setting(&self.lines[i]).is_some()).map_or(start + 1, |i| i + 1)
    }

    /// The value of setting `name` in `section`, as written, if it is there.
    pub fn get(&self, section: &str, name: &str) -> Option<&str> {
        let i = *self.settings(section, name).first()?;
        value(&self.lines[i])
    }

    /// Set `name` in `section` to `value`, or remove it with none. A new
    /// setting goes after the section's last one, as `name = value`; a new
    /// section goes at the end.
    pub fn set_value(&mut self, section: &str, name: &str, value: Option<&str>) -> bool {
        if value.is_some_and(|v| !writable(v)) || !setting_name_ok(name) || !section_name_ok(section) {
            return false;
        }
        let found = self.settings(section, name).first().copied();
        match (found, value) {
            (Some(i), Some(value)) => self.lines[i] = with_value(&self.lines[i], value),
            (Some(i), None) => {
                self.lines.remove(i);
            }
            (None, Some(value)) => {
                self.add_section(section);
                let Some((start, end)) = self.section(section) else { return false };
                let at = self.after_last_setting(start, end);
                self.insert_line(at, &format!("{name} = {value}"));
            }
            (None, None) => return false,
        }
        true
    }

    /// The keys of key set `set`, as written after `key =`: the fields
    /// with the spaces between them as written, for the caller to split on
    /// `|` and trim as the parser does.
    pub fn keys(&self, set: &str) -> Vec<String> {
        self.settings(set, "key").into_iter().filter_map(|i| value(&self.lines[i]).map(String::from)).collect()
    }

    /// Make key `index` of `set` `key` (`label | kind | content ...`).
    pub fn set_key(&mut self, set: &str, index: usize, key: &str) -> bool {
        let Some(&i) = self.settings(set, "key").get(index).filter(|_| writable(key)) else { return false };
        self.lines[i] = with_value(&self.lines[i], key);
        true
    }

    /// Put `key` in as key `index` of `set`: before the key there now, or
    /// after the last.
    pub fn insert_key(&mut self, set: &str, index: usize, key: &str) -> bool {
        let Some((start, end)) = self.section(set).filter(|_| writable(key)) else { return false };
        let keys = self.settings(set, "key");
        let at = match keys.get(index) {
            Some(&i) => i,
            None => keys.last().map_or_else(|| self.after_last_setting(start, end), |&i| i + 1),
        };
        self.insert_line(at, &format!("key = {key}"));
        true
    }

    /// Take key `index` out of `set`.
    pub fn remove_key(&mut self, set: &str, index: usize) -> bool {
        let Some(&i) = self.settings(set, "key").get(index) else { return false };
        self.lines.remove(i);
        true
    }

    /// Move key `from` of `set` to be key `to`, the keys between shifting
    /// over to make room. Each key line moves with its comment; the lines
    /// between the keys stay where they are.
    pub fn move_key(&mut self, set: &str, from: usize, to: usize) -> bool {
        let slots = self.settings(set, "key");
        if from >= slots.len() || to >= slots.len() {
            return false;
        }
        if from == to {
            return true;
        }
        // A key on the last line, moved, may no longer be last.
        if slots.last() == Some(&(self.lines.len() - 1)) {
            self.end_last_line();
        }
        let mut keys: Vec<String> = slots.iter().map(|&i| self.lines[i].clone()).collect();
        let key = keys.remove(from);
        keys.insert(to, key);
        for (&i, key) in slots.iter().zip(keys) {
            self.lines[i] = key;
        }
        true
    }

    /// Add an empty section `name` at the end, after a blank line; false if
    /// there is one already.
    pub fn add_section(&mut self, name: &str) -> bool {
        if self.section(name).is_some() || !section_name_ok(name) {
            return false;
        }
        self.end_last_line();
        if self.lines.last().is_some_and(|l| !body(l).trim().is_empty()) {
            self.lines.push(self.ending().into());
        }
        let line = format!("[{name}]{}", self.ending());
        self.lines.push(line);
        true
    }

    /// Take section `name` out: its header and every line up to the next
    /// header, comments included.
    pub fn remove_section(&mut self, name: &str) -> bool {
        let Some((start, end)) = self.section(name) else { return false };
        self.lines.drain(start..end);
        true
    }

    /// Call key set or profile `old` `new`: `set.a` to `set.b`, `profile.a`
    /// to `profile.b`. Refused when another section is called `new`, or a
    /// key set would be called a name a page key or an edge reads as
    /// something else (`next`, `prev`, `none`). A key set renamed takes its
    /// name along to the page and pie keys that open it and the edges of
    /// profiles that show it.
    pub fn rename_section(&mut self, old: &str, new: &str) -> bool {
        let (Some((kind, old_name)), Some((new_kind, new_name))) = (old.split_once('.'), new.split_once('.')) else { return false };
        let kind = kind.to_ascii_lowercase();
        if !matches!(kind.as_str(), "set" | "profile") || !new_kind.eq_ignore_ascii_case(&kind) {
            return false;
        }
        if !section_name_ok(new) {
            return false;
        }
        let Some((start, _)) = self.section(old) else { return false };
        if self.section(new).is_some_and(|(other, _)| other != start) {
            return false;
        }
        // Within the header, before any comment: one may hold a `]` too.
        let line = &self.lines[start];
        let end = strip_comment(body(line)).len();
        let (Some(open), Some(close)) = (line[..end].find('['), line[..end].rfind(']')) else { return false };
        self.lines[start] = format!("{}{new}{}", &line[..=open], &line[close..]);
        if kind == "set" {
            self.follow_rename(old_name, new_name);
        }
        true
    }

    /// Key set `old` is called `new` now: so are the page and pie keys that
    /// open it and the edges of profiles that show it. A page key's `next`
    /// and `prev` and an edge's `none` mean no set, whatever sets are
    /// called.
    fn follow_rename(&mut self, old: &str, new: &str) {
        let headers: Vec<String> = self.lines.iter().filter_map(|l| header(l)).collect();
        for section in headers {
            let Some((kind @ ("set" | "profile"), _)) = section.split_once('.') else { continue };
            let Some((start, end)) = self.section(&section) else { continue };
            for i in start + 1..end {
                let line = &self.lines[i];
                let (Some(name), Some(value)) = (setting(line), value(line)) else { continue };
                if !value_refers(kind, &name, value, old) {
                    continue;
                }
                self.lines[i] = if kind == "profile" { with_value(line, new) } else { with_content(line, new) };
            }
        }
    }
}

/// Whether setting `name` = `value` in a section of `kind` names key set
/// `old`: a profile's edge, unless `none`, or the content of a pie key, or
/// of a page key unless `next` or `prev`.
fn value_refers(kind: &str, name: &str, value: &str, old: &str) -> bool {
    let is = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    if kind == "profile" {
        return ["left", "right", "top", "bottom"].contains(&name) && is(value, old) && !is(value, "none");
    }
    let fields: Vec<&str> = value.split('|').map(str::trim).collect();
    let (Some(&key_kind), Some(&content)) = (fields.get(1), fields.get(2)) else { return false };
    if name != "key" || !is(content, old) {
        return false;
    }
    is(key_kind, "pie") || (is(key_kind, "page") && !is(content, "next") && !is(content, "prev"))
}

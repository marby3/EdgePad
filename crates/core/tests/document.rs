//! Acceptance tests for Issue #78: writing edits back into the settings
//! file (spec 7.1). A document keeps the file's text line by line; edits
//! change only what they touch, and the text it writes reads back as the
//! settings edited.

use edgepad_core::config::{Action, Edge, MAX_BYTES, Page, parse};
use edgepad_core::document::Document;

/// A file written by hand: a BOM, CRLF endings, comments, odd spacing.
const HAND: &str = "\u{feff}; EdgePad の設定\r\n\
[general]\r\n\
opacity   = 90   ; 通常表示\r\n\
theme=dark\r\n\
\r\n\
[set.pad]          ; 左のパッド\r\n\
key = Ctrl  | modifier | Ctrl   ; 一番上\r\n\
key = 戻す  | shortcut | Ctrl+Z\r\n\
key = 次    | page     | more\r\n\
key = 変形  | pie      | more\r\n\
; ここから下はおまけ\r\n\
[set.more]\r\n\
key = A | shortcut | A\r\n\
key = B | shortcut | B\r\n\
\r\n\
[profile.default]\r\n\
left  = pad\r\n\
right = more  ; 右も\r\n";

fn doc(text: &str) -> Document {
    Document::new(text.as_bytes()).expect("a document")
}

fn text(d: &Document) -> String {
    String::from_utf8(d.to_bytes()).expect("UTF-8")
}

// AC1
#[test]
fn unedited_it_writes_the_very_bytes_it_read() {
    for original in [
        HAND,
        "[general]\nopacity = 90\n[set.a]\nkey = A | hold | A\n[profile.default]\nleft = a",
        "",
        "\n\n; only comments\n",
    ] {
        assert_eq!(doc(original).to_bytes(), original.as_bytes(), "{original:?}");
    }
}

// AC2
#[test]
fn a_value_changes_alone_a_new_one_goes_after_the_last_and_a_removed_one_leaves_its_line() {
    let mut d = doc(HAND);
    d.set_value("general", "opacity", Some("75"));
    d.set_value("profile.default", "right", Some("pad"));
    let t = text(&d);
    assert!(t.contains("opacity   = 75   ; 通常表示\r\n"), "only the value changed: {t}");
    assert!(t.contains("right = pad  ; 右も\r\n"), "{t}");

    d.set_value("general", "latch_ms", Some("300"));
    assert!(text(&d).contains("theme=dark\r\nlatch_ms = 300\r\n\r\n[set.pad]"), "after the last item, before the blank line: {}", text(&d));

    d.set_value("edge.left", "columns", Some("2"));
    assert!(text(&d).ends_with("right = pad  ; 右も\r\n\r\n[edge.left]\r\ncolumns = 2\r\n"), "a new section at the end: {}", text(&d));

    d.set_value("general", "theme", None);
    let t = text(&d);
    assert!(!t.contains("theme"), "removed: {t}");
    assert!(t.contains("opacity   = 75   ; 通常表示\r\nlatch_ms = 300\r\n"), "and nothing else: {t}");
    assert_eq!(t.lines().count(), HAND.lines().count() + 3, "one line in, one out, and a new section of two lines after a blank");
}

// AC3
#[test]
fn keys_change_come_and_go_and_move_with_their_comments() {
    let mut d = doc(HAND);
    assert_eq!(d.keys("set.pad"), ["Ctrl  | modifier | Ctrl", "戻す  | shortcut | Ctrl+Z", "次    | page     | more", "変形  | pie      | more"]);

    d.set_key("set.pad", 1, "やり直し | shortcut | Ctrl+Y");
    d.insert_key("set.pad", 2, "C | shortcut | C");
    d.remove_key("set.more", 0);
    d.move_key("set.pad", 0, 3);
    let t = text(&d);
    let expect = "[set.pad]          ; 左のパッド\r\n\
key = やり直し | shortcut | Ctrl+Y\r\n\
key = C | shortcut | C\r\n\
key = 次    | page     | more\r\n\
key = Ctrl  | modifier | Ctrl   ; 一番上\r\n\
key = 変形  | pie      | more\r\n\
; ここから下はおまけ\r\n\
[set.more]\r\n\
key = B | shortcut | B\r\n";
    assert!(t.contains(expect), "{t}");
    assert!(t.starts_with("\u{feff}; EdgePad の設定\r\n[general]\r\nopacity   = 90   ; 通常表示\r\n"), "the rest as it was: {t}");
}

// AC4
#[test]
fn sections_come_and_go_and_a_renamed_set_takes_its_references_along() {
    let mut d = doc(HAND);
    d.rename_section("set.more", "set.extra");
    let t = text(&d);
    assert!(t.contains("[set.extra]\r\n"), "{t}");
    assert!(t.contains("key = 次    | page     | extra\r\n"), "the page key follows: {t}");
    assert!(t.contains("key = 変形  | pie      | extra\r\n"), "the pie key follows: {t}");
    assert!(t.contains("right = extra  ; 右も\r\n"), "the profile follows: {t}");
    assert!(t.contains("left  = pad\r\n"), "other names stay: {t}");

    d.rename_section("profile.default", "profile.default");
    d.add_section("profile.csp");
    d.set_value("profile.csp", "process", Some("CLIPStudioPaint.exe"));
    d.set_value("profile.csp", "left", Some("extra"));
    assert!(text(&d).ends_with("right = extra  ; 右も\r\n\r\n[profile.csp]\r\nprocess = CLIPStudioPaint.exe\r\nleft = extra\r\n"), "{}", text(&d));

    d.remove_section("set.pad");
    let t = text(&d);
    assert!(!t.contains("[set.pad]") && !t.contains("Ctrl+Z") && !t.contains("おまけ"), "the header to the next header, comments and all: {t}");
    assert!(t.contains("theme=dark\r\n\r\n[set.extra]\r\n"), "{t}");

    // A header's comment stays, `]` in it or not; a name in other case
    // still takes its references along.
    let mut d = doc("[set.a] ; see [b]\nkey = x | page | a\n[profile.default]\nleft = a\n");
    assert!(d.rename_section("Set.A", "set.c"));
    assert_eq!(text(&d), "[set.c] ; see [b]\nkey = x | page | c\n[profile.default]\nleft = c\n");
    // Names taken, and names a page key or an edge reads as no set, are
    // refused; a set called so, renamed, leaves page turns and hidden edges.
    let mut d = doc("[set.next]\nkey = x | page | next\nkey = y | pie | next\n[set.b]\nkey = z | page | prev\n[profile.default]\nleft = none\nright = next\n");
    assert!(!d.rename_section("set.next", "set.b"), "a name taken is refused");
    for reserved in ["set.next", "set.PREV", "set.none"] {
        assert!(!d.rename_section("set.b", reserved), "{reserved} is refused");
    }
    assert!(d.rename_section("set.next", "set.more"));
    assert_eq!(
        text(&d),
        "[set.more]\nkey = x | page | next\nkey = y | pie | more\n[set.b]\nkey = z | page | prev\n[profile.default]\nleft = none\nright = more\n",
        "the pie key and the edge follow; the page turn and the hidden edge stay"
    );
}

// AC5
#[test]
fn what_it_writes_reads_back_as_the_settings_edited() {
    let mut d = doc(HAND);
    d.set_value("general", "opacity", Some("60"));
    d.set_value("edge.left", "columns", Some("2"));
    d.set_key("set.pad", 1, "やり直し | shortcut | Ctrl+Y");
    d.move_key("set.pad", 0, 3);
    d.rename_section("set.more", "set.extra");
    d.add_section("profile.csp");
    d.set_value("profile.csp", "process", Some("CLIPStudioPaint.exe"));
    d.set_value("profile.csp", "top", Some("pad"));

    let config = parse(&d.to_bytes()).expect("the text written reads back");
    assert_eq!(config.general.opacity, 60);
    assert_eq!(config.edges[Edge::Left as usize].lines, 2);
    let pad: Vec<&str> = config.set("pad").unwrap().keys.iter().map(|k| k.label.as_str()).collect();
    assert_eq!(pad, ["やり直し", "次", "変形", "Ctrl"]);
    assert_eq!(config.set("pad").unwrap().keys[1].action, Action::Page(Page::Set("extra".into())));
    assert_eq!(config.set("pad").unwrap().keys[2].action, Action::Pie("extra".into()));
    assert!(config.set("more").is_none() && config.set("extra").is_some());
    assert_eq!(config.default_profile().edges[Edge::Right as usize].as_deref(), Some("extra"));
    let csp = config.profile("csp").expect("profile csp");
    assert_eq!(csp.process.as_deref(), Some("clipstudiopaint.exe"));
    assert_eq!(csp.edges[Edge::Top as usize].as_deref(), Some("pad"));
}

// AC6: malformed input; the settings file comes from outside.
#[test]
fn text_the_parser_cannot_read_makes_no_panic_and_broken_bytes_make_no_document() {
    let mut not_utf8 = b"[general]\nopacity = 90\nkey_size = ".to_vec();
    not_utf8.extend([0xFF, 0xFE, b'\n']);
    let error = Document::new(&not_utf8).err().expect("not UTF-8");
    assert_eq!(error.line, 3, "{error:?}");
    assert_eq!(Some(error), parse(&not_utf8).unwrap_err().into_iter().next(), "for the parser's reason");

    let huge = "; padding\n".repeat(MAX_BYTES / 10 + 1);
    let error = Document::new(huge.as_bytes()).err().expect("too big");
    assert_eq!(error.line, 0);
    assert_eq!(Some(error), parse(huge.as_bytes()).unwrap_err().into_iter().next(), "for the parser's reason");

    let noise: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8 & 0x7F).collect();
    let odds: Vec<&[u8]> = vec![&noise, b"[", b"]", b"=", b"key", b"key =", b"[set.]", b"[profile.default", b"\xEF\xBB\xBF", b"[set.a]\nkey = a | page", b"x = ; y"];
    for odd in odds {
        let mut d = Document::new(odd).expect("UTF-8 within the size");
        assert_eq!(d.to_bytes(), odd, "unedited, as it was");
        // Edits on whatever is there, or is not.
        d.set_value("general", "opacity", Some("50"));
        d.set_value("set.a", "key", None);
        d.set_key("set.a", 5, "x | shortcut | X");
        d.insert_key("set.a", 9, "y | shortcut | Y");
        d.remove_key("set.nope", 0);
        d.move_key("set.a", 3, 0);
        d.rename_section("set.a", "set.b");
        d.remove_section("profile.nope");
        let _ = d.keys("set.b");
        let _ = d.to_bytes();
    }
}

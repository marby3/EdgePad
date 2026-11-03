//! Acceptance tests for Issue #163: kana keys, `kana | か`, which send a
//! kana as its romaji keys for the IME to turn into kanji; and `kana | ゛゜小`,
//! which changes the kana sent last.

use edgepad_core::config::*;

const PROFILE: &str = "[profile.default]\nleft = pad\n";

fn read(text: &str) -> Result<Config, Vec<Error>> {
    parse(text.as_bytes())
}

// AC5: malformed input; a kana key's content comes from the file.
#[test]
fn a_kana_that_cannot_be_sent_is_an_error_on_its_line_not_a_panic() {
    for content in ["漢", "ka", "A", "かか", "ア", "゛", "゛゜", "小", "か゛", "\u{3099}", "｡", "-"] {
        let text = format!("[set.pad]\nkey = 1 | kana | {content}\nkey = 2 | shortcut | A\n{PROFILE}");
        let errors = read(&text).expect_err(content);
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2], "{content:?}: {errors:?}");
        // On a flick, the same.
        let text = format!("[set.pad]\nkey = 1 | shortcut | A\nflick = up | x | kana | {content}\n{PROFILE}");
        let errors = read(&text).expect_err(content);
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [3], "a flick of {content:?}: {errors:?}");
    }
    // An empty one is an empty content, as for any key.
    let errors = read(&format!("[set.pad]\nkey = 1 | kana |\n{PROFILE}")).expect_err("empty");
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2]);
    // What reads, for comparison.
    for content in ["か", "が", "ぱ", "っ", "ゃ", "ゎ", "ゔ", "ん", "を", "ー", "、", "。", "？", "！", "゛゜小"] {
        let text = format!("[set.pad]\nkey = 1 | kana | {content}\n{PROFILE}");
        assert!(read(&text).is_ok(), "{content:?} reads");
    }
}

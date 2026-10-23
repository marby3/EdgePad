//! Acceptance tests for Issue #66: pie keys in the settings file (spec 4.2,
//! 4.6 and chapter 7). The menu itself comes later; here they are read and
//! checked.

use edgepad_core::config::*;

/// A pie key on the left, and the set it opens with `items`.
fn with_items(items: &str) -> String {
    format!("[set.pad]\nkey = 変形 | pie | menu\n[set.menu]\n{items}[profile.default]\nleft = pad\n")
}

fn lines(errors: &[Error]) -> Vec<usize> {
    errors.iter().map(|e| e.line).collect()
}

// AC1
#[test]
fn a_pie_key_names_its_key_set_in_any_case_and_an_unknown_set_is_an_error_with_its_line() {
    // The lines the spec example adds.
    let text = "\
[set.csp-tools]
key = 元に戻す | shortcut | Ctrl+Z
key = 変形     | pie      | CSP-Transform

[set.csp-transform]                ; pie の項目（先頭から2〜8個）
key = 拡大縮小 | shortcut | Ctrl+T
key = 上下反転 | shortcut | F14
key = 回転戻し | shortcut | F15
key = 確定     | shortcut | Enter

[profile.default]
left = csp-tools
";
    let config = parse(text.as_bytes()).expect("pie key parses");
    let keys = &config.set("csp-tools").expect("set csp-tools").keys;
    assert_eq!(keys[1], KeyDef { label: "変形".into(), action: Action::Pie("csp-transform".into()), width: 1 });

    let text = "[set.pad]\nkey = A | shortcut | A\nkey = 変形 | pie | nosuch\n[profile.default]\nleft = pad\n";
    let errors = parse(text.as_bytes()).expect_err("unknown key set");
    assert_eq!(lines(&errors), [3], "{errors:?}");
    assert!(errors[0].reason.contains("キーセット「nosuch」が定義されていない"), "{errors:?}");
}

// AC2
#[test]
fn a_pie_key_needs_two_to_eight_items_or_it_is_an_error_on_its_line() {
    let items = |n: usize| (0..n).map(|i| format!("key = {i} | shortcut | F{}\n", i + 1)).collect::<String>();
    for n in [2, 8] {
        let text = with_items(&items(n));
        assert!(parse(text.as_bytes()).is_ok(), "{n} items are fine");
    }
    for n in [1, 9] {
        let text = with_items(&items(n));
        let errors = parse(text.as_bytes()).expect_err("too few or too many items");
        assert_eq!(lines(&errors), [2], "{n} items: the pie key's line; {errors:?}");
        assert!(errors[0].reason.contains(&n.to_string()), "{n} items: the reason says how many; {errors:?}");
    }
}

// AC3
#[test]
fn held_keys_and_pies_cannot_be_items_and_once_only_keys_can() {
    let allowed = "\
key = 戻す | shortcut | Ctrl+Z
key = 保存 | macro    | Ctrl+S, wait 300, Esc
key = 挨拶 | text     | お疲れさまです
key = 電卓 | run      | calc.exe
key = 次へ | page     | next
";
    assert!(parse(with_items(allowed).as_bytes()).is_ok(), "shortcut / macro / text / run / page are items");

    for (kind, item) in [
        ("modifier", "key = 修飾 | modifier | Ctrl\n"),
        ("hold", "key = 押す | hold | Space\n"),
        ("repeat", "key = 連打 | repeat | ]\n"),
        ("pie", "key = 入れ子 | pie | pad\n"),
    ] {
        let text = with_items(&format!("key = 戻す | shortcut | Ctrl+Z\n{item}"));
        let errors = parse(text.as_bytes()).expect_err("an item of a kind a pie cannot hold");
        assert_eq!(lines(&errors), [2], "{kind}: the pie key's line; {errors:?}");
        let label = item.split('|').next().unwrap().trim_start_matches("key =").trim();
        assert!(errors[0].reason.contains(kind) && errors[0].reason.contains(label), "{kind}: names the kind and the item {label:?}; {errors:?}");
    }
}

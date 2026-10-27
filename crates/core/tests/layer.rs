//! Acceptance tests for Issue #104: layer keys in the settings file.

use edgepad_core::config::*;

fn read(keys: &str) -> Result<Config, Vec<Error>> {
    parse(format!("[set.base]\n{keys}\n[set.raise]\nkey = 1 | shortcut | 1\n[profile.default]\nleft = base\n").as_bytes())
}

// AC7, with the forms of AC1 and AC2
#[test]
fn layer_keys_read_and_their_mistakes_are_errors_on_their_lines() {
    let config = read("key = R | layer | raise\nkey = T | layer | Raise | mode=toggle\nkey = H | layer | raise | mode=hold").unwrap_or_else(|e| panic!("{e:?}"));
    let actions: Vec<&Action> = config.set("base").unwrap().keys.iter().map(|k| &k.action).collect();
    assert_eq!(
        actions,
        [
            &Action::Layer { set: "raise".into(), toggle: false },
            &Action::Layer { set: "raise".into(), toggle: true },
            &Action::Layer { set: "raise".into(), toggle: false },
        ]
    );
    for (keys, why) in [
        ("key = A | shortcut | A\nkey = R | layer | nowhere", "no such set"),
        ("key = A | shortcut | A\nkey = R | layer | raise | mode=latch", "no such mode"),
        ("key = A | shortcut | A\nkey = R | layer |  ", "empty"),
    ] {
        let errors = read(keys).expect_err(why);
        assert_eq!(errors[0].line, 3, "{why}: {errors:?}");
    }
    // A pie's items: no layer key among them.
    let text = "[set.base]\nkey = P | pie | items\n[set.items]\nkey = 1 | shortcut | 1\nkey = R | layer | base\n[profile.default]\nleft = base\n";
    let errors = parse(text.as_bytes()).expect_err("a layer key in a pie");
    assert!(errors.iter().any(|e| e.line == 2 && e.reason.contains("layer")), "{errors:?}");
}

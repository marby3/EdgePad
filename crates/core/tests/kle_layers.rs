//! Acceptance tests for Issue #138: layers of a key set laid out by a KLE
//! file, `[set.name.layer]`, on the same layout; keys left to the set
//! below them with `trans`, or with no key line.

use std::path::PathBuf;

use edgepad_core::config::*;

/// A row of four keys.
const ROW: &[u8] = br#"["A","B","C","D"]"#;

fn read(text: &str) -> Result<Config, Vec<Error>> {
    parse_with(text.as_bytes(), &mut |_: &str| Ok(ROW.to_vec()))
}

fn lines(errors: &[Error]) -> Vec<usize> {
    errors.iter().map(|e| e.line).collect()
}

/// kb on the row, with its Fn layer; on the top edge.
const KB: &str = "\
[set.kb]
layout = kle:row.json
key = A  | shortcut | A
key = Fn | layer    | kb.fn
key = C  | shortcut | C
key = D  | shortcut | D
[set.kb.fn]
key = 1  | shortcut | 1
key = -  | trans    | -
key = 3  | shortcut | 3
[profile.default]
top = kb
";

// AC1
#[test]
fn a_set_named_after_a_kle_set_and_a_dot_is_its_layer_on_its_layout() {
    let config = read(KB).unwrap_or_else(|e| panic!("{e:?}"));
    let (kb, layer) = (config.set("kb").expect("kb"), config.set("kb.fn").expect("kb.fn"));
    assert_eq!(layer.keys.len(), 4, "every key of the layout");
    let places = |set: &KeySet| set.keys.iter().map(|k| k.place).collect::<Vec<_>>();
    assert_eq!(places(layer), places(kb), "the same places");
    assert_eq!(layer.keys[0].label, "1");
    assert_eq!(layer.keys[2].label, "3");

    // Wrong: no set below, a set below not of a KLE file, a layout of its
    // own, and a layer named where only a key set goes.
    let wrong = [
        ("[set.pad.fn]\nkey = 1 | shortcut | 1\n[set.pad]\nkey = A | shortcut | A\n[profile.default]\nleft = pad\n", 1),
        ("[set.pad]\nkey = A | shortcut | A\n[set.pad.fn]\nkey = 1 | shortcut | 1\n[profile.default]\nleft = pad\n", 3),
        ("[set.ghost.fn]\nkey = 1 | shortcut | 1\n[set.pad]\nkey = A | shortcut | A\n[profile.default]\nleft = pad\n", 1),
    ];
    for (text, line) in wrong {
        let errors = read(text).expect_err(text);
        assert_eq!(lines(&errors), [line], "{text}\n{errors:?}");
    }
    let with = |at: &str, add: &str| KB.replacen(at, &format!("{at}{add}"), 1);
    let layout = with("[set.kb.fn]\n", "layout = kle:row.json\n");
    assert_eq!(lines(&read(&layout).expect_err("layout")), [8], "a layer's own layout");
    let profile = with("top = kb\n", "left = kb.fn\n");
    assert_eq!(lines(&read(&profile).expect_err("profile")), [13], "an edge showing a layer");
    for kind in ["pie", "page"] {
        // In D's place: the layout has four keys.
        let text = KB.replace("key = D  | shortcut | D\n", &format!("key = P | {kind} | kb.fn\n"));
        let errors = read(&text).expect_err(kind);
        assert_eq!(lines(&errors), [6], "a {kind} key opening a layer: {errors:?}");
    }
}

// AC3
#[test]
fn trans_and_keys_with_no_line_are_the_set_below_s_keys() {
    let config = read(KB).unwrap_or_else(|e| panic!("{e:?}"));
    let (kb, layer) = (config.set("kb").expect("kb"), config.set("kb.fn").expect("kb.fn"));
    // Fn by trans, D by no line: kb's, label and all.
    assert_eq!(layer.keys[1], kb.keys[1], "Fn, through trans");
    assert_eq!(layer.keys[3], kb.keys[3], "D, with no key line");
    assert_eq!(layer.keys[1].action, Action::Layer { set: "kb.fn".into(), toggle: false });

    // On the set below, trans has nothing below it.
    let text = KB.replace("key = C  | shortcut | C\n", "key = C  | trans | -\n");
    assert_eq!(lines(&read(&text).expect_err("trans below")), [5]);
}

/// The settings shipped before Issue #138, with their layers as key sets of
/// their own.
fn shipped(file: PathBuf) -> Config {
    let presets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../presets");
    let text = std::fs::read(file).expect("the settings");
    parse_with(&text, &mut |name: &str| std::fs::read(presets.join(name)).map_err(|e| e.to_string())).unwrap_or_else(|e| panic!("{e:?}"))
}

// AC6
#[test]
fn the_shipped_layers_are_layers_now_and_send_and_show_as_they_did() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Layers as key sets of their own still read.
    let before = shipped(root.join("tests/fixtures/edgepad-v1.0.ini"));
    let now = shipped(root.join("../../presets/edgepad.ini"));
    let renamed = [
        ("kb-us-fn", "kb-us.fn"),
        ("kb-alice64-fn", "kb-alice64.fn"),
        ("kb-split42-lower", "kb-split42.lower"),
        ("kb-split42-upper", "kb-split42.upper"),
        ("kb-corne-v4-lower", "kb-corne-v4.lower"),
        ("kb-corne-v4-upper", "kb-corne-v4.upper"),
    ];
    let new_name = |name: &str| renamed.iter().find(|(old, _)| *old == name).map_or(name.to_string(), |(_, new)| new.to_string());
    let follow = |key: &KeyDef| match &key.action {
        Action::Layer { set, toggle } => KeyDef { action: Action::Layer { set: new_name(set), toggle: *toggle }, ..key.clone() },
        _ => key.clone(),
    };
    for (old, new) in renamed {
        assert!(now.set(old).is_none(), "{old} is a layer now");
        let (was, is) = (before.set(old).expect(old), now.set(new).expect(new));
        assert_eq!(is.keys, was.keys.iter().map(follow).collect::<Vec<_>>(), "{new} as {old} was");
        let base = new.split('.').next().unwrap();
        assert_eq!(now.set(base).expect(base).keys, before.set(base).expect(base).keys.iter().map(follow).collect::<Vec<_>>(), "{base}");
    }
    // Some keys of each are left to the set below.
    let text = std::fs::read_to_string(root.join("../../presets/edgepad.ini")).unwrap();
    for (_, new) in renamed {
        let at = text.find(&format!("[set.{new}]")).unwrap_or_else(|| panic!("[set.{new}]"));
        let section = &text[at..text[at + 1..].find("\n[").map_or(text.len(), |n| at + 1 + n)];
        let written = section.lines().filter(|l| l.starts_with("key")).count();
        let through = section.lines().filter(|l| l.starts_with("key") && l.contains("| trans")).count();
        let all = now.set(new).unwrap().keys.len();
        assert!(through > 0 || written < all, "{new}: {written} key lines, {through} trans, of {all}");
    }
}

// Malformed input: a layer's section name comes from outside.
#[test]
fn a_layer_section_name_that_does_not_read_is_an_error_not_a_panic() {
    for header in ["[set.kb.]", "[set..fn]", "[set.kb.fn.more]", "[set.kb.f n]", "[set.kb.ｆｎ]", "[set.kb.fn ]x", "[set.kb.next.]"] {
        let text = KB.replace("[set.kb.fn]", header);
        let errors = read(&text).expect_err(header);
        assert!(lines(&errors).contains(&7), "{header}: {errors:?}");
    }
}

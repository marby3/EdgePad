//! Acceptance tests for Issue #127: the settings shipped with EdgePad, in
//! presets/: CLIP STUDIO PAINT's keys on the left, keyboards on the bottom.

use std::path::PathBuf;

use edgepad_core::config::*;
use edgepad_core::keys::{Key, Mods};

fn presets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../presets")
}

/// The keyboards: their sets, KLE files, and how many keys each has.
const BOARDS: [(&str, &str, usize); 4] =
    [("kb-us", "kb-us.json", 71), ("kb-split42", "kb-split42.json", 42), ("kb-corne-v4", "kb-corne-v4.json", 46), ("kb-alice64", "kb-alice64.json", 64)];

fn read() -> Result<Config, Vec<Error>> {
    let text = std::fs::read(presets().join("edgepad.ini")).expect("presets/edgepad.ini");
    parse_with(&text, &mut |name: &str| std::fs::read(presets().join(name)).map_err(|e| e.to_string()))
}

fn config() -> Config {
    read().unwrap_or_else(|e| panic!("{e:?}"))
}

fn def(value: &str) -> KeyDef {
    key_def(value).unwrap_or_else(|e| panic!("{value}: {e}"))
}

// AC1
#[test]
fn the_presets_folder_holds_the_settings_and_the_four_keyboards() {
    assert!(presets().join("edgepad.ini").is_file());
    for (_, file, count) in BOARDS {
        let bytes = std::fs::read(presets().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let keys = kle_keys(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(keys.len(), count, "{file}");
    }
}

// AC2
#[test]
fn the_left_edge_has_the_csp_keys_in_order() {
    let config = config();
    assert_eq!(config.profile_for(None).edges[Edge::Left as usize].as_deref(), Some("csp"));
    let keys = &config.set("csp").expect("[set.csp]").keys;
    let want = [
        "Esc | shortcut | Esc",
        "\u{200b} | none | -",
        "P | shortcut | P",
        "B | shortcut | B",
        "E | shortcut | E",
        "X | shortcut | X",
        "/ | shortcut | /",
        "[ | repeat | [",
        "] | repeat | ]",
        "Ctrl | modifier | Ctrl",
        "Shift | modifier | Shift",
    ];
    let want: Vec<KeyDef> = want.iter().map(|k| def(k)).collect();
    assert_eq!(keys, &want);
}

// AC3
#[test]
fn the_bottom_edge_has_the_us_keyboard_and_the_others_are_there_to_choose() {
    let config = config();
    assert_eq!(config.profile_for(None).edges[Edge::Bottom as usize].as_deref(), Some("kb-us"));
    for (set, file, _) in BOARDS {
        let keys = &config.set(set).unwrap_or_else(|| panic!("[set.{set}]")).keys;
        let kle = kle_keys(&std::fs::read(presets().join(file)).unwrap()).unwrap();
        assert_eq!(keys.len(), kle.len(), "{set}: a key line for each key of the file");
        for (key, (_, label)) in keys.iter().zip(&kle) {
            let at = format!("{set}, KLE「{label}」");
            match label.as_str() {
                "Ctrl" | "Shift" | "Alt" | "Win" | "GUI" => assert!(matches!(key.action, Action::Modifier(_)), "{at}: {:?}", key.action),
                "Fn" | "Lower" | "Upper" => {
                    let Action::Layer { set: to, toggle: false } = &key.action else { panic!("{at}: {:?}", key.action) };
                    assert!(config.set(to).is_some(), "{at}: the layer {to} is there");
                }
                l if l.len() == 1 && l.as_bytes()[0].is_ascii_uppercase() => {
                    let c = l.as_bytes()[0];
                    // Two files call their home row's S a Z: it sends S there.
                    let s = matches!(key.action, Action::Hold(Key::Char(b'S'))) && c == b'Z';
                    assert!(key.action == Action::Hold(Key::Char(c)) || s, "{at}: {:?}", key.action);
                }
                "" => assert!(matches!(key.action, Action::None | Action::Hold(Key::Space)), "{at}: {:?}", key.action),
                _ => assert!(!matches!(key.action, Action::None) || key.label == "Caps", "{at}: sends something"),
            }
        }
    }
    // The layers' keys, by what they send.
    let sends = |set: &str, label: &str| config.set(set).unwrap().keys.iter().find(|k| k.label == label).map(|k| k.action.clone());
    assert_eq!(sends("kb-us-fn", "F1"), Some(Action::Hold(Key::F(1))));
    assert_eq!(sends("kb-us-fn", "F12"), Some(Action::Hold(Key::F(12))));
    assert_eq!(sends("kb-split42-lower", "1"), Some(Action::Hold(Key::Char(b'1'))));
    assert_eq!(sends("kb-split42-lower", "←"), Some(Action::Hold(Key::Left)));
    let shifted = |c: u8| Some(Action::Shortcut(edgepad_core::keys::Combo { mods: Mods::SHIFT, key: Some(Key::Char(c)) }));
    assert_eq!(sends("kb-corne-v4-upper", "!"), shifted(b'1'));
    assert_eq!(sends("kb-corne-v4-upper", "~"), shifted(b'`'));
}

// AC5
#[test]
fn the_shipped_settings_read_without_an_error() {
    if let Err(errors) = read() {
        panic!("{errors:?}");
    }
}

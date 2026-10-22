//! Acceptance tests for Issue #14: the settings file parser (spec, chapter 7).

use edgepad_core::config::*;
use edgepad_core::keys::{Combo, Key, Mods};

/// The example of spec chapter 7, verbatim.
const SPEC_EXAMPLE: &str = "\
[general]
hotkey        = Ctrl+Alt+E   ; 全辺の表示切替
key_size      = 56           ; キー1辺（論理px）
opacity       = 90           ; 通常表示の不透明度 %
dim_opacity   = 35           ; 半透明表示の不透明度 %
latch_ms      = 250          ; これ未満のタップでラッチ
pen_presses   = false        ; ペンでキーを押せるか
theme         = auto         ; light / dark / auto

[edge.left]
enabled  = true
monitor  = primary           ; primary / 1,2... / cursor
columns  = 1
handle   = 6, 40%            ; ハンドル幅px, 長さ

[edge.right]
enabled  = true
monitor  = primary
columns  = 2

[set.mods]
key = Esc   | shortcut | Esc
key = Ctrl  | modifier | Ctrl
key = Shift | modifier | Shift
key = Alt   | modifier | Alt
key = Space | hold     | Space

[set.csp-tools]
key = 元に戻す | shortcut | Ctrl+Z
key = やり直し | shortcut | Ctrl+Y
key = ブラシ小 | repeat   | [        | interval=60
key = ブラシ大 | repeat   | ]        | interval=60
key = 選択解除 | shortcut | Ctrl+D
key = 左右反転 | shortcut | F13      ; CSP側でF13に割り当てる運用
key = 保存     | macro    | Ctrl+S, wait 300, Esc
key = 次へ     | page     | next

[profile.default]
left  = mods
right = none

[profile.csp]
process = CLIPStudioPaint.exe
left    = mods
right   = csp-tools
";

fn combo(mods: Mods, key: Key) -> Combo {
    Combo { mods, key: Some(key) }
}

fn key(label: &str, action: Action) -> KeyDef {
    KeyDef { label: label.into(), action, width: 1 }
}

// AC1
#[test]
fn the_spec_example_reads_without_errors_and_every_value_comes_through() {
    let config = parse(SPEC_EXAMPLE.as_bytes()).expect("spec example parses");

    let g = &config.general;
    assert_eq!(g.hotkey, Some(combo(Mods::CTRL | Mods::ALT, Key::Char(b'E'))));
    assert_eq!((g.key_size, g.opacity, g.dim_opacity, g.latch_ms), (56, 90, 35, 250));
    assert!(!g.pen_presses);
    assert_eq!(g.theme, Theme::Auto);

    let left = &config.edges[Edge::Left as usize];
    assert_eq!(*left, EdgeConfig { enabled: true, monitor: Monitor::Primary, lines: 1, handle: Handle { width: 6, length: 40, position: 50 } });
    let right = &config.edges[Edge::Right as usize];
    assert_eq!((right.enabled, right.monitor, right.lines), (true, Monitor::Primary, 2));

    let ctrl = Mods::CTRL;
    assert_eq!(
        config.set("mods").expect("set mods").keys,
        [
            key("Esc", Action::Shortcut(Combo { mods: Mods::NONE, key: Some(Key::Esc) })),
            key("Ctrl", Action::Modifier(Key::Ctrl)),
            key("Shift", Action::Modifier(Key::Shift)),
            key("Alt", Action::Modifier(Key::Alt)),
            key("Space", Action::Hold(Key::Space)),
        ]
    );
    assert_eq!(
        config.set("csp-tools").expect("set csp-tools").keys,
        [
            key("元に戻す", Action::Shortcut(combo(ctrl, Key::Char(b'Z')))),
            key("やり直し", Action::Shortcut(combo(ctrl, Key::Char(b'Y')))),
            key("ブラシ小", Action::Repeat { combo: combo(Mods::NONE, Key::Char(b'[')), interval_ms: 60 }),
            key("ブラシ大", Action::Repeat { combo: combo(Mods::NONE, Key::Char(b']')), interval_ms: 60 }),
            key("選択解除", Action::Shortcut(combo(ctrl, Key::Char(b'D')))),
            key("左右反転", Action::Shortcut(combo(Mods::NONE, Key::F(13)))),
            key(
                "保存",
                Action::Macro(vec![
                    Step::Keys(combo(ctrl, Key::Char(b'S'))),
                    Step::Wait(300),
                    Step::Keys(combo(Mods::NONE, Key::Esc)),
                ])
            ),
            key("次へ", Action::Page(Page::Next)),
        ]
    );

    let default = config.default_profile();
    assert_eq!(default.edges, [Some("mods".into()), None, None, None]);
    let csp = config.profile("csp").expect("profile csp");
    assert_eq!(csp.process.as_deref(), Some("clipstudiopaint.exe"));
    assert_eq!(csp.edges, [Some("mods".into()), Some("csp-tools".into()), None, None]);

    // The kinds the example leaves out, the width option, and the defaults
    // of everything not written.
    let rest = "\
[set.more]
key = 挨拶 | text  | お疲れさまです
key = 電卓 | run   | calc.exe
key = 戻る | page  | prev
key = 切替 | page  | more
key = 広い | hold  | Shift | width=2
key = 既定 | repeat | Left
[profile.default]
top = more
";
    let config = parse(rest.as_bytes()).expect("second example parses");
    assert_eq!(
        config.set("more").unwrap().keys,
        [
            key("挨拶", Action::Text("お疲れさまです".into())),
            key("電卓", Action::Run("calc.exe".into())),
            key("戻る", Action::Page(Page::Prev)),
            key("切替", Action::Page(Page::Set("more".into()))),
            KeyDef { label: "広い".into(), action: Action::Hold(Key::Shift), width: 2 },
            key("既定", Action::Repeat { combo: combo(Mods::NONE, Key::Left), interval_ms: 50 }),
        ]
    );
    let g = &config.general;
    assert_eq!(g.hotkey, None);
    assert_eq!((g.key_size, g.opacity, g.dim_opacity, g.latch_ms), (56, 90, 35, 250));
    assert_eq!((g.pen_presses, g.theme), (false, Theme::Auto));
    for edge in &config.edges {
        assert_eq!(*edge, EdgeConfig { enabled: true, monitor: Monitor::Primary, lines: 1, handle: Handle { width: 6, length: 40, position: 50 } });
    }
    assert_eq!(config.default_profile().edges, [None, None, Some("more".into()), None]);
}

// AC2
#[test]
fn every_wrong_line_is_reported_with_its_line_number_and_reason() {
    let text = "\
[general]
opacity = 150
theme = blue
colour = red
[edge.middle]
enabled = true
[set.a]
key = X | press | A
key = Y | hold | Foo
key = Z | hold
[set.a]
[profile.p]
process = x.exe
left = nosuch
";
    let errors = parse(text.as_bytes()).expect_err("the text is full of mistakes");
    let got: Vec<usize> = errors.iter().map(|e| e.line).collect();
    // Line 0 is the file as a whole: here, the missing [profile.default].
    assert_eq!(got, [0, 2, 3, 4, 5, 8, 9, 10, 11, 14], "{errors:#?}");
    let expect = ["profile.default", "150", "blue", "colour", "edge.middle", "press", "Foo", "欄", "set.a", "nosuch"];
    for (error, fragment) in errors.iter().zip(expect) {
        assert!(error.reason.contains(fragment), "line {}: {:?} should mention {fragment:?}", error.line, error.reason);
    }
}

// Malformed input: the settings file comes from outside.
#[test]
fn broken_bytes_and_oversized_files_are_errors_not_panics() {
    let mut not_utf8 = b"[general]\nopacity = 90\nkey_size = ".to_vec();
    not_utf8.extend([0xFF, 0xFE, b'\n']);
    let errors = parse(&not_utf8).expect_err("not UTF-8");
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [3], "{errors:?}");

    let huge = "; padding\n".repeat(MAX_BYTES / 10 + 1);
    let errors = parse(huge.as_bytes()).expect_err("over the size limit");
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [0], "{errors:?}");

    // Arbitrary bytes and shapes: any answer but a panic will do.
    let noise: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    let _ = parse(&noise);
    for odd in ["[", "]", "=", "key", "key =", "key = |||||", "[set.]", "[profile.default", "\u{feff}", "[set.a]\nkey = a | macro | ,,,"] {
        let _ = parse(odd.as_bytes());
    }
}

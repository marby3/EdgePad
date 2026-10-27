//! Acceptance tests for Issue #14: the settings file parser (spec, chapter 7).

use edgepad_core::config::*;
use edgepad_core::keys::{Combo, Key, Mods};

/// The example of spec chapter 7, verbatim.
const SPEC_EXAMPLE: &str = "\
[general]
hotkey        = Ctrl+Alt+E   ; 全辺の表示切替
key_size      = 56           ; キー1辺（論理px）
opacity       = 100          ; 通常表示の不透明度 %
dim_opacity   = 35           ; パイメニュー表示中とペンが乗っている時の不透明度 %
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
key = 変形     | pie      | csp-transform

[set.csp-transform]                ; pie の項目（先頭から2〜8個）
key = 拡大縮小 | shortcut | Ctrl+T
key = 上下反転 | shortcut | F14
key = 回転戻し | shortcut | F15
key = 確定     | shortcut | Enter

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
    KeyDef { label: label.into(), action, width: 1, place: None }
}

// AC1
#[test]
fn the_spec_example_reads_without_errors_and_every_value_comes_through() {
    let config = parse(SPEC_EXAMPLE.as_bytes()).expect("spec example parses");

    let g = &config.general;
    assert_eq!(g.hotkey, Some(combo(Mods::CTRL | Mods::ALT, Key::Char(b'E'))));
    assert_eq!((g.key_size, g.opacity, g.dim_opacity, g.latch_ms), (56, 100, 35, 250));
    assert!(!g.pen_presses);
    assert_eq!(g.theme, Theme::Auto);

    let left = &config.edges[Edge::Left as usize];
    assert_eq!(*left, EdgeConfig { enabled: true, monitor: Monitor::Primary, lines: 1, handle: Some(Handle { width: 6, length: 40, position: 50 }), dock: false });
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
            key("変形", Action::Pie("csp-transform".into())),
        ]
    );
    assert_eq!(
        config.set("csp-transform").expect("set csp-transform").keys,
        [
            key("拡大縮小", Action::Shortcut(combo(ctrl, Key::Char(b'T')))),
            key("上下反転", Action::Shortcut(combo(Mods::NONE, Key::F(14)))),
            key("回転戻し", Action::Shortcut(combo(Mods::NONE, Key::F(15)))),
            key("確定", Action::Shortcut(combo(Mods::NONE, Key::Enter))),
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
            KeyDef { label: "広い".into(), action: Action::Hold(Key::Shift), width: 2, place: None },
            key("既定", Action::Repeat { combo: combo(Mods::NONE, Key::Left), interval_ms: 50 }),
        ]
    );
    let g = &config.general;
    assert_eq!(g.hotkey, None);
    assert_eq!((g.key_size, g.opacity, g.dim_opacity, g.latch_ms), (56, 100, 35, 250));
    assert_eq!((g.pen_presses, g.theme), (false, Theme::Auto));
    for edge in &config.edges {
        assert_eq!(*edge, EdgeConfig { enabled: true, monitor: Monitor::Primary, lines: 1, handle: Some(Handle { width: 6, length: 40, position: 50 }), dock: false });
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

// Issue #56 AC4: `accent` is auto or #RRGGBB, anything else is an error.
#[test]
fn accent_takes_auto_or_rrggbb_and_anything_else_is_an_error_with_its_line() {
    let good = "[general]
accent = #1A2B3C
[profile.default]
";
    let config = parse(good.as_bytes()).expect("#RRGGBB reads");
    assert_eq!(config.general.accent, Accent::Rgb(0x1A, 0x2B, 0x3C));
    let config = parse(b"[general]
accent = AUTO
[profile.default]
").expect("auto reads");
    assert_eq!(config.general.accent, Accent::Auto);
    assert_eq!(General::default().accent, Accent::Auto, "auto is the default");

    for value in ["red", "#12345", "#12345G"] {
        let bad = format!("[general]
accent = {value}
[profile.default]
");
        let errors = parse(bad.as_bytes()).expect_err("not a colour");
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2], "{value}: {errors:#?}");
        let reason = &errors[0].reason;
        assert!(reason.contains("#RRGGBB") && reason.contains(value), "{value}: {reason:?}");
    }
}

// Issue #58 AC5: hover_ms is 0 or 100..=5000, auto_hide_ms 0 or
// 1000..=3600000; anything else is an error with its line.
#[test]
fn hover_ms_and_auto_hide_ms_take_zero_or_their_range_and_anything_else_is_an_error() {
    let defaults = General::default();
    assert_eq!((defaults.hover_ms, defaults.auto_hide_ms), (0, 0), "both are off by default");
    let good = "[general]\nhover_ms = 100\nauto_hide_ms = 3600000\n[profile.default]\n";
    let g = parse(good.as_bytes()).expect("in range").general;
    assert_eq!((g.hover_ms, g.auto_hide_ms), (100, 3_600_000));
    let off = "[general]\nhover_ms = 0\nauto_hide_ms = 0\n[profile.default]\n";
    let g = parse(off.as_bytes()).expect("0 is off").general;
    assert_eq!((g.hover_ms, g.auto_hide_ms), (0, 0));

    for (name, value) in [("hover_ms", "99"), ("hover_ms", "5001"), ("hover_ms", "soon"), ("auto_hide_ms", "999"), ("auto_hide_ms", "3600001"), ("auto_hide_ms", "-5")] {
        let bad = format!("[general]\n{name} = {value}\n[profile.default]\n");
        let errors = parse(bad.as_bytes()).expect_err("out of range");
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2], "{name} = {value}: {errors:#?}");
        let reason = &errors[0].reason;
        assert!(reason.contains(name) && reason.contains(value), "{name} = {value}: {reason:?}");
    }
}

// Issue #60 AC4: `dock` is true or false; anything else is an error.
#[test]
fn dock_is_true_or_false_and_anything_else_is_an_error_with_its_line() {
    assert!(!EdgeConfig::default().dock, "off by default");
    let config = parse(b"[edge.left]
dock = true
[profile.default]
").expect("true reads");
    assert!(config.edges[Edge::Left as usize].dock);

    let errors = parse(b"[edge.left]
dock = yes
[profile.default]
").expect_err("yes is not a boolean");
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2], "{errors:#?}");
    let reason = &errors[0].reason;
    assert!(reason.contains("dock") && reason.contains("yes"), "{reason:?}");
}

// Issue #64 AC5: slide_ms is 0 or 50..=500; anything else is an error.
#[test]
fn slide_ms_takes_zero_or_50_to_500_and_anything_else_is_an_error() {
    assert_eq!(General::default().slide_ms, 0, "off by default");
    for (value, want) in [("0", 0), ("50", 50), ("500", 500)] {
        let text = format!("[general]
slide_ms = {value}
[profile.default]
");
        assert_eq!(parse(text.as_bytes()).expect("in range").general.slide_ms, want);
    }
    for value in ["49", "501", "fast"] {
        let text = format!("[general]
slide_ms = {value}
[profile.default]
");
        let errors = parse(text.as_bytes()).expect_err("out of range");
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2], "{value}: {errors:#?}");
        let reason = &errors[0].reason;
        assert!(reason.contains("slide_ms") && reason.contains(value), "{value}: {reason:?}");
    }
}

//! Acceptance tests for Issue #101: key sets of keys placed freely, by
//! hand or from Keyboard Layout Editor's raw data (spec 4.4, 5.2, 7).

use edgepad_core::config::*;
use edgepad_core::layout::{Place, Rect, Tilt, free_panel};

const PROFILE: &str = "[profile.default]\nleft = pad\n";

/// A place in keys: (x, y, w, h).
fn at(x: f32, y: f32, w: f32, h: f32) -> Place {
    let q = |v: f32| (v * 4.0) as i32;
    Place { x: q(x), y: q(y), w: q(w), h: q(h), tilt: None }
}

fn read(text: &str, files: &[(&str, &str)]) -> Result<Config, Vec<Error>> {
    parse_with(text.as_bytes(), &mut |name: &str| {
        files.iter().find(|(n, _)| *n == name).map(|(_, t)| t.as_bytes().to_vec()).ok_or_else(|| "ない".to_string())
    })
}

fn places(config: &Config) -> Vec<Option<Place>> {
    config.set("pad").expect("pad").keys.iter().map(|k| k.place).collect()
}

/// A raw data of KLE, as its Raw data box has it: rows, no outer brackets,
/// names unquoted, a key 1.5 wide, a gap, a key 2 tall.
const RAW: &str = r#"["Esc",{x:0.5},"Tab"],
[{w:1.5},"Ctrl",{h:2},"Enter"],
["Alt"]"#;

// AC1
#[test]
fn a_free_set_places_each_key_by_x_y_w_h_in_quarter_keys() {
    let text = format!(
        "[set.pad]\nlayout = free\nkey = A | hold | A | x=0, y=0\nkey = B | hold | B | x=1.25, y=0, w=1.5\nkey = C | hold | C | x=0, y=1, w=2, h=0.75\n{PROFILE}"
    );
    let config = read(&text, &[]).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(places(&config), [Some(at(0.0, 0.0, 1.0, 1.0)), Some(at(1.25, 0.0, 1.5, 1.0)), Some(at(0.0, 1.0, 2.0, 0.75))]);
    // Not in quarters, or no place: refused, on the key's line.
    for (key, line) in [("key = A | hold | A | x=0.3, y=0", 3), ("key = A | hold | A | w=2", 3)] {
        let text = format!("[set.pad]\nlayout = free\n{key}\n{PROFILE}");
        let errors = read(&text, &[]).expect_err(key);
        assert_eq!(errors[0].line, line, "{key}: {errors:?}");
    }
    // The panel holds every key, and no more: 2.75 keys by 1.75.
    let monitor = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    let ps: Vec<Place> = places(&config).into_iter().flatten().collect();
    let (panel, keys) = free_panel(monitor, Edge::Top, 40, &ps);
    assert_eq!((panel.w, panel.h), (110, 70), "{panel:?}");
    assert_eq!(keys[1], Rect { x: 50, y: 0, w: 60, h: 40 });
}

// AC2
#[test]
fn a_kle_set_reads_its_keys_places_from_the_raw_data_file() {
    let text = format!("[set.pad]\nlayout = kle:pad.txt\nkey = 1 | hold | A\nkey = 2 | hold | B\nkey = 3 | hold | C\nkey = 4 | hold | D\nkey = 5 | hold | E\n{PROFILE}");
    let config = read(&text, &[("pad.txt", RAW)]).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        places(&config),
        [
            Some(at(0.0, 0.0, 1.0, 1.0)),
            Some(at(1.5, 0.0, 1.0, 1.0)),
            Some(at(0.0, 1.0, 1.5, 1.0)),
            Some(at(1.5, 1.0, 1.0, 2.0)),
            Some(at(0.0, 2.0, 1.0, 1.0)),
        ]
    );
    // The file as KLE downloads it: JSON, a name first, all in brackets.
    let json = r#"[{"name":"pad"},["Esc",{"x":0.5},"Tab"],[{"w":1.5},"Ctrl",{"h":2},"Enter"],["Alt"]]"#;
    let downloaded = read(&text, &[("pad.txt", json)]).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(places(&downloaded), places(&config));
}

// AC3
#[test]
fn kle_keys_go_to_the_key_lines_in_order_and_the_rest_send_nothing() {
    let text = format!("[set.pad]\nlayout = kle:pad.txt\nkey = 1 | hold | A\nkey = 2 | hold | B\n{PROFILE}");
    let config = read(&text, &[("pad.txt", RAW)]).unwrap_or_else(|e| panic!("{e:?}"));
    let keys = &config.set("pad").unwrap().keys;
    let labels: Vec<&str> = keys.iter().map(|k| k.label.as_str()).collect();
    assert_eq!(labels, ["1", "2", "Ctrl", "Enter", "Alt"], "the key lines first, then KLE's labels");
    assert_eq!(keys[0].action, Action::Hold(edgepad_core::keys::Key::parse("A").unwrap()));
    assert!(keys[2..].iter().all(|k| k.action == Action::None), "{keys:?}");

    // More key lines than keys: the first one over, by its line.
    let mut text = String::from("[set.pad]\nlayout = kle:pad.txt\n");
    for i in 0..6 {
        text += &format!("key = {i} | hold | A\n");
    }
    let errors = read(&(text + PROFILE), &[("pad.txt", RAW)]).expect_err("6 lines, 5 keys");
    assert_eq!(errors[0].line, 8, "{errors:?}");
}

// AC4
#[test]
fn a_free_layout_turns_on_the_side_edges_its_bottom_row_against_the_screen_edge() {
    let monitor = Rect { x: 0, y: 0, w: 1000, h: 800 };
    // Two keys in the first row, one 2 wide below: 2 keys by 2.
    let ps = [at(0.0, 0.0, 1.0, 1.0), at(1.0, 0.0, 1.0, 1.0), at(0.0, 1.0, 2.0, 1.0)];
    let k = 40;
    // The top and the bottom: as laid out, the first row on top.
    let as_laid_out = [Rect { x: 0, y: 0, w: 40, h: 40 }, Rect { x: 40, y: 0, w: 40, h: 40 }, Rect { x: 0, y: 40, w: 80, h: 40 }];
    let (top, keys) = free_panel(monitor, Edge::Top, k, &ps);
    assert_eq!((top.x, top.y, top.w, top.h), (460, 0, 80, 80));
    assert_eq!(keys, as_laid_out);
    let (bottom, keys) = free_panel(monitor, Edge::Bottom, k, &ps);
    assert_eq!((bottom.y, bottom.h), (720, 80));
    assert_eq!(keys, as_laid_out, "the first row on top on the bottom edge too");
    // The left edge: turned clockwise, the bottom row against the edge, a
    // row's left end at the top.
    let (left, keys) = free_panel(monitor, Edge::Left, k, &ps);
    assert_eq!((left.x, left.y, left.w, left.h), (0, 360, 80, 80));
    assert_eq!(keys, [Rect { x: 40, y: 0, w: 40, h: 40 }, Rect { x: 40, y: 40, w: 40, h: 40 }, Rect { x: 0, y: 0, w: 40, h: 80 }]);
    // The right edge: turned anticlockwise, the bottom row against the
    // edge, a row's left end at the bottom.
    let (right, keys) = free_panel(monitor, Edge::Right, k, &ps);
    assert_eq!(right.x, 920);
    assert_eq!(keys, [Rect { x: 0, y: 40, w: 40, h: 40 }, Rect { x: 0, y: 0, w: 40, h: 40 }, Rect { x: 40, y: 0, w: 40, h: 80 }]);
}

// AC5
#[test]
fn a_free_layout_shows_every_key_on_one_page_whatever_the_columns() {
    // Ten keys in a row, on a left edge set to one column: all of them.
    let monitor = Rect { x: 0, y: 0, w: 1000, h: 800 };
    let ps: Vec<Place> = (0..10).map(|i| at(i as f32, 0.0, 1.0, 1.0)).collect();
    let (panel, keys) = free_panel(monitor, Edge::Left, 40, &ps);
    assert_eq!(keys.len(), 10);
    assert_eq!((panel.w, panel.h), (40, 400));
    let text = "[edge.left]\ncolumns = 3\n[set.pad]\nlayout = free\nkey = A | page | next | x=0, y=0\n[profile.default]\nleft = pad\n";
    assert!(read(text, &[]).is_ok(), "columns are no error, a page key none either");
}

// AC6
#[test]
fn a_kle_file_that_cannot_be_used_is_an_error_on_the_layout_line() {
    for (file, why) in [
        (None, "missing"),
        (Some("[\"A\",{w:2"), "cut short"),
        (Some("not kle"), "not raw data"),
        (Some("[{r:15,rx:1},\"A\"]"), "rotated"),
    ] {
        let text = format!("[set.pad]\nkey = A | hold | A\nlayout = kle:pad.txt\n{PROFILE}");
        let files: Vec<(&str, &str)> = file.map(|f| ("pad.txt", f)).into_iter().collect();
        let errors = read(&text, &files).expect_err(why);
        assert_eq!(errors[0].line, 3, "{why}: {errors:?}");
    }
    // Colours, fonts and such: passed over.
    let looks = r##"[{c:"#cccccc",t:"#000000",a:4,f:3,p:"DSA"},"A",{g:true},"B"]"##;
    let text = format!("[set.pad]\nlayout = kle:pad.txt\nkey = A | hold | A\n{PROFILE}");
    let config = read(&text, &[("pad.txt", looks)]).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(places(&config), [Some(at(0.0, 0.0, 1.0, 1.0)), Some(at(1.0, 0.0, 1.0, 1.0))]);
}

// AC8
#[test]
fn a_set_without_layout_runs_along_the_edge_as_before() {
    let config = parse(b"[set.pad]\nkey = A | hold | A | width=2\n[profile.default]\nleft = pad\n").unwrap_or_else(|e| panic!("{e:?}"));
    let key = &config.set("pad").unwrap().keys[0];
    assert_eq!((key.width, key.place), (2, None));
    // x, y, w, h belong to a free set.
    let errors = parse(b"[set.pad]\nkey = A | hold | A | x=0, y=0\n[profile.default]\nleft = pad\n").expect_err("x outside free");
    assert_eq!(errors[0].line, 2);
}

// Issue #126, AC1: turned keys are read as KLE places them. A turns 90°
// about (1, 1), and B after it, in the next row, starts again from rx and
// turns as A does. C, after rx and ry written again, starts from them, not
// turned. Each turned key keeps its size, its middle where KLE turns it to:
// its place is the square about that middle, in quarters (where it goes
// should it lose the turn), its tilt the middle in thousandths of a key and
// the angle in hundredths of a degree, clockwise.
#[test]
fn turned_kle_keys_are_read_where_kle_turns_them() {
    let kle = r#"[{r:90,rx:1,ry:1},"A"],["B"],[{r:0,rx:5,ry:0},"C"]"#;
    let text = format!("[set.pad]\nlayout = kle:pad.txt\nkey = 1 | hold | A\nkey = 2 | hold | B\nkey = 3 | hold | C\n{PROFILE}");
    let config = read(&text, &[("pad.txt", kle)]).unwrap_or_else(|e| panic!("{e:?}"));
    let turned = |x, y| Some(Tilt { x, y, angle: 9000 });
    assert_eq!(
        places(&config),
        [
            Some(Place { x: 24, y: 0, w: 4, h: 4, tilt: None }),
            Some(Place { x: 0, y: 4, w: 4, h: 4, tilt: turned(500, 1500) }),
            Some(Place { x: 4, y: 4, w: 4, h: 4, tilt: turned(1500, 1500) }),
        ]
    );
}

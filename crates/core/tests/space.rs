//! Acceptance tests for Issue #139: room left between the keys of a key set
//! laid along its edge, written `space = length` between its key lines.

use edgepad_core::config::*;
use edgepad_core::layout::{Rect, edge_panel, pages};

const PROFILE: &str = "[profile.default]\nleft = pad\n";

/// A monitor 10 keys of 40 px tall (and wide): a quarter key is 10 px.
const SCREEN: Rect = Rect { x: 0, y: 0, w: 400, h: 400 };

fn read(text: &str) -> Result<Config, Vec<Error>> {
    parse_with(text.as_bytes(), &mut |_: &str| Ok(b"[\"A\",\"B\"]".to_vec()))
}

/// Each key's room after it, in quarter keys.
fn spaces(config: &Config) -> Vec<u8> {
    config.set("pad").expect("pad").keys.iter().map(|k| k.space).collect()
}

fn lines(errors: &[Error]) -> Vec<usize> {
    errors.iter().map(|e| e.line).collect()
}

// AC1
#[test]
fn space_lines_leave_room_after_a_key_in_quarter_keys_from_a_quarter_to_two() {
    let text = format!(
        "[set.pad]\nkey = A | hold | A\nspace = 0.5\nkey = B | hold | B\nkey = C | hold | C\nspace = 2\nkey = D | hold | D\n{PROFILE}"
    );
    let config = read(&text).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(spaces(&config), [2, 0, 8, 0]);

    // Out of range, or not in quarters: refused on its line.
    for value in ["0", "0.1", "2.25", "3", "0.3", "1.6"] {
        let text = format!("[set.pad]\nkey = A | hold | A\nspace = {value}\nkey = B | hold | B\n{PROFILE}");
        let errors = read(&text).expect_err(value);
        assert_eq!(lines(&errors), [3], "{value}: {errors:?}");
        assert!(errors[0].reason.contains("space"), "{value}: {errors:?}");
    }
    // A free or KLE layout places its keys: no space there.
    for layout in ["free", "kle:pad.txt"] {
        let key = if layout == "free" { "key = A | hold | A | x=0, y=0" } else { "key = A | hold | A" };
        let text = format!("[set.pad]\nlayout = {layout}\n{key}\nspace = 1\n{PROFILE}");
        let errors = read(&text).expect_err(layout);
        assert_eq!(lines(&errors), [4], "{layout}: {errors:?}");
    }
}

// AC2
#[test]
fn a_space_takes_its_room_in_the_row_except_where_the_row_breaks_or_a_page_ends() {
    // One column: A, half a key, B, then C.
    let (panel, keys) = edge_panel(SCREEN, Edge::Left, 40, 1, &[1, 1, 1], &[2, 0, 0]);
    assert_eq!(panel.h, 140, "three keys and half a key long: {panel:?}");
    assert_eq!(keys.iter().map(|k| (k.y, k.h)).collect::<Vec<_>>(), [(0, 40), (60, 40), (100, 40)]);

    // After the last key, it takes no room.
    let (panel, _) = edge_panel(SCREEN, Edge::Left, 40, 1, &[1, 1], &[0, 4]);
    assert_eq!(panel.h, 80, "{panel:?}");

    // Two columns: the room after B, where the first column ends, is left
    // out; the quarter after A is not, and makes the panel the longer.
    let (panel, keys) = edge_panel(SCREEN, Edge::Left, 40, 2, &[1, 1, 1, 1], &[1, 4, 0, 0]);
    let at: Vec<(i32, i32)> = keys.iter().map(|k| (k.x, k.y)).collect();
    assert_eq!(at, [(0, 0), (0, 50), (40, 0), (40, 40)], "{panel:?}");
    assert_eq!(panel.h, 110, "{panel:?}");

    // Split into pages, a page's keys end where the room would have gone:
    // nine keys with a key's room after each fill pages of five, which with
    // their four rooms are nine keys long.
    let split = pages(SCREEN, Edge::Left, 40, 1, &[1; 9], &[4; 9], &[false; 9]);
    assert_eq!(split, [vec![0, 1, 2, 3, 4], vec![5, 6, 7, 8]]);
}

// AC5
#[test]
fn a_file_with_no_space_lines_reads_and_lays_out_as_before() {
    let text = format!("[set.pad]\nkey = A | hold | A\nkey = B | hold | B | width=2\nkey = C | hold | C\n{PROFILE}");
    let config = read(&text).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(spaces(&config), [0, 0, 0]);
    let (panel, keys) = edge_panel(SCREEN, Edge::Left, 40, 1, &[1, 2, 1], &[0, 0, 0]);
    assert_eq!(panel.h, 160);
    assert_eq!(keys.iter().map(|k| (k.y, k.h)).collect::<Vec<_>>(), [(0, 40), (40, 80), (120, 40)]);
    assert_eq!(edge_panel(SCREEN, Edge::Left, 40, 1, &[1, 2, 1], &[]), (panel, keys), "no spaces given: none");
}

// Malformed input: the settings file comes from outside.
#[test]
fn a_space_that_does_not_read_is_an_error_not_a_panic() {
    for value in ["", "abc", "-1", "1e3", "0.25.5", "nan", "inf", "99999999999999999999", "½", " 1 x"] {
        let text = format!("[set.pad]\nkey = A | hold | A\nspace = {value}\nkey = B | hold | B\n{PROFILE}");
        let errors = read(&text).expect_err(value);
        assert_eq!(lines(&errors), [3], "{value:?}: {errors:?}");
    }
    // Before any key, there is nothing for it to follow.
    let errors = read(&format!("[set.pad]\nspace = 1\nkey = A | hold | A\n{PROFILE}")).expect_err("first");
    assert_eq!(lines(&errors), [2], "{errors:?}");
}

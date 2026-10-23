//! Acceptance tests for run and page keys (roadmap v0.3, spec 4.2 and 4.4):
//! the core half, splitting a run key's command and a key set into pages.

use edgepad_core::config::{Edge, parse, run_target};
use edgepad_core::layout::{Rect, pages};

// AC1
#[test]
fn a_run_command_is_a_program_then_its_arguments_with_quotes_around_spaces() {
    assert_eq!(run_target("calc.exe"), ("calc.exe", ""));
    assert_eq!(run_target("notepad.exe C:\\memo.txt"), ("notepad.exe", "C:\\memo.txt"));
    assert_eq!(run_target("\"C:\\Program Files\\App\\app.exe\" -a  b"), ("C:\\Program Files\\App\\app.exe", "-a  b"));
    assert_eq!(run_target("\"C:\\資料 フォルダ\""), ("C:\\資料 フォルダ", ""));
    assert_eq!(run_target("https://example.com/a?b=c"), ("https://example.com/a?b=c", ""));
}

// Malformed input: the settings file comes from outside.
#[test]
fn an_unclosed_quote_in_a_run_command_is_an_error_with_its_line() {
    let text = "[set.a]\nkey = x | run | \"C:\\Program Files\\x.exe\nkey = y | run | \"\"\n[profile.default]\nleft = a\n";
    let errors = parse(text.as_bytes()).expect_err("unclosed and empty quotes");
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2, 3], "{errors:?}");
    assert!(errors.iter().all(|e| e.reason.contains("run")), "{errors:?}");
}

/// A monitor 10 keys of 10 px tall (and wide).
const SCREEN: Rect = Rect { x: 0, y: 0, w: 100, h: 100 };

fn split(n: usize, sticky: &[usize]) -> Vec<Vec<usize>> {
    let flags: Vec<bool> = (0..n).map(|i| sticky.contains(&i)).collect();
    pages(SCREEN, Edge::Left, 10, 1, &vec![1; n], &flags)
}

// AC3
#[test]
fn keys_that_fit_make_one_page_in_their_order() {
    assert_eq!(split(4, &[0]), [vec![0, 1, 2, 3]], "everything fits: as written, page key first");
    assert_eq!(split(10, &[]), [(0..10).collect::<Vec<_>>()]);
}

// AC3
#[test]
fn keys_past_the_edge_go_to_further_pages_with_the_page_keys_on_every_page() {
    // 20 keys, 9 and 19 turn pages: 8 others fit beside them on each page.
    let got = split(20, &[9, 19]);
    assert_eq!(
        got,
        [
            vec![0, 1, 2, 3, 4, 5, 6, 7, 9, 19],
            vec![8, 10, 11, 12, 13, 14, 15, 16, 9, 19],
            vec![17, 18, 9, 19],
        ]
    );
    // Without page keys the keys past the edge are on pages no key reaches.
    assert_eq!(split(12, &[])[0], (0..10).collect::<Vec<_>>());
}

// AC3: wide keys and several lines.
#[test]
fn pages_follow_the_layout_of_wide_keys_and_lines() {
    let widths = [3, 3, 3, 3, 1];
    let sticky = [false, false, false, false, true];
    // Two columns of 10 cells: 3+3+3 fits in a column, then 3+3+1 in the next.
    let got = pages(SCREEN, Edge::Left, 10, 2, &widths, &sticky);
    assert_eq!(got, [vec![0, 1, 2, 3, 4]]);
    let got = pages(SCREEN, Edge::Left, 10, 1, &widths, &sticky);
    assert_eq!(got, [vec![0, 1, 2, 4], vec![3, 4]]);
}

// Nothing fits beside the page keys: what fits is one page, as before.
#[test]
fn a_panel_too_short_for_its_page_keys_keeps_what_fits() {
    let tiny = Rect { x: 0, y: 0, w: 100, h: 20 };
    let got = pages(tiny, Edge::Left, 10, 1, &[1, 1, 1, 1], &[false, false, true, true]);
    assert_eq!(got, [vec![0, 1]]);
}

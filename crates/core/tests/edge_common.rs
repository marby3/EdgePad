//! Acceptance tests for Issue #97: [edge], settings common to every edge,
//! which [edge.*] overrides edge by edge (spec 7).

use edgepad_core::config::*;

const KEYS: &str = "[set.a]\nkey = A | hold | A\n[profile.default]\nleft = a\n";

fn edges(text: &str) -> [EdgeConfig; 4] {
    parse(format!("{text}\n{KEYS}").as_bytes()).unwrap_or_else(|e| panic!("{e:?}")).edges
}

// AC1
#[test]
fn edge_settings_apply_to_every_edge_that_does_not_write_them() {
    let e = edges("[edge]\nenabled = false\ndock = true\nmonitor = 2\ncolumns = 3\nrows = 2\nhandle = 8, 60%, 20%\n");
    for (i, edge) in e.iter().enumerate() {
        assert!(!edge.enabled && edge.dock && edge.monitor == Monitor::Number(2), "edge {i}: {edge:?}");
        assert_eq!(edge.handle, Some(Handle { width: 8, length: 60, position: 20 }), "edge {i}");
    }
    // columns on the left and right, rows on the top and bottom.
    assert_eq!([e[0].lines, e[1].lines, e[2].lines, e[3].lines], [3, 3, 2, 2]);
}

// AC2
#[test]
fn an_edge_overrides_the_common_settings_it_writes_wherever_written() {
    // The edge's section before the common one, and after.
    for text in [
        "[edge.right]\nenabled = true\ncolumns = 1\n[edge]\nenabled = false\ncolumns = 2\n",
        "[edge]\nenabled = false\ncolumns = 2\n[edge.right]\nenabled = true\ncolumns = 1\n",
    ] {
        let e = edges(text);
        let right = Edge::Right as usize;
        assert!(e[right].enabled && e[right].lines == 1, "right writes its own: {:?}", e[right]);
        assert!(!e[Edge::Left as usize].enabled && e[Edge::Left as usize].lines == 2, "left takes the common");
        // Neither: the default.
        assert_eq!(e[right].monitor, EdgeConfig::default().monitor);
        assert_eq!(e[right].handle, EdgeConfig::default().handle);
    }
}

// AC3
#[test]
fn a_file_without_edge_reads_as_before() {
    let e = edges("[edge.left]\ncolumns = 2\n[edge.top]\nenabled = false\n");
    let d = EdgeConfig::default();
    assert_eq!(e[Edge::Left as usize], EdgeConfig { lines: 2, ..d });
    assert_eq!(e[Edge::Top as usize], EdgeConfig { enabled: false, ..d });
    assert_eq!(e[Edge::Right as usize], d);
}

// AC4
#[test]
fn a_wrong_common_setting_is_an_error_with_its_line() {
    let text = format!("[edge]\nenabled = maybe\ncolour = red\ncolumns = 9\nhandle = 2, 40%\n{KEYS}");
    let errors = parse(text.as_bytes()).expect_err("errors");
    let lines: Vec<usize> = errors.iter().map(|e| e.line).collect();
    assert_eq!(lines, [2, 3, 4, 5], "{errors:?}");
    assert!(errors[1].reason.contains("colour"), "{errors:?}");
}

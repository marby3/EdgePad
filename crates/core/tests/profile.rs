//! Acceptance tests for switching profiles by the foreground app (roadmap
//! v0.3, spec 4.4): the core half, which profile a process gets.

use edgepad_core::config::parse;

const TEXT: &str = "\
[set.a]
key = A | hold | A
[profile.default]
left = a
[profile.paint]
process = CLIPStudioPaint.exe
right = a
[profile.quiet]
process = notepad.exe
[profile.paint-again]
process = clipstudiopaint.EXE
top = a
";

fn profile_for(process: Option<&str>) -> String {
    parse(TEXT.as_bytes()).expect("parses").profile_for(process).name.clone()
}

// AC1
#[test]
fn a_process_gets_the_first_profile_naming_it_ignoring_case() {
    assert_eq!(profile_for(Some("clipstudiopaint.exe")), "paint");
    assert_eq!(profile_for(Some("CLIPSTUDIOPAINT.EXE")), "paint", "the first of two that match");
    assert_eq!(profile_for(Some("Notepad.exe")), "quiet");
}

// AC1
#[test]
fn any_other_process_or_none_known_gets_the_default_profile() {
    assert_eq!(profile_for(Some("explorer.exe")), "default");
    assert_eq!(profile_for(Some("paint.exe")), "default", "the whole name must match");
    assert_eq!(profile_for(None), "default", "a process whose name cannot be read");
}

// AC2
#[test]
fn a_profile_shows_only_the_edges_it_names() {
    let config = parse(TEXT.as_bytes()).expect("parses");
    assert_eq!(config.profile_for(Some("notepad.exe")).edges, [None, None, None, None], "no edge: no panel");
    assert_eq!(config.profile_for(Some("clipstudiopaint.exe")).edges, [None, Some("a".into()), None, None]);
}

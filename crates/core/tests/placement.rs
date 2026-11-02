//! Issue #137: where the settings window was, handed to its next process
//! on the command line. The command line comes from outside: what does not
//! read is no place, not a panic.

use edgepad_core::placement::Placement;

// Malformed input
#[test]
fn a_place_that_does_not_read_is_none_not_a_panic() {
    let place = Placement { x: -1200, y: 40, w: 1100, h: 760, maximized: true };
    assert_eq!(Placement::read(&place.to_string()), Some(place), "{place}");
    for text in [
        "",
        "1,2,3",
        "1,2,3,4,5,6",
        "a,b,c,d,0",
        "1,2,3,4,2",
        "1,2,0,4,0",
        "1,2,3,-4,0",
        "1,2,99999999,4,0",
        "99999999999999999999,2,3,4,0",
        "1, 2, 3, 4, 0",
        "1,2,3,4,",
        "１,2,3,4,0",
    ] {
        assert_eq!(Placement::read(text), None, "{text:?}");
    }
}

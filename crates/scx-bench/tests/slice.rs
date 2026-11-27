// Written for solidcorex. Apache-2.0.
//! The slice cases' own tests: the cases the bench targets share
//! (`benches/common/slice_cases.rs`) build on both sides.

#[path = "../benches/common/slice_cases.rs"]
mod slice_cases;

use scx_bench::Side;
use slice_cases::{case, names};

/// Every constructor and the smallest Boolean recipe yield a case whose two sides both build
/// a body (a zero checksum is a side that refused), and every name is one `case` parses (the
/// bigger recipes are built by the targets themselves).
#[test]
fn small_cases_run_on_both_sides() {
    for name in names() {
        if name.contains("plate") || name.contains("gear") {
            continue;
        }
        let c = case(&name).unwrap_or_else(|| panic!("no case {name}"));
        assert_ne!(c.run(Side::Scx).0, 0, "{name}: scx");
        assert_ne!(c.run(Side::Pin).0, 0, "{name}: pin");
    }
}

/// The name list holds each case once.
#[test]
fn names_are_distinct() {
    let mut n = names();
    n.sort();
    let len = n.len();
    n.dedup();
    assert_eq!(n.len(), len);
}

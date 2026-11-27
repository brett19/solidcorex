// Written for solidcorex. Apache-2.0.
//! What an edit changed (01 §2.6): returned by [`Draft::finish`](crate::Draft::finish).
//!
//! Empty in BR1. BR3 fills it (`faces`, `edges` and `vertices` as `EntityChanges`, each
//! touch a `Boundary` or `Geometry`, `Origin`, and `carry`) from events the draft records;
//! the signature of `finish` does not change.

/// What an edit changed. Empty until BR3.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Changes {}

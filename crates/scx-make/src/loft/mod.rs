// Written for solidcorex. Apache-2.0.
//! Lofts through planar sections (research 75 §4). So far the pairing of their vertices
//! ([`pair`], task MK9); the loft itself and its certificate follow (MK10).

mod pair;

pub use pair::{
    Alignment, Event, MAX_CELLS, Optimality, PairError, PairRefusal, Pairing, align, align_banded,
    pair,
};

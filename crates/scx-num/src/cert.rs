// Written for solidcorex (docs/09-tolerances.md §5, docs/01-architecture.md §4.1).
//! Certificates: a stored claim, its proved bound, how it was proved and what it was
//! computed from, and the accuracy record that carries one.
//!
//! These live in layer 0 so that the NURBS, geometry and query layers can produce them.
//! The bound arithmetic is in this crate's interval modules and in the query layers.
//!
//! A [`Claim`] says *what* is bounded, not *whose*: the owner is wherever the record is
//! stored (an edge, a vertex, an edge use, or a span of a procedural surface's payload).
//! So the claim names no topology ids, which are defined two layers up. [`Reads`] names
//! the inputs by opaque keys that the body layer assigns: an entity changed in a draft is a
//! new entity with a new key, so a certificate never outlives its inputs.
//!
//! Exact kinds (a line on a plane, say) carry no record and no proof: there is no
//! `Method::Exact` with a bound of 0.

use crate::units::{Accuracy, Length};

/// What a certificate bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Claim {
    /// An edge use's parameter curve on its surface, against the edge's curve:
    /// `|S(p(t)) - C(t)|`.
    PcurveOnSurface,
    /// An edge's curve, against the intersection of its two faces' surfaces: the curve
    /// lies within the bound of `S1 ∩ S2`.
    CurveToIntersection,
    /// A procedural surface's net (its stored approximant), against its definition, over
    /// one span.
    NetToDefinition,
    /// An edge's curve end, against the vertex at that end.
    EndsAtVertex(EdgeEnd),
    /// A predicate: the body is valid and its faces point outward.
    Outward,
}

/// An end of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgeEnd {
    /// Where the edge starts.
    Start,
    /// Where it ends.
    End,
}

/// How a bound was proved (`docs/09-tolerances.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// Interval arithmetic with outward rounding.
    Interval,
    /// The convex hull of Bernstein (Bézier) coefficients.
    Bernstein,
    /// A Taylor model with an interval remainder.
    Taylor,
    /// A Krawczyk test: a unique root in a box, or none.
    Krawczyk,
}

/// The most inputs a certificate reads: an edge-use claim reads the parameter curve, the
/// surface and the edge's curve; an intersection claim the curve and two surfaces.
pub const MAX_READS: usize = 4;

/// The inputs a certificate was computed from, by key, in the order given. At most
/// [`MAX_READS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reads {
    keys: [u64; MAX_READS],
    len: u8,
}

impl Reads {
    /// No inputs (a predicate that reads the whole body).
    pub const NONE: Self = Self {
        keys: [0; MAX_READS],
        len: 0,
    };

    /// The inputs `keys`, or `None` when there are more than [`MAX_READS`].
    #[must_use]
    pub fn new(keys: &[u64]) -> Option<Self> {
        let len = u8::try_from(keys.len())
            .ok()
            .filter(|&n| usize::from(n) <= MAX_READS)?;
        let mut out = Self {
            keys: [0; MAX_READS],
            len,
        };
        for (d, s) in out.keys.iter_mut().zip(keys) {
            *d = *s;
        }
        Some(out)
    }

    /// The keys, in the order given.
    #[must_use]
    pub fn keys(&self) -> &[u64] {
        self.keys.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

/// A stored claim and its proved bound: the true distance is at most `bound`, rounding
/// included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cert {
    /// What is bounded.
    pub claim: Claim,
    /// The bound.
    pub bound: Length,
    /// How it was proved.
    pub method: Method,
    /// What it was computed from.
    pub reads: Reads,
}

/// Where a recorded accuracy came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Basis {
    /// Measured by sampling: not a proof.
    Measured,
    /// Proved by the certificate.
    Certified(Cert),
    /// A conservative bound that is not yet proved, kept with the certificate that
    /// would prove it (a fallback trim on an offset or procedural-blend surface).
    Unproven(Cert),
    /// Taken from an imported file that cannot be measured.
    Stated,
}

/// An entity's recorded accuracy and its basis. Stored as `Option<Recorded>` on an edge,
/// a vertex or an edge use: `None` means within the body's resolution stamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recorded {
    /// The accuracy.
    pub acc: Accuracy,
    /// Where it came from.
    pub basis: Basis,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hold_at_most_four_keys_in_order() {
        assert_eq!(
            Reads::new(&[3, 1, 2]).map(|r| r.keys().to_vec()),
            Some(vec![3, 1, 2])
        );
        assert_eq!(Reads::new(&[1, 2, 3, 4, 5]), None);
        assert_eq!(Reads::new(&[]), Some(Reads::NONE));
        assert!(Reads::NONE.keys().is_empty());
        assert_ne!(Reads::new(&[1, 2]), Reads::new(&[2, 1]));
    }

    #[test]
    fn a_record_carries_its_certificate() {
        let cert = Cert {
            claim: Claim::EndsAtVertex(EdgeEnd::End),
            bound: Length::from_mm(1.0e-6),
            method: Method::Interval,
            reads: Reads::new(&[10, 11]).unwrap_or(Reads::NONE),
        };
        let rec = Recorded {
            acc: Accuracy::from_mm(1.0e-6).unwrap_or(Accuracy::ZERO),
            basis: Basis::Certified(cert),
        };
        assert!(matches!(rec.basis, Basis::Certified(c) if c.reads.keys() == [10, 11]));
    }
}

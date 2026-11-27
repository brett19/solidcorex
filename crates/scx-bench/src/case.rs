// Written for solidcorex. Apache-2.0.
//! A benchmark case: the scx and pin functions over inputs converted from the same seeded
//! values, each run as one batch with every input through `black_box` and every result
//! consumed whole. The criterion targets time a batch; the instruction-count targets count
//! one.

use crate::{Checksum, Consume};
use core::fmt;
use std::hint::black_box;

/// Which side of a case to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The scx function.
    Scx,
    /// The pin's.
    Pin,
}

/// One case: a name (`<crate>/<case>`, `<crate>/<case>/edge` for its edge inputs), the
/// number of inputs a batch reads, and one batch of each side.
pub struct Case {
    /// `<crate>/<case>`, or `<crate>/<case>/edge`.
    pub name: String,
    /// The inputs a batch reads: the throughput unit.
    pub len: usize,
    scx: Box<dyn Fn() -> Checksum>,
    pin: Box<dyn Fn() -> Checksum>,
}

impl fmt::Debug for Case {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Case")
            .field("name", &self.name)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

/// `f` on each input, every result folded into the sum; the inputs are opaque, and so is
/// each one as it is read.
fn batch<I, R: Consume>(inputs: &[I], f: &impl Fn(&I) -> R) -> Checksum {
    let mut sum = Checksum::default();
    for x in black_box(inputs) {
        f(black_box(x)).consume(&mut sum);
    }
    sum
}

impl Case {
    /// A per-input case: `scx` on each scx input and `pin` on each of the pin's (the two
    /// converted from the same values, as [`both`] makes them).
    pub fn map<A, B, RA, RB>(
        name: &str,
        inputs: (Vec<A>, Vec<B>),
        scx: impl Fn(&A) -> RA + 'static,
        pin: impl Fn(&B) -> RB + 'static,
    ) -> Self
    where
        A: 'static,
        B: 'static,
        RA: Consume,
        RB: Consume,
    {
        let (a, b) = inputs;
        debug_assert_eq!(a.len(), b.len());
        Self {
            name: name.to_string(),
            len: a.len(),
            scx: Box::new(move || batch(&a, &scx)),
            pin: Box::new(move || batch(&b, &pin)),
        }
    }

    /// A whole-slice case (a fold): `scx` on all the scx inputs at once and `pin` on all
    /// the pin's.
    pub fn fold<A, B, RA, RB>(
        name: &str,
        inputs: (Vec<A>, Vec<B>),
        scx: impl Fn(&[A]) -> RA + 'static,
        pin: impl Fn(&[B]) -> RB + 'static,
    ) -> Self
    where
        A: 'static,
        B: 'static,
        RA: Consume,
        RB: Consume,
    {
        let (a, b) = inputs;
        debug_assert_eq!(a.len(), b.len());
        let whole = |r: &dyn Consume| {
            let mut sum = Checksum::default();
            r.consume(&mut sum);
            sum
        };
        Self {
            name: name.to_string(),
            len: a.len(),
            scx: Box::new(move || whole(&scx(black_box(&a)))),
            pin: Box::new(move || whole(&pin(black_box(&b)))),
        }
    }

    /// One batch of `side`: its checksum, which the caller black-boxes.
    pub fn run(&self, side: Side) -> Checksum {
        match side {
            Side::Scx => (self.scx)(),
            Side::Pin => (self.pin)(),
        }
    }
}

/// The inputs of both sides from the same values, kept where both sides can hold the
/// value (an scx type refuses some inputs the pin's holds, such as a NaN matrix).
pub fn both<T, A, B>(
    raw: &[T],
    a: impl Fn(&T) -> Option<A>,
    b: impl Fn(&T) -> Option<B>,
) -> (Vec<A>, Vec<B>) {
    raw.iter().filter_map(|r| Some((a(r)?, b(r)?))).unzip()
}

/// The same inputs for both sides, where both read one type.
pub fn same<T: Clone>(raw: &[T]) -> (Vec<T>, Vec<T>) {
    (raw.to_vec(), raw.to_vec())
}

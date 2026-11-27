// Written for solidcorex. Apache-2.0.
//! Closed `f64` intervals rounded outward: the enclosure arithmetic of [`super`].
//!
//! Every operation rounds to nearest and then steps one `f64` outward
//! (`next_down`/`next_up`). IEEE 754 rounds `+`, `-`, `*`, `/` and `sqrt` to nearest, so the
//! exact result lies within half a unit in the last place of the rounded one, and one step
//! outward covers it: the inclusion property (the exact value of the operation on any
//! members lies in the result) holds for each operation, hence for any formula built from
//! them. `min`, `max` and negation are exact. Inputs here are finite and far from overflow
//! (the caller keeps magnitudes below `2^500`), so no `NaN` arises.

/// A closed interval `[lo, hi]` certain to contain a value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Enclosure {
    /// The lower end.
    pub lo: f64,
    /// The upper end.
    pub hi: f64,
}

fn dn(x: f64) -> f64 {
    x.next_down()
}

fn up(x: f64) -> f64 {
    x.next_up()
}

impl Enclosure {
    /// The point `[x, x]`.
    pub(crate) const fn point(x: f64) -> Enclosure {
        Enclosure { lo: x, hi: x }
    }

    /// `[x - r, x + r]`, rounded outward.
    pub(crate) fn around(x: f64, r: f64) -> Enclosure {
        Enclosure {
            lo: dn(x - r),
            hi: up(x + r),
        }
    }

    /// Whether `0` lies in `[lo, hi]` (also when either end is `NaN`).
    pub fn holds_zero(self) -> bool {
        !(self.lo > 0.0 || self.hi < 0.0)
    }

    pub(crate) fn add(self, o: Enclosure) -> Enclosure {
        Enclosure {
            lo: dn(self.lo + o.lo),
            hi: up(self.hi + o.hi),
        }
    }

    pub(crate) fn sub(self, o: Enclosure) -> Enclosure {
        Enclosure {
            lo: dn(self.lo - o.hi),
            hi: up(self.hi - o.lo),
        }
    }

    pub(crate) fn neg(self) -> Enclosure {
        Enclosure {
            lo: -self.hi,
            hi: -self.lo,
        }
    }

    pub(crate) fn mul(self, o: Enclosure) -> Enclosure {
        let c = [
            self.lo * o.lo,
            self.lo * o.hi,
            self.hi * o.lo,
            self.hi * o.hi,
        ];
        Enclosure {
            lo: dn(c.iter().copied().fold(f64::INFINITY, f64::min)),
            hi: up(c.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        }
    }

    /// `self / o` for an `o` that does not hold zero (the caller checks).
    pub(crate) fn div(self, o: Enclosure) -> Enclosure {
        let c = [
            self.lo / o.lo,
            self.lo / o.hi,
            self.hi / o.lo,
            self.hi / o.hi,
        ];
        Enclosure {
            lo: dn(c.iter().copied().fold(f64::INFINITY, f64::min)),
            hi: up(c.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        }
    }

    /// `{x^2}`: tighter than `self * self` when the interval holds zero.
    pub(crate) fn sqr(self) -> Enclosure {
        let (a, b) = (self.lo * self.lo, self.hi * self.hi);
        if self.lo >= 0.0 {
            Enclosure {
                lo: dn(a),
                hi: up(b),
            }
        } else if self.hi <= 0.0 {
            Enclosure {
                lo: dn(b),
                hi: up(a),
            }
        } else {
            Enclosure {
                lo: 0.0,
                hi: up(a.max(b)),
            }
        }
    }

    /// `{sqrt x}` over the non-negative part (the callers' arguments are sums of squares).
    pub(crate) fn sqrt(self) -> Enclosure {
        Enclosure {
            lo: if self.lo > 0.0 {
                dn(self.lo.sqrt()).max(0.0)
            } else {
                0.0
            },
            hi: up(self.hi.max(0.0).sqrt()),
        }
    }

    pub(crate) fn max(self, o: Enclosure) -> Enclosure {
        Enclosure {
            lo: self.lo.max(o.lo),
            hi: self.hi.max(o.hi),
        }
    }

    pub(crate) fn min(self, o: Enclosure) -> Enclosure {
        Enclosure {
            lo: self.lo.min(o.lo),
            hi: self.hi.min(o.hi),
        }
    }

    /// The middle, rounded.
    pub(crate) fn mid(self) -> f64 {
        0.5 * self.lo + 0.5 * self.hi
    }

    /// An upper bound on the larger distance from `m` to either end.
    pub(crate) fn reach_from(self, m: f64) -> f64 {
        up(m - self.lo).max(up(self.hi - m))
    }
}

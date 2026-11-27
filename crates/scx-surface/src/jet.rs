// Written for solidcorex. Apache-2.0.
//! A stand-in for `scx-num`'s `SurfaceJet` (63 X10, builder NS2), which is not on main yet:
//! the same name, constructor and readers (`from_fn`, `point`, `get`), so the swap is one
//! `use`. Crate-private; no second public jet type exists.

/// The partials `d^(i+j) S / du^i dv^j` of a surface for `i + j <= D` (at most 8 here).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceJet<V, const D: usize> {
    d: [V; CAP],
}

/// The triangle of order 8.
const CAP: usize = 45;

impl<V: Copy + Default, const D: usize> SurfaceJet<V, D> {
    const FITS: () = assert!(D <= 8, "the stand-in jet holds orders up to 8");

    /// The jet with partial `(i, j)` from `f(i, j)`, called once for each `i + j <= D`, `i`
    /// ascending, then `j`.
    pub fn from_fn(mut f: impl FnMut(usize, usize) -> V) -> Self {
        let () = Self::FITS;
        let mut d = [V::default(); CAP];
        let mut n = 0;
        for i in 0..=D {
            for j in 0..=D - i {
                if let Some(slot) = d.get_mut(n) {
                    *slot = f(i, j);
                }
                n += 1;
            }
        }
        Self { d }
    }

    /// The point, partial `(0, 0)`.
    pub fn point(&self) -> V {
        self.d.first().copied().unwrap_or_default()
    }

    /// Partial `(i, j)`, `None` outside the triangle.
    pub fn get(&self, i: usize, j: usize) -> Option<V> {
        if i + j > D {
            return None;
        }
        self.d.get(slot(i, j, D)).copied()
    }
}

/// The place of `(i, j)`: rows `0..i` hold `D + 1 - r` entries each.
const fn slot(i: usize, j: usize, d: usize) -> usize {
    i * (d + 1) - i * i.saturating_sub(1) / 2 + j
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_follow_the_fill_order() {
        let j = SurfaceJet::<(usize, usize), 3>::from_fn(|i, j| (i, j));
        for i in 0..=3 {
            for k in 0..=3 - i {
                assert_eq!(j.get(i, k), Some((i, k)));
            }
            assert_eq!(j.get(i, 4 - i), None);
        }
    }
}

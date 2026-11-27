// Written for solidcorex. Apache-2.0.
//! [`SurfaceJet`]: a surface's partial derivatives to a fixed total order.

/// The partials `S^(i,j) = ∂^(i+j) S / ∂u^i ∂v^j` of a surface at a parameter for every
/// `i + j <= D`: the triangle, not the square (research 63 X10, ruling D3). `S^(0,0)` is
/// the point.
///
/// `V` is the value of one partial: a point of any dimension (`[f64; 3]`, `Vec3`), or an
/// interval vector for a certified jet (`Vec3<Ivl>`), so one type serves every surface kind
/// and every scalar.
///
/// Only the triangle can be read or written. Stable Rust cannot size an array by
/// `(D + 1)(D + 2) / 2`, so the storage is the point, the pure `u` partials `S^(i,0)`
/// (`i = 1..=D`) and a `D × D` block for those with `j >= 1`: exactly the triangle at order
/// 1, one spare entry at order 2 (7 for 6), 13 for 10 at order 3. The spare entries hold
/// copies of the point and are never returned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceJet<V, const D: usize> {
    /// `S^(0,0)`.
    point: V,
    /// `u[i - 1]` is `S^(i,0)`.
    u: [V; D],
    /// `rest[i][j - 1]` is `S^(i,j)` for `j >= 1`, `i + j <= D`.
    rest: [[V; D]; D],
}

impl<V: Copy, const D: usize> SurfaceJet<V, D> {
    /// The jet whose partial `(i, j)` is `f(i, j)`. `f` is called once for each `i + j <= D`,
    /// `i` ascending and, for each `i`, `j` ascending (so `(0, 0)` first).
    pub fn from_fn(mut f: impl FnMut(usize, usize) -> V) -> Self {
        let point = f(0, 0);
        let mut jet = Self {
            point,
            u: [point; D],
            rest: [[point; D]; D],
        };
        for i in 0..=D {
            for j in 0..=D - i {
                if (i, j) != (0, 0)
                    && let Some(e) = jet.get_mut(i, j)
                {
                    *e = f(i, j);
                }
            }
        }
        jet
    }

    /// The highest total order, `D`.
    #[must_use]
    pub const fn order(&self) -> usize {
        D
    }

    /// The point `S^(0,0)`.
    #[must_use]
    pub const fn point(&self) -> V {
        self.point
    }

    /// `S^(i,j)`, or `None` when `i + j > D`.
    #[must_use]
    pub fn get(&self, i: usize, j: usize) -> Option<V> {
        match (i, j) {
            _ if i.checked_add(j)? > D => None,
            (0, 0) => Some(self.point),
            (i, 0) => self.u.get(i - 1).copied(),
            (i, j) => self.rest.get(i)?.get(j - 1).copied(),
        }
    }

    /// `S^(i,j)` to write, or `None` when `i + j > D`.
    pub fn get_mut(&mut self, i: usize, j: usize) -> Option<&mut V> {
        match (i, j) {
            _ if i.checked_add(j)? > D => None,
            (0, 0) => Some(&mut self.point),
            (i, 0) => self.u.get_mut(i - 1),
            (i, j) => self.rest.get_mut(i)?.get_mut(j - 1),
        }
    }

    /// Every partial with its `(i, j)`, in [`SurfaceJet::from_fn`]'s order.
    pub fn iter(&self) -> impl Iterator<Item = ((usize, usize), V)> + '_ {
        (0..=D)
            .flat_map(|i| (0..=D - i).map(move |j| (i, j)))
            .filter_map(|ij| Some((ij, self.get(ij.0, ij.1)?)))
    }

    /// The jet of `f` of each partial (a change of type: to an interval jet, say).
    #[must_use]
    pub fn map<W: Copy>(&self, mut f: impl FnMut(V) -> W) -> SurfaceJet<W, D> {
        SurfaceJet::from_fn(|i, j| f(self.get(i, j).unwrap_or(self.point)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri<const D: usize>() -> SurfaceJet<(usize, usize), D> {
        SurfaceJet::from_fn(|i, j| (i, j))
    }

    #[test]
    fn every_partial_of_the_triangle_and_no_other() {
        fn check<const D: usize>() {
            let t = tri::<D>();
            assert_eq!(t.order(), D);
            assert_eq!(t.point(), (0, 0));
            for i in 0..=D + 2 {
                for j in 0..=D + 2 {
                    let want = (i + j <= D).then_some((i, j));
                    assert_eq!(t.get(i, j), want, "D = {D}");
                }
            }
            let all: Vec<_> = t.iter().collect();
            assert_eq!(all.len(), (D + 1) * (D + 2) / 2);
            assert!(all.iter().all(|&(ij, v)| ij == v));
            assert_eq!(t.get(usize::MAX, 1), None);
        }
        check::<0>();
        check::<1>();
        check::<2>();
        check::<3>();
        check::<5>();
    }

    #[test]
    fn from_fn_order_get_mut_and_map() {
        let mut calls = Vec::new();
        let _ = SurfaceJet::<u8, 2>::from_fn(|i, j| {
            calls.push((i, j));
            0
        });
        assert_eq!(calls, [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (2, 0)]);

        let mut t = SurfaceJet::<f64, 2>::from_fn(|i, j| (10 * i + j) as f64);
        *t.get_mut(1, 1).unwrap() = -1.0;
        assert!(t.get_mut(2, 1).is_none());
        assert_eq!(t.get(1, 1), Some(-1.0));
        let m = t.map(|x| [x, 2.0 * x]);
        assert_eq!(m.get(2, 0), Some([20.0, 40.0]));
        assert_eq!(m.get(1, 1), Some([-1.0, -2.0]));
    }
}

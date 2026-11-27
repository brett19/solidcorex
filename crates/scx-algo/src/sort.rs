// Moved from solidcore `solidcore/crates/solidcore/src/geom/geom_utility/p1.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h`.
// Changes: the order is `f64::total_cmp` (NaN and signed zeros have a place); the shell sort
// is the standard library's stable sort; `smgu_AddSorted`'s `bool` direction is an
// [`Order`] and its optional parallel index array is the caller's payload, kept beside each
// value; `smgu_DoubleLessThan` is `<` and is not moved. Results unchanged for values with
// no NaN (the order of `-0.0` and `0.0` aside).
//! Sorting `f64`s: one total order, ties kept in the order they came.
//!
//! Upstream: `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`.
//!
//! | Rust | Upstream |
//! |---|---|
//! | [`sort`] | `smgu_ShellSort_double_array` |
//! | [`insert_sorted`], [`Order`] | `smgu_AddSorted` |
//! | [`sorted_indices`] | none: the index array `smgu_AddSorted` keeps beside the values |
//! | `<` | `smgu_DoubleLessThan` |
//!
//! Every function orders by [`f64::total_cmp`]: `-NaN < -inf < … < -0.0 < 0.0 < … < inf <
//! NaN`. Equal values keep the order they came in (the sorts are stable, and
//! [`insert_sorted`] puts a value after those equal to it), so ties are broken by index.
//!
//! Deviations:
//! - `smgu_ShellSort_double_array` compares with `<`, so a NaN stops nothing and lands
//!   wherever the passes leave it, and the sort is not stable. [`sort`] puts NaNs at the
//!   ends (by sign) and keeps equal values in order.
//! - `smgu_AddSorted` scans for the first value past the new one with `>` (`<` when
//!   decreasing), so the result depends on where a NaN already sits; [`insert_sorted`]
//!   searches by the total order (in `O(log n)`, the list being sorted by it). Its
//!   optional index array, grown with zeros when shorter than the value list, is gone: a
//!   caller sorts `(value, payload)` pairs.

use std::cmp::Ordering;

/// Which way a list runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Order {
    /// Smallest first.
    Increasing,
    /// Largest first.
    Decreasing,
}

impl Order {
    /// `a` against `b` in this order.
    #[must_use]
    pub fn cmp(self, a: f64, b: f64) -> Ordering {
        match self {
            Self::Increasing => a.total_cmp(&b),
            Self::Decreasing => b.total_cmp(&a),
        }
    }
}

/// Sorts `values` increasing by [`f64::total_cmp`], equal values kept in order
/// (`smgu_ShellSort_double_array`).
pub fn sort(values: &mut [f64]) {
    values.sort_by(f64::total_cmp);
}

/// The indices of `values` in increasing order of their values, ties by index.
#[must_use]
pub fn sorted_indices(values: &[f64]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&a, &b| {
        let (va, vb) = (values.get(a), values.get(b));
        va.zip(vb)
            .map_or(Ordering::Equal, |(va, vb)| va.total_cmp(vb))
    });
    idx
}

/// Inserts `(value, payload)` into `sorted` (sorted by its values in `order`) after every
/// entry whose value is not past `value`, and returns where it went (`smgu_AddSorted`).
pub fn insert_sorted<T>(sorted: &mut Vec<(f64, T)>, value: f64, payload: T, order: Order) -> usize {
    let at = sorted.partition_point(|(v, _)| order.cmp(*v, value) != Ordering::Greater);
    sorted.insert(at, (value, payload));
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_is_total_and_stable() {
        let mut v = [
            3.0,
            f64::NAN,
            -0.0,
            1.0,
            0.0,
            -f64::NAN,
            f64::NEG_INFINITY,
            1.0,
        ];
        sort(&mut v);
        let bits: Vec<u64> = v.iter().map(|x| x.to_bits()).collect();
        let want = [
            -f64::NAN,
            f64::NEG_INFINITY,
            -0.0,
            0.0,
            1.0,
            1.0,
            3.0,
            f64::NAN,
        ];
        assert_eq!(bits, want.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
    }

    #[test]
    fn sorted_indices_break_ties_by_index() {
        assert_eq!(sorted_indices(&[2.0, 1.0, 2.0, 0.0, 1.0]), [3, 1, 4, 0, 2]);
        assert!(sorted_indices(&[]).is_empty());
    }

    #[test]
    fn insert_sorted_keeps_ties_in_arrival_order() {
        let mut up = Vec::new();
        for (i, v) in [2.0, 1.0, 2.0, 3.0, 1.0].into_iter().enumerate() {
            insert_sorted(&mut up, v, i, Order::Increasing);
        }
        assert_eq!(up, [(1.0, 1), (1.0, 4), (2.0, 0), (2.0, 2), (3.0, 3)]);

        let mut down = Vec::new();
        for (i, v) in [2.0, 1.0, 2.0, 3.0].into_iter().enumerate() {
            insert_sorted(&mut down, v, i, Order::Decreasing);
        }
        assert_eq!(down, [(3.0, 3), (2.0, 0), (2.0, 2), (1.0, 1)]);
        assert_eq!(insert_sorted(&mut down, 2.5, 9, Order::Decreasing), 1);
    }
}

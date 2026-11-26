// Written for solidcorex. Apache-2.0.
//! The inputs on which the pinned solidcore is known to be wrong, one [`PinDefect`] per test.
//! On those inputs the pin is not compared; scx's result must pass the entry's check against
//! the true value instead. Each entry gives its evidence: the true value, scx's error and the
//! pin's.

use crate::harness::PinDefect;
use scx_num::certified::Verdict;

type M = [[f64; 4]; 4];

/// `Transform::is_regular` on a linear part whose largest entry is of `f64::MAX`'s scale
/// (its correctly rounded `log2` is 1024), at a positive finite `ratio`.
///
/// The pin scales the matrix by `f64::powi(2, -floor(log2(big)))`: `log2` of `f64::MAX` is
/// `1024 - 1.6e-16`, which rounds to 1024, and `powi(2, -1024)` computes `1 / 2^1024 =
/// 1 / inf = 0`. The scaled matrix is zero and the pin answers `Undecided`. scx reads the
/// exponent from the bits (1023) and scales by the subnormal `2^-1023`.
///
/// The true value, for the pool's matrices (the identity but one off-diagonal entry `x`,
/// `|x|` at least `2^1023`): the linear part's singular values are 1 and those of
/// `[[1, x], [0, 1]]`, whose product is 1 and the larger of which exceeds `|x|`. Its
/// conditioning is below `1 / x^2 <= 2^-2046`, under every positive double, so the true
/// answer is `No` at every positive `ratio`. The pin answers `Undecided`. scx answers `No`
/// from `ratio` 1e-160 up (156 inputs), and `Undecided`, which is sound, below: it proves
/// `No` from the rows' lengths with an absolute margin of 1e-320 on their squares, which
/// decides once `ratio^2 * 4` (the largest scaled row's square is about 4, the smallest
/// underflows) passes `2e-320`. Below 1e-160 what it measures is inside that margin, so
/// `Undecided` is the honest answer there, not a miss: the check accepts it only there, and
/// `Yes` nowhere. The pin's `Undecided` is the same answer below 1e-160, so no defect shows
/// there; the check still holds scx to it.
pub fn tr_is_regular() -> PinDefect<(M, f64), Verdict> {
    PinDefect {
        reason: "the pin's scale powi(2, -1024) is 0",
        applies: |(t, ratio)| {
            let finite = t.iter().flatten().all(|x| x.is_finite());
            let affine = t[0][3] == 0.0 && t[1][3] == 0.0 && t[2][3] == 0.0 && t[3][3] == 1.0;
            let big = linear(t).fold(0.0_f64, |b, v| b.max(v.abs()));
            finite
                && affine
                && ratio.is_finite()
                && *ratio > 0.0
                && scx_num::fmath::log2(big) == 1024.0
        },
        check: |(t, ratio), v| {
            // the identity but one off-diagonal entry: the only shape with a true value here
            let off: Vec<(usize, usize)> = (0..3)
                .flat_map(|i| (0..3).map(move |j| (i, j)))
                .filter(|&(i, j)| t[i][j] != if i == j { 1.0 } else { 0.0 })
                .collect();
            match off.as_slice() {
                [(i, j)] if i != j => match v {
                    Verdict::No => Ok(()),
                    Verdict::Undecided if *ratio < 1e-160 => Ok(()),
                    _ => Err(format!("the true answer is No; scx says {v:?}")),
                },
                _ => Err("no true value known for this matrix: extend the check".to_owned()),
            }
        },
    }
}

/// The linear part's nine entries.
fn linear(t: &M) -> impl Iterator<Item = f64> + '_ {
    t.iter().take(3).flat_map(|r| r.iter().take(3).copied())
}

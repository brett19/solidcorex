// Written for solidcorex. Apache-2.0.
//! Adaptive Gauss-Kronrod quadrature, the pin's rules and step control (`mass/integrator/
//! properties.rs`, `kronrod_integrate_with`), with an error estimate kept beside each value.
//!
//! - Two rules: the 7-point Kronrod rule with its embedded 3-point Gauss rule (exact to
//!   degree 10 and 5), and the 15-point rule with its embedded 7-point Gauss rule (22 and
//!   13). Nodes and weights are the hard-coded literals of the pin (72 §9's Gauss-Kronrod
//!   row), not re-derived.
//! - A piece's error estimate is `|K - G|`, the error of the Gauss value; the Kronrod value
//!   kept is far more accurate on a smooth integrand, so the estimate bounds it with room.
//! - Each rule is summed about its centre value `c` as `half (2 c + sum w (f - c))`, so a
//!   constant integrand is integrated exactly whatever the weights' rounding (pin #15021).
//! - No node lies at a piece's end, so a piece that ends on a knot never reads across it.
//! - A piece is accepted when, per component, its estimate is within `accuracy` of the
//!   larger of its own value and its width's share of the component's floor, or within a few
//!   roundings of the integrand's own size. Otherwise it is halved, at most
//!   [`MAX_DEPTH`] times and [`MAX_PIECES`] pieces in one call.
//! - Every piece charges its node count to the budget and checks the stop flag first.

use scx_num::budget::{Budget, Stop};

use crate::rounding::{Sums, U, gamma, larger};

/// One rule: the nodes on `[0, 1)` (the centre first, then each `±x`), the Kronrod
/// weights and the embedded Gauss weights (zero off the Gauss nodes).
pub(crate) struct Rule {
    nodes: &'static [f64],
    kronrod: &'static [f64],
    gauss: &'static [f64],
    /// Whether a sample's rounding is read at the centre and the outermost nodes only, not
    /// at every node (see [`Sample::rounds`]).
    sparse: bool,
}

impl Rule {
    /// The integrand evaluations a piece takes.
    const fn evaluations(&self) -> u64 {
        (2 * self.nodes.len() - 1) as u64
    }

    /// The roundings in the rule's sum of differences from the centre: each difference,
    /// its product by a weight, the weight's own rounding, and the additions.
    const fn sum_roundings(&self) -> u32 {
        (2 * self.nodes.len() + 1) as u32
    }

    /// Whether the node at index `i` of [`Rule::nodes`] reads the sample's rounding.
    fn rounds_at(&self, i: usize) -> bool {
        !self.sparse || i == 0 || i + 1 == self.nodes.len()
    }

    /// The node nearest the centre, as a fraction of the half width.
    fn nearest(&self) -> f64 {
        self.nodes.get(1).copied().unwrap_or(1.0)
    }
}

/// The 7-point Gauss-Kronrod rule on `[-1, 1]`: the nodes `0, ±x1, ±x2, ±x3`
/// (`x2 = sqrt(3/5)`, a Gauss node), the Kronrod weights and the 3-point Gauss weights.
pub(crate) const K7: Rule = Rule {
    nodes: &[
        0.0,
        0.434_243_749_346_802_558_002_071_502_844_628,
        0.774_596_669_241_483_377_035_853_079_956_480,
        0.960_491_268_708_020_283_423_507_092_629_080,
    ],
    kronrod: &[
        0.450_916_538_658_474_142_345_110_087_045_571,
        0.401_397_414_775_962_222_905_051_818_618_432,
        0.268_488_089_868_333_440_728_569_280_666_710,
        0.104_656_226_026_467_265_193_823_857_192_073,
    ],
    gauss: &[8.0 / 9.0, 0.0, 5.0 / 9.0, 0.0],
    sparse: false,
};

/// The 15-point Gauss-Kronrod rule on `[-1, 1]` and its embedded 7-point Gauss rule.
pub(crate) const K15: Rule = Rule {
    nodes: &[
        0.0,
        0.207_784_955_007_898_467_600_689_403_773_245,
        0.405_845_151_377_397_166_906_606_412_076_961,
        0.586_087_235_467_691_130_294_144_845_693_013,
        0.741_531_185_599_394_439_863_864_773_280_788,
        0.864_864_423_359_769_072_789_712_788_640_926,
        0.949_107_912_342_758_524_526_189_684_047_851,
        0.991_455_371_120_812_639_206_854_697_526_329,
    ],
    kronrod: &[
        0.209_482_141_084_727_828_012_999_174_891_714,
        0.204_432_940_075_298_892_414_161_999_234_649,
        0.190_350_578_064_785_409_913_256_402_421_014,
        0.169_004_726_639_267_902_826_583_426_598_550,
        0.140_653_259_715_525_918_745_189_590_510_238,
        0.104_790_010_322_250_183_839_876_322_541_518,
        0.063_092_092_629_978_553_290_700_663_189_204,
        0.022_935_322_010_529_224_963_732_008_058_970,
    ],
    gauss: &[
        0.417_959_183_673_469_387_755_102_040_816_327,
        0.0,
        0.381_830_050_505_118_944_950_369_775_488_975,
        0.0,
        0.279_705_391_489_276_667_901_467_771_423_780,
        0.0,
        0.129_484_966_168_869_693_270_611_432_679_082,
        0.0,
    ],
    sparse: true,
};

/// The most bisections of one piece.
pub(crate) const MAX_DEPTH: u32 = 48;
/// The most pieces one call evaluates.
pub(crate) const MAX_PIECES: u32 = 100_000;
/// The most components an integrand has.
pub(crate) const MAX_COMPONENTS: usize = 24;
/// A piece whose estimate is within this many roundings of its integrand's size passes:
/// there is nothing left to resolve.
const ROUNDING_PASS: f64 = 64.0 * f64::EPSILON;

/// Why an integral stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fail {
    /// The step control gave up: the depth or the piece bound.
    Step,
    /// The integrand was not finite.
    NotFinite,
    /// The budget ran out.
    Exhausted(u64),
    /// The stop flag was set.
    Cancelled,
}

/// The budget and stop flag one measurement charges and checks, and the evaluations it took.
pub(crate) struct Meter<'a> {
    budget: &'a mut Budget,
    stop: &'a Stop,
    /// The evaluations a first attempt may take before it gives up, and what it has taken.
    allowance: Option<u64>,
    spent: u64,
}

impl<'a> Meter<'a> {
    pub(crate) fn new(budget: &'a mut Budget, stop: &'a Stop) -> Self {
        Self {
            budget,
            stop,
            allowance: None,
            spent: 0,
        }
    }

    /// Lets the work from now on take at most `work` evaluations before it gives up with
    /// [`Fail::Step`]; `None` lifts the limit.
    pub(crate) fn allow(&mut self, work: Option<u64>) {
        self.allowance = work;
        self.spent = 0;
    }

    /// Charges `work` after checking the stop flag.
    pub(crate) fn charge(&mut self, work: u64) -> Result<(), Fail> {
        self.stop.check().map_err(|_| Fail::Cancelled)?;
        self.spent = self.spent.saturating_add(work);
        if self.allowance.is_some_and(|a| self.spent > a) {
            return Err(Fail::Step);
        }
        self.budget
            .charge(work)
            .map_err(|e| Fail::Exhausted(e.limit))
    }
}

/// What an integrand writes at one point: its values and, for each, a bound of their own
/// error (an inner integral's estimate), integrated beside the values but not judged; and,
/// where `rounds` is set, a bound of their rounding.
///
/// The rounding is read at every node of [`K7`] and at the centre and the two outermost
/// nodes of [`K15`] (whose integrand is the costly inner one), and a piece's is its width
/// times the largest read: the rounding bound is a smooth size (the terms' absolute
/// values), not a value to resolve, and so is held by its largest over the piece's reach
/// (72 §14.31).
pub(crate) struct Sample<'s> {
    pub(crate) value: &'s mut [f64],
    pub(crate) error: &'s mut [f64],
    pub(crate) rounding: &'s mut [f64],
    pub(crate) rounds: bool,
}

/// Where a piece is accepted: `accuracy` relative to the piece's value or to its width's
/// share of `floor` (per component, the size the component is held to over the whole range).
#[derive(Clone, Copy)]
pub(crate) struct Tol<'f> {
    pub(crate) accuracy: f64,
    pub(crate) floor: &'f [f64],
}

/// Adds `∫_a^b f` to `value` and its error estimate to `error`, per component (the first
/// `value.len()`), by `rule`.
pub(crate) fn integrate<F>(
    rule: &Rule,
    range: (f64, f64),
    tol: Tol<'_>,
    value: &mut [f64],
    error: &mut [f64],
    meter: &mut Meter<'_>,
    f: F,
) -> Result<(), Fail>
where
    F: FnMut(f64, Sample<'_>, &mut Meter<'_>) -> Result<(), Fail>,
{
    let n = value.len().min(error.len()).min(MAX_COMPONENTS);
    // The component loops run over a length known when compiled, so they are straight
    // line; the components past `n` are zero throughout and never written out.
    match n {
        0 | 1 => integrate_n::<1, F>(rule, range, tol, (value, error), n, meter, f),
        2 => integrate_n::<2, F>(rule, range, tol, (value, error), n, meter, f),
        _ => integrate_n::<MAX_COMPONENTS, F>(rule, range, tol, (value, error), n, meter, f),
    }
}

/// [`integrate`] over `N >= n` components.
fn integrate_n<const N: usize, F>(
    rule: &Rule,
    (a, b): (f64, f64),
    tol: Tol<'_>,
    (value, error): (&mut [f64], &mut [f64]),
    n: usize,
    meter: &mut Meter<'_>,
    mut f: F,
) -> Result<(), Fail>
where
    F: FnMut(f64, Sample<'_>, &mut Meter<'_>) -> Result<(), Fail>,
{
    let width = b - a;
    if width.is_nan() || width <= 0.0 {
        return Ok(());
    }
    let mut floor = [0.0; N];
    for (f, t) in floor.iter_mut().zip(tol.floor.iter().take(n)) {
        *f = *t;
    }
    let mut stack = vec![(a, b, 0_u32)];
    let mut pieces = 0_u32;
    let mut sums = Sums::<N>::ZERO;
    let mut rounded = [0.0; N];
    let mut centre = [0.0; N];
    let mut centre_err = [0.0; N];
    let mut at = [0.0; N];
    let mut at_err = [0.0; N];
    let mut at_round = [0.0; N];
    let w0 = rule.kronrod.first().copied().unwrap_or(0.0);
    while let Some((lo, hi, depth)) = stack.pop() {
        pieces += 1;
        if pieces > MAX_PIECES {
            return Err(Fail::Step);
        }
        meter.charge(rule.evaluations())?;
        let mid = 0.5 * (lo + hi);
        let half = 0.5 * (hi - lo);
        let mut kron = [0.0; N];
        let mut gauss = [0.0; N];
        let mut size = [0.0; N];
        let mut inner = [0.0; N];
        let mut spread = [0.0; N];
        let mut jump = [0.0; N];
        let mut read = (&mut centre, &mut centre_err, &mut at_round);
        sample(&mut f, mid, n, &mut read, true, meter)?;
        let mut most = at_round;
        for ((s, i), (v, e)) in size
            .iter_mut()
            .zip(inner.iter_mut())
            .zip(centre.iter().zip(&centre_err))
        {
            *s = w0 * v.abs();
            *i = w0 * e;
        }
        for (i, &x) in rule.nodes.iter().enumerate().skip(1) {
            let wk = rule.kronrod.get(i).copied().unwrap_or(0.0);
            let wg = rule.gauss.get(i).copied().unwrap_or(0.0);
            let rounds = rule.rounds_at(i);
            for side in [-1.0, 1.0] {
                let mut read = (&mut at, &mut at_err, &mut at_round);
                sample(&mut f, mid + side * x * half, n, &mut read, rounds, meter)?;
                if rounds {
                    for (m, r) in most.iter_mut().zip(&at_round) {
                        *m = larger(*r, *m);
                    }
                }
                // Off the Gauss nodes the Gauss weight is zero: its product adds a zero
                // to a sum that is never `-0`, which leaves it as it is (a non-finite
                // sample is refused by the size sums either way).
                if wg != 0.0 {
                    for ((g, v), c) in gauss.iter_mut().zip(&at).zip(&centre) {
                        *g += wg * (v - c);
                    }
                }
                let sums = kron.iter_mut().zip(size.iter_mut()).zip(inner.iter_mut());
                let reads = at.iter().zip(&at_err).zip(&centre);
                let abs = spread.iter_mut().zip(jump.iter_mut());
                for (((k, s), inn), (((v, e), c), (sp, j))) in sums.zip(reads.zip(abs)) {
                    let off = v - c;
                    *k += wk * off;
                    *s += wk * v.abs();
                    *inn += wk * e;
                    *sp += wk * off.abs();
                    *j = larger(off.abs(), *j);
                }
            }
        }
        // A sample's value or error that is not finite makes its size or error sum so.
        if !size
            .iter()
            .zip(&inner)
            .fold(true, |ok, (s, i)| ok & s.is_finite() & i.is_finite())
        {
            return Err(Fail::NotFinite);
        }
        let share = (hi - lo) / width;
        let mut accepted = true;
        for ((((k, g), c), s), fl) in kron
            .iter_mut()
            .zip(gauss.iter_mut())
            .zip(&centre)
            .zip(&size)
            .zip(&floor)
        {
            let kv = half * (2.0 * c + *k);
            let gv = half * (2.0 * c + *g);
            *k = kv;
            let estimate = (kv - gv).abs();
            *g = estimate;
            let floor = fl * share;
            let held = tol.accuracy * kv.abs().max(floor);
            let rounding = ROUNDING_PASS * half * s;
            if estimate.is_nan() || (estimate > held && estimate > rounding) {
                accepted = false;
            }
        }
        if accepted {
            // The piece's rounding (72 §14.31): the samples' own, the centre's doubling, the
            // sum and the half width's product (`gamma(3)` of the value), the rule's sum of
            // differences, and where the nodes fall. A node lands within `place` of where
            // the rule puts it on `[lo, hi]` (the centre's and half width's rounding, the
            // product's and the sum's) and moves its sample by the slope there times that:
            // to first order the slope at a node is its difference from the centre over
            // their distance, and at the centre the largest such, each at most over the
            // nearest node's distance. The half width's own rounding is the value's.
            let place = 2.0 * U * (mid.abs() + half);
            let g_sum = gamma(rule.sum_roundings());
            let nearest = rule.nearest();
            sums.add(&kron);
            let reads = kron.iter().zip(&spread).zip(most.iter().zip(&jump));
            for (r, ((k, sp), (m, j))) in rounded.iter_mut().zip(reads) {
                let rounding = 2.0 * half * m
                    + gamma(3) * k.abs()
                    + half * g_sum * sp
                    + place * (sp + w0 * j) / nearest;
                *r += rounding;
            }
            for (e, (g, i)) in error.iter_mut().zip(gauss.iter().zip(&inner)) {
                *e += g + half * i;
            }
            continue;
        }
        if depth >= MAX_DEPTH || !(lo < mid && mid < hi) {
            return Err(Fail::Step);
        }
        stack.push((mid, hi, depth + 1));
        stack.push((lo, mid, depth + 1));
    }
    // The pieces' compensated sum is added once: its bound, and the addition's rounding.
    let pieces = sums.each().zip(&rounded);
    for ((v, e), ((sum, bound), r)) in value.iter_mut().zip(error.iter_mut()).zip(pieces) {
        *v += sum;
        *e += r + bound + gamma(1) * v.abs();
    }
    Ok(())
}

/// The value, error and rounding one sample is read into.
type Read<'r, const N: usize> = (&'r mut [f64; N], &'r mut [f64; N], &'r mut [f64; N]);

/// `f` at `x` into `read`, cleared first and checked finite; its rounding only if `rounds`.
fn sample<const N: usize, F>(
    f: &mut F,
    x: f64,
    n: usize,
    (value, error, rounding): &mut Read<'_, N>,
    rounds: bool,
    meter: &mut Meter<'_>,
) -> Result<(), Fail>
where
    F: FnMut(f64, Sample<'_>, &mut Meter<'_>) -> Result<(), Fail>,
{
    // Whole arrays, so the clearing is straight-line; past `n` stays zero.
    value.fill(0.0);
    error.fill(0.0);
    if rounds {
        rounding.fill(0.0);
    }
    let out = Sample {
        value: value.get_mut(..n).unwrap_or(&mut []),
        error: error.get_mut(..n).unwrap_or(&mut []),
        rounding: rounding.get_mut(..n).unwrap_or(&mut []),
        rounds,
    };
    f(x, out, meter)?;
    // The values and errors are checked by the piece's sums of their sizes (see
    // [`integrate`]); a rounding is read only where it is reported.
    if !rounds || rounding.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(Fail::NotFinite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::fmath;

    fn run(rule: &Rule, (a, b): (f64, f64), acc: f64, f: impl Fn(f64) -> f64) -> (f64, f64, u64) {
        let mut budget = Budget::unlimited();
        let stop = Stop::new();
        let mut meter = Meter::new(&mut budget, &stop);
        let (mut v, mut e) = ([0.0], [0.0]);
        let tol = Tol {
            accuracy: acc,
            floor: &[0.0],
        };
        integrate(rule, (a, b), tol, &mut v, &mut e, &mut meter, |x, s, _| {
            if let Some(y) = s.value.first_mut() {
                *y = f(x);
            }
            Ok(())
        })
        .map_or((f64::NAN, f64::NAN, 0), |()| (v[0], e[0], budget.used()))
    }

    /// Each rule integrates monomials exactly to its degree, in one piece (the pin's
    /// `kronrod_rules_are_exact_to_their_degrees`).
    #[test]
    fn rules_are_exact_to_their_degrees() {
        for (rule, degree) in [(&K7, 10_i32), (&K15, 22)] {
            for p in 0..=degree {
                let exact = 2.0 / f64::from(p + 1) * f64::from(i32::from(p % 2 == 0));
                let (v, _, work) = run(rule, (-1.0, 1.0), 1.0, |x| fmath::powi(x, p));
                assert!(
                    (v - exact).abs() <= 1e-14,
                    "degree {p}: {v} against {exact}"
                );
                assert_eq!(work, rule.evaluations());
            }
        }
    }

    /// A constant is integrated exactly, whatever the weights' rounding (pin #15021), and
    /// its stated error is the few roundings of the arithmetic's size that the rules'
    /// difference cannot see (research 72 §14.31): no longer zero, on purpose.
    #[test]
    fn a_constant_is_exact() {
        for rule in [&K7, &K15] {
            let (v, e, _) = run(rule, (0.0, 100.0), 1e-12, |_| 200.0);
            assert_eq!(v, 20000.0);
            assert!(e > 0.0 && e <= 8.0 * f64::EPSILON * v, "{e}");
        }
    }

    /// A smooth integrand is held to the accuracy asked, and the estimate covers the error.
    #[test]
    fn the_estimate_covers_the_error() {
        for acc in [1e-3, 1e-6, 1e-9, 1e-12] {
            let (v, e, _) = run(&K7, (0.0, 3.0), acc, fmath::exp);
            let exact = fmath::exp(3.0) - 1.0;
            assert!((v - exact).abs() <= acc * exact, "{acc}: {v}");
            assert!(
                (v - exact).abs() <= e.max(4.0 * f64::EPSILON * exact),
                "{acc}: {e}"
            );
        }
    }

    /// A non-finite integrand is reported, not bisected to the depth bound.
    #[test]
    fn a_non_finite_integrand_is_named() {
        let mut budget = Budget::unlimited();
        let stop = Stop::new();
        let mut meter = Meter::new(&mut budget, &stop);
        let (mut v, mut e) = ([0.0], [0.0]);
        let tol = Tol {
            accuracy: 1e-6,
            floor: &[0.0],
        };
        let r = integrate(
            &K7,
            (0.0, 1.0),
            tol,
            &mut v,
            &mut e,
            &mut meter,
            |_, s, _| {
                s.value.fill(f64::NAN);
                Ok(())
            },
        );
        assert_eq!(r, Err(Fail::NotFinite));
    }

    /// A stop flag set before the call cancels it; a spent budget exhausts it.
    #[test]
    fn stop_and_budget_are_checked_per_piece() {
        let tol = Tol {
            accuracy: 1e-6,
            floor: &[0.0],
        };
        let stop = Stop::new();
        stop.set();
        let mut budget = Budget::unlimited();
        let mut meter = Meter::new(&mut budget, &stop);
        let (mut v, mut e) = ([0.0], [0.0]);
        let r = integrate(
            &K7,
            (0.0, 1.0),
            tol,
            &mut v,
            &mut e,
            &mut meter,
            |_, _, _| Ok(()),
        );
        assert_eq!(r, Err(Fail::Cancelled));
        let stop = Stop::new();
        let mut budget = Budget::new(10);
        let mut meter = Meter::new(&mut budget, &stop);
        let r = integrate(
            &K15,
            (0.0, 1.0),
            tol,
            &mut v,
            &mut e,
            &mut meter,
            |_, _, _| Ok(()),
        );
        assert_eq!(r, Err(Fail::Exhausted(10)));
    }
}

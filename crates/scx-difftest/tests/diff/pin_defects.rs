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

type P = [f64; 2];
type Circle = Option<([f64; 2], f64, f64)>;

/// `three_point_circle_2d(s, e, t)` where the pin's plain-`f64` evaluation is ill-conditioned:
/// the forward error bound of its every step ([`PinError`], each rounding a relative `2^-53`
/// plus an absolute `2^-1074`, through the pin's own formulas), on the centre, the radius or
/// the sweep, passes a quarter of `Tol::DEFAULT`, or a decision it takes (the area against
/// `1e-12`, the chord and the radius against `1e-9`, the long or short arc) is within its
/// bound of the other side, or a step overflows.
///
/// The pin computes `t - s` and `e - s` (exact to a rounding each), then the centre
/// `s + (perp numerators) / d` in `f64`: the numerators and `d` cancel when the points are
/// nearly on one line or of very different sizes (`1e20` beside unit or subnormal points),
/// with a condition number up to `1e30` on the pool. The radius is the distance from that
/// centre, and the sweep `2 asin(chord / 2r)` multiplies the radius's error by
/// `1 / sqrt(1 - q^2)` as `q` nears 1. Its squares of differences overflow from about
/// `1e154`, where the circle is finite and the pin answers `None`.
///
/// The true value is computed in the check, exactly in integers ([`scx_testkit::exact`]): the
/// area and the decisions exactly, the centre `t + N / (2 a x b)` and the radius
/// `|s - centre|` from exact numerators each rounded once (within `2^-51`; whether each is
/// finite decided exactly, against `2^1024 - 2^970`), and the sweep as
/// `atan2` of the exact cross and dot products of `s - centre` and `e - centre` with the long
/// or short arc by the exact side of the chord `t` and the centre are on: a different route
/// from scx's (the inscribed angle at `t`, in expansions with unbounded exponents). scx must
/// match it within `Tol::DEFAULT`, on every input.
///
/// The worst input before scx was exact: `(1, 0)`, `(1e-13, 1)`, `(1e-6, 1e-6)`: true sweep
/// `3.1415886535859934`, the pin's off by `1.7e-10` relative and the old scx's (the pin's
/// method with a correctly rounded `hypot`) by `1.5e-11`; on the pool's ill-conditioned
/// inputs both were off by up to O(1), and agreed only where they ran the same steps.
pub fn three_point_circle_2d() -> PinDefect<(P, P, P), Circle> {
    PinDefect {
        reason: "the pin's f64 circumcentre and asin sweep are ill-conditioned here",
        applies: |&(s, e, t)| pin_ill_conditioned(s, e, t),
        check: |&(s, e, t), got| {
            let want = true_circle(s, e, t);
            let tol = crate::harness::Tol::DEFAULT;
            let agree = match (want, *got) {
                (None, None) => true,
                (Some((wc, wr, ws)), Some((gc, gr, gs))) => {
                    tol.floats_agree(wc[0], gc[0])
                        && tol.floats_agree(wc[1], gc[1])
                        && tol.floats_agree(wr, gr)
                        && tol.floats_agree(ws, gs)
                }
                _ => false,
            };
            if agree {
                Ok(())
            } else {
                Err(format!("true circle {want:?}, scx {got:?}"))
            }
        },
    }
}

/// A double the pin computed and a bound on its distance from the true value of the step.
#[derive(Clone, Copy, Debug)]
struct PinError {
    v: f64,
    e: f64,
}

/// `2^-53`, the relative rounding error, and `2^-1074`, the absolute one in the subnormals.
const U: f64 = 1.1102230246251565e-16;
const ETA: f64 = 4.9406564584124654e-324;

impl PinError {
    fn exact(v: f64) -> Self {
        PinError { v, e: 0.0 }
    }

    /// `v`, rounded once from a value within `e` of the truth.
    fn rounded(v: f64, e: f64) -> Self {
        let e = if v.is_finite() {
            e + U * v.abs() + ETA
        } else {
            f64::INFINITY
        };
        PinError { v, e }
    }

    fn add(self, o: Self) -> Self {
        Self::rounded(self.v + o.v, self.e + o.e)
    }

    fn sub(self, o: Self) -> Self {
        Self::rounded(self.v - o.v, self.e + o.e)
    }

    fn mul(self, o: Self) -> Self {
        Self::rounded(
            self.v * o.v,
            self.v.abs() * o.e + o.v.abs() * self.e + self.e * o.e,
        )
    }

    fn div(self, o: Self) -> Self {
        let q = self.v / o.v;
        let room = o.v.abs() - o.e;
        if room > 0.0 {
            Self::rounded(q, (self.e + q.abs() * o.e) / room)
        } else {
            PinError {
                v: q,
                e: f64::INFINITY,
            }
        }
    }

    fn hypot(x: Self, y: Self) -> Self {
        Self::rounded(scx_num::fmath::hypot(x.v, y.v), x.e + y.e)
    }

    /// Whether the true value may be on the other side of `at` (or on it).
    fn straddles(self, at: f64) -> bool {
        (self.v - at).abs() <= self.e + 8.0 * U * self.v.abs()
    }

    /// Whether the bound passes a quarter of `Tol::DEFAULT` on this value.
    fn loose(self) -> bool {
        let tol = crate::harness::Tol::DEFAULT;
        self.e.is_nan() || self.e > 0.25 * tol.abs.max(tol.rel * self.v.abs())
    }
}

/// Whether the pin's evaluation (its steps, one by one, as `scx_num::math` had them before
/// it was made exact) cannot be trusted to `Tol::DEFAULT / 4` on these points.
fn pin_ill_conditioned(s: P, e: P, t: P) -> bool {
    type E = PinError;
    if !s.iter().chain(&e).chain(&t).all(|v| v.is_finite()) {
        return false;
    }
    let [sx, sy] = s.map(E::exact);
    let [ex, ey] = e.map(E::exact);
    let [tx, ty] = t.map(E::exact);
    // circumcentre_2d(s, t, e)
    let (abx, aby) = (tx.sub(sx), ty.sub(sy));
    let (acx, acy) = (ex.sub(sx), ey.sub(sy));
    let ab2 = abx.mul(abx).add(aby.mul(aby));
    let ac2 = acx.mul(acx).add(acy.mul(acy));
    let d = E::exact(2.0).mul(abx.mul(acy).sub(aby.mul(acx)));
    if !d.v.is_finite() {
        return true;
    }
    let abs_d = E {
        v: d.v.abs(),
        e: d.e,
    };
    if d.v.abs() < 1e-12 && !abs_d.straddles(1e-12) {
        return false;
    }
    let cx = sx.add(acy.mul(ab2).sub(aby.mul(ac2)).div(d));
    let cy = sy.add(abx.mul(ac2).sub(acx.mul(ab2)).div(d));
    if !(cx.v.is_finite() && cy.v.is_finite()) || d.straddles(1e-12) || d.straddles(-1e-12) {
        return true;
    }
    // three_point_circle_2d
    let r = E::hypot(cx.sub(sx), cy.sub(sy));
    let (chx, chy) = (ex.sub(sx), ey.sub(sy));
    let len = E::hypot(chx, chy);
    if !(r.v.is_finite() && len.v.is_finite()) {
        return true;
    }
    if len.straddles(1e-9) || r.straddles(1e-9) {
        return true;
    }
    if len.v <= 1e-9 || r.v <= 1e-9 {
        return false;
    }
    let (nx, ny) = (E::exact(0.0).sub(chy.div(len)), chx.div(len));
    let half = E::exact(0.5);
    let (mx, my) = (sx.add(ex).mul(half), sy.add(ey).mul(half));
    let side_t = nx.mul(tx.sub(mx)).add(ny.mul(ty.sub(my)));
    let side_c = nx.mul(cx.sub(mx)).add(ny.mul(cy.sub(my)));
    if side_t.straddles(0.0) || side_c.straddles(0.0) {
        return true;
    }
    let q = len.div(E::exact(2.0).mul(r));
    let asin = |x: f64| scx_num::fmath::asin(x.clamp(-1.0, 1.0));
    let mv = 2.0 * asin(q.v);
    let me = 2.0 * (asin(q.v + q.e) - asin(q.v)).max(asin(q.v) - asin(q.v - q.e));
    let minor = E::rounded(mv, me);
    let long = side_t.v * side_c.v > 0.0;
    // TAU itself is 2.4e-16 from 2 pi
    let sweep = if long {
        E::rounded(core::f64::consts::TAU - minor.v, minor.e + 2.5e-16)
    } else {
        minor
    };
    cx.loose() || cy.loose() || r.loose() || sweep.loose()
}

/// The true `three_point_circle_2d(s, e, t)`, in exact integers, each result rounded once
/// (within `2^-51`): see [`three_point_circle_2d`].
fn true_circle(s: P, e: P, t: P) -> Circle {
    use core::cmp::Ordering::{Greater, Less};
    use scx_testkit::exact::{ldexp, ratio, sqrt_ratio, Int};
    if !s.iter().chain(&e).chain(&t).all(|v| v.is_finite()) {
        return None;
    }
    let ([sx, sy, ex, ey, tx, ty], u) = Int::of_doubles([s[0], s[1], e[0], e[1], t[0], t[1]]);
    let ([thr_d, thr_l], v) = Int::of_doubles([1e-12, 1e-9]);
    let (ax, ay) = (sx.sub(&tx), sy.sub(&ty));
    let (bx, by) = (ex.sub(&tx), ey.sub(&ty));
    let (cx, cy) = (ex.sub(&sx), ey.sub(&sy));
    let cross = ax.mul(&by).sub(&ay.mul(&bx));
    let d = cross.add(&cross);
    // twice the area at least 1e-12
    if d.cmp_scaled(2 * u, &thr_d, v) == Less {
        return None;
    }
    let sq = |x: &Int, y: &Int| x.mul(x).add(&y.mul(y));
    let (a2, b2, c2) = (sq(&ax, &ay), sq(&bx, &by), sq(&cx, &cy));
    // the chord above 1e-9, and finite
    let thr_l2 = thr_l.mul(&thr_l);
    // A result is finite when below 2^1024 - 2^970 (halfway past the largest double), in
    // units of 2^w: decided exactly, as the rounding near it is not.
    let ([max, half], w) = Int::of_doubles([f64::MAX, ldexp(1.0, 970)]);
    let top = max.add(&half);
    let top2 = top.mul(&top);
    let clamp = |x: f64| x.clamp(-f64::MAX, f64::MAX);
    if c2.cmp_scaled(2 * u, &thr_l2, 2 * v) != Greater || c2.cmp_scaled(2 * u, &top2, 2 * w) != Less
    {
        return None;
    }
    // the centre's offset from t is N / d
    let nx = by.mul(&a2).sub(&ay.mul(&b2));
    let ny = ax.mul(&b2).sub(&bx.mul(&a2));
    // s - centre = (a d - N) / d, e - centre = (b d - N) / d
    let (px, py) = (ax.mul(&d).sub(&nx), ay.mul(&d).sub(&ny));
    let (qx, qy) = (bx.mul(&d).sub(&nx), by.mul(&d).sub(&ny));
    // the radius above 1e-9, and finite: |p|^2 > 1e-18 d^2
    let p2 = sq(&px, &py);
    if p2.cmp_scaled(2 * u, &thr_l2.mul(&d.mul(&d)), 2 * v) != Greater {
        return None;
    }
    if p2.cmp_scaled(2 * u, &top2.mul(&d.mul(&d)), 2 * w) != Less {
        return None;
    }
    let r = clamp(sqrt_ratio(&p2, &d.mul(&d), 2 * u));
    let (cnx, cny) = (tx.mul(&d).add(&nx), ty.mul(&d).add(&ny));
    let centre = [clamp(ratio(&cnx, &d, u)), clamp(ratio(&cny, &d, u))];
    if [&cnx, &cny]
        .iter()
        .any(|c| c.cmp_scaled(u, &top.mul(&d), w) != Less)
    {
        return None;
    }
    // p x q = d (d cross + (b - a) x N), exactly; p . q from rounded parts (no cancellation
    // that matters: its error moves the angle by a relative 2^-51).
    let bma = (bx.sub(&ax), by.sub(&ay));
    let pxq = d.mul(&d.mul(&cross).add(&bma.0.mul(&ny).sub(&bma.1.mul(&nx))));
    let f = |x: &Int| x.approx();
    let ((mpx, kpx), (mpy, kpy), (mqx, kqx), (mqy, kqy)) = (f(&px), f(&py), f(&qx), f(&qy));
    let top = [kpx, kpy, kqx, kqy].into_iter().max().unwrap_or(0);
    let g = |m: f64, k: i64| ldexp(m, k - top);
    let dot = g(mpx, kpx) * g(mqx, kqx) + g(mpy, kpy) * g(mqy, kqy);
    let (mc, kc) = pxq.approx();
    let crs = ldexp(mc.abs(), kc - 2 * top);
    let minor = scx_num::fmath::atan2(crs, dot);
    // long when t and the centre are on one side of the chord: n = (-cy, cx),
    // side_t ~ n . (2t - s - e), side_c ~ n . (2 N + (2t - s - e) d) / d
    let w = (tx.add(&tx).sub(&sx).sub(&ex), ty.add(&ty).sub(&sy).sub(&ey));
    let side = |x: &Int, y: &Int| cy.neg().mul(x).add(&cx.mul(y));
    let side_t = side(&w.0, &w.1).signum();
    let side_c = side(
        &nx.add(&nx).add(&w.0.mul(&d)),
        &ny.add(&ny).add(&w.1.mul(&d)),
    )
    .signum()
        * d.signum();
    let sweep = if side_t * side_c > 0 {
        core::f64::consts::TAU - minor
    } else {
        minor
    };
    Some((centre, r, sweep))
}

// ---- scx-p21 against solidcore-record's Part 21 reader (`t_p21`) ---------------------------
//
// Each predicate reads the DATA section's text (the pools have no strings or comments that
// could hide the construct). The clauses are those of ISO 10303-21:2016.

use crate::t_p21::{
    Text, XRead, EMPTY_COMPLEX, ENUM_DASH, LOWER_HEX, PAGE_LATIN, TYPED_MANY, TYPED_NONE,
};
use scx_p21::{P21Error, Param, Refusal, Syntax};

/// scx's refusal cause, through any `Within` chain.
fn syntax_cause(r: &XRead) -> Option<&Syntax> {
    match r.0.as_ref().err().map(P21Error::innermost) {
        Some(P21Error::Refused(Refusal::Syntax { cause, .. })) => Some(cause),
        _ => None,
    }
}

/// `#1`'s first parameter, as scx read it.
fn first_param(r: &XRead) -> Result<&Param, String> {
    let parsed = r.0.as_ref().map_err(|e| format!("scx refused: {e}"))?;
    parsed
        .exchange
        .get(1)
        .and_then(|e| e.params().first())
        .ok_or_else(|| "scx read no #1 with a parameter".to_owned())
}

/// The number of values in each typed parameter `KEYWORD(...)` inside a record's
/// parameters (a keyword directly before `(` at depth 1 or more).
fn typed_arities(body: &str) -> Vec<usize> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' => {
                let keyword = i > 0 && b[i - 1].is_ascii_alphabetic();
                if keyword && depth >= 1 {
                    // count the values up to the matching parenthesis
                    let (mut d, mut commas, mut empty) = (0usize, 0usize, true);
                    for &k in &b[i + 1..] {
                        match k {
                            b'(' => d += 1,
                            b')' if d == 0 => break,
                            b')' => d -= 1,
                            b',' if d == 0 => commas += 1,
                            _ => {}
                        }
                        empty = false;
                    }
                    out.push(if empty { 0 } else { commas + 1 });
                }
                depth += 1;
            }
            b')' => depth = depth.saturating_sub(1),
            b';' => depth = 0,
            _ => {}
        }
    }
    out
}

/// `#n=();`: a complex instance with no records.
///
/// §5.5 `SUBSUPER_RECORD = "(" SIMPLE_RECORD_LIST ")"`, `SIMPLE_RECORD_LIST = SIMPLE_RECORD
/// { SIMPLE_RECORD }`: at least one record. The true reading is a refusal; scx refuses with
/// `Syntax::ExpectedKeyword` at the `)`. The pin accepts an instance with no records.
pub fn p21_empty_complex() -> PinDefect<(Text<EMPTY_COMPLEX>,), XRead> {
    PinDefect {
        reason: "the pin reads an empty complex instance",
        applies: |(t,)| t.0.replace(' ', "").contains("=();"),
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::ExpectedKeyword) => Ok(()),
            _ => Err(format!("§5.5 needs a record; scx: {:?}", r.0)),
        },
    }
}

/// `B(1,2)` as a parameter: a typed parameter with more than one value.
///
/// §5.5 `TYPED_PARAMETER = KEYWORD "(" PARAMETER ")"`: exactly one parameter (a list is one).
/// The true reading is a refusal; scx refuses with `Syntax::TypedArity`. The pin accepts
/// and keeps every value.
pub fn p21_typed_many() -> PinDefect<(Text<TYPED_MANY>,), XRead> {
    PinDefect {
        reason: "the pin reads a typed parameter of several values",
        applies: |(t,)| typed_arities(t.0).iter().any(|&n| n > 1),
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::TypedArity(_)) => Ok(()),
            _ => Err(format!("§5.5 allows one value; scx: {:?}", r.0)),
        },
    }
}

/// `B()` as a parameter: a typed parameter with no value.
///
/// The same clause as [`p21_typed_many`]: one parameter, so none is a syntax error. scx
/// refuses with `Syntax::TypedArity`; the pin accepts a typed value with no values.
pub fn p21_typed_none() -> PinDefect<(Text<TYPED_NONE>,), XRead> {
    PinDefect {
        reason: "the pin reads a typed parameter of no value",
        applies: |(t,)| typed_arities(t.0).contains(&0),
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::TypedArity(_)) => Ok(()),
            _ => Err(format!("§5.5 needs one value; scx: {:?}", r.0)),
        },
    }
}

/// `.A-B.`: an enumeration value with a `-`.
///
/// §5.4 `ENUMERATION = "." UPPER { UPPER | DIGIT } "."` (with `_` among the upper-case
/// characters, §5.3), and §6.4.5: no `-`. The true reading is a refusal; scx refuses with
/// `Syntax::BadEnumeration`. The pin reads `A-B`.
pub fn p21_enum_dash() -> PinDefect<(Text<ENUM_DASH>,), XRead> {
    PinDefect {
        reason: "the pin reads an enumeration with a dash",
        applies: |(t,)| {
            t.0.split('.').skip(1).step_by(2).any(|tok| {
                tok.contains('-')
                    && tok.bytes().all(|c| {
                        c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                    })
            })
        },
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::BadEnumeration) => Ok(()),
            _ => Err(format!("§6.4.5 has no '-'; scx: {:?}", r.0)),
        },
    }
}

/// `'\PA\\S\a'`: a string with a `\PA\` page directive.
///
/// §6.4.3.3: `\PA\` is a directive, not text; it selects ISO 8859-1 for the `\S\`
/// directives that follow, and `\S\c` is the character at `c`'s code plus 0x80 in that
/// part: `\S\a` is 0x61 + 0x80 = 0xE1, `á`. The pin does not read the page directive and
/// keeps the string's text raw (`\PA\\S\a`, and `\PA\abc` for `abc`); scx reads `á` and
/// `abc`. The check decodes the string independently: Latin-1 is
/// the first 256 code points, so the character is `char::from(c + 0x80)`.
pub fn p21_page_latin() -> PinDefect<(Text<PAGE_LATIN>,), XRead> {
    PinDefect {
        reason: "the pin keeps a \\PA\\ string raw",
        applies: |(t,)| t.0.contains(r"\PA\"),
        check: |(t,), r| {
            let raw = t.0.split('\'').nth(1).ok_or("no string in the input")?;
            let mut truth = String::new();
            let mut rest = raw;
            while !rest.is_empty() {
                if let Some(r) = rest.strip_prefix(r"\PA\") {
                    rest = r;
                } else if let Some(r) = rest.strip_prefix(r"\S\") {
                    let c = r.bytes().next().ok_or("\\S\\ at the end")?;
                    truth.push(char::from(c + 0x80));
                    rest = &r[1..];
                } else {
                    let c = rest.chars().next().unwrap();
                    truth.push(c);
                    rest = &rest[c.len_utf8()..];
                }
            }
            match first_param(r)? {
                Param::Str(s) if *s == truth => Ok(()),
                p => Err(format!("§6.4.3.3 gives {truth:?}; scx: {p:?}")),
            }
        },
    }
}

/// `"0f"`: a binary with lower-case hexadecimal digits.
///
/// §5.4 `BINARY = """" ("0" | "1" | "2" | "3") { HEX } """"`, `HEX` being `0`-`9` and
/// `A`-`F` (§5.3, §6.4.6): lower case is outside the grammar. Both readers accept it
/// (leniency kept for real files); scx normalises the digits to upper case, so the value
/// it holds (and writes back) is valid, while the pin keeps `0f`. The check: scx holds the
/// input's digits upper-cased.
pub fn p21_lower_hex() -> PinDefect<(Text<LOWER_HEX>,), XRead> {
    PinDefect {
        reason: "the pin keeps lower-case hex in a binary",
        applies: |(t,)| {
            t.0.split('"')
                .nth(1)
                .is_some_and(|h| h.bytes().any(|c| c.is_ascii_lowercase()))
        },
        check: |(t,), r| {
            let hex = t.0.split('"').nth(1).ok_or("no binary in the input")?;
            match first_param(r)? {
                Param::Binary(b) if *b == hex.to_ascii_uppercase() => Ok(()),
                p => Err(format!(
                    "the digits upper-cased are {:?}; scx: {p:?}",
                    hex.to_ascii_uppercase()
                )),
            }
        },
    }
}

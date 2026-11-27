// Written for solidcorex. Apache-2.0.
//! Enclosures: every partial `S^(a,b)`, `a + b <= N`, of a surface over a box of its
//! parameters, as intervals that hold the partial at every point of the box (research 63
//! §2.6, NS4).
//!
//! - [`NurbsSurface::partials_over_piece`]: one polynomial piece (a span of each direction)
//!   over a box, the piece continued past its span where the box reaches out of it;
//! - [`NurbsSurface::partials_over`]: a box anywhere in the domain, cut at the knot lines
//!   strictly inside it, each part enclosed on its own piece, and the enclosures' hull.
//!
//! The form is the pin's (`geom::nurbs_enclosure`): a centred Taylor form. The homogeneous
//! surface's partials `H^(i,j)`, every `i <= p`, `j <= q`, are taken in intervals at the
//! box's middle `m` (A2.3's basis derivatives at the point `m`, so only rounding widens
//! them); the piece is a polynomial of degree `(p, q)`, so its Taylor series about `m` is
//! finite and
//!
//! `A^(a,b)(x) = Σ_{i >= a, j >= b} H^(i,j)(m) (x_u - m_u)^(i-a) / (i-a)! · (x_v - m_v)^(j-b) / (j-b)!`
//!
//! is exact for every `x`: the box `[du] × [dv] = box - m` in place of `x - m` encloses it.
//! A rational surface then divides by the weight function in intervals (A4.4, the quotient
//! [`NurbsSurface::jet`] takes in doubles). [`About`] picks the point the quotient is taken
//! about: far from the origin, `S - o` for an `o` near the box keeps the quotient's
//! `w^(i,j) S^(k-i,l-j)` terms small.
//!
//! On a rational surface the arithmetic is the pin's, in its order, so the enclosures are its
//! bits (the differential test checks them). A polynomial surface skips the weight function
//! and the quotient, whose weights there are intervals holding 1 and 0: its enclosure is the
//! pin's numerator, a subset of the pin's enclosure (inclusion isotonicity), never wider.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `nurbs_partials_over_piece`, `nurbs_partials_over_piece_about` | [`NurbsSurface::partials_over_piece`] with [`About::Origin`], [`About::Point`] |
//! | `partials_over_piece(.., Shift::Middle)` (crate-only) | [`About::Middle`] |
//! | `partials_over_piece(.., weights: Some(w), ..)` (crate-only: the weight cut to a certificate's) | not ported: a cut no caller can check is unsound in a public method |
//! | `nurbs_partials_over_box`, `nurbs_partials_over_box_about` | [`NurbsSurface::partials_over`] |
//! | `Partials<Ivl>` (orders to `MAX_K` = 13) | [`SurfaceJet`]`<[Ivl; D], N>` (orders to [`MAX_ORDER`] = 31) |
//! | `Unsupported`: an order past `MAX_K` | [`NurbsSurfaceError::Basis`] over `NurbsError::DerivativeOrder`, as [`NurbsSurface::jet`] |
//! | `Unsupported`: a box past the knots, a crossed knot below the continuity | [`EnclosureCause::BoxOutside`], [`EnclosureCause::BelowContinuity`]: the caller's data, `Refused` |
//! | none: a span index is trusted, a box is not checked | [`EnclosureCause::NotAPiece`], [`EnclosureCause::BoxNotFinite`] |
//! | `SectionNotProved(FaceWeightPositive)` | [`EnclosureCause::WeightsNotPositive`], `Unsupported` (a smaller box may prove them) |
//! | `ders_basis`'s `p!/(p-k)!` as a double | the same double while it is exact (below 2^53, every degree the pin allows), an outward interval past it |
//!
//! The scratch the tables need is sized by the degrees (research 66 §4.1, `OpenNURBS`'s one
//! buffer): on the stack up to degree 7 each way in three dimensions, on the heap past it.

use scx_num::interval::Ivl;
use scx_num::jet::SurfaceJet;
use scx_nurbs::{Approach, KnotVector, MAX_ORDER, NurbsError};

use crate::{EnclosureCause, NurbsSurface, NurbsSurfaceError, ParamAxis, Result};

/// The point the partials are enclosed about: the partials of `S - o`, whose order-0 entry
/// is a box around `S - o` and whose others are `S`'s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum About<const D: usize> {
    /// The origin: the partials of `S`.
    Origin,
    /// `o`: the partials of `S - o` (the pin's `_about` forms). Narrower than those of `S`
    /// where the weights vary and the box lies far from the origin; `o` near the box is best.
    Point([f64; D]),
    /// The surface's point at the middle of each piece's box, added back to the order-0
    /// entry: the partials of `S`, as narrow as [`About::Point`] at that point. Where that
    /// point is not finite, [`About::Origin`].
    Middle,
}

/// The point interval 0.
const IZ: Ivl = Ivl::new(0.0, 0.0);

/// The scratch on the stack: the tables of degree 7 each way in three dimensions (an
/// order of [`scx_nurbs`]'s small basis width, 8), 8 KiB.
const STACK: usize = 512;

impl<const D: usize> NurbsSurface<D> {
    /// The partials `S^(a,b)`, `a + b <= N`, of the polynomial piece on the spans `span =
    /// [u span, v span]` over the box `uv = [u range, v range]`, enclosed: each interval
    /// holds that partial of the piece at every point of the box. The piece is the
    /// polynomial the surface is on its spans, continued past them where the box reaches out
    /// (the knot vector's span indices, `p <= i < count` with `U[i] < U[i + 1]`, as
    /// [`KnotVector::span`] returns them).
    ///
    /// An end of an interval may be infinite where the arithmetic overflows (an enclosure
    /// still, of nothing useful).
    ///
    /// Errors with [`NurbsSurfaceError::Basis`] over `NurbsError::DerivativeOrder` when `N`
    /// is above [`MAX_ORDER`]; then, u before v, with [`EnclosureCause::NotAPiece`] for a
    /// span that is not one and [`EnclosureCause::BoxNotFinite`] for a range whose ends are
    /// not finite and in order; and with [`EnclosureCause::WeightsNotPositive`] where the
    /// weight function's enclosure is not positive (the box is too wide for this form:
    /// smaller ones may succeed).
    pub fn partials_over_piece<const N: usize>(
        &self,
        span: [usize; 2],
        uv: [Ivl; 2],
        about: About<D>,
    ) -> Result<SurfaceJet<[Ivl; D], N>> {
        check_order::<N>()?;
        for (axis, k) in AXES.into_iter().zip(span) {
            check_piece(axis, self.knot_vector(axis), k)?;
        }
        for (axis, t) in AXES.into_iter().zip(uv) {
            check_range(axis, t)?;
        }
        self.piece::<N>(span, uv, about)
    }

    /// The partials `S^(a,b)`, `a + b <= N`, over the box `uv = [u range, v range]` of the
    /// domain, enclosed: the box cut at the knot lines strictly inside it, each part
    /// enclosed on its own piece ([`NurbsSurface::partials_over_piece`]), and the hull.
    ///
    /// Every point of the box lies in some closed part, where that piece is the surface, so
    /// each interval holds that partial of the surface at every point, from either side on
    /// a knot line inside the box. A box that ends on a knot line holds there its own side's
    /// partials; a box of zero width on a knot line strictly inside the domain holds both
    /// sides'. A use that integrates over the box (a mean-value form) needs the surface
    /// continuous across the lines it crosses: a crossed line where the degree minus the
    /// knot's multiplicity is below `continuity` is refused (`continuity = 0` refuses
    /// none: the surface is continuous everywhere).
    ///
    /// Errors as [`NurbsSurface::partials_over_piece`] for the order, the ranges and the
    /// weights; with [`EnclosureCause::BoxOutside`] for a range not inside the domain and
    /// [`EnclosureCause::BelowContinuity`] for a crossed knot line, each direction's checked
    /// before any part is enclosed, u before v.
    pub fn partials_over<const N: usize>(
        &self,
        uv: [Ivl; 2],
        continuity: usize,
        about: About<D>,
    ) -> Result<SurfaceJet<[Ivl; D], N>> {
        check_order::<N>()?;
        let [u, v] = uv;
        let us = cuts(ParamAxis::U, self.knot_vector(ParamAxis::U), u, continuity)?;
        let vs = cuts(ParamAxis::V, self.knot_vector(ParamAxis::V), v, continuity)?;
        let mut out: Option<SurfaceJet<[Ivl; D], N>> = None;
        for (ur, ku) in us {
            for (vr, kv) in vs.clone() {
                let p = self.piece::<N>([ku, kv], [ur, vr], about)?;
                out = Some(match out {
                    None => p,
                    Some(h) => hull(&h, &p),
                });
            }
        }
        // Each direction has at least one part.
        out.ok_or(NurbsSurfaceError::Defect {
            why: "partials_over: a box with no part",
        })
    }

    /// [`NurbsSurface::partials_over_piece`] after its checks.
    fn piece<const N: usize>(
        &self,
        span: [usize; 2],
        uv: [Ivl; 2],
        about: About<D>,
    ) -> Result<SurfaceJet<[Ivl; D], N>> {
        let (p, q) = (
            self.knot_vector(ParamAxis::U).degree(),
            self.knot_vector(ParamAxis::V).degree(),
        );
        let lay = Layout::new(p, q, D);
        if lay.total <= STACK {
            let mut buf = [IZ; STACK];
            self.piece_in::<N>(
                span,
                uv,
                about,
                lay,
                buf.get_mut(..lay.total).unwrap_or_default(),
            )
        } else {
            let mut buf = vec![IZ; lay.total];
            self.piece_in::<N>(span, uv, about, lay, &mut buf)
        }
    }

    /// The enclosure in the scratch `buf` (`lay.total` long, zero).
    fn piece_in<const N: usize>(
        &self,
        [ku, kv]: [usize; 2],
        [u, v]: [Ivl; 2],
        about: About<D>,
        lay: Layout,
        buf: &mut [Ivl],
    ) -> Result<SurfaceJet<[Ivl; D], N>> {
        let (p, q) = (lay.p, lay.q);
        let (nu, rest) = buf.split_at_mut(lay.nu);
        let (nv, rest) = rest.split_at_mut(lay.nv);
        let (work, rest) = rest.split_at_mut(lay.work);
        let (hm, rest) = rest.split_at_mut(lay.hm);
        let (pu, pv) = rest.split_at_mut(p + 1);
        let um = 0.5 * (u.lo + u.hi);
        let vm = 0.5 * (v.lo + v.hi);
        ders_basis(self.knots(ParamAxis::U), ku, p, Ivl::from(um), nu, work);
        work.fill(IZ);
        ders_basis(self.knots(ParamAxis::V), kv, q, Ivl::from(vm), nv, work);

        // H^(a,b)(m), a <= p, b <= q, each coordinate then the weight: `hm[(a (q + 1) + b)
        // (D + 1) + e]`, summed over the net in the pin's order (i, then j).
        let st = D + 1;
        let rational = self.weighted_points().zip(self.weights());
        // A polynomial surface's weight sum is needed only for the middle's point.
        let w_lanes = match (rational.is_some(), about) {
            (true, _) => (q + 1) * (p + 1),
            (false, About::Middle) => 1,
            (false, _) => 0,
        };
        for i in 0..=p {
            for j in 0..=q {
                let (row, col) = (ku + i - p, kv + j - q);
                let (point, w) = match rational {
                    Some((wp, wn)) => (wp.get(row, col), wn.get(row, col).copied()),
                    None => (self.points().get(row, col), Some(1.0)),
                };
                let point = point.copied().unwrap_or([f64::NAN; D]);
                let w = Ivl::from(w.unwrap_or(f64::NAN));
                for a in 0..=p {
                    let na = get(nu, a * (p + 1) + i);
                    for b in 0..=q {
                        let c = na * get(nv, b * (q + 1) + j);
                        let at = (a * (q + 1) + b) * st;
                        let Some(h) = hm.get_mut(at..at + st) else {
                            continue;
                        };
                        let (hx, hw) = h.split_at_mut(D);
                        for (he, &x) in hx.iter_mut().zip(&point) {
                            *he = *he + c * Ivl::from(x);
                        }
                        if a * (q + 1) + b < w_lanes
                            && let Some(he) = hw.first_mut()
                        {
                            *he = *he + c * w;
                        }
                    }
                }
            }
        }

        // The point the quotient is taken about, and whether it is added back.
        let (o, add_back) = match about {
            About::Origin => (None, false),
            About::Point(o) => (Some(o), false),
            About::Middle => {
                let w = get(hm, D).mid();
                let o: [f64; D] = core::array::from_fn(|e| get(hm, e).mid() / w);
                let finite = o.iter().all(|x| x.is_finite());
                (finite.then_some(o), finite)
            }
        };
        if let Some(o) = o {
            if rational.is_some() {
                // (A - o w): the homogeneous form of S - o, every partial.
                for h in hm.chunks_exact_mut(st) {
                    let (hx, hw) = h.split_at_mut(D);
                    let w = hw.first().copied().unwrap_or_default();
                    for (he, &oe) in hx.iter_mut().zip(&o) {
                        *he = *he - Ivl::from(oe) * w;
                    }
                }
            } else {
                // A polynomial's weight is 1: S - o moves only the point.
                for (he, &oe) in hm.iter_mut().zip(&o) {
                    *he = *he - Ivl::from(oe);
                }
            }
        }

        // The Taylor factors (x - m)^i / i!.
        let du = u - Ivl::from(um);
        let dv = v - Ivl::from(vm);
        powers(pu, du);
        powers(pv, dv);

        // The homogeneous partials over the box: A^(a,b) and w^(a,b).
        let mut av: SurfaceJet<[Ivl; D], N> = SurfaceJet::from_fn(|_, _| [IZ; D]);
        let mut wv: SurfaceJet<Ivl, N> = SurfaceJet::from_fn(|_, _| IZ);
        for a in 0..=N {
            for b in 0..=N - a {
                let mut h = [IZ; D];
                let mut hw = IZ;
                for i in a..=p {
                    for j in b..=q {
                        let c = get(pu, i - a) * get(pv, j - b);
                        let at = (i * (q + 1) + j) * st;
                        let Some(hij) = hm.get(at..at + st) else {
                            continue;
                        };
                        for (he, &x) in h.iter_mut().zip(hij) {
                            *he = *he + x * c;
                        }
                        if rational.is_some() {
                            hw = hw + get(hij, D) * c;
                        }
                    }
                }
                if let (Some(e), Some(ew)) = (av.get_mut(a, b), wv.get_mut(a, b)) {
                    *e = h;
                    *ew = hw;
                }
            }
        }

        let mut out = if rational.is_some() {
            quotient(&av, &wv)?
        } else {
            av
        };
        if let (true, Some(o), Some(pt)) = (add_back, o, out.get_mut(0, 0)) {
            // S = (S - o) + o, rounded outward.
            for (x, &oe) in pt.iter_mut().zip(&o) {
                *x = *x + Ivl::from(oe);
            }
        }
        Ok(out)
    }
}

/// The two directions, for the checks in order.
const AXES: [ParamAxis; 2] = [ParamAxis::U, ParamAxis::V];

/// The order `N` is one the binomials and the basis derivatives hold exactly.
fn check_order<const N: usize>() -> Result<()> {
    if N > MAX_ORDER {
        return Err(NurbsSurfaceError::Basis {
            axis: ParamAxis::U,
            cause: NurbsError::DerivativeOrder {
                order: N,
                max: MAX_ORDER,
            },
        });
    }
    Ok(())
}

/// `span` is a non-empty span of `knots`, `p <= span < count`.
fn check_piece(axis: ParamAxis, knots: &KnotVector, span: usize) -> Result<()> {
    let k = knots.knots();
    let inside = span >= knots.degree() && span < knots.point_count();
    let open = matches!((k.get(span), k.get(span + 1)), (Some(a), Some(b)) if a < b);
    if inside && open {
        Ok(())
    } else {
        Err(NurbsSurfaceError::Enclosure(EnclosureCause::NotAPiece {
            axis,
            span,
        }))
    }
}

/// The range's ends are finite and in order.
fn check_range(axis: ParamAxis, t: Ivl) -> Result<()> {
    if t.is_finite() {
        Ok(())
    } else {
        Err(NurbsSurfaceError::Enclosure(EnclosureCause::BoxNotFinite {
            axis,
            range: [t.lo, t.hi],
        }))
    }
}

/// The parts of the range `t` of `axis` between the knots strictly inside it, each with the
/// span whose closed interval holds it, after the checks [`NurbsSurface::partials_over`]
/// names.
fn cuts(axis: ParamAxis, knots: &KnotVector, t: Ivl, continuity: usize) -> Result<Cuts<'_>> {
    check_range(axis, t)?;
    let domain = knots.domain();
    let (low, high) = (domain.min(), domain.max());
    if !(t.lo >= low && t.hi <= high) {
        return Err(NurbsSurfaceError::Enclosure(EnclosureCause::BoxOutside {
            axis,
            range: [t.lo, t.hi],
            domain: [low, high],
        }));
    }
    let k = knots.knots();
    let inner = k
        .get(k.partition_point(|&x| x <= t.lo)..k.partition_point(|&x| x < t.hi))
        .unwrap_or_default();
    let p = knots.degree();
    // `breakpoints` would walk the whole vector; the knots inside are a slice of it.
    let mut prev = f64::NAN;
    for &x in inner {
        if x != prev {
            let has = p.saturating_sub(knots.multiplicity(x));
            if has < continuity {
                return Err(NurbsSurfaceError::Enclosure(
                    EnclosureCause::BelowContinuity {
                        axis,
                        knot: x,
                        continuity: has,
                        needed: continuity,
                    },
                ));
            }
        }
        prev = x;
    }
    // A point on a knot strictly inside the domain lies in both spans' closed intervals.
    let both = t.lo == t.hi && low < t.lo && t.lo < high && knots.multiplicity(t.lo) > 0;
    let below = both.then(|| span(knots, t.lo, Approach::Below));
    Ok(Cuts {
        knots,
        inner,
        at: t.lo,
        hi: t.hi,
        below,
        done: false,
    })
}

/// The span of a finite `t` inside the domain on `side`.
fn span(knots: &KnotVector, t: f64, side: Approach) -> usize {
    // `t` is finite (checked), so `span` cannot fail; the first span otherwise, which a
    // checked `t` never reaches.
    knots.span(t, side).unwrap_or_else(|_| knots.degree())
}

/// The parts of a range, in order: `([a, b], span)`.
#[derive(Clone)]
struct Cuts<'a> {
    /// The knot vector.
    knots: &'a KnotVector,
    /// The knots strictly inside the range not yet passed.
    inner: &'a [f64],
    /// The start of the next part.
    at: f64,
    /// The range's end.
    hi: f64,
    /// For a point on an inner knot: the span before it, the first part's.
    below: Option<usize>,
    /// The last part has been returned.
    done: bool,
}

impl Iterator for Cuts<'_> {
    type Item = (Ivl, usize);

    fn next(&mut self) -> Option<(Ivl, usize)> {
        if let Some(k) = self.below.take() {
            return Some((Ivl::new(self.at, self.at), k));
        }
        if self.done {
            return None;
        }
        let passed = self.inner.partition_point(|&x| x <= self.at);
        self.inner = self.inner.get(passed..).unwrap_or_default();
        let b = if let Some(&x) = self.inner.first() {
            x
        } else {
            self.done = true;
            self.hi
        };
        let a = self.at;
        self.at = b;
        // The start picks the span to its right (a part one ulp wide may end on the next
        // knot, so its middle is no guide).
        Some((Ivl::new(a, b), span(self.knots, a, Approach::Above)))
    }
}

/// The hull of two enclosures.
fn hull<const D: usize, const N: usize>(
    h: &SurfaceJet<[Ivl; D], N>,
    p: &SurfaceJet<[Ivl; D], N>,
) -> SurfaceJet<[Ivl; D], N> {
    SurfaceJet::from_fn(|a, b| match (h.get(a, b), p.get(a, b)) {
        (Some(x), Some(y)) => core::array::from_fn(|e| get(&x, e).hull(get(&y, e))),
        _ => [IZ; D],
    })
}

/// The quotient rule (A4.4) in intervals, in the pin's order: `S^(a,b) = (A^(a,b) -
/// Σ_{(i,j) ≠ (0,0)} C(a,i) C(b,j) w^(i,j) S^(a-i,b-j)) / w^(0,0)`.
fn quotient<const D: usize, const N: usize>(
    av: &SurfaceJet<[Ivl; D], N>,
    wv: &SurfaceJet<Ivl, N>,
) -> Result<SurfaceJet<[Ivl; D], N>> {
    let w00 = wv.point();
    // NaN is not proved positive either.
    if w00.lo.partial_cmp(&0.0) != Some(core::cmp::Ordering::Greater) {
        return Err(NurbsSurfaceError::Enclosure(
            EnclosureCause::WeightsNotPositive { weight: w00 },
        ));
    }
    let mut out: SurfaceJet<[Ivl; D], N> = SurfaceJet::from_fn(|_, _| [IZ; D]);
    for a in 0..=N {
        for b in 0..=N - a {
            let mut v = av.get(a, b).unwrap_or([IZ; D]);
            // The binomials are exact: `C(a, i) C(b, j) <= C(31, 15)` for `a + b <= 31`.
            let mut cka = 1.0_f64;
            for i in 0..=a {
                if i > 0 {
                    cka = cka * (a - i + 1) as f64 / i as f64;
                }
                let mut cbj = 1.0_f64;
                for j in 0..=b {
                    if j > 0 {
                        cbj = cbj * (b - j + 1) as f64 / j as f64;
                    }
                    if i == 0 && j == 0 {
                        continue;
                    }
                    let c = Ivl::from(cka * cbj) * wv.get(i, j).unwrap_or(IZ);
                    let s = out.get(a - i, b - j).unwrap_or([IZ; D]);
                    for (ve, &se) in v.iter_mut().zip(&s) {
                        *ve = *ve - c * se;
                    }
                }
            }
            if let Some(e) = out.get_mut(a, b) {
                *e = v.map(|x| x / w00);
            }
        }
    }
    Ok(out)
}

/// `out[i] = out[i - 1] d / i`, `out[0] = 1`, in the pin's order.
fn powers(out: &mut [Ivl], d: Ivl) {
    let mut prev = Ivl::from(1.0);
    for (i, x) in out.iter_mut().enumerate() {
        if i > 0 {
            prev = prev * d * (Ivl::from(1.0) / Ivl::from(i as f64));
        }
        *x = prev;
    }
}

/// `s[i]`, 0 outside.
fn get(s: &[Ivl], i: usize) -> Ivl {
    s.get(i).copied().unwrap_or(IZ)
}

/// Where the tables lie in the scratch, by the degrees.
#[derive(Clone, Copy)]
struct Layout {
    p: usize,
    q: usize,
    /// The u basis derivatives, `(p + 1)²`.
    nu: usize,
    /// The v basis derivatives, `(q + 1)²`.
    nv: usize,
    /// A2.3's work tables for the larger degree `r`: `ndu` `(r + 1)²`, `left`, `right`
    /// `r + 1` each, `a` `2 (r + 1)`.
    work: usize,
    /// The homogeneous partials at the middle, `(p + 1)(q + 1)(D + 1)`.
    hm: usize,
    /// Everything, with the Taylor factors (`p + 1` and `q + 1`).
    total: usize,
}

impl Layout {
    fn new(p: usize, q: usize, d: usize) -> Self {
        let r = p.max(q) + 1;
        let (nu, nv) = ((p + 1) * (p + 1), (q + 1) * (q + 1));
        let work = r * r + 4 * r;
        let hm = (p + 1) * (q + 1) * (d + 1);
        Self {
            p,
            q,
            nu,
            nv,
            work,
            hm,
            total: nu + nv + work + hm + p + q + 2,
        }
    }
}

/// A row-major matrix of `cols` columns over a slice, read and written by signed indices:
/// 0 outside, a write outside dropped (the pin's `Mat`).
struct Mat<'a> {
    s: &'a mut [Ivl],
    cols: usize,
}

impl Mat<'_> {
    fn idx(&self, i: isize, j: isize) -> Option<usize> {
        let (i, j) = (usize::try_from(i).ok()?, usize::try_from(j).ok()?);
        (j < self.cols).then_some(i * self.cols + j)
    }

    fn at(&self, i: isize, j: isize) -> Ivl {
        self.idx(i, j)
            .and_then(|k| self.s.get(k))
            .copied()
            .unwrap_or(IZ)
    }

    fn set(&mut self, i: isize, j: isize, x: Ivl) {
        if let Some(y) = self.idx(i, j).and_then(|k| self.s.get_mut(k)) {
            *y = x;
        }
    }
}

/// `knots[i]`, NaN outside.
fn knot(knots: &[f64], i: isize) -> f64 {
    usize::try_from(i)
        .ok()
        .and_then(|i| knots.get(i))
        .copied()
        .unwrap_or(f64::NAN)
}

/// A2.3 in intervals at the point `x` on the span `k`: every derivative to order `p` of the
/// `p + 1` basis functions there, into `ders` (row `kk` the `kk`-th derivatives, `(p + 1)²`),
/// in the pin's order (`station::ders_basis`), so its bits. `work` is zero and holds A2.3's
/// tables. The scale `p! / (p - kk)!` is the pin's double while that is exact (below 2^53,
/// every degree the pin allows); past it, the exact integer rounded one ulp outward.
fn ders_basis(knots: &[f64], k: usize, p: usize, x: Ivl, ders: &mut [Ivl], work: &mut [Ivl]) {
    let r1 = work.len();
    let w = p + 1;
    let (ndu, rest) = work.split_at_mut((w * w).min(r1));
    let (left, rest) = rest.split_at_mut(w.min(rest.len()));
    let (right, a) = rest.split_at_mut(w.min(rest.len()));
    let a = a.get_mut(..2 * w).unwrap_or_default();
    let mut ndu = Mat { s: ndu, cols: w };
    let mut a = Mat { s: a, cols: w };
    let mut ders = Mat { s: ders, cols: w };
    let (k, p) = (k as isize, p as isize);
    let line = |v: &[Ivl], i: isize| {
        usize::try_from(i)
            .ok()
            .and_then(|i| v.get(i))
            .copied()
            .unwrap_or(IZ)
    };
    let put = |v: &mut [Ivl], i: isize, y: Ivl| {
        if let Some(e) = usize::try_from(i).ok().and_then(|i| v.get_mut(i)) {
            *e = y;
        }
    };
    let of = Ivl::from;
    ndu.set(0, 0, of(1.0));
    for j in 1..=p {
        put(left, j, x - of(knot(knots, k + 1 - j)));
        put(right, j, of(knot(knots, k + j)) - x);
        let mut saved = IZ;
        for r in 0..j {
            ndu.set(
                j,
                r,
                of(knot(knots, k + r + 1)) - of(knot(knots, k + 1 - j + r)),
            );
            let temp = ndu.at(r, j - 1) / ndu.at(j, r);
            ndu.set(r, j, saved + line(right, r + 1) * temp);
            saved = line(left, j - r) * temp;
        }
        ndu.set(j, j, saved);
    }
    for j in 0..=p {
        ders.set(0, j, ndu.at(j, p));
    }
    for r in 0..=p {
        let (mut s1, mut s2) = (0_isize, 1_isize);
        a.set(0, 0, of(1.0));
        for kk in 1..=p {
            let mut dd = IZ;
            let rk = r - kk;
            let pk = p - kk;
            if r >= kk {
                a.set(s2, 0, a.at(s1, 0) / ndu.at(pk + 1, rk));
                dd = a.at(s2, 0) * ndu.at(rk, pk);
            }
            let j1 = if rk >= -1 { 1 } else { -rk };
            let j2 = if r - 1 <= pk { kk - 1 } else { p - r };
            for j in j1..=j2 {
                a.set(
                    s2,
                    j,
                    (a.at(s1, j) - a.at(s1, j - 1)) / ndu.at(pk + 1, rk + j),
                );
                dd = dd + a.at(s2, j) * ndu.at(rk + j, pk);
            }
            if r <= pk {
                a.set(s2, kk, (of(0.0) - a.at(s1, kk - 1)) / ndu.at(pk + 1, r));
                dd = dd + a.at(s2, kk) * ndu.at(r, pk);
            }
            ders.set(kk, r, dd);
            core::mem::swap(&mut s1, &mut s2);
        }
    }
    // p! / (p - kk)!, exact in a u128 (31! < 2^103).
    let mut f = p as u128;
    for kk in 1..=p {
        let scale = if f < 1 << 53 {
            of(f as f64)
        } else {
            let x = f as f64;
            Ivl::outward_ulp(x, x)
        };
        for j in 0..=p {
            ders.set(kk, j, ders.at(kk, j) * scale);
        }
        f *= (p - kk) as u128;
    }
}

#[cfg(test)]
mod tests;

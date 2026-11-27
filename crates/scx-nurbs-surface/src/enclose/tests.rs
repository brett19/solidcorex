// Written for solidcorex. Apache-2.0.
//! The enclosures against the exact partials of `scx-testkit`, at points of each box.

use super::*;
use crate::Net;
use scx_num::error::ErrorKind;
use scx_testkit::data::SurfaceData;
use scx_testkit::nurbs as tk;
use scx_testkit::rat::Rat;

fn build(d: &SurfaceData<3>) -> NurbsSurface<3> {
    let (p, q) = d.degrees;
    let u = KnotVector::new(p, d.knots_u.clone()).unwrap();
    let v = KnotVector::new(q, d.knots_v.clone()).unwrap();
    let cols = d.knots_v.len() - q - 1;
    let rows = d.points.len() / cols;
    let net = Net::new(rows, cols, d.points.clone()).unwrap();
    let w = d
        .weights
        .as_ref()
        .map(|w| Net::new(rows, cols, w.clone()).unwrap());
    NurbsSurface::new(u, v, net, w).unwrap()
}

/// A biquadratic-by-cubic patch with an interior knot each way (`eval`'s), moved far from
/// the origin when `far`.
fn data(rational: bool, far: bool) -> SurfaceData<3> {
    let knots_u = vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0];
    let knots_v = vec![0.0, 0.0, 0.0, 0.0, 0.25, 1.0, 1.0, 1.0, 1.0];
    let (rows, cols) = (4, 5);
    let shift = if far { 1000.0 } else { 0.0 };
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for i in 0..rows {
        for j in 0..cols {
            let (x, y) = (f64::from(i), f64::from(j));
            points.push([
                x + shift,
                y - shift,
                0.25 * x * y - 0.5 * y + f64::from((i + j) % 3),
            ]);
            weights.push(1.0 + 0.5 * f64::from((i * 3 + j) % 4));
        }
    }
    SurfaceData {
        degrees: (2, 3),
        knots_u,
        knots_v,
        points,
        weights: rational.then_some(weights),
    }
}

/// A Bezier patch of degree `(p, 1)` (`p` past 18 scales by an inexact factorial).
fn bezier(p: usize, rational: bool) -> SurfaceData<3> {
    let knots_u = [vec![0.0; p + 1], vec![1.0; p + 1]].concat();
    let knots_v = vec![0.0, 0.0, 1.0, 1.0];
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for i in 0..=p {
        for j in 0..2 {
            let (x, y) = (i as f64, j as f64);
            points.push([x * 0.1, y + 0.01 * x * x, ((i * 7 + j * 3) % 5) as f64]);
            weights.push(1.0 + 0.25 * f64::from(((i + j) % 3) as u32));
        }
    }
    SurfaceData {
        degrees: (p, 1),
        knots_u,
        knots_v,
        points,
        weights: rational.then_some(weights),
    }
}

fn holds(i: Ivl, x: &Rat) -> bool {
    let lo = Rat::from_f64(i.lo).is_none_or(|lo| lo <= *x);
    let hi = Rat::from_f64(i.hi).is_none_or(|hi| *x <= hi);
    lo && hi && !(i.lo.is_nan() || i.hi.is_nan())
}

/// Points of `[lo, hi]`: the ends, the middle and two between.
fn samples(t: Ivl) -> Vec<f64> {
    let mut s = vec![t.lo, t.hi];
    for f in [0.5, 0.125, 0.875] {
        s.push(t.lo + f * (t.hi - t.lo));
    }
    s.retain(|x| t.lo <= *x && *x <= t.hi);
    s.dedup();
    s
}

/// The sides at which the enclosure over `t` must hold the partials at `x`: its own side at
/// an end of a range of nonzero width, both elsewhere.
fn sides(t: Ivl, x: f64) -> Vec<tk::Side> {
    if t.lo < t.hi && x == t.lo {
        vec![tk::Side::Above]
    } else if t.lo < t.hi && x == t.hi {
        vec![tk::Side::Below]
    } else {
        vec![tk::Side::Below, tk::Side::Above]
    }
}

/// Every partial of `jet` holds the exact partial at every sample of the box, less `o`.
fn check_holds<const N: usize>(
    t: &tk::Surface<3>,
    jet: &SurfaceJet<[Ivl; 3], N>,
    [u, v]: [Ivl; 2],
    o: [f64; 3],
) {
    for x in samples(u) {
        for y in samples(v) {
            for su in sides(u, x) {
                for sv in sides(v, y) {
                    let want = t.derivs((x, y), (su, sv), N).unwrap();
                    for a in 0..=N {
                        for b in 0..=N - a {
                            let w = &want.get(a, b).unwrap().exact;
                            let g = jet.get(a, b).unwrap();
                            for e in 0..3 {
                                let we = if (a, b) == (0, 0) {
                                    &w[e] - &Rat::from_f64(o[e]).unwrap()
                                } else {
                                    w[e].clone()
                                };
                                assert!(
                                    holds(g[e], &we),
                                    "({x}, {y}) {su:?} {sv:?} ({a},{b})[{e}]: {:?} {}",
                                    g[e],
                                    we.to_f64()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Boxes narrow and wide, inside a span, across knots, at knots and at the ends.
fn boxes() -> Vec<[Ivl; 2]> {
    let r = |a: f64, b: f64| Ivl::new(a, b);
    vec![
        [r(0.1, 0.2), r(0.3, 0.4)],
        [r(0.3, 0.3), r(0.6, 0.6)],
        [r(0.5, 0.5), r(0.25, 0.25)],
        [r(0.0, 0.0), r(1.0, 1.0)],
        [r(0.45, 0.55), r(0.2, 0.3)],
        [r(0.0, 1.0), r(0.0, 1.0)],
        [r(0.5, 0.75), r(0.25, 0.5)],
        [r(0.25, 0.5), r(0.0, 0.25)],
        [r(0.4999, 0.5001), r(0.9, 1.0)],
        [r(0.3, 0.3000001), r(0.7, 0.7)],
    ]
}

#[test]
fn over_holds_the_exact_partials() {
    for rational in [false, true] {
        for far in [false, true] {
            let d = data(rational, far);
            let (s, t) = (build(&d), d.exact().unwrap());
            let o = s.point([0.3, 0.4], [Approach::Above; 2]).unwrap();
            for uv in boxes() {
                for about in [About::Origin, About::Point(o), About::Middle] {
                    let off = if let About::Point(o) = about {
                        o
                    } else {
                        [0.0; 3]
                    };
                    let Ok(j) = s.partials_over::<3>(uv, 0, about) else {
                        // the widest box may not prove the weights positive
                        assert!(rational && uv[0].hi - uv[0].lo == 1.0, "{uv:?}");
                        continue;
                    };
                    check_holds(&t, &j, uv, off);
                    let j0 = s.partials_over::<0>(uv, 0, about).unwrap();
                    assert_eq!(j0.point(), j.point(), "the order does not change a partial");
                }
            }
        }
    }
}

#[test]
fn a_point_box_is_tight() {
    for rational in [false, true] {
        let s = build(&data(rational, false));
        for uv in [[0.3, 0.6], [0.7, 0.1]] {
            let at = uv.map(Ivl::from);
            let jet = s.jet::<2>(uv, [Approach::Above; 2]).unwrap();
            let enc = s.partials_over::<2>(at, 0, About::Origin).unwrap();
            let scale = jet
                .iter()
                .flat_map(|(_, w)| w)
                .fold(1.0_f64, |m, x| m.max(x.abs()));
            for ((ab, g), (_, w)) in enc.iter().zip(jet.iter()) {
                for (g, w) in g.iter().zip(w) {
                    assert!(g.lo <= w && w <= g.hi, "{ab:?}");
                    assert!(g.hi - g.lo <= 1e-11 * scale, "{ab:?} {g:?} {scale}");
                }
            }
        }
    }
}

/// The middle and a point near the box narrow a far surface's rational enclosure.
#[test]
fn about_a_near_point_is_narrower_far_away() {
    let s = build(&data(true, true));
    let uv = [Ivl::new(0.1, 0.2), Ivl::new(0.3, 0.4)];
    let width = |a: About<3>| {
        let j = s.partials_over::<2>(uv, 0, a).unwrap();
        let d = j.get(1, 0).unwrap();
        d.iter().map(|x| x.hi - x.lo).fold(0.0, f64::max)
    };
    let o = s.point([0.15, 0.35], [Approach::Above; 2]).unwrap();
    assert!(width(About::Point(o)) < 0.1 * width(About::Origin));
    assert!(width(About::Middle) < 0.1 * width(About::Origin));
}

/// The piece form over a box inside its span is the box form's bits, and continued past
/// the span it still holds the piece's own polynomial where it is the surface.
#[test]
fn piece_is_over_inside_a_span() {
    for rational in [false, true] {
        let d = data(rational, false);
        let (s, t) = (build(&d), d.exact().unwrap());
        let uv = [Ivl::new(0.1, 0.2), Ivl::new(0.3, 0.4)];
        for about in [About::Origin, About::Middle] {
            let a = s.partials_over_piece::<2>([2, 4], uv, about).unwrap();
            assert_eq!(a, s.partials_over::<2>(uv, 0, about).unwrap());
        }
        // the first u piece over [0.25, 0.75]: the surface on [0.25, 0.5]
        let wide = [Ivl::new(0.25, 0.75), Ivl::new(0.3, 0.4)];
        let j = s
            .partials_over_piece::<2>([2, 4], wide, About::Origin)
            .unwrap();
        check_holds(&t, &j, [Ivl::new(0.25, 0.5), wide[1]], [0.0; 3]);
    }
}

/// Past degree 18 the factorial scale is inexact in doubles and enclosed outward; the
/// enclosure still holds. The scratch is on the heap there.
#[test]
fn high_degrees_hold() {
    for p in [5, 19, 25] {
        for rational in [false, true] {
            let d = bezier(p, rational);
            let (s, t) = (build(&d), d.exact().unwrap());
            assert_eq!(Layout::new(p, 1, 3).total > STACK, p > 7);
            for uv in [
                [Ivl::new(0.3, 0.3), Ivl::new(0.5, 0.5)],
                [Ivl::new(0.4, 0.45), Ivl::new(0.0, 1.0)],
            ] {
                let j = s.partials_over::<3>(uv, 0, About::Middle).unwrap();
                check_holds(&t, &j, uv, [0.0; 3]);
            }
        }
    }
}

#[test]
fn refusals() {
    let s = build(&data(true, false));
    let uv = [Ivl::new(0.1, 0.2), Ivl::new(0.3, 0.4)];
    let cause = |e: NurbsSurfaceError| match e {
        NurbsSurfaceError::Enclosure(c) => c,
        e => panic!("{e:?}"),
    };
    assert!(matches!(
        s.partials_over::<{ MAX_ORDER + 1 }>(uv, 0, About::Origin),
        Err(NurbsSurfaceError::Basis {
            cause: NurbsError::DerivativeOrder { .. },
            ..
        })
    ));
    assert!(matches!(
        s.partials_over_piece::<{ MAX_ORDER + 1 }>([2, 3], uv, About::Origin),
        Err(NurbsSurfaceError::Basis { .. })
    ));
    // spans: below p, empty, past the net; u before v
    for (span, axis, bad) in [
        ([1, 3], ParamAxis::U, 1),
        ([2, 2], ParamAxis::V, 2),
        ([4, 3], ParamAxis::U, 4),
        ([2, 5], ParamAxis::V, 5),
        ([0, 0], ParamAxis::U, 0),
    ] {
        let e = s
            .partials_over_piece::<1>(span, uv, About::Origin)
            .unwrap_err();
        assert_eq!(
            cause(e),
            EnclosureCause::NotAPiece { axis, span: bad },
            "{span:?}"
        );
        assert_eq!(e.kind(), ErrorKind::Refused);
    }
    // ranges not finite or reversed, in both forms
    for (bad, axis) in [
        ([Ivl::new(0.2, 0.1), uv[1]], ParamAxis::U),
        ([uv[0], Ivl::new(f64::NAN, 0.5)], ParamAxis::V),
        ([uv[0], Ivl::new(0.5, f64::INFINITY)], ParamAxis::V),
    ] {
        for e in [
            s.partials_over_piece::<1>([2, 3], bad, About::Origin)
                .unwrap_err(),
            s.partials_over::<1>(bad, 0, About::Origin).unwrap_err(),
        ] {
            assert!(
                matches!(cause(e), EnclosureCause::BoxNotFinite { axis: a, .. } if a == axis),
                "{e:?}"
            );
        }
    }
    // outside the domain
    let e = s
        .partials_over::<1>([uv[0], Ivl::new(0.5, 1.5)], 0, About::Origin)
        .unwrap_err();
    assert_eq!(
        cause(e),
        EnclosureCause::BoxOutside {
            axis: ParamAxis::V,
            range: [0.5, 1.5],
            domain: [0.0, 1.0],
        }
    );
    // the u knot 0.5 is C1 (degree 2, multiplicity 1), the v knot 0.25 C2
    let across = [Ivl::new(0.4, 0.6), Ivl::new(0.2, 0.3)];
    assert!(s.partials_over::<1>(across, 1, About::Origin).is_ok());
    let e = s.partials_over::<1>(across, 2, About::Origin).unwrap_err();
    assert_eq!(
        cause(e),
        EnclosureCause::BelowContinuity {
            axis: ParamAxis::U,
            knot: 0.5,
            continuity: 1,
            needed: 2,
        }
    );
    let e = s
        .partials_over::<1>([Ivl::new(0.4, 0.45), across[1]], 3, About::Origin)
        .unwrap_err();
    assert!(matches!(
        cause(e),
        EnclosureCause::BelowContinuity {
            axis: ParamAxis::V,
            continuity: 2,
            ..
        }
    ));
    // a knot at the end of a range is not crossed
    assert!(
        s.partials_over::<1>([Ivl::new(0.5, 0.6), Ivl::new(0.0, 0.25)], 9, About::Origin)
            .is_ok()
    );
}

/// Weights 1000 at the ends of a row of degree 9 and 0.001 between, over the whole
/// domain: the weight at the middle is small against its derivatives, and the form cannot
/// prove it positive.
#[test]
fn weights_not_proved_positive() {
    let mut d = bezier(9, true);
    for (k, w) in d.weights.as_mut().unwrap().iter_mut().enumerate() {
        *w = if k / 2 == 0 || k / 2 == 9 {
            1000.0
        } else {
            0.001
        };
    }
    let s = build(&d);
    let e = s
        .partials_over::<1>([Ivl::new(0.0, 1.0), Ivl::new(0.0, 1.0)], 0, About::Origin)
        .unwrap_err();
    assert!(
        matches!(
            e,
            NurbsSurfaceError::Enclosure(EnclosureCause::WeightsNotPositive { .. })
        ),
        "{e:?}"
    );
    assert_eq!(e.kind(), ErrorKind::Unsupported);
    // a point box proves them
    assert!(
        s.partials_over::<1>([Ivl::from(0.5), Ivl::from(0.5)], 0, About::Origin)
            .is_ok()
    );
}

/// The cuts of a range: the parts between the knots strictly inside, their spans, and both
/// spans for a point on an inner knot.
#[test]
fn cuts_of_ranges() {
    let k = KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.25, 0.5, 0.5, 1.0, 1.0, 1.0]).unwrap();
    let parts = |lo: f64, hi: f64| -> Vec<([f64; 2], usize)> {
        cuts(ParamAxis::U, &k, Ivl::new(lo, hi), 0)
            .unwrap()
            .map(|(t, s)| ([t.lo, t.hi], s))
            .collect()
    };
    assert_eq!(
        parts(0.0, 1.0),
        [([0.0, 0.25], 2), ([0.25, 0.5], 3), ([0.5, 1.0], 5)]
    );
    assert_eq!(parts(0.25, 0.5), [([0.25, 0.5], 3)]);
    assert_eq!(parts(0.1, 0.3), [([0.1, 0.25], 2), ([0.25, 0.3], 3)]);
    assert_eq!(parts(0.5, 0.5), [([0.5, 0.5], 3), ([0.5, 0.5], 5)]);
    assert_eq!(parts(0.0, 0.0), [([0.0, 0.0], 2)]);
    assert_eq!(parts(1.0, 1.0), [([1.0, 1.0], 5)]);
    assert_eq!(parts(0.3, 0.3), [([0.3, 0.3], 3)]);
    assert!(cuts(ParamAxis::U, &k, Ivl::new(0.1, 0.6), 1).is_err());
    assert!(cuts(ParamAxis::U, &k, Ivl::new(0.1, 0.3), 1).is_ok());
}

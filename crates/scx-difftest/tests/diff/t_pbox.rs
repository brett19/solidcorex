// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `pseudo_box.rs` (and `Extent3d::circumscribe`).
//!
//! The two sides are written out: scx's checked `BoxBasis` and immutable `PseudoBox`
//! against the pin's `set_basis`, `set_intervals` and `_into` forms. A box is built on both
//! sides from the same numbers; where scx refuses the basis the pin's own statuses refuse it
//! too (`set_basis` and `invert_absolute`), so both are `None` (`pb_basis` compares that
//! decision). Results are compared as plain numbers: basis vectors, intervals, points,
//! the line hit as its tag and parameters, an axis map as upstream's `±1, ±2, ±3`.
//!
//! The pin's flatness tests read `tol == SM_EFF_ZERO` as "scaled by the box"; scx's take an
//! explicit tolerance, given `relative_zero()` for that input.

use crate::diff;
use crate::facade::{s as sf, x as xf};
use crate::harness::{Gen, List, SplitMix64};

use scx_algo::pseudo_box as xp;
use scx_num::vector3d::{Axis, UnitVec3};
use solidcore::geom::pseudo_box as sp;

type V = [f64; 3];
type XV = scx_num::vector3d::Vec3;
type XP = scx_num::vector3d::Point3;
type SV = solidcore::geom::vector3d::Vec3;

/// A box: three basis vectors and an interval `[lo, hi]` along each.
#[derive(Clone, Debug)]
pub struct BoxR {
    pub b: [V; 3],
    pub i: [[f64; 2]; 3],
}

/// The pin's unbounded end, `SM_INFINITE_PARAMETER`: no extent on either side (an extent's ends
/// are finite, [`crate::facade::plain_ends`]), so a box with one is `None` on both.
const BIG: f64 = solidcore::math::INFINITE_PARAMETER;

fn sorted(a: f64, b: f64) -> [f64; 2] {
    if a <= b { [a, b] } else { [b, a] }
}

impl Gen for BoxR {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let ax = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let unit = [[0.0, 1.0]; 3];
        let c = 0.5_f64.sqrt();
        let mut v = vec![
            BoxR { b: ax, i: unit },
            BoxR {
                b: ax,
                i: [[-1.0, 2.0], [0.0, 0.0], [3.0, 5.0]],
            },
            // rotated 45 degrees about z
            BoxR {
                b: [[c, c, 0.0], [-c, c, 0.0], [0.0, 0.0, 1.0]],
                i: [[0.0, 2.0], [-1.0, 1.0], [0.0, 3.0]],
            },
            // skew
            BoxR {
                b: [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.3, 0.2, 1.0]],
                i: [[0.0, 1.0], [0.0, 2.0], [-1.0, 1.0]],
            },
            // left-handed
            BoxR {
                b: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
                i: [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]],
            },
            // reversed axes
            BoxR {
                b: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                i: [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]],
            },
            // unnormalised vectors
            BoxR {
                b: [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 0.5]],
                i: unit,
            },
            // nearly dependent: refused
            BoxR {
                b: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 1e-13]],
                i: unit,
            },
            // point sized, linear, planar
            BoxR {
                b: ax,
                i: [[1.0, 1.0 + 1e-15], [2.0, 2.0], [3.0, 3.0]],
            },
            BoxR {
                b: [[c, c, 0.0], [-c, c, 0.0], [0.0, 0.0, 1.0]],
                i: [[0.0, 5.0], [1.0, 1.0], [2.0, 2.0 + 1e-14]],
            },
            BoxR {
                b: ax,
                i: [[0.0, 5.0], [1.0, 1.0], [2.0, 4.0]],
            },
            BoxR {
                b: ax,
                i: [[0.0, 0.0], [1.0, 3.0], [2.0, 4.0]],
            },
            // the pin's unbounded and partly unbounded boxes: no box on either side
            BoxR {
                b: ax,
                i: [[-BIG, BIG]; 3],
            },
            BoxR {
                b: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                i: [[-BIG, BIG], [0.0, 1.0], [0.0, 1.0]],
            },
            BoxR {
                b: [[c, c, 0.0], [-c, c, 0.0], [0.0, 0.0, 1.0]],
                i: [[-BIG, BIG], [0.0, 1.0], [0.0, 1.0]],
            },
        ];
        for _ in 0..60 {
            let mut b = [[0.0; 3]; 3];
            for r in &mut b {
                for x in r.iter_mut() {
                    *x = rng.uniform(-10.0, 10.0);
                }
            }
            let mut i = [[0.0; 2]; 3];
            for s in &mut i {
                let lo = rng.uniform(-10.0, 10.0);
                // a quarter flat
                let hi = if rng.below(4) == 0 {
                    lo
                } else {
                    rng.uniform(-10.0, 10.0)
                };
                *s = sorted(lo, hi);
            }
            v.push(BoxR { b, i });
        }
        // random orthogonal frames
        for _ in 0..20 {
            let z = [
                rng.uniform(-1.0, 1.0),
                rng.uniform(-1.0, 1.0),
                rng.uniform(-1.0, 1.0),
            ];
            if let Ok([a, b, c]) = xv(z).make_unit_ortho_vectors(None) {
                let f = |u: UnitVec3| [u.x, u.y, u.z];
                let mut i = [[0.0; 2]; 3];
                for s in &mut i {
                    *s = sorted(rng.uniform(-5.0, 5.0), rng.uniform(-5.0, 5.0));
                }
                v.push(BoxR {
                    b: [f(a), f(b), f(c)],
                    i,
                });
            }
        }
        v
    }
}

/// A projection: `kind` 0 parallel, 1 perspective, 2 rotation; a point, a direction
/// (normalised for both sides) and the eye or the rotation's reference.
#[derive(Clone, Debug)]
pub struct ProjR {
    pub kind: usize,
    pub p: V,
    pub n: V,
    pub e: V,
}

impl Gen for ProjR {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = Vec::new();
        for kind in 0..3 {
            v.push(ProjR {
                kind,
                p: [0.0; 3],
                n: [0.0, 0.0, 1.0],
                e: [0.0, 0.0, 10.0],
            });
            v.push(ProjR {
                kind,
                p: [1.0, 2.0, 3.0],
                n: [1.0, 1.0, 0.0],
                e: [0.0, 1.0, 20.0],
            });
            v.push(ProjR {
                kind,
                p: [0.0; 3],
                n: [0.0, 1.0, 0.0],
                e: [1.0, 0.0, 0.0],
            });
            // a zero direction: no projection
            v.push(ProjR {
                kind,
                p: [0.0; 3],
                n: [0.0; 3],
                e: [1.0, 0.0, 0.0],
            });
        }
        // eyes on the view plane
        v.push(ProjR {
            kind: 1,
            p: [0.0; 3],
            n: [0.0, 0.0, 1.0],
            e: [0.5, 0.5, 0.0],
        });
        v.push(ProjR {
            kind: 1,
            p: [0.0; 3],
            n: [0.0, 0.0, 1.0],
            e: [0.0; 3],
        });
        for _ in 0..30 {
            let mut r = || {
                [
                    rng.uniform(-10.0, 10.0),
                    rng.uniform(-10.0, 10.0),
                    rng.uniform(-10.0, 10.0),
                ]
            };
            let (p, n, e) = (r(), r(), r());
            v.push(ProjR {
                kind: rng.below(3),
                p,
                n,
                e,
            });
        }
        v
    }
}

fn xv(a: V) -> XV {
    XV::new(a[0], a[1], a[2])
}
fn xpt(a: V) -> XP {
    XP::new(a[0], a[1], a[2])
}
fn sv(a: V) -> SV {
    SV::new(a[0], a[1], a[2])
}

/// The projection's numbers once normalised: (point, unit direction, eye or `x` axis).
fn proj_parts(r: &ProjR) -> Option<(V, V, V)> {
    let n = xv(r.n).normalized()?;
    let third = if r.kind == 2 {
        // the rotation's `x` axis: square to the axis, towards `e`
        let [_, x, _] = n.make_unit_ortho_vectors(Some(xv(r.e))).ok()?;
        [x.x, x.y, x.z]
    } else {
        r.e
    };
    Some((r.p, [n.x, n.y, n.z], third))
}

fn xproj(r: &ProjR) -> Option<xp::Projection> {
    // the unit vectors as `proj_parts` makes them (normalising again would round again)
    let n = xv(r.n).normalized()?;
    Some(match r.kind {
        0 => xp::Projection::Parallel {
            point: xpt(r.p),
            normal: n,
        },
        1 => xp::Projection::Perspective {
            point: xpt(r.p),
            normal: n,
            eye: xpt(r.e),
        },
        _ => {
            let [_, x, _] = n.make_unit_ortho_vectors(Some(xv(r.e))).ok()?;
            xp::Projection::Rotation {
                point: xpt(r.p),
                axis: n,
                x_axis: x,
            }
        }
    })
}

fn sproj(r: &ProjR) -> Option<sp::Projection> {
    let (p, n, t) = proj_parts(r)?;
    Some(match r.kind {
        0 => sp::Projection::Parallel {
            point: sv(p),
            normal: sv(n),
        },
        1 => sp::Projection::Perspective {
            point: sv(p),
            normal: sv(n),
            eye: sv(t),
        },
        _ => sp::Projection::Rotation {
            point: sv(p),
            axis: sv(n),
            x_axis: sv(t),
        },
    })
}

pub(crate) fn xbasis(b: &[V; 3]) -> Option<xp::BoxBasis> {
    xp::BoxBasis::new(xv(b[0]), xv(b[1]), xv(b[2]))
}

/// The pin's box of these vectors, when its own statuses accept the basis: `set_basis`
/// and `invert_absolute`'s determinant test.
fn sbasis(b: &[V; 3]) -> Option<sp::PseudoBox> {
    let mut pb = sp::PseudoBox::new();
    pb.set_basis(sv(b[0]), sv(b[1]), sv(b[2])).ok()?;
    pb.invert_absolute(SV::ZERO).ok()?;
    Some(pb)
}

pub(crate) fn xbox(r: &BoxR) -> Option<xp::PseudoBox> {
    let basis = xbasis(&r.b)?;
    let [a, b, c] = r.i.map(xf::e1);
    Some(xp::PseudoBox::new(basis, [a?, b?, c?]))
}

fn sbox(r: &BoxR) -> Option<sp::PseudoBox> {
    let mut pb = sbasis(&r.b)?;
    let [a, b, c] = r.i.map(sf::e1);
    pb.set_intervals(a?, b?, c?);
    Some(pb)
}

type XIv = scx_num::extent1d::Extent1d;
type SIv = solidcore::geom::extent1d::Extent1d;

pub(crate) fn xparts(b: &xp::PseudoBox) -> ([XV; 3], [XIv; 3]) {
    (b.basis().vectors(), *b.intervals())
}
pub(crate) fn sparts(b: &sp::PseudoBox) -> ([SV; 3], [SIv; 3]) {
    (*b.basis(), *b.intervals())
}

fn xhit(h: scx_num::extent3d::LineHit) -> (&'static str, Vec<f64>) {
    use scx_num::extent3d::LineHit as H;
    match h {
        H::Miss => ("Miss", vec![]),
        H::Touch { t } => ("Touch", vec![t]),
        H::Through { enter, exit } => ("Through", vec![enter, exit]),
    }
}
fn shit(h: sp::LineHit) -> (&'static str, Vec<f64>) {
    match h {
        sp::LineHit::Miss => ("Miss", vec![]),
        sp::LineHit::Graze(t) => ("Touch", vec![t]),
        sp::LineHit::Segment(a, b) => ("Through", vec![a, b]),
    }
}

/// A signed axis as upstream's `±1, ±2, ±3`.
fn xmap(m: Option<[xp::SignedAxis; 3]>) -> Option<[i32; 3]> {
    m.map(|m| {
        m.map(|s| {
            let k = match s.axis {
                Axis::X => 1,
                Axis::Y => 2,
                Axis::Z => 3,
            };
            if s.reversed { -k } else { k }
        })
    })
}

fn xplanes(b: &xp::PseudoBox) -> Vec<(XP, XV)> {
    b.planes().iter().map(|p| (p.point, *p.normal)).collect()
}
fn splanes(b: &sp::PseudoBox) -> Vec<(SV, SV)> {
    let (p, n) = b.calc_planes();
    p.into_iter().zip(n).collect()
}

/// The pin's tolerance for its flatness tests, as scx is given it.
fn xtol(b: &xp::PseudoBox, tol: f64) -> f64 {
    if tol == scx_num::math::EFF_ZERO {
        b.relative_zero()
    } else {
        tol
    }
}

diff!(
    pb_basis,
    pin_defect = crate::pin_defects::pb_basis,
    |a: V, b: V, c: V| xbasis(&[a, b, c]).map(|b| (b.vectors(), b.is_orthogonal())),
    sbasis(&[a, b, c]).map(|b| (*b.basis(), b.is_orthogonal_basis()))
);
diff!(
    pb_basis_boxes,
    |r: BoxR| xbasis(&r.b).map(|b| (b.vectors(), b.is_orthogonal())),
    sbasis(&r.b).map(|b| (*b.basis(), b.is_orthogonal_basis()))
);
diff!(
    pb_coordinates,
    |r: BoxR, p: V| xbasis(&r.b).map(|b| b.coordinates(xpt(p))),
    sbasis(&r.b).and_then(|b| b.invert_absolute(sv(p)).map(|a| [a.x, a.y, a.z]).ok())
);
diff!(
    pb_point_queries,
    |r: BoxR, p: V, t: f64| xbox(&r).map(|b| (
        b.contains_point(xpt(p), t),
        b.classify_point(xpt(p), t),
        b.distance_to_point(xpt(p)),
    )),
    sbox(&r).map(|b| (
        b.contains_point(sv(p), t),
        b.classify_point(sv(p), t),
        b.distance_to_point(sv(p)),
    ))
);
diff!(
    pb_evaluate,
    |r: BoxR, n: V| xbox(&r).map(|b| (
        b.evaluate(n[0], n[1], n[2]),
        b.corners(),
        b.edges()
            .iter()
            .flat_map(|&(a, c)| [a, c])
            .collect::<Vec<_>>(),
        xplanes(&b),
    )),
    sbox(&r).map(|b| (
        b.evaluate(n[0], n[1], n[2]),
        b.corners(),
        b.edges().to_vec(),
        splanes(&b),
    ))
);
diff!(
    pb_intersect_line,
    pin_defect = crate::pin_defects::pb_intersect_line,
    |r: BoxR, p: V, v: V| xbox(&r).map(|b| b.intersect_line(xpt(p), xv(v)).map(xhit)),
    sbox(&r).map(|b| b.intersect_line(sv(p), sv(v)).map(shit))
);
diff!(
    pb_measures,
    |r: BoxR, t: f64| xbox(&r).map(|b| {
        let tol = xtol(&b, t);
        (
            (b.max_dimension(), b.xy_area(), b.volume()),
            scx_num::tol::ScaledZero::scaled_zero(&b),
            b.point_sized(tol).map(|s| (s.center, s.deviation)),
            b.linear(tol).map(|s| (s.point, s.tangent, s.deviation)),
            b.planar(tol).map(|s| (s.point, s.normal, s.deviation)),
        )
    }),
    sbox(&r).map(|b| {
        let ps = b.is_point_sized(t);
        let li = b.is_linear(t);
        let pl = b.is_planar(t);
        (
            (b.max_dimension(), b.xy_area(), b.volume()),
            solidcore::geom::tol::ScaledZero::scaled_zero(&b),
            ps.is_point_sized.then_some((ps.center, ps.deviation)),
            li.is_linear.then_some((li.point, li.tangent, li.deviation)),
            pl.is_planar.then_some((pl.point, pl.normal, pl.deviation)),
        )
    })
);
diff!(
    pb_pair,
    |r: BoxR, q: BoxR, t: f64| xbox(&r).zip(xbox(&q)).map(|(a, b)| (
        a.are_disjoint(&b),
        a.is_contained_by(&b, t),
        xmap(a.alignment_with(&b)),
    )),
    sbox(&r).zip(sbox(&q)).map(|(a, b)| (
        a.are_disjoint(&b, None),
        a.is_contained_by(&b, t),
        a.are_aligned(&b),
    ))
);
diff!(
    pb_frame,
    |r: BoxR| xbox(&r).map(|b| (xmap(b.axis_map()), b.circumscribing_extent(),)),
    sbox(&r).map(|b| {
        let mut e = solidcore::geom::extent3d::Extent3d::new();
        e.circumscribe(&b);
        (b.is_axis_aligned(), crate::pin_extent::finite3(e))
    })
);
diff!(
    pb_bounding,
    pin_defect = crate::pin_defects::pb_bounding,
    |r: BoxR, pts: List<V>| xbasis(&r.b).map(|basis| {
        xp::PseudoBox::bounding(basis, pts.0.iter().map(|&p| xpt(p))).map(|b| xparts(&b))
    }),
    sbasis(&r.b).map(|basis| {
        let mut b = basis;
        b.init();
        for &p in &pts.0 {
            b.add_point(sv(p));
        }
        (!b.has_negative_volume()).then(|| sparts(&b))
    })
);
diff!(
    pb_expand_union,
    pin_defect = crate::pin_defects::pb_expand_union,
    |r: BoxR, q: BoxR, d: f64| xbox(&r).zip(xbasis(&q.b)).map(|(a, basis)| {
        a.expanded(d.abs())
            .map(|ex| (a.union(&ex, basis).map(|b| xparts(&b)), xparts(&ex)))
    }),
    sbox(&r).zip(sbasis(&q.b)).map(|(a, basis)| {
        let mut ex = a;
        ex.expand_absolute(d.abs());
        // a box grown past the finite range is no box
        if ex
            .intervals()
            .iter()
            .any(|i| crate::pin_extent::finite1(*i).is_none())
        {
            return None;
        }
        let mut u = basis;
        u.init();
        a.union_into(&ex, basis.basis(), &mut u);
        let finite = u
            .intervals()
            .iter()
            .all(|i| crate::pin_extent::finite1(*i).is_some());
        Some((
            (!u.has_negative_volume() && finite).then(|| sparts(&u)),
            sparts(&ex),
        ))
    })
);
diff!(
    pb_with_interval,
    |r: BoxR, i: [f64; 2], k: usize| {
        let axis = [Axis::X, Axis::Y, Axis::Z][k % 3];
        xbox(&r)
            .zip(xf::e1(i))
            .map(|(a, i)| xparts(&a.with_interval(axis, i)))
    },
    sbox(&r).zip(sf::e1(i)).map(|(a, i)| {
        let mut w = a;
        let _ = w.set_interval(k % 3, i);
        sparts(&w)
    })
);
diff!(
    pb_from_extent3d,
    |a: [f64; 6]| xf::e3(a).map(|e| xparts(&xp::PseudoBox::from_extent3d(&e))),
    sf::e3(a).map(|e| sparts(&sp::PseudoBox::from_extent3d(&e)))
);
diff!(
    pb_project,
    pin_defect = crate::pin_defects::pb_project,
    |r: BoxR, p: ProjR| xbox(&r)
        .zip(xproj(&p))
        .map(|(b, pr)| b.project_to_plane(&pr).map(|b| xparts(&b)).ok()),
    sbox(&r)
        .zip(sproj(&p))
        .map(|(b, pr)| b.project_to_plane(&pr).map(|b| sparts(&b)).ok())
);
diff!(
    pb_project_disjoint,
    pin_defect = crate::pin_defects::pb_project_disjoint,
    |r: BoxR, q: BoxR, p: ProjR| xbox(&r)
        .zip(xbox(&q))
        .zip(xproj(&p))
        .map(|((a, b), pr)| a.are_project_disjoint(&b, &pr)),
    sbox(&r)
        .zip(sbox(&q))
        .zip(sproj(&p))
        .map(|((a, b), pr)| a.are_project_disjoint(&b, &pr))
);

// Written for solidcorex. Apache-2.0.
//! The Bernstein polynomials' side of the facade (report 53 section 14): the recipes the
//! inputs are drawn from, and `scx_algo::bernstein` and the pin's
//! `offset::construction::bernstein` (with `query::tangent_edge`'s `bernstein_roots`, made
//! public by the build script) behind one set of functions over plain data ([`x`], [`s`]).
//!
//! A polynomial is [`Pl`] (degrees and coefficients), a bounded one [`Bd`] (and its error
//! bound), a refusal [`Er`] (operation and reason). The pin's operations take its binomial
//! table, one shared here; scx's read its own.

use crate::harness::{Gen, SplitMix64, edge_f64};

/// A polynomial: degrees `(du, dv)` and the coefficients, row by row.
pub type Pl = (usize, usize, Vec<f64>);
/// A bounded polynomial: the polynomial and its error bound.
pub type Bd = (Pl, f64);
/// A refusal: operation and reason.
pub type Er = (&'static str, &'static str);
/// An outcome.
pub type R<T> = Result<T, Er>;

/// The degrees the recipes take.
const DEGREES: [(usize, usize); 10] = [
    (0, 0),
    (1, 0),
    (0, 1),
    (1, 1),
    (2, 0),
    (2, 1),
    (1, 3),
    (3, 2),
    (4, 4),
    (5, 2),
];

/// The coefficients of degrees `(du, dv)` in fill `kind`.
fn fill(rng: &mut SplitMix64, du: usize, dv: usize, kind: usize, edge: &[f64]) -> Vec<f64> {
    let n = (du + 1) * (dv + 1);
    match kind {
        0 => (0..n).map(|_| rng.uniform(-2.0, 2.0)).collect(),
        // five decades, both signs
        1 => (0..n)
            .map(|_| {
                let e = rng.below(11) as i32 - 5;
                rng.uniform(-1.0, 1.0) * scx_num::fmath::powi(10.0, e)
            })
            .collect(),
        2 => vec![0.0; n],
        3 => vec![1.0; n],
        // short dyadics: the sign changes and exact zeros the roots and splits meet
        4 => (0..n).map(|_| (rng.below(9) as f64 - 4.0) * 0.25).collect(),
        // one edge value among ordinary ones
        _ => {
            let mut c: Vec<f64> = (0..n).map(|_| rng.uniform(-1.0, 1.0)).collect();
            let k = rng.below(n);
            c[k] = edge[rng.below(edge.len())];
            c
        }
    }
}

/// A polynomial recipe.
#[derive(Clone, Debug)]
pub struct BernR(pub Pl);

impl Gen for BernR {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut edge = edge_f64();
        edge.extend([1e300, -1e300, f64::MAX, f64::INFINITY, f64::NAN]);
        let mut v = Vec::new();
        for (du, dv) in DEGREES {
            for kind in 0..7 {
                v.push(BernR((du, dv, fill(rng, du, dv, kind, &edge))));
            }
        }
        v
    }
}

/// A bounded polynomial recipe.
#[derive(Clone, Debug)]
pub struct BdR(pub Bd);

impl Gen for BdR {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let errs = [0.0, 1e-16, 3e-12, 0.5, f64::INFINITY, f64::NAN];
        BernR::pool(rng)
            .into_iter()
            .enumerate()
            .map(|(i, BernR(p))| BdR((p, errs[i % errs.len()])))
            .collect()
    }
}

/// A recipe of three polynomials.
#[derive(Clone, Debug)]
pub struct V3R(pub [Pl; 3]);

impl Gen for V3R {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let p = BernR::pool(rng);
        (0..60)
            .map(|_| {
                let mut pick = || p[rng.below(p.len())].0.clone();
                V3R([pick(), pick(), pick()])
            })
            .collect()
    }
}

/// Defines the shared surface of both sides over the module `b` and its error `E`, which the
/// caller names; `$b` is how each side passes its binomial table (nothing for scx), `$e`
/// maps its error to [`Er`], and the closures call the operations whose shapes differ.
macro_rules! side {
    (
        bin = [$($b:tt)*],
        err = $e:expr,
        quarter = $quarter:expr,
        roots = $roots:expr,
        eval3 = $eval3:expr,
        v3 = $v3:expr,
        unv3 = $unv3:expr,
        dot = $dot:expr,
        cross = $cross:expr,
        sub3 = $sub3:expr,
        mul3 = $mul3:expr,
        du3 = $du3:expr,
        dv3 = $dv3:expr,
        restrict3 = $restrict3:expr,
        bdot = $bdot:expr,
        bcross = $bcross:expr,
        bsub3 = $bsub3:expr,
        bmul3 = $bmul3:expr,
        bdu3 = $bdu3:expr,
        bdv3 = $bdv3:expr,
        unbounded3 = $unbounded3:expr
    ) => {
        use super::{Bd, Er, Pl, R};
        use b::{Bernstein2, Bounded};

        fn er(e: E) -> Er {
            ($e)(e)
        }

        fn pl(p: &Bernstein2) -> Pl {
            let (du, dv) = p.degrees();
            (du, dv, p.coefficients().to_vec())
        }

        fn mk(p: &Pl) -> R<Bernstein2> {
            Bernstein2::new(p.0, p.1, p.2.clone()).map_err(er)
        }

        fn bd(p: &Bounded) -> Bd {
            (pl(&p.f), p.err)
        }

        fn mkb(p: &Bd) -> R<Bounded> {
            Ok(Bounded {
                f: mk(&p.0)?,
                err: p.1,
            })
        }

        pub fn new(p: &Pl) -> R<Pl> {
            mk(p).map(|p| pl(&p))
        }

        pub fn constant(x: f64) -> Pl {
            pl(&Bernstein2::constant(x))
        }

        /// The smallest, largest and largest magnitude coefficients.
        pub fn extremes(p: &Pl) -> R<[f64; 3]> {
            let p = mk(p)?;
            Ok([
                p.min_coefficient(),
                p.max_coefficient(),
                p.max_abs_coefficient(),
            ])
        }

        pub fn mul(p: &Pl, q: &Pl) -> R<Pl> {
            mk(p)?.mul(&mk(q)? $($b)*).map(|p| pl(&p)).map_err(er)
        }

        pub fn elevate(p: &Pl, du: usize, dv: usize) -> R<Pl> {
            mk(p)?.elevate(du, dv $($b)*).map(|p| pl(&p)).map_err(er)
        }

        pub fn add_scaled(p: &Pl, q: &Pl, x: f64) -> R<Pl> {
            mk(p)?.add_scaled(&mk(q)?, x $($b)*).map(|p| pl(&p)).map_err(er)
        }

        pub fn add(p: &Pl, q: &Pl) -> R<Pl> {
            mk(p)?.add(&mk(q)? $($b)*).map(|p| pl(&p)).map_err(er)
        }

        pub fn sub(p: &Pl, q: &Pl) -> R<Pl> {
            mk(p)?.sub(&mk(q)? $($b)*).map(|p| pl(&p)).map_err(er)
        }

        pub fn scale(p: &Pl, x: f64) -> R<Pl> {
            Ok(pl(&mk(p)?.scale(x)))
        }

        pub fn deriv_u(p: &Pl) -> R<Pl> {
            Ok(pl(&mk(p)?.deriv_u()))
        }

        pub fn deriv_v(p: &Pl) -> R<Pl> {
            Ok(pl(&mk(p)?.deriv_v()))
        }

        pub fn split_u(p: &Pl, t: f64) -> R<(Pl, Pl)> {
            let (l, r) = mk(p)?.split_u(t);
            Ok((pl(&l), pl(&r)))
        }

        pub fn split_v(p: &Pl, t: f64) -> R<(Pl, Pl)> {
            let (l, r) = mk(p)?.split_v(t);
            Ok((pl(&l), pl(&r)))
        }

        pub fn restrict(p: &Pl, u0: f64, u1: f64, v0: f64, v1: f64) -> R<Pl> {
            Ok(pl(&mk(p)?.restrict(u0, u1, v0, v1)))
        }

        pub fn eval(p: &Pl, u: f64, v: f64) -> R<f64> {
            Ok(mk(p)?.eval(u, v))
        }

        pub fn product_rounding(du: usize, dv: usize) -> usize {
            b::product_rounding(du, dv)
        }

        pub fn bmul(p: &Bd, q: &Bd) -> R<Bd> {
            mkb(p)?.mul(&mkb(q)? $($b)*).map(|p| bd(&p)).map_err(er)
        }

        pub fn badd_scaled(p: &Bd, q: &Bd, x: f64) -> R<Bd> {
            mkb(p)?.add_scaled(&mkb(q)?, x $($b)*).map(|p| bd(&p)).map_err(er)
        }

        pub fn badd(p: &Bd, q: &Bd) -> R<Bd> {
            mkb(p)?.add(&mkb(q)? $($b)*).map(|p| bd(&p)).map_err(er)
        }

        pub fn bsub(p: &Bd, q: &Bd) -> R<Bd> {
            mkb(p)?.sub(&mkb(q)? $($b)*).map(|p| bd(&p)).map_err(er)
        }

        pub fn bscale(p: &Bd, x: f64) -> R<Bd> {
            Ok(bd(&mkb(p)?.scale(x)))
        }

        pub fn bderiv_u(p: &Bd) -> R<Bd> {
            Ok(bd(&mkb(p)?.deriv_u()))
        }

        pub fn bderiv_v(p: &Bd) -> R<Bd> {
            Ok(bd(&mkb(p)?.deriv_v()))
        }

        pub fn bfinite(p: &Bd) -> R<bool> {
            Ok(mkb(p)?.is_finite())
        }

        /// The quarter `[1/2, 1]` in a parameter where its flag is set, else `[0, 1/2]`.
        pub fn bquarter(p: &Bd, upper_u: bool, upper_v: bool) -> R<Bd> {
            Ok(bd(&($quarter)(&mkb(p)?, upper_u, upper_v)))
        }

        /// The roots and whether all were isolated.
        pub fn roots(c: &[f64], lo: f64, hi: f64, depth: u32, work: usize) -> (Vec<f64>, bool) {
            ($roots)(c, lo, hi, depth, work)
        }

        fn mk3(p: &[Pl; 3]) -> R<[Bernstein2; 3]> {
            Ok([mk(&p[0])?, mk(&p[1])?, mk(&p[2])?])
        }

        fn mkb3(p: &[Pl; 3], err: f64) -> R<[Bounded; 3]> {
            let [x, y, z] = mk3(p)?;
            Ok([x, y, z].map(|f| Bounded { f, err }))
        }

        fn pl3(p: [Bernstein2; 3]) -> [Pl; 3] {
            p.map(|p| pl(&p))
        }

        fn bd3(p: [Bounded; 3]) -> [Bd; 3] {
            p.map(|p| bd(&p))
        }

        pub fn dot(a: &[Pl; 3], c: &[Pl; 3]) -> R<Pl> {
            ($dot)(&($v3)(mk3(a)?), &($v3)(mk3(c)?)).map(|p| pl(&p)).map_err(er)
        }

        pub fn cross(a: &[Pl; 3], c: &[Pl; 3]) -> R<[Pl; 3]> {
            ($cross)(&($v3)(mk3(a)?), &($v3)(mk3(c)?)).map(|p| pl3(($unv3)(p))).map_err(er)
        }

        pub fn sub3(a: &[Pl; 3], c: &[Pl; 3]) -> R<[Pl; 3]> {
            ($sub3)(&($v3)(mk3(a)?), &($v3)(mk3(c)?)).map(|p| pl3(($unv3)(p))).map_err(er)
        }

        pub fn mul3(a: &[Pl; 3], s: &Pl) -> R<[Pl; 3]> {
            ($mul3)(&($v3)(mk3(a)?), &mk(s)?).map(|p| pl3(($unv3)(p))).map_err(er)
        }

        pub fn deriv_u3(a: &[Pl; 3]) -> R<[Pl; 3]> {
            Ok(pl3(($unv3)(($du3)(&($v3)(mk3(a)?)))))
        }

        pub fn deriv_v3(a: &[Pl; 3]) -> R<[Pl; 3]> {
            Ok(pl3(($unv3)(($dv3)(&($v3)(mk3(a)?)))))
        }

        pub fn restrict3(a: &[Pl; 3], u0: f64, u1: f64, v0: f64, v1: f64) -> R<[Pl; 3]> {
            Ok(pl3(($unv3)(($restrict3)(&($v3)(mk3(a)?), u0, u1, v0, v1))))
        }

        pub fn eval3(a: &[Pl; 3], u: f64, v: f64) -> R<[f64; 3]> {
            Ok(($eval3)(&($v3)(mk3(a)?), u, v))
        }

        pub fn bdot(a: &[Pl; 3], c: &[Pl; 3], err: f64) -> R<Bd> {
            ($bdot)(&($v3)(mkb3(a, err)?), &($v3)(mkb3(c, err)?))
                .map(|p| bd(&p))
                .map_err(er)
        }

        pub fn bcross(a: &[Pl; 3], c: &[Pl; 3], err: f64) -> R<[Bd; 3]> {
            ($bcross)(&($v3)(mkb3(a, err)?), &($v3)(mkb3(c, err)?))
                .map(|p| bd3(($unv3)(p)))
                .map_err(er)
        }

        pub fn bsub3(a: &[Pl; 3], c: &[Pl; 3], err: f64) -> R<[Bd; 3]> {
            ($bsub3)(&($v3)(mkb3(a, err)?), &($v3)(mkb3(c, err)?))
                .map(|p| bd3(($unv3)(p)))
                .map_err(er)
        }

        pub fn bmul3(a: &[Pl; 3], s: &Pl, err: f64) -> R<[Bd; 3]> {
            let s = Bounded { f: mk(s)?, err };
            ($bmul3)(&($v3)(mkb3(a, err)?), &s)
                .map(|p| bd3(($unv3)(p)))
                .map_err(er)
        }

        pub fn bderiv_u3(a: &[Pl; 3], err: f64) -> R<[Bd; 3]> {
            Ok(bd3(($unv3)(($bdu3)(&($v3)(mkb3(a, err)?)))))
        }

        pub fn bderiv_v3(a: &[Pl; 3], err: f64) -> R<[Bd; 3]> {
            Ok(bd3(($unv3)(($bdv3)(&($v3)(mkb3(a, err)?)))))
        }

        pub fn unbounded3(a: &[Pl; 3], err: f64) -> R<[Pl; 3]> {
            Ok(pl3(($unv3)(($unbounded3)(($v3)(mkb3(a, err)?)))))
        }
    };
}

/// scx.
pub mod x {
    use scx_algo::bernstein as b;
    use scx_algo::bernstein::{Bernstein3, Bounded3, Half, Poly3, RootLimits, bernstein_roots};
    use scx_num::NumError;
    type E = NumError;

    fn half(upper: bool) -> Half {
        if upper { Half::Upper } else { Half::Lower }
    }

    fn v3<P>([x, y, z]: [P; 3]) -> Poly3<P> {
        Poly3::new(x, y, z)
    }

    fn unv3<P>(p: Poly3<P>) -> [P; 3] {
        [p.x, p.y, p.z]
    }

    side!(
        bin = [],
        // scx's reason is typed and the pin's is text: an invalid input compares by
        // class and operation only (research 62 T4), as the facade's `num_as_pin`
        err = |e: NumError| match e {
            NumError::InvalidInput { operation, .. } => (operation, "invalid input"),
            _ => ("failed", "failed"),
        },
        quarter = |p: &Bounded, u: bool, v: bool| p.quarter(half(u), half(v)),
        roots = |c: &[f64], lo: f64, hi: f64, depth: u32, work: usize| {
            let r = bernstein_roots(
                c,
                lo,
                hi,
                RootLimits {
                    depth,
                    subdivisions: work,
                },
            );
            (r.places, r.isolated)
        },
        eval3 = |p: &Bernstein3, u: f64, v: f64| {
            let e = p.eval(u, v);
            [e.x, e.y, e.z]
        },
        v3 = v3,
        unv3 = unv3,
        dot = |a: &Bernstein3, c: &Bernstein3| a.dot(c),
        cross = |a: &Bernstein3, c: &Bernstein3| a.cross(c),
        sub3 = |a: &Bernstein3, c: &Bernstein3| a.sub(c),
        mul3 = |a: &Bernstein3, s: &Bernstein2| a.mul_each(s),
        du3 = |a: &Bernstein3| a.deriv_u(),
        dv3 = |a: &Bernstein3| a.deriv_v(),
        restrict3 = |a: &Bernstein3, u0, u1, v0, v1| a.restrict(u0, u1, v0, v1),
        bdot = |a: &Bounded3, c: &Bounded3| a.dot(c),
        bcross = |a: &Bounded3, c: &Bounded3| a.cross(c),
        bsub3 = |a: &Bounded3, c: &Bounded3| a.sub(c),
        bmul3 = |a: &Bounded3, s: &Bounded| a.mul_each(s),
        bdu3 = |a: &Bounded3| Poly3::deriv_u(a),
        bdv3 = |a: &Bounded3| Poly3::deriv_v(a),
        unbounded3 = |a: Bounded3| a.unbounded()
    );

    /// scx's shared binomial table.
    pub fn binomial(n: usize, k: usize) -> f64 {
        scx_algo::bernstein::binomial(n, k)
    }
}

/// The pinned solidcore.
pub mod s {
    #[allow(clippy::all, clippy::pedantic, dead_code)]
    mod pin {
        include!(concat!(env!("OUT_DIR"), "/pin_bernstein_roots.rs"));
    }
    use solidcore::SolidError;
    use solidcore::offset::construction::bernstein as b;
    use solidcore::offset::construction::bernstein::{self as pb, Bernstein3, Binomials, Bounded3};
    use std::sync::OnceLock;
    type E = SolidError;

    /// One binomial table for every call.
    fn bi() -> &'static Binomials {
        static B: OnceLock<Binomials> = OnceLock::new();
        B.get_or_init(Binomials::new)
    }

    fn v3<P>(p: [P; 3]) -> [P; 3] {
        p
    }

    fn unv3<P>(p: [P; 3]) -> [P; 3] {
        p
    }

    side!(
        bin = [, bi()],
        err = |e: SolidError| match e {
            SolidError::InvalidInput { operation, .. } => (operation, "invalid input"),
            _ => ("failed", "failed"),
        },
        quarter = |p: &Bounded, u: bool, v: bool| p.quarter(u, v),
        roots = |c: &[f64], lo: f64, hi: f64, depth: u32, mut work: usize| {
            let mut out = Vec::new();
            let ok = pin::bernstein_roots(c, lo, hi, depth, &mut work, &mut out);
            (out, ok)
        },
        eval3 = |p: &Bernstein3, u: f64, v: f64| {
            let e = pb::eval3(p, u, v);
            [e.x, e.y, e.z]
        },
        v3 = v3,
        unv3 = unv3,
        dot = |a: &Bernstein3, c: &Bernstein3| pb::dot(a, c, bi()),
        cross = |a: &Bernstein3, c: &Bernstein3| pb::cross(a, c, bi()),
        sub3 = |a: &Bernstein3, c: &Bernstein3| pb::sub3(a, c, bi()),
        mul3 = |a: &Bernstein3, s: &Bernstein2| pb::mul3(a, s, bi()),
        du3 = |a: &Bernstein3| pb::deriv_u3(a),
        dv3 = |a: &Bernstein3| pb::deriv_v3(a),
        restrict3 = |a: &Bernstein3, u0, u1, v0, v1| pb::restrict3(a, u0, u1, v0, v1),
        bdot = |a: &Bounded3, c: &Bounded3| pb::dot_bounded(a, c, bi()),
        bcross = |a: &Bounded3, c: &Bounded3| pb::cross_bounded(a, c, bi()),
        bsub3 = |a: &Bounded3, c: &Bounded3| pb::sub3_bounded(a, c, bi()),
        bmul3 = |a: &Bounded3, s: &Bounded| pb::mul3_bounded(a, s, bi()),
        bdu3 = |a: &Bounded3| pb::deriv_u3_bounded(a),
        bdv3 = |a: &Bounded3| pb::deriv_v3_bounded(a),
        unbounded3 = |a: Bounded3| pb::unbounded3(a)
    );

    /// The pin's binomial table.
    pub fn binomial(n: usize, k: usize) -> f64 {
        bi().get(n, k)
    }
}

// Written for solidcorex. Apache-2.0.
//! Two facades with the same module names, `x` (scx-num) and `s` (the pinned
//! solidcore), so one test body compiles against either; the recipes that build each
//! side's values from plain inputs; and the fingerprints of each side's types.

use crate::harness::{Bits, Gen, MatR, Out, SplitMix64};

/// A frame recipe for `transform::Transform::new(origin, x, y, z)`.
#[derive(Clone, Debug)]
pub struct TfR(
    pub [f64; 3],
    pub [f64; 3],
    pub [f64; 3],
    pub Option<[f64; 3]>,
);

impl Gen for TfR {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let e = crate::harness::edge_f64();
        let mut v = vec![
            TfR([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], None),
            TfR([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], None),
            TfR(
                [0.0; 3],
                [2.0, 0.0, 0.0],
                [0.0, 3.0, 0.0],
                Some([0.0, 0.0, 4.0]),
            ),
            TfR([0.0; 3], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], None),
            TfR(
                [0.0; 3],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                Some([0.0, 0.0, 0.0]),
            ),
            TfR([0.0; 3], [0.0; 3], [0.0; 3], None),
        ];
        for &x in &e {
            v.push(TfR([x, 0.0, 1.0], [1.0, x, 0.0], [0.0, 1.0, 0.0], None));
        }
        for _ in 0..30 {
            let mut r = || {
                [
                    rng.uniform(-3.0, 3.0),
                    rng.uniform(-3.0, 3.0),
                    rng.uniform(-3.0, 3.0),
                ]
            };
            let o = r();
            let x = r();
            let y = r();
            let z = r();
            v.push(TfR(o, x, y, if x[0] > 0.0 { Some(z) } else { None }));
        }
        v
    }
}

/// A tolerance-set recipe: the seven fields of `Tolerances` in declaration order.
#[derive(Clone, Debug)]
pub struct TolR(pub [f64; 7]);

impl Gen for TolR {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let d = [1.0e-5, 2.0e-5, 0.5e-5, 0.5, 1.1, 50.0, 1000.0];
        let mut v = vec![TolR(d)];
        let undef = 1.23456e-61;
        for (i, &x) in [undef, 0.0, -1.0e-5, 1.0e-3, f64::NAN, f64::INFINITY, 1.0e-9]
            .iter()
            .enumerate()
        {
            let mut a = d;
            a[0] = x;
            v.push(TolR(a));
            let mut b = d;
            b[(i + 1) % 7] = x;
            v.push(TolR(b));
        }
        for _ in 0..12 {
            let z = 10f64.powf(rng.uniform(-8.0, -2.0));
            v.push(TolR([
                z,
                2.0 * z,
                z / 2.0,
                rng.uniform(0.0, 2.0),
                1.1,
                50.0,
                1000.0,
            ]));
        }
        v
    }
}

macro_rules! helpers {
    () => {
        use crate::facade::{TfR, TolR};
        use crate::harness::{List, MatR};

        pub fn v3(a: [f64; 3]) -> vector3d::Vec3 {
            vector3d::Vec3::new(a[0], a[1], a[2])
        }
        pub fn v2(a: [f64; 2]) -> vector2d::Vec2 {
            vector2d::Vec2::new(a[0], a[1])
        }
        pub fn e1(a: [f64; 2]) -> extent1d::Extent1d {
            extent1d::Extent1d::from_raw(a[0], a[1])
        }
        pub fn e2(a: [f64; 4]) -> extent2d::Extent2d {
            extent2d::Extent2d::from_bounds(a[0], a[1], a[2], a[3])
        }
        pub fn e3(a: [f64; 6]) -> extent3d::Extent3d {
            extent3d::Extent3d::from_bounds(a[0], a[1], a[2], a[3], a[4], a[5])
        }
        pub fn pe(a: [f64; 3], full: bool) -> periodic_extent1d::PeriodicExtent1d {
            periodic_extent1d::PeriodicExtent1d::from_parts(a[0], a[1], a[2], full)
        }
        pub fn mt(m: [[f64; 4]; 4]) -> math::Transform {
            math::Transform { m }
        }
        pub fn tf(r: &TfR) -> transform::Transform {
            transform::Transform::new(v3(r.0), v3(r.1), v3(r.2), r.3.map(v3))
        }
        pub fn mat(r: &MatR) -> matrix::Matrix {
            matrix::Matrix::from_row_major(r.0, r.1, r.2.clone()).unwrap()
        }
        pub fn iv(a: [f64; 2]) -> interval::Ivl {
            interval::Ivl::new(a[0], a[1])
        }
        pub fn ivs(a: &List<[f64; 2]>) -> Vec<interval::Ivl> {
            a.0.iter().map(|&x| iv(x)).collect()
        }
        /// A Taylor model from interval coefficients and a remainder.
        pub fn tm(c: &List<[f64; 2]>, rem: [f64; 2]) -> taylor_model::Tm {
            taylor_model::Tm::from_poly(&ivs(c), iv(rem))
        }
        /// A truncation from two small numbers (orders past the table refuse).
        pub fn trunc(s: usize, e: usize) -> series::Trunc {
            series::Trunc::new(s % 8, e % 5).unwrap()
        }
        /// A series of `tr`'s size with its coefficients drawn cyclically from `v`.
        pub fn ser<T: series::GnScalar>(v: &[T], tr: series::Trunc) -> series::Ser<T> {
            let mut r = series::Ser::constant(<T as series::GnScalar>::of(0.0));
            if v.is_empty() {
                return r;
            }
            let mut n = 0;
            for i in 0..=tr.s() {
                for k in 0..=tr.e() {
                    r.set(i, k, v[n % v.len()]);
                    n += 1;
                }
            }
            r
        }
        pub fn v3i(a: [[f64; 2]; 3]) -> series::V3<interval::Ivl> {
            series::V3 {
                x: iv(a[0]),
                y: iv(a[1]),
                z: iv(a[2]),
            }
        }
        pub fn tol_set(r: &TolR) -> Tolerances {
            let [zone_tol_3d, xsect_tol_3d, approx_tol_3d, ang_tol_deg, tol_gap_gain, model_size_estimate, large_small_size_ratio] =
                r.0;
            Tolerances {
                zone_tol_3d,
                xsect_tol_3d,
                approx_tol_3d,
                ang_tol_deg,
                tol_gap_gain,
                model_size_estimate,
                large_small_size_ratio,
            }
        }
    };
}

/// scx-num and scx-algo, by solidcore's module names.
pub mod x {
    pub use crate::solvers::x as sv;
    pub use scx_algo::polynomial;
    pub use scx_num::tol::Tolerances;
    pub use scx_num::{
        certified, extent1d, extent2d, extent3d, interval, interval_vector, limits, math, matrix,
        periodic_extent1d, ring, series, small_matrix, taylor_model, tol, transform, vector2d,
        vector3d,
    };
    helpers!();
    /// What the moved tolerance functions take: the tolerance set itself.
    pub fn tolk(r: &TolR) -> Tolerances {
        tol_set(r)
    }
}

/// The pinned solidcore.
pub mod s {
    pub use crate::solvers::s as sv;
    pub use solidcore::blend::fillet_section::tay as ring;
    pub use solidcore::blend::fillet_section::{interval, series, taylor_model};
    pub use solidcore::geom::{
        certified, extent1d, extent2d, extent3d, matrix, periodic_extent1d, polynomial, tol,
        transform, vector2d, vector3d,
    };
    pub use solidcore::kernel::Tolerances;
    pub use solidcore::{limits, math};
    /// `geom::interval_vector` is `pub(crate)` at the pin: no function to call. Its five
    /// bodies, as the pin's file writes them, over the pin's `Ivl`, so the moved file is
    /// compared with the operations it is made of.
    pub mod interval_vector {
        use super::interval::Ivl;
        pub fn cross(a: [Ivl; 3], b: [Ivl; 3]) -> [Ivl; 3] {
            let [ax, ay, az] = a;
            let [bx, by, bz] = b;
            [ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx]
        }
        pub fn dot(a: [Ivl; 3], b: [Ivl; 3]) -> Ivl {
            let [ax, ay, az] = a;
            let [bx, by, bz] = b;
            ax * bx + ay * by + az * bz
        }
        pub fn square(x: Ivl) -> Ivl {
            let s = x * x;
            Ivl::new(s.lo.max(0.0), s.hi)
        }
        pub fn norm(a: [Ivl; 3]) -> Ivl {
            let [x, y, z] = a;
            (square(x) + square(y) + square(z)).sqrt()
        }
        pub fn unit(a: [Ivl; 3]) -> Option<[Ivl; 3]> {
            let length = norm(a);
            if !length.is_finite() || length.lo <= 0.0 {
                return None;
            }
            Some(a.map(|v| v / length))
        }
    }
    /// `smgu_*` small-matrix helpers, where solidcore keeps them.
    pub mod small_matrix {
        pub use solidcore::geom::geom_utility::p1::{
            determinate_3x3, determinate_4x4, inverse_3x3, vec_triple_product,
        };
        pub use solidcore::geom::geom_utility::p2::{inverse_4x4, mult_4x4};
    }
    helpers!();
    /// What solidcore's tolerance functions take: a kernel holding the tolerance set.
    pub fn tolk(r: &TolR) -> solidcore::Kernel {
        solidcore::Kernel::new(&solidcore::KernelOptions {
            tolerances: tol_set(r),
            ..Default::default()
        })
    }
}

/// `impl Bits` for the same-named type on both sides.
macro_rules! both {
    ($($p:ident)::+ => |$e:ident, $o:ident| $body:block) => {
        impl Bits for x::$($p)::+ {
            fn bits(&self, $o: &mut Out) {
                let $e = self;
                $body
            }
        }
        impl Bits for s::$($p)::+ {
            fn bits(&self, $o: &mut Out) {
                let $e = self;
                $body
            }
        }
    };
}

/// Fingerprint by `Debug` (field-less enums only, where `Debug` is the variant name).
macro_rules! by_debug {
    ($($($p:ident)::+),* $(,)?) => {$(
        both!($($p)::+ => |e, o| { o.tag(&format!("{e:?}")); });
    )*};
}

by_debug!(
    extent1d::ExtentPointType,
    extent3d::BoundaryType,
    certified::Verdict,
    tol::SmallTopoType,
    math::AxisRefusal,
);

both!(interval::Ivl => |e, o| { e.lo.bits(o); e.hi.bits(o); });
both!(taylor_model::Tm => |e, o| { e.c.bits(o); e.rem.bits(o); });
both!(series::Trunc => |e, o| {
    e.s().bits(o);
    e.e().bits(o);
    e.degree().bits(o);
    e.computed_mask().bits(o);
});

/// The series types, generic over the scalar, on one side.
macro_rules! series_bits {
    ($m:ident) => {
        impl<T: $m::series::GnScalar + Bits> Bits for $m::series::Ser<T> {
            fn bits(&self, o: &mut Out) {
                use $m::series::SeriesStorage;
                for row in self.c.rows() {
                    row.bits(o);
                }
            }
        }
        impl<T: $m::series::GnScalar + Bits> Bits for $m::series::VSer<T> {
            fn bits(&self, o: &mut Out) {
                for s in &self.a {
                    s.bits(o);
                }
            }
        }
        impl<T: Bits> Bits for $m::series::V3<T> {
            fn bits(&self, o: &mut Out) {
                self.x.bits(o);
                self.y.bits(o);
                self.z.bits(o);
            }
        }
    };
}
series_bits!(x);
series_bits!(s);

both!(vector3d::Vec3 => |e, o| { e.x.bits(o); e.y.bits(o); e.z.bits(o); });
both!(vector2d::Vec2 => |e, o| { e.x.bits(o); e.y.bits(o); });
both!(extent1d::Extent1d => |e, o| { e.min().bits(o); e.max().bits(o); });
both!(extent2d::Extent2d => |e, o| { e.min().bits(o); e.max().bits(o); });
both!(extent3d::Extent3d => |e, o| { e.min().bits(o); e.max().bits(o); });
both!(periodic_extent1d::PeriodicExtent1d => |e, o| {
    e.min().bits(o);
    e.max().bits(o);
    e.period().bits(o);
    e.is_full_period().bits(o);
});
both!(math::Transform => |e, o| { e.m.bits(o); });
both!(transform::Transform => |e, o| { e.matrix().bits(o); });
both!(matrix::Matrix => |e, o| {
    e.num_rows().bits(o);
    e.num_columns().bits(o);
    e.as_slice().bits(o);
});
both!(matrix::LuDecomposition => |e, o| { e.index.bits(o); e.near_singular.bits(o); });
both!(matrix::LinearSolution => |e, o| { e.x.bits(o); e.near_singular.bits(o); });
both!(matrix::EigenSystem => |e, o| { e.values.bits(o); e.vectors.bits(o); e.rotations.bits(o); });
both!(extent1d::Inversion => |e, o| { e.parameter.bits(o); e.contained.bits(o); });
both!(extent1d::PeriodicContainment => |e, o| {
    e.contained.bits(o);
    e.periodic_value.bits(o);
    e.dist_to_min.bits(o);
    e.dist_to_max.bits(o);
});
both!(extent1d::ExtentNd => |e, o| {
    e.dimension().bits(o);
    for i in 0..e.dimension() {
        e.get(i).bits(o);
    }
});
both!(extent3d::SphereBound => |e, o| { e.center.bits(o); e.radius.bits(o); e.min_radius.bits(o); });
both!(certified::Bounded => |e, o| { e.value.bits(o); e.bound.bits(o); });
both!(math::PointSetSummary => |e, o| { e.centroid.bits(o); e.mean_distance.bits(o); });
both!(Tolerances => |e, o| {
    e.zone_tol_3d.bits(o);
    e.xsect_tol_3d.bits(o);
    e.approx_tol_3d.bits(o);
    e.ang_tol_deg.bits(o);
    e.tol_gap_gain.bits(o);
    e.model_size_estimate.bits(o);
    e.large_small_size_ratio.bits(o);
});

impl Bits for x::extent3d::LineHit {
    fn bits(&self, o: &mut Out) {
        use x::extent3d::LineHit as H;
        match *self {
            H::Miss => o.tag("Miss"),
            H::Touch { t } => {
                o.tag("Touch");
                t.bits(o);
            }
            H::Through { enter, exit } => {
                o.tag("Through");
                enter.bits(o);
                exit.bits(o);
            }
        }
    }
}
impl Bits for s::extent3d::LineHit {
    fn bits(&self, o: &mut Out) {
        use s::extent3d::LineHit as H;
        match *self {
            H::Miss => o.tag("Miss"),
            H::Touch { t } => {
                o.tag("Touch");
                t.bits(o);
            }
            H::Through { enter, exit } => {
                o.tag("Through");
                enter.bits(o);
                exit.bits(o);
            }
        }
    }
}

/// A typed failure, field by field: its variant name, then each field (`f64`s by their
/// bits, so a NaN payload or a signed zero counts). The variants and fields are the same on
/// both sides (`NumFailure` here, `GeomFailure` at the pin).
macro_rules! failure_bits {
    ($f:expr, $o:expr, $F:path) => {{
        use $F as F;
        let o: &mut Out = $o;
        match $f {
            F::CallFailed { class } => {
                o.tag("CallFailed");
                o.tag(&format!("{class:?}"));
            }
            F::Disjoint { a, b } => {
                o.tag("Disjoint");
                a.bits(o);
                b.bits(o);
            }
            F::IndexOutOfRange { index, len } => {
                o.tag("IndexOutOfRange");
                index.bits(o);
                len.bits(o);
            }
            F::JacobiDidNotConverge {
                sweeps,
                off_diagonal,
            } => {
                o.tag("JacobiDidNotConverge");
                sweeps.bits(o);
                off_diagonal.bits(o);
            }
            F::NoPerpendicular => o.tag("NoPerpendicular"),
            F::NotSquare { rows, cols } => {
                o.tag("NotSquare");
                rows.bits(o);
                cols.bits(o);
            }
            F::NotSymmetric => o.tag("NotSymmetric"),
            F::PointOutsideExtent { u, v } => {
                o.tag("PointOutsideExtent");
                u.bits(o);
                v.bits(o);
            }
            F::Singular { measure } => {
                o.tag("Singular");
                measure.bits(o);
            }
            F::SpanOutsidePeriod { from, to, period } => {
                o.tag("SpanOutsidePeriod");
                from.bits(o);
                to.bits(o);
                period.bits(o);
            }
            F::TooFewRows { rows, cols } => {
                o.tag("TooFewRows");
                rows.bits(o);
                cols.bits(o);
            }
            #[allow(unreachable_patterns)]
            other => o.tag(&format!("other {other:?}")),
        }
    }};
}

/// An error: its class, operation, `InvalidInput` reason or typed failure (fields by bits),
/// its `Display` text, then its cause's.
impl Bits for scx_num::NumError {
    fn bits(&self, o: &mut Out) {
        use scx_num::error::NumFailure;
        o.tag(&format!("{:?}", self.kind()));
        o.tag(self.operation());
        match self {
            scx_num::NumError::InvalidInput { reason, .. } => {
                o.tag("InvalidInput");
                o.tag(reason);
            }
            scx_num::NumError::Failed { failure, .. } => match failure {
                NumFailure::VectorVanishes => o.tag("VectorVanishes"),
                NumFailure::Defect { why } => {
                    o.tag("Defect");
                    o.tag(why);
                }
                f => failure_bits!(f, o, scx_num::error::NumFailure),
            },
        }
        o.tag(&self.to_string());
        if let Some(c) = self.cause() {
            c.bits(o);
        }
    }
}
impl Bits for solidcore::SolidError {
    fn bits(&self, o: &mut Out) {
        // The series' blend failures, by the names scx-num gives them (`NumFailure`).
        use solidcore::error::blend::{BlendFailure, SectionDegeneracy};
        o.tag(&format!("{:?}", self.kind()));
        o.tag(self.operation());
        if let solidcore::SolidError::InvalidInput { reason, .. } = self {
            o.tag("InvalidInput");
            o.tag(reason);
        } else {
            match (self.blend_failure(), self.geom_failure()) {
                (Some(BlendFailure::SectionDegenerate(SectionDegeneracy::VectorVanishes)), _) => {
                    o.tag("VectorVanishes");
                }
                (Some(BlendFailure::Defect { why }), _) => {
                    o.tag("Defect");
                    o.tag(why);
                }
                (Some(other), _) => o.tag(&format!("blend {other:?}")),
                (None, Some(f)) => failure_bits!(f, o, solidcore::error::geom::GeomFailure),
                (None, None) => o.tag(&format!("no typed failure: {self:?}")),
            }
        }
        o.tag(&self.to_string());
        if let Some(c) =
            std::error::Error::source(self).and_then(|c| c.downcast_ref::<solidcore::SolidError>())
        {
            c.bits(o);
        }
    }
}

/// Unused-import guard: both facades are used by the test modules.
#[allow(dead_code)]
fn _uses(_: MatR) {}

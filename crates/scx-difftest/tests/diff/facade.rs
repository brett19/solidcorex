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
        use crate::harness::MatR;

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

/// scx-num, by solidcore's module names.
pub mod x {
    pub use scx_num::tol::Tolerances;
    pub use scx_num::{
        certified, extent1d, extent2d, extent3d, limits, math, matrix, periodic_extent1d,
        small_matrix, tol, transform, vector2d, vector3d,
    };
    helpers!();
    /// What the moved tolerance functions take: the tolerance set itself.
    pub fn tolk(r: &TolR) -> Tolerances {
        tol_set(r)
    }
}

/// The pinned solidcore.
pub mod s {
    pub use solidcore::geom::{
        certified, extent1d, extent2d, extent3d, matrix, periodic_extent1d, tol, transform,
        vector2d, vector3d,
    };
    pub use solidcore::kernel::Tolerances;
    pub use solidcore::{limits, math};
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

/// An error: its class, operation and typed failure (by `Debug`, which names the
/// variant and prints each field; the variants are the same on both sides).
impl Bits for scx_num::NumError {
    fn bits(&self, o: &mut Out) {
        o.tag(&format!("{:?}", self.kind()));
        o.tag(self.operation());
        o.tag(&format!("{:?}", self.failure()));
        if let Some(c) = self.cause() {
            c.bits(o);
        }
    }
}
impl Bits for solidcore::SolidError {
    fn bits(&self, o: &mut Out) {
        o.tag(&format!("{:?}", self.kind()));
        o.tag(self.operation());
        o.tag(&format!("{:?}", self.geom_failure()));
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

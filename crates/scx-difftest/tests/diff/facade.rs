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

/// A tolerance-set recipe: the seven fields of the pin's `Tolerances` in declaration
/// order. scx reads the zone as a `Resolution` where it is one and the other fields hold
/// their defaults (`t_tol`).
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
            let z = scx_num::fmath::powf(10.0, rng.uniform(-8.0, -2.0));
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
        #[allow(unused_imports)]
        use self::ext::*;
        use crate::facade::TfR;
        use crate::harness::{List, MatR};

        pub fn v3(a: [f64; 3]) -> vector3d::Vec3 {
            vector3d::Vec3::new(a[0], a[1], a[2])
        }
        pub fn v2(a: [f64; 2]) -> vector2d::Vec2 {
            vector2d::Vec2::new(a[0], a[1])
        }
        /// A point (the pin: the same class as a vector).
        pub fn p3(a: [f64; 3]) -> vector3d::Point3 {
            vector3d::Point3::new(a[0], a[1], a[2])
        }
        pub fn p2(a: [f64; 2]) -> vector2d::Point2 {
            vector2d::Point2::new(a[0], a[1])
        }
        /// The interval `[a[0], a[1]]`, `None` when inverted.
        pub fn e1(a: [f64; 2]) -> Option<extent1d::Extent1d> {
            extent1d::Extent1d::from_bounds(a[0], a[1])
        }
        /// The box with corners `(a[0], a[1])` and `(a[2], a[3])`, `None` when inverted.
        pub fn e2(a: [f64; 4]) -> Option<extent2d::Extent2d> {
            extent2d::Extent2d::from_corners(p2([a[0], a[1]]), p2([a[2], a[3]]))
        }
        /// The box with corners `a[..3]` and `a[3..]`, `None` when inverted.
        pub fn e3(a: [f64; 6]) -> Option<extent3d::Extent3d> {
            extent3d::Extent3d::from_corners(p3([a[0], a[1], a[2]]), p3([a[3], a[4], a[5]]))
        }
        pub fn pe(a: [f64; 3], full: bool) -> periodic_extent1d::PeriodicExtent1d {
            periodic_extent1d::PeriodicExtent1d::from_parts(a[0], a[1], a[2], full)
        }
        pub fn tf(r: &TfR) -> transform::Transform {
            transform::Transform::new(p3(r.0), v3(r.1), v3(r.2), r.3.map(v3))
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
        pub fn v3i(a: [[f64; 2]; 3]) -> SerV3<interval::Ivl> {
            sv3(iv(a[0]), iv(a[1]), iv(a[2]))
        }
    };
}

/// scx-num and scx-algo, by solidcore's module names.
/// The generated 4x4 matrix made affine: column 3 set to `(0, 0, 0, 1)` (scx's `Affine` holds
/// no projective part; [`x::mt`] and [`s::mt`] read the same 12 entries).
pub fn affine_part(mut m: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    for (i, row) in m.iter_mut().enumerate() {
        row[3] = if i == 3 { 1.0 } else { 0.0 };
    }
    m
}

/// Fingerprint an upstream row-vector matrix's affine entries: rows 0 to 3, columns 0 to 2.
fn affine_bits(m: &[[f64; 4]; 4], o: &mut Out) {
    for row in m {
        row[..3].bits(o);
    }
}

pub mod x {
    pub use crate::solvers::x as sv;
    pub use scx_algo::hash_table;
    /// `find_coincident_pairs`, which reads no tolerances.
    pub fn ht_find_pairs(
        v: &mut hash_table::ObjsInVoxels<u32>,
        opts: hash_table::FindPairsOptions<'_>,
    ) -> scx_num::Result<hash_table::CoincidentPairs> {
        v.find_coincident_pairs(opts)
    }
    /// A point item's position and tolerance, `None` once removed.
    pub fn ht_placed(p: &hash_table::PointItem<u32>) -> Option<[f64; 4]> {
        p.placed
            .map(|q| [q.point.x, q.point.y, q.point.z, q.zone_tol])
    }
    /// The model's box, `None` when it has none.
    pub fn ht_bbox(v: &hash_table::ObjsInVoxels<u32>) -> Option<extent3d::Extent3d> {
        v.bbox()
    }
    /// Voxel `i`'s box, `None` when there is none.
    pub fn ht_voxel_bbox(
        v: &hash_table::ObjsInVoxels<u32>,
        i: usize,
    ) -> Option<extent3d::Extent3d> {
        v.map_voxel_index_to_bbox(i)
    }
    pub use scx_algo::krawczyk;
    /// A Krawczyk-module error as (class, operation, failure): `solve4`'s singular system.
    pub fn kz_err(e: &scx_num::NumError) -> (String, &'static str, String) {
        let f = match e {
            scx_num::NumError::Failed {
                failure: scx_num::error::NumFailure::Singular { .. },
                ..
            } => "singular".to_string(),
            other => format!("{other:?}"),
        };
        (
            format!("{:?}", crate::facade::num_as_pin(e).kind()),
            e.operation(),
            f,
        )
    }
    pub use scx_algo::polynomial;
    pub use scx_num::{
        certified, extent1d, extent2d, extent3d, interval, limits, matrix, periodic_extent1d, ring,
        series, small_matrix, taylor_model, transform, vector2d, vector3d,
    };
    helpers!();
    /// scx's extents take no adapter: [`crate::pin_extent`] gives the pin scx's API.
    pub mod ext {}
    pub fn end(v: Vec<extent1d::Extent1d>) -> extent1d::ExtentNd {
        extent1d::ExtentNd::new(v)
    }
    /// The affine map type: scx's column-vector `Affine`, the pin's row-vector `Transform`.
    pub type Map = math::Affine;
    pub fn mt(m: [[f64; 4]; 4]) -> math::Affine {
        math::Affine::try_from(super::affine_part(m)).unwrap()
    }
    use vector2d::{Axis2, Vec2};
    use vector3d::{Alignment, Axis, Vec3};
    fn axis3(i: usize) -> Option<Axis> {
        Axis::ALL.get(i).copied()
    }
    fn axis2(i: usize) -> Option<Axis2> {
        Axis2::ALL.get(i).copied()
    }
    /// The pin's degree angle tolerance as scx's radians, by the pin's own `SM_DEG2RAD`.
    pub fn angle_tol(deg: f64) -> f64 {
        deg * (scx_num::math::PI / 180.0)
    }
    /// The pin's `operator[] const` over an axis: 0.0 past 2.
    pub fn component3(v: Vec3, i: usize) -> f64 {
        axis3(i).map_or(0.0, |a| v[a])
    }
    /// The pin's `operator[]` write: whether `i` names an axis.
    pub fn set3(v: &mut Vec3, i: usize, x: f64) -> bool {
        axis3(i).map(|a| v[a] = x).is_some()
    }
    pub fn component2(v: Vec2, i: usize) -> Option<f64> {
        axis2(i).map(|a| v[a])
    }
    pub fn set2(v: &mut Vec2, i: usize, x: f64) -> bool {
        axis2(i).map(|a| v[a] = x).is_some()
    }
    /// `GetMin(i)`, `GetMax(i)`: 0.0 past 2.
    pub fn e3_at(e: &extent3d::Extent3d, i: usize) -> (f64, f64) {
        axis3(i).map_or((0.0, 0.0), |a| (e.min_at(a), e.max_at(a)))
    }
    /// The alignment, as the pin's `(parallel, anti)` says it: 0 neither, 1 same, 2 opposite.
    pub fn alignment(a: Vec3, b: Vec3, deg: f64) -> u32 {
        match a.alignment(b, angle_tol(deg)) {
            Alignment::Neither => 0,
            Alignment::Same => 1,
            Alignment::Opposite => 2,
        }
    }
    /// The series' vector of scalars (the pin's `series::V3`): `Vec3<T>`.
    pub type SerV3<T> = Vec3<T>;
    pub fn sv3<T>(x: T, y: T, z: T) -> SerV3<T> {
        Vec3::new(x, y, z)
    }
    /// The pin's `geom::interval_vector` functions, over arrays: `Vec3<Ivl>`'s methods.
    pub mod interval_vector {
        use scx_num::interval::Ivl;
        use scx_num::vector3d::Vec3;
        pub fn cross(a: [Ivl; 3], b: [Ivl; 3]) -> [Ivl; 3] {
            Vec3::from(a).cross(Vec3::from(b)).to_array()
        }
        pub fn dot(a: [Ivl; 3], b: [Ivl; 3]) -> Ivl {
            Vec3::from(a).dot(Vec3::from(b))
        }
        pub fn norm(a: [Ivl; 3]) -> Ivl {
            Vec3::from(a).norm()
        }
        pub fn unit(a: [Ivl; 3]) -> Option<[Ivl; 3]> {
            Vec3::from(a).unit().map(Vec3::to_array)
        }
    }

    /// scx's `math`, with the pin's two- to four-operand maxima and minima as the
    /// `f64::max`/`f64::min` chains that replace them.
    pub mod math {
        pub use scx_num::math::*;
        pub fn max2(a: f64, b: f64) -> f64 {
            a.max(b)
        }
        pub fn min2(a: f64, b: f64) -> f64 {
            a.min(b)
        }
        pub fn max3(a: f64, b: f64, c: f64) -> f64 {
            a.max(b).max(c)
        }
        pub fn min3(a: f64, b: f64, c: f64) -> f64 {
            a.min(b).min(c)
        }
        pub fn max4(a: f64, b: f64, c: f64, d: f64) -> f64 {
            a.max(b).max(c.max(d))
        }
        pub fn min4(a: f64, b: f64, c: f64, d: f64) -> f64 {
            a.min(b).min(c.min(d))
        }
    }

    /// scx's `tol`, with the pin's tolerance helpers by their names over scx's
    /// replacements: the `Resolution` methods, `f64::abs`, the `math` constants and the
    /// one `scaled_zero` over magnitudes.
    pub mod tol {
        use scx_num::extent1d::Extent1d;
        use scx_num::extent2d::Extent2d;
        use scx_num::extent3d::Extent3d;
        use scx_num::math::{Vec3, EFF_ZERO, EFF_ZERO_PARAM, EFF_ZERO_RAD};
        use scx_num::math::{EFF_ZERO_SQ, EFF_ZERO_SQRT};
        pub use scx_num::tol::*;
        use scx_num::vector2d::Vec2;
        use scx_num::{Accuracy, Length, Resolution};

        pub fn tol3d(d: f64) -> f64 {
            d.abs()
        }
        pub fn tol_mul(a: f64, b: f64) -> f64 {
            a * b
        }
        pub fn zone_tol3d(k: Resolution, d: Option<f64>) -> f64 {
            d.unwrap_or_else(|| k.zone().mm()).abs()
        }
        pub fn xsect_tol3d(k: Resolution, d: Option<f64>) -> f64 {
            d.unwrap_or_else(|| k.xsect().mm()).abs()
        }
        pub fn approx_tol3d(k: Resolution, d: Option<f64>) -> f64 {
            d.unwrap_or_else(|| k.approx().mm()).abs()
        }
        pub fn contact_tol_3d(k: Resolution) -> f64 {
            k.contact().mm()
        }
        pub fn zone_tol_3d(k: Resolution, s: SmallTopoType) -> f64 {
            k.topology_zone(s).mm()
        }
        pub fn approx_tol_3d(k: Resolution, s: SmallTopoType) -> f64 {
            (k.topology_zone(s) * 0.5).mm()
        }
        pub fn xsect_tol_3d(k: Resolution, s: SmallTopoType) -> f64 {
            (k.topology_zone(s) * 2.0).mm()
        }
        pub fn xsect_tol_3d_from_zones(a: Accuracy, b: Accuracy) -> f64 {
            a.sum(b).mm()
        }
        pub fn coincidence_reach_3d(k: Resolution, a: Accuracy, b: Accuracy, s: f64) -> f64 {
            scx_num::tol::coincidence_reach_3d(k, a, b, s).mm()
        }
        pub fn within_contact_tol(d: f64, c: f64, s: f64) -> bool {
            scx_num::tol::within_contact_tol(d, Length::from_mm(c), s)
        }
        pub fn zone_reach_3d(z: f64, s: f64) -> f64 {
            scx_num::tol::zone_reach_3d(Length::from_mm(z), s).mm()
        }
        /// The pin's override, where given, in place of the size-relative zero.
        pub fn is_zero_speed(a: f64, d: f64, o: Option<f64>) -> bool {
            match o {
                Some(z) => a.abs() < z,
                None => scx_num::tol::is_zero_speed(a, d),
            }
        }
        /// The pin's override, where given, in place of the section tolerance.
        pub fn is_degenerate_vec3(k: Resolution, v: Vec3, o: Option<f64>) -> bool {
            is_degenerate(v, o.map_or_else(|| k.xsect(), Length::from_mm))
        }
        pub fn map_to_1d(t: f64, d: Vec3, i: &Extent1d) -> f64 {
            scx_num::tol::map_to_1d(Length::from_mm(t), d, i).value()
        }
        pub fn eff_zero() -> f64 {
            EFF_ZERO
        }
        pub fn eff_zero_sq() -> f64 {
            EFF_ZERO_SQ
        }
        pub fn eff_zero_param() -> f64 {
            EFF_ZERO_PARAM
        }
        pub fn eff_zero_ang_rad() -> f64 {
            EFF_ZERO_RAD
        }
        pub fn scaled_zero() -> f64 {
            EFF_ZERO
        }
        pub fn scaled_zero_sq() -> f64 {
            EFF_ZERO_SQ
        }
        pub fn scaled_zero_sqrt() -> f64 {
            EFF_ZERO_SQRT
        }
        pub fn scaled_zero_sq_of<A: ScaledZero>(a: &A) -> f64 {
            let z = a.scaled_zero();
            z * z
        }
        pub fn scaled_zero_sqrt_of<A: ScaledZero>(a: &A) -> f64 {
            scx_num::vector3d::sqrt(a.scaled_zero())
        }

        /// A value's size, the magnitude `scaled_zero` reads for it: the size its
        /// `ScaledZero` impl reads.
        pub trait Mag {
            fn mag(&self) -> f64;
        }
        impl Mag for f64 {
            fn mag(&self) -> f64 {
                self.abs()
            }
        }
        impl Mag for Vec2 {
            fn mag(&self) -> f64 {
                self.x.abs().max(self.y.abs())
            }
        }
        impl Mag for Vec3 {
            fn mag(&self) -> f64 {
                self.x.abs().max(self.y.abs()).max(self.z.abs())
            }
        }
        impl Mag for Extent1d {
            fn mag(&self) -> f64 {
                self.max_dimension()
            }
        }
        impl Mag for Extent2d {
            fn mag(&self) -> f64 {
                self.max_dimension()
            }
        }
        impl Mag for Extent3d {
            fn mag(&self) -> f64 {
                self.max_dimension()
            }
        }

        fn sz(m: &[f64]) -> f64 {
            scx_num::tol::scaled_zero(m.iter().copied())
        }
        pub fn scaled_zero2(a: &impl Mag, b: &impl Mag) -> f64 {
            sz(&[a.mag(), b.mag()])
        }
        pub fn scaled_zero_sq2(a: &impl Mag, b: &impl Mag) -> f64 {
            let z = scaled_zero2(a, b);
            z * z
        }
        pub fn scaled_zero_sqrt2(a: &impl Mag, b: &impl Mag) -> f64 {
            scx_num::vector3d::sqrt(scaled_zero2(a, b))
        }
        pub fn scaled_zero3(a: &impl Mag, b: &impl Mag, c: &impl Mag) -> f64 {
            sz(&[a.mag(), b.mag(), c.mag()])
        }
        pub fn scaled_zero_sq3(a: &impl Mag, b: &impl Mag, c: &impl Mag) -> f64 {
            let z = scaled_zero3(a, b, c);
            z * z
        }
        pub fn scaled_zero_sqrt3(a: &impl Mag, b: &impl Mag, c: &impl Mag) -> f64 {
            scx_num::vector3d::sqrt(scaled_zero3(a, b, c))
        }
        /// The pin's four-operand maximum pairs its operands (`smos_4Max`), so the
        /// adapter pairs the magnitudes the same way.
        pub fn scaled_zero4(a: &impl Mag, b: &impl Mag, c: &impl Mag, d: &impl Mag) -> f64 {
            sz(&[a.mag().max(b.mag()), c.mag().max(d.mag())])
        }
        pub fn scaled_zero_sq4(a: &impl Mag, b: &impl Mag, c: &impl Mag, d: &impl Mag) -> f64 {
            let z = scaled_zero4(a, b, c, d);
            z * z
        }
        pub fn scaled_zero_sqrt4(a: &impl Mag, b: &impl Mag, c: &impl Mag, d: &impl Mag) -> f64 {
            scx_num::vector3d::sqrt(scaled_zero4(a, b, c, d))
        }
    }
}

/// The pinned solidcore.
pub mod s {
    pub use crate::solvers::s as sv;
    pub use solidcore::geom::hash_table;
    /// `find_coincident_pairs` on the default kernel.
    pub fn ht_find_pairs(
        v: &mut hash_table::ObjsInVoxels<u32>,
        opts: hash_table::FindPairsOptions<'_>,
    ) -> solidcore::Result<hash_table::CoincidentPairs> {
        v.find_coincident_pairs(&solidcore::kernel::Kernel::default(), opts)
    }
    /// A point item's position and tolerance, `None` once removed: the pin marks a
    /// removed item by `SM_UNDEF_DOUBLE` in all four (scx: `placed: None`).
    pub fn ht_placed(p: &hash_table::PointItem<u32>) -> Option<[f64; 4]> {
        let u = math::UNDEF_DOUBLE;
        let n = [p.point.x, p.point.y, p.point.z, p.zone_tol];
        (n != [u; 4]).then_some(n)
    }
    /// The model's box, `None` for the pin's inside-out `Extent3d::new` (no box).
    pub fn ht_bbox(v: &hash_table::ObjsInVoxels<u32>) -> Option<extent3d::Extent3d> {
        Some(*v.bbox()).filter(|b| !b.is_init())
    }
    /// Voxel `i`'s box, `None` for the pin's inside-out `Extent3d::new` (no box).
    pub fn ht_voxel_bbox(
        v: &hash_table::ObjsInVoxels<u32>,
        i: usize,
    ) -> Option<extent3d::Extent3d> {
        Some(v.map_voxel_index_to_bbox(i)).filter(|b| !b.is_init())
    }
    /// `geom::krawczyk`'s items are `pub(crate)` at the pin: the pin's file, made public by
    /// the build script (`build.rs`, `pin_krawczyk`), over the pin's own types.
    pub mod krawczyk {
        #[allow(clippy::all, clippy::pedantic, dead_code, unused_imports)]
        mod pin {
            pub use solidcore::blend::fillet_section::certificate::inverse4;
            use solidcore::blend::fillet_section::interval::{round_up, Ivl};
            /// The pin's `interval::cmax` (`pub(crate)` there), as its file writes it.
            fn cmax(a: f64, b: f64) -> f64 {
                if a < b {
                    b
                } else {
                    a
                }
            }
            pub use solidcore::blend::fillet_section::station::solve4;
            use solidcore::blend::fillet_section::tay::Ring;
            use solidcore::blend::fillet_section::taylor_model::TmN;
            use solidcore::error::Result;
            use solidcore::geom::vector3d::Vec3;
            include!(concat!(env!("OUT_DIR"), "/pin_krawczyk.rs"));
        }
        pub use pin::*;
        use solidcore::blend::fillet_section::interval::Ivl;
        use solidcore::error::Result;

        /// scx's `Invert`: the pin's three inverses under one name.
        pub trait Invert: Sized {
            fn inverse_of_columns(&self) -> Option<Self>;
        }
        impl Invert for [[f64; 2]; 2] {
            fn inverse_of_columns(&self) -> Option<Self> {
                inverse_of_columns2(self)
            }
        }
        impl Invert for [[f64; 3]; 3] {
            fn inverse_of_columns(&self) -> Option<Self> {
                inverse_of_columns3(self)
            }
        }
        impl Invert for [[f64; 4]; 4] {
            fn inverse_of_columns(&self) -> Option<Self> {
                inverse_of_columns4(self)
            }
        }

        /// The pin's `newton` with the inverse of its size, as scx's.
        pub fn newton<const N: usize>(
            start: [f64; N],
            at: &dyn Fn([f64; N]) -> Result<Linearised<N>>,
        ) -> Result<Option<[f64; N]>>
        where
            [[f64; N]; N]: Invert,
        {
            pin::newton(start, at, &|j: &[[f64; N]; N]| j.inverse_of_columns())
        }

        /// scx's `ProvedRoot`: the pin's `(root, region)` pair.
        pub struct ProvedRoot<const N: usize> {
            pub root: [Ivl; N],
            pub region: [Ivl; N],
        }

        /// The pin's `krawczyk_region`, its pair as a [`ProvedRoot`].
        pub fn krawczyk_region<const N: usize>(
            mid: [f64; N],
            c: [[f64; N]; N],
            value: &dyn Fn([Ivl; N]) -> Result<Option<[Ivl; N]>>,
            jacobian: &dyn Fn([Ivl; N]) -> Result<Option<[[Ivl; N]; N]>>,
        ) -> Result<Option<ProvedRoot<N>>> {
            Ok(pin::krawczyk_region(mid, c, value, jacobian)?
                .map(|(root, region)| ProvedRoot { root, region }))
        }
    }
    /// A Krawczyk-module error as (class, operation, failure): `solve4`'s singular system.
    pub fn kz_err(e: &solidcore::SolidError) -> (String, &'static str, String) {
        use solidcore::error::blend::{BlendFailure, SectionDegeneracy};
        let f = match e.blend_failure() {
            Some(BlendFailure::SectionDegenerate(SectionDegeneracy::NormalsAgree)) => {
                "singular".to_string()
            }
            _ => format!("{e:?}"),
        };
        (format!("{:?}", e.kind()), e.operation(), f)
    }
    pub use crate::pin_extent::{end, extent1d, extent2d, extent3d};
    pub use solidcore::blend::fillet_section::tay as ring;
    pub use solidcore::blend::fillet_section::{interval, series, taylor_model};
    /// The pin's extents under scx's names, imported by every diff body.
    pub mod ext {
        pub use crate::pin_extent::{E1, E2, E3};
    }
    pub use solidcore::geom::{
        certified, matrix, periodic_extent1d, polynomial, tol, transform, vector2d, vector3d,
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
    pub type Map = math::Transform;
    pub fn mt(m: [[f64; 4]; 4]) -> math::Transform {
        math::Transform {
            m: super::affine_part(m),
        }
    }
    pub fn angle_tol(deg: f64) -> f64 {
        deg
    }
    pub fn component3(v: vector3d::Vec3, i: usize) -> f64 {
        v.component(i)
    }
    pub fn set3(v: &mut vector3d::Vec3, i: usize, x: f64) -> bool {
        v.component_mut(i).map(|c| *c = x).is_ok()
    }
    pub fn component2(v: vector2d::Vec2, i: usize) -> Option<f64> {
        v.component(i).ok()
    }
    pub fn set2(v: &mut vector2d::Vec2, i: usize, x: f64) -> bool {
        v.component_mut(i).map(|c| *c = x).is_ok()
    }
    pub fn e3_at(e: &extent3d::Extent3d, i: usize) -> (f64, f64) {
        (e.min_at(i), e.max_at(i))
    }
    pub fn alignment(a: vector3d::Vec3, b: vector3d::Vec3, deg: f64) -> u32 {
        match a.parallel_sense(b, deg) {
            (false, _) => 0,
            (true, Some(true)) => 2,
            (true, _) => 1,
        }
    }
    pub type SerV3<T> = series::V3<T>;
    pub fn sv3<T>(x: T, y: T, z: T) -> SerV3<T> {
        series::V3 { x, y, z }
    }
    use crate::facade::TolR;
    /// The pin's tolerance set of recipe `r` (scx has none: `Tolerances` is retired).
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
    };
}
series_bits!(x);
series_bits!(s);

/// A point, a vector and a unit vector are one class at the pin: each by its coordinates.
macro_rules! xyz_bits {
    ($($t:ty),*) => {$(
        impl Bits for $t {
            fn bits(&self, o: &mut Out) {
                self.x.bits(o);
                self.y.bits(o);
                self.z.bits(o);
            }
        }
    )*};
}
macro_rules! xy_bits {
    ($($t:ty),*) => {$(
        impl Bits for $t {
            fn bits(&self, o: &mut Out) {
                self.x.bits(o);
                self.y.bits(o);
            }
        }
    )*};
}
impl<T: Bits> Bits for x::vector3d::Vec3<T> {
    fn bits(&self, o: &mut Out) {
        self.x.bits(o);
        self.y.bits(o);
        self.z.bits(o);
    }
}
impl<T: Bits> Bits for x::vector3d::Point3<T> {
    fn bits(&self, o: &mut Out) {
        self.x.bits(o);
        self.y.bits(o);
        self.z.bits(o);
    }
}
impl<T: Bits> Bits for s::series::V3<T> {
    fn bits(&self, o: &mut Out) {
        self.x.bits(o);
        self.y.bits(o);
        self.z.bits(o);
    }
}
xyz_bits!(x::vector3d::UnitVec3, s::vector3d::Vec3);
xy_bits!(
    x::vector2d::Vec2,
    x::vector2d::Point2,
    x::vector2d::UnitVec2,
    s::vector2d::Vec2
);
both!(extent1d::Extent1d => |e, o| { e.min().bits(o); e.max().bits(o); });
both!(extent2d::Extent2d => |e, o| { e.min().bits(o); e.max().bits(o); });
both!(extent3d::Extent3d => |e, o| { e.min().bits(o); e.max().bits(o); });
both!(periodic_extent1d::PeriodicExtent1d => |e, o| {
    e.min().bits(o);
    e.max().bits(o);
    e.period().bits(o);
    e.is_full_period().bits(o);
});
impl Bits for x::math::Affine {
    fn bits(&self, o: &mut Out) {
        affine_bits(&(*self).into(), o);
    }
}
impl Bits for s::math::Transform {
    fn bits(&self, o: &mut Out) {
        affine_bits(&self.m, o);
    }
}
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

/// The pin's error for a layer-0 error: the map from scx's causes to the pin's class and
/// text (research 62 D5). scx's kinds and `Display` are its own; the difftest compares what
/// the pin would report for the same cause, so each cause maps to the pin value it replaced
/// (`GeomFailure` of the same name and fields, the series' two `BlendFailure`s, and a
/// [`scx_num::NumError::Call`] frame to `GeomFailure::CallFailed` with the cause's class,
/// as the pin's `geom_call` builds it). A cause not mapped here mismatches.
pub fn num_as_pin(e: &scx_num::NumError) -> solidcore::SolidError {
    use scx_num::error::NumFailure as N;
    use scx_num::NumError;
    use solidcore::error::blend::{BlendFailure, SectionDegeneracy};
    use solidcore::error::geom::GeomFailure as G;
    use solidcore::error::Failure;
    use solidcore::SolidError;
    match e {
        NumError::InvalidInput { operation, reason } => {
            SolidError::InvalidInput { operation, reason }
        }
        NumError::Failed { operation, failure } => {
            let g = match *failure {
                N::Disjoint { a, b } => G::Disjoint { a, b },
                N::IndexOutOfRange { index, len } => G::IndexOutOfRange { index, len },
                N::JacobiDidNotConverge {
                    sweeps,
                    off_diagonal,
                } => G::JacobiDidNotConverge {
                    sweeps,
                    off_diagonal,
                },
                N::NoPerpendicular => G::NoPerpendicular,
                N::NotSquare { rows, cols } => G::NotSquare { rows, cols },
                N::NotSymmetric => G::NotSymmetric,
                N::PointOutsideExtent { u, v } => G::PointOutsideExtent { u, v },
                N::Singular { measure } => G::Singular { measure },
                N::SpanOutsidePeriod { from, to, period } => {
                    G::SpanOutsidePeriod { from, to, period }
                }
                N::TooFewRows { rows, cols } => G::TooFewRows { rows, cols },
                N::Defect { why } => {
                    return Failure::new(operation, BlendFailure::Defect { why }).into_error()
                }
                N::VectorVanishes => {
                    return Failure::new(
                        operation,
                        BlendFailure::SectionDegenerate(SectionDegeneracy::VectorVanishes),
                    )
                    .into_error()
                }
                // `NumFailure` is `#[non_exhaustive]`: a new cause mismatches until mapped
                _ => G::Defect {
                    why: "a layer-0 failure the facade does not map",
                },
            };
            Failure::new(operation, g).into_error()
        }
        NumError::Call { operation, cause } => {
            let cause = num_as_pin(cause);
            Failure::new(
                operation,
                G::CallFailed {
                    class: cause.kind(),
                },
            )
            .caused_by(cause)
            .into_error()
        }
    }
}

/// A layer-0 error, as the pin's error its causes map to ([`num_as_pin`]).
impl Bits for scx_num::NumError {
    fn bits(&self, o: &mut Out) {
        num_as_pin(self).bits(o);
    }
}

/// An error: its class, operation, `InvalidInput` reason or typed failure (fields by bits),
/// its `Display` text, then its cause's. An `InvalidInput` refusal is compared by its class
/// alone: which operation names the refusal and the words of its reason are text, free to
/// change (a refused input is the decision compared; scx names its own operation where the
/// pin names the helper that refused).
impl Bits for solidcore::SolidError {
    fn bits(&self, o: &mut Out) {
        // The series' blend failures, by the names scx-num gives them (`NumFailure`).
        use solidcore::error::blend::{BlendFailure, SectionDegeneracy};
        o.tag(&format!("{:?}", self.kind()));
        if let solidcore::SolidError::InvalidInput { .. } = self {
            o.tag("InvalidInput");
            return;
        }
        o.tag(self.operation());
        {
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

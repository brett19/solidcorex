// Moved from solidcore `solidcore/crates/solidcore/src/geom/tol.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8, with `Tolerances` from
// `solidcore/crates/solidcore/src/kernel.rs` (l.46-89) at the same commit.
// Upstream record (verbatim from those files): tol.rs: Upstream: `source/SMLib/inc/SmTol.h`,
// `source/SMLib/src/SmTol.cpp`, `source/SMLib/inc/SmRelation.h`. kernel.rs: Upstream:
// `source/SMLib/inc/SmContext.h` (`SmContext`), `SmTol.h` (`SmTol`, `SmTol3d`), the
// tolerance defaults of `SmMath.h`, the `SmCacheMgr*` classes, and `SM_API`'s
// `SmApiGetOrCreateContext()`.
// Changes: the scalar part only (no context, B-rep, curve, volume, equation-set or
// `SmRelation` overloads, no `update_object_tolerance`); of kernel.rs only the
// `Tolerances` value; `&Kernel` became `&Tolerances`; `size_estimate_to_zone_tol_3d`
// renamed `zone_for_size`; the zone constant points at `consts`, and `UNINIT_TOL` is defined as
// `math::UNDEF_DOUBLE` (the same 1.23456e-61 as the pin's literal); `Tolerances::default`
// reads the named constants in place of the same literals; paths. Tests: `system_tolerances`
// drops its `context_zone_tol_3d` assertion (not moved) and builds `tolerances` in place of
// a kernel; added `tolerances` (helper), `zone_for_size_boundaries_keep_todays_answers`,
// `small_topo_codes_round_trip` and `default_tolerances_are_solidcores`.
//! Tolerance policy (`SmTol`), the scalar part: the tolerance constructors, model-size
//! based default tolerances, zone / intersection / approximation tolerances, the
//! contact and coincidence rules, scaled zeros and simple tolerance predicates, and the
//! [`Tolerances`] value they read.
//!
//! Upstream: `source/SMLib/inc/SmTol.h`, `source/SMLib/src/SmTol.cpp`.
//!
//! | Rust                                   | Upstream                                       |
//! |----------------------------------------|------------------------------------------------|
//! | [`tol3d`]                              | `SmTol3d(double)`, `SmScaledZero(double)`, `SmTol2d(double)`, `SmTol1d(double)`, every `operator=(double)` of `SM_TOL_CALLS`, `SmTol3d::operator=(const SmTol3d)` |
//! | [`zone_tol3d`]                         | `SmZoneTol3d(double)`                          |
//! | [`xsect_tol3d`]                        | `SmXSectTol3d(double)`                         |
//! | [`approx_tol3d`]                       | `SmApproxTol3d(double)`                        |
//! | [`tol_mul`]                            | `operator*=(SmTol3d&, double)`                 |
//! | (plain `f64`)                          | `operator double&`, `operator const double&` of every tolerance class |
//! | [`SmallTopoType`]                      | `SmSmallTopoType`                              |
//! | [`zone_for_size`]                      | `SmTol::SizeEstimateToZoneTol3d`               |
//! | [`in_tol`]                             | `SmTol::InTol(double, const SmTol3d&)`         |
//! | [`is_zero`], [`is_zero_squared`], [`is_zero_to_tol`] | `SM_IS_ZERO`, `SM_IS_ZERO_SQUARED`, `SM_IS_ZERO_TO_TOL` (`SmMath.h`) |
//! | [`are_same`], [`are_same_to_tol`]      | `SM_ARE_SAME`, `SM_ARE_SAME_TO_TOL` (`SmMath.h`) |
//! | [`eff_zero`], [`eff_zero_sq`], [`eff_zero_param`], [`eff_zero_ang_rad`] | `SmTol::GetEffZero`, `GetEffZeroSq`, `GetEffZeroParam`, `GetEffZeroAngRad` |
//! | [`xsect_tol_3d_from_zones`]            | `SmTol::GetXSectTol3d(const SmZoneTol3d&, const SmZoneTol3d&)` |
//! | [`contact_tol_3d`]                     | not upstream (WR-787, #15429): when circles a hair less than tangent touch |
//! | [`within_contact_tol`]                 | not upstream: the one reading of a contact at the tolerance, shared by every contact site |
//! | [`coincidence_tol_3d`], [`coincidence_reach_3d`], [`within_coincidence_tol`] | not upstream: when two places are one, the larger of the contact tolerance and the two zone tolerances |
//! | [`zone_reach_3d`]                      | not upstream: when two places are one within a body's own tolerance, no contact floor |
//! | [`ang_tol_rad`]                        | `SmTol::GetAngTolRad()`                        |
//! | [`model_size_estimate`]                | `SmTol::GetModelSizeEstimate()`                |
//! | [`are_parallel`]                       | `SmTol::AreParallel`                           |
//! | [`are_perpendicular`]                  | `SmTol::ArePerpendicular`                      |
//! | [`is_zero_speed`]                      | `SmTol::IsZeroSpeed`                           |
//! | [`is_degenerate_vec3`]                 | `SmTol::IsDegenerate(const SmVector3d, SmXSectTol3d*)` |
//! | [`gap_gain`]                           | `SmTol::GetGapGain`                            |
//! | [`zone_tol_3d`]                        | `SmTol::GetZoneTol3d(SmSmallTopoType)` (system) |
//! | [`approx_tol_3d`]                      | `SmTol::GetApproxTol3d(SmSmallTopoType)`       |
//! | [`xsect_tol_3d`]                       | `SmTol::GetXSectTol3d(SmSmallTopoType)`        |
//! | [`scaled_zero`], [`scaled_zero_sq`], [`scaled_zero_sqrt`] | `SmTol::GetScaledZero()`, `GetScaledZeroSq()`, `GetScaledZeroSqrt()` |
//! | [`ScaledZero`] (`f64`, [`Vec2`], [`Vec3`], [`Extent1d`], `Extent2d`, [`Extent3d`]) | `SmTol::GetScaledZero(const double)`, `(const SmVector2d&)`, `(const SmVector3d&)`, `(const SmExtent1d&)`, `(const SmExtent2d&)` (impl in [`crate::extent2d`]), `(const SmExtent3d&)` |
//! | [`map_to_1d`]                          | `SmTol::MapTo1d(SmTol3d&, SmVector3d&, const SmExtent1d&)` |
//! | [`scaled_zero_sq_of`], [`scaled_zero_sqrt_of`] | unary `GetScaledZeroSq<cA>`, `GetScaledZeroSqrt<cA>` |
//! | [`scaled_zero2`], [`scaled_zero_sq2`], [`scaled_zero_sqrt2`] | binary `GetScaledZero<cA,cB>`, `...Sq`, `...Sqrt` |
//! | [`scaled_zero3`], [`scaled_zero_sq3`], [`scaled_zero_sqrt3`] | tertiary templates     |
//! | [`scaled_zero4`], [`scaled_zero_sq4`], [`scaled_zero_sqrt4`] | quaternary templates   |
//!
//! System state: upstream keeps the system tolerances in thread-local statics
//! (`s_ZoneTol3d`, `s_TolGapGain`, `s_ModelSizeEstimate`, `s_LargeSmallSizeRatio`);
//! here, as in solidcore's `Kernel`, they are a [`Tolerances`] value passed by
//! reference (solidcore reads the same fields through `k.tolerances`).
//!
//! Stay in solidcore for now (they need a context, a B-rep, a curve, a volume or an
//! equation set, all above this layer): the `SmContext`, `SmBrep`, `SmCurve`,
//! `SmVolume`, `SmEvalNFunctionsObject` and `SmObject` overloads, `CalcFaceZoneTol3d`,
//! `CalcEdgeZoneTol3d`, `CalcVertexZoneTol3d`, `SmRelation` and
//! `UpdateObjectTolerance`.
//!
//! Deviations:
//! - The tolerance classes (`SmTol3d`, `SmZoneTol3d`, ...) are plain `f64` values; their
//!   converting constructors are the functions [`tol3d`], [`zone_tol3d`],
//!   [`xsect_tol3d`] and [`approx_tol3d`], applied exactly where upstream converts a
//!   `double` into a tolerance class (a return statement, a cast, an initialisation),
//!   so that `fabs` and the `SM_USE_DEFAULT` substitution happen at the same places.
//! - Optional pointer arguments become `Option<f64>` holding the pointee's value.
//! - `SM_ASSERT*`, `WARN`, `ERR_MSG` and `SM_DEBUG_CODE` blocks are not ported (they do
//!   not change results in the release build).

use crate::certified::gamma;
use crate::consts::ZONE_DEFAULT;
use crate::extent1d::Extent1d;
use crate::extent3d::Extent3d;
use crate::math::{
    max2, max3, max4, rad_to_deg, Vec3, EFF_ZERO, EFF_ZERO_PARAM, EFF_ZERO_RAD, EFF_ZERO_SQ,
    EFF_ZERO_SQRT, PI, UNDEF_DOUBLE,
};
use crate::vector2d::Vec2;
use crate::vector3d::sqrt as sm_sqrt;

/// Geometric tolerances in model units (upstream defaults from `SmMath.h`): the
/// system values the functions here read. solidcore's `kernel::Tolerances`, moved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerances {
    /// Local-neighbourhood size: points closer than this are coincident
    /// (`SM_ZONE_TOL_3D` = `1e-5`).
    pub zone_tol_3d: f64,
    /// Default gap / intersection tolerance (`SM_XSECT_TOL_3D` = `2 * zone`).
    pub xsect_tol_3d: f64,
    /// Default approximation tolerance (`SM_APPROX_TOL_3D` = `zone / 2`).
    pub approx_tol_3d: f64,
    /// Maximum angle between parallel vectors, degrees (`SM_ANG_TOL_DEG` = `0.5`).
    pub ang_tol_deg: f64,
    /// Over-size factor applied above an object's max gap (`SM_TOL_GAP_GAIN` = `1.1`).
    pub tol_gap_gain: f64,
    /// Expected model size used to pick tolerances
    /// (`SM_MODEL_SIZE_ESTIMATE` = `50.0`).
    pub model_size_estimate: f64,
    /// Ratio of largest to smallest feature size
    /// (`SM_LARGE_SMALL_SIZE_RATIO` = `1000`).
    pub large_small_size_ratio: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        let zone = ZONE_DEFAULT;
        Self {
            zone_tol_3d: zone,
            xsect_tol_3d: 2.0 * zone,
            approx_tol_3d: zone / 2.0,
            ang_tol_deg: ANG_TOL_DEG,
            tol_gap_gain: TOL_GAP_GAIN,
            model_size_estimate: MODEL_SIZE_ESTIMATE,
            large_small_size_ratio: LARGE_SMALL_SIZE_RATIO,
        }
    }
}

impl Tolerances {
    /// [`Tolerances::ang_tol_deg`] in radians (`SM_ANG_TOL_RAD`).
    #[must_use]
    pub fn ang_tol_rad(&self) -> f64 {
        self.ang_tol_deg * PI / 180.0
    }
}

/// `SM_USE_DEFAULT`: sentinel meaning "use the current system value".
pub const USE_DEFAULT: f64 = 9876543.21;
/// `SM_UNINIT_TOL`: sentinel of an uninitialised tolerance.
pub const UNINIT_TOL: f64 = UNDEF_DOUBLE;
/// `SM_MODEL_SIZE_ESTIMATE`: default model size estimate.
pub const MODEL_SIZE_ESTIMATE: f64 = 50.0;
/// `SM_LARGE_SMALL_SIZE_RATIO`: default large/small topology size ratio. Owner: the
/// tolerance policy (the default of [`Tolerances::large_small_size_ratio`]). Its own
/// literal, as at the pin: the validity limit [`crate::consts::MAX_ACCURACY_RATIO`] has
/// the same value but is a separate decision.
pub const LARGE_SMALL_SIZE_RATIO: f64 = 1000.0;
/// `SM_ZONE_TOL_3D`: default zone tolerance.
pub const ZONE_TOL_3D: f64 = ZONE_DEFAULT;
/// `SM_TOL_GAP_GAIN`: over-size factor above an object's largest gap.
pub const TOL_GAP_GAIN: f64 = 1.1;
/// `SM_ANG_TOL_DEG`: angle tolerance in degrees.
pub const ANG_TOL_DEG: f64 = 0.5;
/// `SM_ANG_TOL_RAD` = `SM_ANG_TOL_DEG * SM_PI / 180.0`.
pub const ANG_TOL_RAD: f64 = ANG_TOL_DEG * PI / 180.0;

// ---- tolerance values ------------------------------------------------------------

/// `SmTol3d(double)` (and the `SmScaledZero`, `SmTol2d`, `SmTol1d` constructors and
/// every `operator=(double)`): a tolerance is stored as `fabs(d)`.
#[must_use]
pub fn tol3d(d: f64) -> f64 {
    d.abs()
}

/// `SmZoneTol3d(double)`: [`USE_DEFAULT`] becomes the system zone tolerance, then
/// `fabs`.
#[must_use]
pub fn zone_tol3d(t: &Tolerances, d: f64) -> f64 {
    tol3d(if d == USE_DEFAULT { t.zone_tol_3d } else { d })
}

/// `SmXSectTol3d(double)`: [`USE_DEFAULT`] becomes twice the system zone tolerance,
/// then `fabs`.
#[must_use]
pub fn xsect_tol3d(t: &Tolerances, d: f64) -> f64 {
    tol3d(if d == USE_DEFAULT {
        t.zone_tol_3d * 2.0
    } else {
        d
    })
}

/// `SmApproxTol3d(double)`: [`USE_DEFAULT`] becomes half the system zone tolerance,
/// then `fabs`.
#[must_use]
pub fn approx_tol3d(t: &Tolerances, d: f64) -> f64 {
    tol3d(if d == USE_DEFAULT {
        t.zone_tol_3d / 2.0
    } else {
        d
    })
}

/// `operator*=(SmTol3d&, double)`: `tol * d` (no `fabs`, as upstream).
#[must_use]
pub fn tol_mul(tol: f64, d: f64) -> f64 {
    tol * d
}

/// Control flags for small-topology tolerances (`SmSmallTopoType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SmallTopoType {
    /// `SM_ST_NORMAL_SIZE`: normal (large) scale unless the object says otherwise.
    #[default]
    NormalSize,
    /// `SM_ST_SMALL_SIZE`: small ("pinhole in a battleship") scale unless the object
    /// says otherwise.
    SmallSize,
    /// `SM_ST_FORCE_NORMAL_SIZE`: always normal scale.
    ForceNormalSize,
    /// `SM_ST_FORCE_SMALL_SIZE`: always small scale.
    ForceSmallSize,
    /// `SM_ST_NO_CHANGE`: no-action value for `SetZoneTol3d`.
    NoChange,
}

impl SmallTopoType {
    /// Upstream enum value (`SM_ST_NORMAL_SIZE` = 0 .. `SM_ST_NO_CHANGE` = 4).
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::NormalSize => 0,
            Self::SmallSize => 1,
            Self::ForceNormalSize => 2,
            Self::ForceSmallSize => 3,
            Self::NoChange => 4,
        }
    }

    /// The flag with upstream value `code`, if any.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::NormalSize),
            1 => Some(Self::SmallSize),
            2 => Some(Self::ForceNormalSize),
            3 => Some(Self::ForceSmallSize),
            4 => Some(Self::NoChange),
            _ => None,
        }
    }

    /// `eIsSmall == SM_ST_SMALL_SIZE || eIsSmall == SM_ST_FORCE_SMALL_SIZE`.
    const fn is_small(self) -> bool {
        matches!(self, Self::SmallSize | Self::ForceSmallSize)
    }
}

// ---- model size and system tolerances ----------------------------------------------

/// Maps a model size estimate to a default zone tolerance
/// (`SmTol::SizeEstimateToZoneTol3d`):
///
/// | size estimate         | zone tolerance |
/// |-----------------------|----------------|
/// | `[0.00005, 0.005)`    | `1e-7`         |
/// | `[0.005, 0.5)`        | `1e-6`         |
/// | `[0.5, 500]`          | `1e-5`         |
/// | `(500, 50000]`        | `1e-4`         |
/// | `(50000, 5e6]`        | `1e-3`         |
///
/// The ranges upstream checks are closed and overlap at their ends; it checks
/// `0.5..=500` first, then `500..=50000`, `0.005..=0.5`, `50000..=5e6` and
/// `0.00005..=0.005`, so a boundary size takes the first range that holds it, which
/// gives the half-open ranges above.
///
/// Outside them the zone is `10^floor(log10(|size|) / 2) * 1e-5`; [`USE_DEFAULT`]
/// means [`MODEL_SIZE_ESTIMATE`]. (The `SmZoneTol3d` conversions of the result never
/// see `SM_USE_DEFAULT` and only apply `fabs`.)
#[must_use]
pub fn zone_for_size(size: f64) -> f64 {
    if size == USE_DEFAULT {
        zone_for_size(MODEL_SIZE_ESTIMATE)
    } else if (0.5..=500.0).contains(&size) {
        1.0e-5
    } else if (500.0..=50000.0).contains(&size) {
        1.0e-4
    } else if (0.005..=0.5).contains(&size) {
        1.0e-6
    } else if (50000.0..=5e6).contains(&size) {
        1.0e-3
    } else if (0.00005..=0.005).contains(&size) {
        1.0e-7
    } else {
        tol3d(10f64.powf((size.abs().log10() / 2.0).floor()) * 1.0e-5)
    }
}

/// The system model size estimate (`SmTol::GetModelSizeEstimate()`).
#[must_use]
pub fn model_size_estimate(t: &Tolerances) -> f64 {
    t.model_size_estimate
}

/// The gap gain: the over-size factor that keeps an object's zone tolerance a little
/// larger than half its largest gap (`SmTol::GetGapGain`).
#[must_use]
pub fn gap_gain(t: &Tolerances) -> f64 {
    t.tol_gap_gain
}

/// The system zone tolerance (`SmTol::GetZoneTol3d(SmSmallTopoType)`): multiplied by
/// the large/small size ratio for small topology.
#[must_use]
pub fn zone_tol_3d(t: &Tolerances, small: SmallTopoType) -> f64 {
    if t.zone_tol_3d == UNINIT_TOL {
        // upstream: smos_AssertErrorMessage(.., "Uninit-Tol", ..)
        zone_tol3d(t, UNINIT_TOL)
    } else if small.is_small() {
        zone_tol3d(t, zone_tol3d(t, t.large_small_size_ratio) * t.zone_tol_3d)
    } else {
        t.zone_tol_3d
    }
}

/// The system approximation tolerance, half the zone tolerance
/// (`SmTol::GetApproxTol3d(SmSmallTopoType)`).
#[must_use]
pub fn approx_tol_3d(t: &Tolerances, small: SmallTopoType) -> f64 {
    approx_tol3d(t, 0.5 * zone_tol_3d(t, small))
}

/// The system intersection tolerance, twice the zone tolerance
/// (`SmTol::GetXSectTol3d(SmSmallTopoType)`).
#[must_use]
pub fn xsect_tol_3d(t: &Tolerances, small: SmallTopoType) -> f64 {
    xsect_tol3d(t, 2.0 * zone_tol_3d(t, small))
}

/// The intersection tolerance of two objects with zone tolerances `zone1`, `zone2`:
/// their sum (`SmTol::GetXSectTol3d(const SmZoneTol3d&, const SmZoneTol3d&)`).
#[must_use]
pub fn xsect_tol_3d_from_zones(t: &Tolerances, zone1: f64, zone2: f64) -> f64 {
    xsect_tol3d(t, zone1 + zone2)
}

/// The tolerance up to which two circles or parallel cylinders, or a circle and a line in
/// its plane, that overlap past touching touch on the boolean's path (read by
/// [`within_contact_tol`]): the system zone
/// tolerance, 1e-5 by default. Not upstream (WR-787, #15429, #14599): one value for every
/// site that decides the contact (the walls, the rims, the curve/surface classifier),
/// whatever tolerances earlier operations grew the edges and faces to, so the same two
/// holes give the same topology whatever came before. The touching point midway lies δ/2 from each operand, so below it the
/// point lies within the approximation tolerance (half the zone tolerance) of both, as
/// the intersection curves through it do. At the intersection tolerance (twice the zone
/// tolerance) two through holes overlapping by 1.02e-5 to 1.99e-5 came out a valid solid
/// that kept the second tool whole, as a void.
#[must_use]
pub fn contact_tol_3d(t: &Tolerances) -> f64 {
    zone_tol_3d(t, SmallTopoType::NormalSize)
}

/// Whether `distance`, a length that decides a contact (how far two circles, or a circle
/// and a line in its plane, overlap past touching; how far a plane dips into a cylinder or
/// stands off a cone's generator; how far apart such curves lie all round), is within the
/// contact tolerance `contact`: not proved past it. Not upstream: the one reading of the
/// contact rule, shared by every site that decides a contact
/// (`circle_coplanar_circle_intersect_in_contact` and
/// `line_coplanar_circle_intersect_distinct` in `geom::geom_utility`, which the walls, the
/// rims and the curve/surface classifier call with [`contact_tol_3d`]; a plane's section
/// of a cylinder or a cone touching a side of the plane's domain,
/// `Plane::make_new_plane_ellipse_in_contact`, with it too; and the plane's tangent line
/// on a cylinder or a cone, `Cone::intersect_with_plane`, with the faces' zone
/// tolerance), as two points are one unless proved apart
/// (`ProtoTopologyManager::find_proto_vertex_within`).
///
/// Each site decides one contact from its own copies of the two curves: an edge's curve,
/// a face pair's intersection curve, a surface's osculating circle. They are the same
/// curves computed by different constructions, so their coordinates differ by a few
/// roundings of their size, and a length read off them differs by that much from site to
/// site. Read sharply, a contact at the tolerance (a boss placed exactly the tolerance over
/// a rim) touched at one site and crossed at another, 9.99999999962e-6 against
/// 1.0000000000673e-5; and where every site crossed, the vertices on the two sides of the
/// lens were one point by their tolerances while the curves between them crossed. So a
/// distance at the tolerance is within it, and so is one up to `γ_32` of `size` past it,
/// `size` the largest coordinate a point of either curve has: the overlap's own arithmetic
/// (a projection, a distance and two sums, about fifteen roundings) and as many again for
/// the constructions of the copies (sixteen failing booleans of the stress corpus read
/// their copies at most 3.3 unit roundoffs of `size` apart). A distance proved past the
/// tolerance by more than that is past it at every site.
#[must_use]
pub fn within_contact_tol(distance: f64, contact: f64, size: f64) -> bool {
    distance <= contact + rounding_of_copies(size)
}

/// How far a distance read off two copies of the same geometry can stand past the true
/// one by rounding alone: `γ_32` of `size`, the largest coordinate a point of either copy
/// has ([`within_contact_tol`] derives it). Shared by the contact and the coincidence rule,
/// so both read a length at their tolerance the same way.
fn rounding_of_copies(size: f64) -> f64 {
    gamma(32) * size.abs()
}

/// The tolerance within which two places coincide (are one place), for two objects with
/// zone tolerances `zone1` and `zone2`: the larger of the contact tolerance
/// ([`contact_tol_3d`]) and the two zone tolerances. Not upstream: one rule for every site
/// that decides whether two places on two objects are one (as the sites move to it, a curve
/// along an edge and two points one vertex; where a body's own tolerances decide, as for a
/// sample of an edge on a side of its face's domain, [`zone_reach_3d`]), in place of
/// the intersection tolerance, the two zone tolerances' sum, that upstream reads there.
///
/// A zone tolerance is how far an object's stored geometry may stand from the place it
/// stands for: within it of an object, a place is on the object, by that object's own
/// statement. Two places farther apart than both objects' zone tolerances are on neither
/// by either object's statement, so nothing makes them one; the sum makes them one up to
/// twice the tolerance, where neither object says so. The contact rule already reads two
/// circles that far into each other (past the contact tolerance, the system zone
/// tolerance) as two crossings, two places; at the sum, an edge and a curve 1.5e-5 beside
/// it are one place where the tolerance is 1e-5, and a groove that deep cut into a block
/// comes out as no groove at all while a site still reads the sum (docs/booleans.md §10,
/// the coincidence rule in migration). With every site on the larger
/// tolerance, a place is decided once: within the larger of the two objects' tolerances
/// (and never under the contact tolerance, so a contact and a coincidence agree), one;
/// past it, two. An object whose tolerance an earlier operation grew keeps it: whatever
/// lies within it is on it. Read by [`within_coincidence_tol`], at the tolerance or a
/// rounding of the copies past it, as [`within_contact_tol`] reads a contact.
#[must_use]
pub fn coincidence_tol_3d(t: &Tolerances, zone1: f64, zone2: f64) -> f64 {
    // A NaN zone tolerance stays NaN (`f64::max` would pass over it and let the other
    // tolerance stand): no distance is within it, so an object whose tolerance is unknown
    // is never made one with another place, as the two tolerances' sum read it before
    if zone1.is_nan() || zone2.is_nan() {
        return f64::NAN;
    }
    contact_tol_3d(t).max(zone1).max(zone2)
}

/// The largest distance at which two places coincide ([`coincidence_tol_3d`]): the
/// tolerance and the rounding of the copies past it, `size` the largest coordinate of
/// either place, as [`within_contact_tol`] reads a contact. For a site that compares in
/// its own measure (a band mapped onto a surface's parameters) rather than a distance.
#[must_use]
pub fn coincidence_reach_3d(t: &Tolerances, zone1: f64, zone2: f64, size: f64) -> f64 {
    coincidence_tol_3d(t, zone1, zone2) + rounding_of_copies(size)
}

/// The largest distance at which two places held to tolerance `zone` are one place: that
/// tolerance and the rounding of the copies past it, `size` the largest coordinate of either
/// place. Not upstream. For a site where the body's own tolerances decide, so the contact
/// tolerance ([`coincidence_tol_3d`]'s floor, which keeps a contact between two operands'
/// curves and a coincidence of them in agreement) does not apply: the side test of an edge
/// use, where a sample of the edge dropped onto its face's surface meets a side of that
/// surface's domain, `zone` the larger of the two tolerances. A NaN zone tolerance gives
/// NaN, within which nothing lies.
#[must_use]
pub fn zone_reach_3d(zone: f64, size: f64) -> f64 {
    zone + rounding_of_copies(size)
}

/// Whether two places `distance` apart, on objects with zone tolerances `zone1` and
/// `zone2`, coincide: `distance` is within [`coincidence_reach_3d`]. Not upstream: the
/// one reading of the coincidence rule ([`coincidence_tol_3d`]). A NaN distance, or a NaN
/// zone tolerance, is apart.
#[must_use]
pub fn within_coincidence_tol(
    t: &Tolerances,
    distance: f64,
    zone1: f64,
    zone2: f64,
    size: f64,
) -> bool {
    distance <= coincidence_reach_3d(t, zone1, zone2, size)
}

/// The system angle tolerance in radians (`SmTol::GetAngTolRad()`, `SM_ANG_TOL_RAD`).
#[must_use]
pub fn ang_tol_rad() -> f64 {
    ANG_TOL_RAD
}

/// `SM_EFF_ZERO` as a scaled zero (`SmTol::GetEffZero`).
#[must_use]
pub fn eff_zero() -> f64 {
    tol3d(EFF_ZERO)
}

/// `SM_EFF_ZERO_SQ` as a scaled zero (`SmTol::GetEffZeroSq`).
#[must_use]
pub fn eff_zero_sq() -> f64 {
    tol3d(EFF_ZERO_SQ)
}

/// `SM_EFF_ZERO_PARAM` as a scaled zero (`SmTol::GetEffZeroParam`): the smallest
/// distance between distinct parameter values.
#[must_use]
pub fn eff_zero_param() -> f64 {
    tol3d(EFF_ZERO_PARAM)
}

/// `SM_EFF_ZERO_RAD` as a scaled zero (`SmTol::GetEffZeroAngRad`).
#[must_use]
pub fn eff_zero_ang_rad() -> f64 {
    tol3d(EFF_ZERO_RAD)
}

// ---- predicates --------------------------------------------------------------------

/// `|val| <= tol` (`SmTol::InTol(double, const SmTol3d&)`).
#[must_use]
pub fn in_tol(val: f64, tol: f64) -> bool {
    val.abs() <= tol
}

/// `SM_IS_ZERO(a)`: `|a| <=` [`scaled_zero`] (`SmMath.h`).
#[must_use]
pub fn is_zero(a: f64) -> bool {
    in_tol(a, scaled_zero())
}

/// `SM_IS_ZERO_SQUARED(a)`: `|a| <=` [`scaled_zero_sq`] (`SmMath.h`).
#[must_use]
pub fn is_zero_squared(a: f64) -> bool {
    in_tol(a, scaled_zero_sq())
}

/// `SM_IS_ZERO_TO_TOL(a, tol)`: `|a| <= |tol|` (`SmMath.h`).
#[must_use]
pub fn is_zero_to_tol(a: f64, tol: f64) -> bool {
    in_tol(a, tol3d(tol))
}

/// `SM_ARE_SAME(a, b)`: `|a - b| <=` the larger scaled zero of `a` and `b`
/// (`SmMath.h`).
#[must_use]
pub fn are_same(a: f64, b: f64) -> bool {
    in_tol(a - b, scaled_zero2(&a, &b))
}

/// `SM_ARE_SAME_TO_TOL(a, b, tol)`: `|a - b| <= |tol|` (`SmMath.h`).
#[must_use]
pub fn are_same_to_tol(a: f64, b: f64, tol: f64) -> bool {
    in_tol(a - b, tol3d(tol))
}

/// True when `speed` is zero relative to the size `max_pos_dimension`:
/// `|speed| < override` or, without override, `|speed| < SM_EFF_ZERO * (1 + |max|)`
/// (`SmTol::IsZeroSpeed`).
#[must_use]
pub fn is_zero_speed(
    speed: f64,
    max_pos_dimension: f64,
    override_scaled_zero: Option<f64>,
) -> bool {
    speed.abs()
        < match override_scaled_zero {
            Some(z) => z,
            None => EFF_ZERO * (1.0 + max_pos_dimension.abs()),
        }
}

/// True when `v` is shorter than the intersection tolerance (the override, else the
/// system [`xsect_tol_3d`]) (`SmTol::IsDegenerate(const SmVector3d, SmXSectTol3d*)`).
#[must_use]
pub fn is_degenerate_vec3(t: &Tolerances, v: Vec3, override_xsect_tol: Option<f64>) -> bool {
    let tol = match override_xsect_tol {
        Some(t) => t,
        None => xsect_tol_3d(t, SmallTopoType::NormalSize),
    };
    v.is_zero(tol)
}

/// True when `v1` and `v2` are parallel within the angle tolerance (the override in
/// radians, else [`ang_tol_rad`]) (`SmTol::AreParallel`).
#[must_use]
pub fn are_parallel(v1: Vec3, v2: Vec3, override_ang_tol_rad: Option<f64>) -> bool {
    let ang = override_ang_tol_rad.unwrap_or_else(ang_tol_rad);
    v1.is_parallel_to(v2, rad_to_deg(ang))
}

/// True when `v1` and `v2` are perpendicular within the angle tolerance (the override
/// in radians, else [`ang_tol_rad`]) (`SmTol::ArePerpendicular`).
#[must_use]
pub fn are_perpendicular(v1: Vec3, v2: Vec3, override_ang_tol_rad: Option<f64>) -> bool {
    let ang = override_ang_tol_rad.unwrap_or_else(ang_tol_rad);
    v1.is_perpendicular_to(v2, rad_to_deg(ang))
}

// ---- parameter-space mapping ------------------------------------------------------

/// Maps the 3D tolerance `tol` to a curve's parameter space at a point with first
/// derivative `deriv`: `tol / |deriv|`, or `natural_interval.length() / 100` where the
/// derivative is (near) zero (`SmTol::MapTo1d(SmTol3d&, SmVector3d&, const
/// SmExtent1d&)`). `tol` is the value of an `SmTol3d` (already `fabs`).
#[must_use]
pub fn map_to_1d(tol: f64, deriv: Vec3, natural_interval: &Extent1d) -> f64 {
    let scaled_zero = scaled_zero();
    let deriv_size = deriv.length();
    tol3d(if deriv_size < scaled_zero {
        natural_interval.length() / 100.0
    } else {
        tol / deriv_size
    })
}

// ---- scaled zeros ------------------------------------------------------------------

/// The default scaled zero, `SM_EFF_ZERO` (`SmTol::GetScaledZero()`).
#[must_use]
pub fn scaled_zero() -> f64 {
    tol3d(EFF_ZERO)
}

/// `SM_EFF_ZERO_SQ` (`SmTol::GetScaledZeroSq()`).
#[must_use]
pub fn scaled_zero_sq() -> f64 {
    tol3d(EFF_ZERO_SQ)
}

/// `SM_EFF_ZERO_SQRT` (`SmTol::GetScaledZeroSqrt()`).
#[must_use]
pub fn scaled_zero_sqrt() -> f64 {
    tol3d(EFF_ZERO_SQRT)
}

/// Values with a size-relative numerical zero (the unary `SmTol::GetScaledZero`
/// overloads): `SM_EFF_ZERO * (1 + size)`.
pub trait ScaledZero {
    /// The scaled zero of `self`.
    fn scaled_zero(&self) -> f64;
}

impl ScaledZero for f64 {
    /// `SmTol::GetScaledZero(const double)`: `SM_EFF_ZERO * (1 + |d|)`.
    fn scaled_zero(&self) -> f64 {
        tol3d(EFF_ZERO * (1.0 + self.abs()))
    }
}

impl ScaledZero for Vec2 {
    /// `SmTol::GetScaledZero(const SmVector2d&)`: `SM_EFF_ZERO * (1 + max |coord|)`.
    fn scaled_zero(&self) -> f64 {
        tol3d(EFF_ZERO * (1.0 + self.max_dimension()))
    }
}

impl ScaledZero for Vec3 {
    /// `SmTol::GetScaledZero(const SmVector3d&)`: `SM_EFF_ZERO * (1 + max |coord|)`.
    fn scaled_zero(&self) -> f64 {
        tol3d(EFF_ZERO * (1.0 + self.max_dimension()))
    }
}

impl ScaledZero for Extent1d {
    /// `SmTol::GetScaledZero(const SmExtent1d&)`: `SM_EFF_ZERO * (1 + max |end|)`.
    fn scaled_zero(&self) -> f64 {
        tol3d(EFF_ZERO * (1.0 + self.max_dimension()))
    }
}

impl ScaledZero for Extent3d {
    /// `SmTol::GetScaledZero(const SmExtent3d&)`: `SM_EFF_ZERO * (1 + max |coord|)`
    /// over both corners.
    fn scaled_zero(&self) -> f64 {
        tol3d(EFF_ZERO * (1.0 + max2(self.min().max_dimension(), self.max().max_dimension())))
    }
}

/// Square of the scaled zero of `a` (unary `SmTol::GetScaledZeroSq<cA>`).
#[must_use]
pub fn scaled_zero_sq_of<A: ScaledZero + ?Sized>(a: &A) -> f64 {
    let r = a.scaled_zero();
    tol3d(r * r)
}

/// Square root of the scaled zero of `a` (unary `SmTol::GetScaledZeroSqrt<cA>`).
#[must_use]
pub fn scaled_zero_sqrt_of<A: ScaledZero + ?Sized>(a: &A) -> f64 {
    tol3d(sm_sqrt(a.scaled_zero()))
}

/// Largest scaled zero of two values (binary `SmTol::GetScaledZero<cA, cB>`).
#[must_use]
pub fn scaled_zero2<A: ScaledZero + ?Sized, B: ScaledZero + ?Sized>(a: &A, b: &B) -> f64 {
    max2(a.scaled_zero(), b.scaled_zero())
}

/// Square of [`scaled_zero2`] (binary `SmTol::GetScaledZeroSq<cA, cB>`).
#[must_use]
pub fn scaled_zero_sq2<A: ScaledZero + ?Sized, B: ScaledZero + ?Sized>(a: &A, b: &B) -> f64 {
    let r = scaled_zero2(a, b);
    tol3d(r * r)
}

/// Square root of [`scaled_zero2`] (binary `SmTol::GetScaledZeroSqrt<cA, cB>`).
#[must_use]
pub fn scaled_zero_sqrt2<A: ScaledZero + ?Sized, B: ScaledZero + ?Sized>(a: &A, b: &B) -> f64 {
    tol3d(sm_sqrt(scaled_zero2(a, b)))
}

/// Largest scaled zero of three values (tertiary `SmTol::GetScaledZero<cA, cB, cC>`).
#[must_use]
pub fn scaled_zero3<A, B, C>(a: &A, b: &B, c: &C) -> f64
where
    A: ScaledZero + ?Sized,
    B: ScaledZero + ?Sized,
    C: ScaledZero + ?Sized,
{
    max3(a.scaled_zero(), b.scaled_zero(), c.scaled_zero())
}

/// Square of [`scaled_zero3`] (tertiary `SmTol::GetScaledZeroSq`).
#[must_use]
pub fn scaled_zero_sq3<A, B, C>(a: &A, b: &B, c: &C) -> f64
where
    A: ScaledZero + ?Sized,
    B: ScaledZero + ?Sized,
    C: ScaledZero + ?Sized,
{
    let r = scaled_zero3(a, b, c);
    tol3d(r * r)
}

/// Square root of [`scaled_zero3`] (tertiary `SmTol::GetScaledZeroSqrt`).
#[must_use]
pub fn scaled_zero_sqrt3<A, B, C>(a: &A, b: &B, c: &C) -> f64
where
    A: ScaledZero + ?Sized,
    B: ScaledZero + ?Sized,
    C: ScaledZero + ?Sized,
{
    tol3d(sm_sqrt(scaled_zero3(a, b, c)))
}

/// Largest scaled zero of four values (quaternary `SmTol::GetScaledZero`).
#[must_use]
pub fn scaled_zero4<A, B, C, D>(a: &A, b: &B, c: &C, d: &D) -> f64
where
    A: ScaledZero + ?Sized,
    B: ScaledZero + ?Sized,
    C: ScaledZero + ?Sized,
    D: ScaledZero + ?Sized,
{
    max4(
        a.scaled_zero(),
        b.scaled_zero(),
        c.scaled_zero(),
        d.scaled_zero(),
    )
}

/// Square of [`scaled_zero4`] (quaternary `SmTol::GetScaledZeroSq`).
#[must_use]
pub fn scaled_zero_sq4<A, B, C, D>(a: &A, b: &B, c: &C, d: &D) -> f64
where
    A: ScaledZero + ?Sized,
    B: ScaledZero + ?Sized,
    C: ScaledZero + ?Sized,
    D: ScaledZero + ?Sized,
{
    let r = scaled_zero4(a, b, c, d);
    tol3d(r * r)
}

/// Square root of [`scaled_zero4`] (quaternary `SmTol::GetScaledZeroSqrt`).
#[must_use]
pub fn scaled_zero_sqrt4<A, B, C, D>(a: &A, b: &B, c: &C, d: &D) -> f64
where
    A: ScaledZero + ?Sized,
    B: ScaledZero + ?Sized,
    C: ScaledZero + ?Sized,
    D: ScaledZero + ?Sized,
{
    tol3d(sm_sqrt(scaled_zero4(a, b, c, d)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tolerances(model: f64, ratio: f64) -> Tolerances {
        let model = if model == USE_DEFAULT {
            MODEL_SIZE_ESTIMATE
        } else {
            model
        };
        let zone = zone_for_size(model);
        Tolerances {
            model_size_estimate: model,
            zone_tol_3d: zone,
            xsect_tol_3d: zone * 2.0,
            approx_tol_3d: zone / 2.0,
            large_small_size_ratio: ratio,
            ..Tolerances::default()
        }
    }

    #[test]
    fn size_estimate_table() {
        // SmTol.cpp sm_TemplateTrial.
        assert_eq!(zone_for_size(50.0), 1.0e-5);
        assert_eq!(zone_for_size(50000.0), 1.0e-4);
        assert_eq!(zone_for_size(0.005), 1.0e-6);
        assert_eq!(zone_for_size(USE_DEFAULT), 1.0e-5);
        assert_eq!(zone_for_size(1.0e8), 1.0e-1);
        assert_eq!(zone_for_size(0.0), 0.0);
    }

    #[test]
    fn system_tolerances() {
        let k = Tolerances::default();
        assert_eq!(zone_tol_3d(&k, SmallTopoType::NormalSize), 1.0e-5);
        assert_eq!(xsect_tol_3d(&k, SmallTopoType::NormalSize), 2.0e-5);
        assert_eq!(approx_tol_3d(&k, SmallTopoType::NormalSize), 5.0e-6);
        assert_eq!(zone_tol_3d(&k, SmallTopoType::SmallSize), 1000.0 * 1.0e-5);
        assert_eq!(gap_gain(&k), 1.1);
        assert_eq!(model_size_estimate(&k), 50.0);
        let k = tolerances(50000.0, -2.0);
        assert_eq!(zone_tol_3d(&k, SmallTopoType::ForceSmallSize), 2.0e-4);
    }

    #[test]
    fn tol_constructors() {
        let k = Tolerances::default();
        assert_eq!(tol3d(-0.5), 0.5);
        assert_eq!(zone_tol3d(&k, USE_DEFAULT), 1.0e-5);
        assert_eq!(xsect_tol3d(&k, USE_DEFAULT), 2.0e-5);
        assert_eq!(approx_tol3d(&k, USE_DEFAULT), 5.0e-6);
        assert_eq!(zone_tol3d(&k, -3.0), 3.0);
        assert_eq!(tol_mul(tol3d(-2.0), -3.0), -6.0);
    }

    #[test]
    fn in_tol_and_predicates() {
        // SmTol.cpp sm_TemplateTrial: InTol(.001, .004) etc.
        assert!(in_tol(0.001, 0.004));
        assert!(in_tol(-0.001, 0.004));
        assert!(!in_tol(0.008, 0.004));
        assert!(is_zero_speed(1e-13, 0.0, None));
        assert!(!is_zero_speed(1e-11, 0.0, None));
        assert!(is_zero_speed(0.5, 0.0, Some(1.0)));
        let k = Tolerances::default();
        assert!(is_degenerate_vec3(&k, Vec3::new(1e-6, 0.0, 0.0), None));
        assert!(!is_degenerate_vec3(&k, Vec3::new(1e-4, 0.0, 0.0), None));
        assert!(are_parallel(Vec3::X, -Vec3::X, None));
        assert!(!are_parallel(Vec3::X, Vec3::Y, None));
        assert!(are_perpendicular(Vec3::X, Vec3::Y, Some(0.1)));
        assert!(!are_perpendicular(Vec3::X, Vec3::ZERO, Some(0.1)));
    }

    #[test]
    fn scaled_zeros() {
        // SmTol.cpp sm_TemplateTrial values.
        let v2 = Vec2::new(1.0, -80.0);
        let v3 = Vec3::new(16.0, 100.0, 1000.0);
        let e3 = Extent3d::from_bounds(0.0, -30.0, -600.0, 2000.0, 100.0, 10000.0);
        assert_eq!(v2.scaled_zero(), EFF_ZERO * 81.0);
        assert_eq!(v3.scaled_zero(), EFF_ZERO * 1001.0);
        assert_eq!(e3.scaled_zero(), EFF_ZERO * 10001.0);
        assert_eq!(scaled_zero2(&v2, &v3), EFF_ZERO * 1001.0);
        assert_eq!(scaled_zero3(&v2, &v3, &e3), EFF_ZERO * 10001.0);
        assert_eq!(scaled_zero4(&1.0, &v2, &v3, &e3), EFF_ZERO * 10001.0);
        let z = 7.0f64.scaled_zero();
        assert_eq!(scaled_zero_sq_of(&7.0), z * z);
        assert_eq!(scaled_zero_sqrt_of(&7.0), z.sqrt());
        assert_eq!(scaled_zero(), EFF_ZERO);
        assert_eq!(scaled_zero_sq(), EFF_ZERO_SQ);
        assert_eq!(scaled_zero_sqrt(), EFF_ZERO_SQRT);
        assert_eq!(eff_zero_param(), EFF_ZERO_PARAM);
        assert_eq!(eff_zero_ang_rad(), EFF_ZERO_RAD);
        assert_eq!(eff_zero(), EFF_ZERO);
        assert_eq!(eff_zero_sq(), EFF_ZERO_SQ);
    }
    #[test]
    fn coincidence_is_the_larger_tolerance_and_a_nan_tolerance_is_apart() {
        let k = Tolerances::default();
        let contact = contact_tol_3d(&k);
        assert_eq!(coincidence_tol_3d(&k, 0.0, 0.0), contact);
        assert_eq!(
            coincidence_tol_3d(&k, 4.0 * contact, contact),
            4.0 * contact
        );
        assert!(within_coincidence_tol(&k, contact, 0.0, 0.0, 1.0));
        assert!(!within_coincidence_tol(
            &k,
            2.0 * contact,
            contact,
            contact,
            1.0
        ));
        // an unknown tolerance never makes two places one, however near they are
        for (zone1, zone2) in [
            (f64::NAN, contact),
            (contact, f64::NAN),
            (f64::NAN, f64::NAN),
        ] {
            assert!(coincidence_tol_3d(&k, zone1, zone2).is_nan());
            assert!(!within_coincidence_tol(&k, 0.0, zone1, zone2, 1.0));
        }
        assert!(!within_coincidence_tol(&k, f64::NAN, contact, contact, 1.0));
    }

    #[test]
    fn zone_for_size_boundaries_keep_todays_answers() {
        // the checks run 0.5..=500, 500..=50000, 0.005..=0.5, 50000..=5e6,
        // 0.00005..=0.005: a boundary takes the first range listed that holds it
        assert_eq!(zone_for_size(0.5), 1.0e-5);
        assert_eq!(zone_for_size(500.0), 1.0e-5);
        assert_eq!(zone_for_size(50000.0), 1.0e-4);
        assert_eq!(zone_for_size(0.005), 1.0e-6);
        assert_eq!(zone_for_size(5e6), 1.0e-3);
        assert_eq!(zone_for_size(0.00005), 1.0e-7);
        assert!(zone_for_size(f64::NAN).is_nan());
    }

    #[test]
    fn small_topo_codes_round_trip() {
        for c in 0..5 {
            assert_eq!(
                SmallTopoType::from_code(c).map(SmallTopoType::code),
                Some(c)
            );
        }
        assert_eq!(SmallTopoType::from_code(5), None);
    }

    #[test]
    fn default_tolerances_are_solidcores() {
        let t = Tolerances::default();
        assert_eq!(
            (
                t.zone_tol_3d,
                t.xsect_tol_3d,
                t.approx_tol_3d,
                t.ang_tol_deg
            ),
            (1.0e-5, 2.0e-5, 5.0e-6, 0.5)
        );
        assert_eq!(
            (
                t.tol_gap_gain,
                t.model_size_estimate,
                t.large_small_size_ratio
            ),
            (1.1, 50.0, 1000.0)
        );
        assert_eq!(t.ang_tol_rad(), 0.5 * PI / 180.0);
    }
}

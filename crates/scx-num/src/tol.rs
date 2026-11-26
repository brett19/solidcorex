// Moved from solidcore `solidcore/crates/solidcore/src/geom/tol.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8, with `Tolerances` from
// `solidcore/crates/solidcore/src/kernel.rs` (l.46-89) at the same commit.
// Upstream record (verbatim from those files): tol.rs: Upstream: `source/SMLib/inc/SmTol.h`,
// `source/SMLib/src/SmTol.cpp`, `source/SMLib/inc/SmRelation.h`. kernel.rs: Upstream:
// `source/SMLib/inc/SmContext.h` (`SmContext`), `SmTol.h` (`SmTol`, `SmTol3d`), the
// tolerance defaults of `SmMath.h`, the `SmCacheMgr*` classes, and `SM_API`'s
// `SmApiGetOrCreateContext()`.
// Changes: the scalar part only (no context, B-rep, curve, volume, equation-set or
// `SmRelation` overloads, no `update_object_tolerance`); `size_estimate_to_zone_tol_3d`
// renamed `zone_for_size`; paths. Then `Tolerances` retired (docs/01-architecture.md
// §10.1, docs/09-tolerances.md §3.1): `zone_tol3d`, `xsect_tol3d` and `approx_tol3d` take
// a `Resolution` and an `Option<f64>` in place of `&Tolerances` and the `SM_USE_DEFAULT`
// sentinel; `zone_tol_3d`, `approx_tol_3d`, `xsect_tol_3d`, `contact_tol_3d` and
// `is_degenerate_vec3` take a `Resolution`, and the first reads the
// `LARGE_SMALL_SIZE_RATIO` constant (no uninitialised-tolerance branch: a resolution is
// above 0 by construction); `xsect_tol_3d_from_zones`, `coincidence_reach_3d` and
// `within_coincidence_tol` take `Accuracy` values, and `coincidence_tol_3d` became
// `units::coincident`; `are_parallel` and `are_perpendicular` take the angle, no
// default; `zone_for_size` lost its `SM_USE_DEFAULT` branch; `model_size_estimate`,
// `gap_gain`, `ang_tol_rad`, `Tolerances::ang_tol_rad`, `Tolerances::default` and the
// constants `USE_DEFAULT`, `UNINIT_TOL`, `ZONE_TOL_3D`, `TOL_GAP_GAIN`, `ANG_TOL_DEG`
// and `ANG_TOL_RAD` removed (the angle is `consts::PARALLEL_ANGLE_DEG`/`_RAD`). Tests:
// `system_tolerances`, `tol_constructors`, `in_tol_and_predicates`,
// `size_estimate_table` and the coincidence test read a `Resolution`;
// `default_tolerances_are_solidcores` became `default_resolution_is_solidcores`; added
// `zone_for_size_boundaries_keep_todays_answers` and `small_topo_codes_round_trip`.
// Then the transcendental functions became `fmath`'s (correctly rounded, the same bits on every
// platform) in place of the C library's, so results may differ from the pin's in the last bit.
//! Tolerance policy (`SmTol`), the scalar part: the tolerance constructors, model-size
//! based default tolerances, zone / intersection / approximation tolerances, the
//! contact and coincidence rules, scaled zeros and simple tolerance predicates.
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
//! | [`crate::units::coincident`], [`coincidence_reach_3d`], [`within_coincidence_tol`] | not upstream: when two places are one, the larger of the contact tolerance and the two accuracies |
//! | [`zone_reach_3d`]                      | not upstream: when two places are one within a body's own tolerance, no contact floor |
//! | [`crate::consts::PARALLEL_ANGLE_RAD`]  | `SmTol::GetAngTolRad()`, `SM_ANG_TOL_RAD`      |
//! | [`MODEL_SIZE_ESTIMATE`]                | `SmTol::GetModelSizeEstimate()` (the default)  |
//! | [`are_parallel`]                       | `SmTol::AreParallel`                           |
//! | [`are_perpendicular`]                  | `SmTol::ArePerpendicular`                      |
//! | [`is_zero_speed`]                      | `SmTol::IsZeroSpeed`                           |
//! | [`is_degenerate_vec3`]                 | `SmTol::IsDegenerate(const SmVector3d, SmXSectTol3d*)` |
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
//! (`s_ZoneTol3d`, `s_TolGapGain`, `s_ModelSizeEstimate`, `s_LargeSmallSizeRatio`), and
//! solidcore in a `Tolerances` value on its kernel. Here the one that varies, the zone,
//! is the operation's [`Resolution`], passed by value; the others are constants
//! ([`MODEL_SIZE_ESTIMATE`], [`LARGE_SMALL_SIZE_RATIO`],
//! [`crate::consts::PARALLEL_ANGLE_RAD`]); the gap gain has no reader at this layer and
//! is not kept (`docs/09-tolerances.md` §3.1).
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
//!   so that `fabs` happens at the same places. `SM_USE_DEFAULT` ("use the system
//!   value") is `None`.
//! - Optional pointer arguments become `Option<f64>` holding the pointee's value.
//! - `SM_ASSERT*`, `WARN`, `ERR_MSG` and `SM_DEBUG_CODE` blocks are not ported (they do
//!   not change results in the release build).

use crate::certified::gamma;
use crate::extent1d::Extent1d;
use crate::extent3d::Extent3d;
use crate::math::{
    max2, max3, max4, rad_to_deg, Vec3, EFF_ZERO, EFF_ZERO_PARAM, EFF_ZERO_RAD, EFF_ZERO_SQ,
    EFF_ZERO_SQRT,
};
use crate::units::{coincident, Accuracy, Resolution};
use crate::vector2d::Vec2;
use crate::vector3d::sqrt as sm_sqrt;

/// `SM_MODEL_SIZE_ESTIMATE`: the model size [`zone_for_size`] reads when none is given.
pub const MODEL_SIZE_ESTIMATE: f64 = 50.0;
/// `SM_LARGE_SMALL_SIZE_RATIO`: the large/small topology size ratio, the factor of a
/// small-topology zone ([`zone_tol_3d`]). Owner: the tolerance policy. Its own literal,
/// as at the pin: the validity limit [`crate::consts::MAX_ACCURACY_RATIO`] has the same
/// value but is a separate decision.
pub const LARGE_SMALL_SIZE_RATIO: f64 = 1000.0;

// ---- tolerance values ------------------------------------------------------------

/// `SmTol3d(double)` (and the `SmScaledZero`, `SmTol2d`, `SmTol1d` constructors and
/// every `operator=(double)`): a tolerance is stored as `fabs(d)`.
#[must_use]
pub fn tol3d(d: f64) -> f64 {
    d.abs()
}

/// `SmZoneTol3d(double)`: `fabs(d)`, or with no `d` (upstream's `SM_USE_DEFAULT`) the
/// resolution.
#[must_use]
pub fn zone_tol3d(res: Resolution, d: Option<f64>) -> f64 {
    tol3d(d.unwrap_or_else(|| res.mm()))
}

/// `SmXSectTol3d(double)`: `fabs(d)`, or with no `d` twice the resolution.
#[must_use]
pub fn xsect_tol3d(res: Resolution, d: Option<f64>) -> f64 {
    tol3d(d.unwrap_or_else(|| res.mm() * 2.0))
}

/// `SmApproxTol3d(double)`: `fabs(d)`, or with no `d` half the resolution.
#[must_use]
pub fn approx_tol3d(res: Resolution, d: Option<f64>) -> f64 {
    tol3d(d.unwrap_or_else(|| res.mm() / 2.0))
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
/// Outside them the zone is `10^floor(log10(|size|) / 2) * 1e-5`. Upstream's
/// `SM_USE_DEFAULT` size (no estimate) is the caller's [`MODEL_SIZE_ESTIMATE`]. (The
/// `SmZoneTol3d` conversions of the result only apply `fabs`.)
#[must_use]
pub fn zone_for_size(size: f64) -> f64 {
    if (0.5..=500.0).contains(&size) {
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
        tol3d(crate::fmath::powf(10.0, (crate::fmath::log10(size.abs()) / 2.0).floor()) * 1.0e-5)
    }
}

/// The zone tolerance at resolution `res` (`SmTol::GetZoneTol3d(SmSmallTopoType)`):
/// multiplied by [`LARGE_SMALL_SIZE_RATIO`] for small topology. (Upstream's
/// uninitialised-tolerance branch has no counterpart: a resolution is above 0.)
#[must_use]
pub fn zone_tol_3d(res: Resolution, small: SmallTopoType) -> f64 {
    let zone = res.mm();
    if small.is_small() {
        tol3d(tol3d(LARGE_SMALL_SIZE_RATIO) * zone)
    } else {
        zone
    }
}

/// The system approximation tolerance, half the zone tolerance
/// (`SmTol::GetApproxTol3d(SmSmallTopoType)`).
#[must_use]
pub fn approx_tol_3d(res: Resolution, small: SmallTopoType) -> f64 {
    tol3d(0.5 * zone_tol_3d(res, small))
}

/// The system intersection tolerance, twice the zone tolerance
/// (`SmTol::GetXSectTol3d(SmSmallTopoType)`).
#[must_use]
pub fn xsect_tol_3d(res: Resolution, small: SmallTopoType) -> f64 {
    tol3d(2.0 * zone_tol_3d(res, small))
}

/// The intersection tolerance of two objects with accuracies `a`, `b`: their sum
/// (`SmTol::GetXSectTol3d(const SmZoneTol3d&, const SmZoneTol3d&)`).
#[must_use]
pub fn xsect_tol_3d_from_zones(a: Accuracy, b: Accuracy) -> f64 {
    tol3d(a.sum(b).mm())
}

/// The tolerance up to which two circles or parallel cylinders, or a circle and a line in
/// its plane, that overlap past touching touch on the boolean's path (read by
/// [`within_contact_tol`]): the resolution, 1e-5 by default. Not upstream (WR-787, #15429, #14599): one value for every
/// site that decides the contact (the walls, the rims, the curve/surface classifier),
/// whatever tolerances earlier operations grew the edges and faces to, so the same two
/// holes give the same topology whatever came before. The touching point midway lies δ/2 from each operand, so below it the
/// point lies within the approximation tolerance (half the zone tolerance) of both, as
/// the intersection curves through it do. At the intersection tolerance (twice the zone
/// tolerance) two through holes overlapping by 1.02e-5 to 1.99e-5 came out a valid solid
/// that kept the second tool whole, as a void.
#[must_use]
pub fn contact_tol_3d(res: Resolution) -> f64 {
    zone_tol_3d(res, SmallTopoType::NormalSize)
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

/// The largest distance at which two places coincide (are one place), on two objects of
/// accuracies `a` and `b`: the tolerance [`crate::units::coincident`], the larger of the
/// contact tolerance (the resolution, [`contact_tol_3d`]) and the two accuracies, and the
/// rounding of the copies past it, `size` the largest coordinate of either place, as
/// [`within_contact_tol`] reads a contact. Not upstream: one rule for every site
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
pub fn coincidence_reach_3d(res: Resolution, a: Accuracy, b: Accuracy, size: f64) -> f64 {
    coincident(res, a, b).mm() + rounding_of_copies(size)
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

/// Whether two places `distance` apart, on objects of accuracies `a` and `b`, coincide
/// at resolution `res`: `distance` is within [`coincidence_reach_3d`]. Not upstream: the
/// one reading of the coincidence rule. A NaN distance is apart.
#[must_use]
pub fn within_coincidence_tol(
    res: Resolution,
    distance: f64,
    a: Accuracy,
    b: Accuracy,
    size: f64,
) -> bool {
    distance <= coincidence_reach_3d(res, a, b, size)
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

/// True when `v` is shorter than the intersection tolerance (the override, else
/// [`xsect_tol_3d`] at `res`) (`SmTol::IsDegenerate(const SmVector3d, SmXSectTol3d*)`).
#[must_use]
pub fn is_degenerate_vec3(res: Resolution, v: Vec3, override_xsect_tol: Option<f64>) -> bool {
    let tol = match override_xsect_tol {
        Some(t) => t,
        None => xsect_tol_3d(res, SmallTopoType::NormalSize),
    };
    v.is_zero(tol)
}

/// True when `v1` and `v2` are parallel within `max_angle` radians
/// (`SmTol::AreParallel`; upstream's default angle is
/// [`crate::consts::PARALLEL_ANGLE_RAD`], which a caller now names).
#[must_use]
pub fn are_parallel(v1: Vec3, v2: Vec3, max_angle: f64) -> bool {
    v1.is_parallel_to(v2, rad_to_deg(max_angle))
}

/// True when `v1` and `v2` are perpendicular within `max_angle` radians
/// (`SmTol::ArePerpendicular`; upstream's default angle is
/// [`crate::consts::PARALLEL_ANGLE_RAD`]).
#[must_use]
pub fn are_perpendicular(v1: Vec3, v2: Vec3, max_angle: f64) -> bool {
    v1.is_perpendicular_to(v2, rad_to_deg(max_angle))
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
    use crate::consts::{PARALLEL_ANGLE_RAD, ZONE_DEFAULT};

    fn res(mm: f64) -> Resolution {
        Resolution::from_mm(mm).unwrap_or(Resolution::DEFAULT)
    }
    fn acc(mm: f64) -> Accuracy {
        Accuracy::from_mm(mm).unwrap_or(Accuracy::ZERO)
    }

    #[test]
    fn size_estimate_table() {
        // SmTol.cpp sm_TemplateTrial.
        assert_eq!(zone_for_size(50.0), 1.0e-5);
        assert_eq!(zone_for_size(50000.0), 1.0e-4);
        assert_eq!(zone_for_size(0.005), 1.0e-6);
        assert_eq!(zone_for_size(MODEL_SIZE_ESTIMATE), 1.0e-5);
        assert_eq!(zone_for_size(1.0e8), 1.0e-1);
        assert_eq!(zone_for_size(0.0), 0.0);
    }

    #[test]
    fn system_tolerances() {
        let k = Resolution::DEFAULT;
        assert_eq!(zone_tol_3d(k, SmallTopoType::NormalSize), 1.0e-5);
        assert_eq!(xsect_tol_3d(k, SmallTopoType::NormalSize), 2.0e-5);
        assert_eq!(approx_tol_3d(k, SmallTopoType::NormalSize), 5.0e-6);
        assert_eq!(zone_tol_3d(k, SmallTopoType::SmallSize), 1000.0 * 1.0e-5);
        let k = res(zone_for_size(50000.0));
        assert_eq!(
            zone_tol_3d(k, SmallTopoType::ForceSmallSize),
            1000.0 * 1.0e-4
        );
    }

    #[test]
    fn tol_constructors() {
        let k = Resolution::DEFAULT;
        assert_eq!(tol3d(-0.5), 0.5);
        assert_eq!(zone_tol3d(k, None), 1.0e-5);
        assert_eq!(xsect_tol3d(k, None), 2.0e-5);
        assert_eq!(approx_tol3d(k, None), 5.0e-6);
        assert_eq!(zone_tol3d(k, Some(-3.0)), 3.0);
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
        let k = Resolution::DEFAULT;
        assert!(is_degenerate_vec3(k, Vec3::new(1e-6, 0.0, 0.0), None));
        assert!(!is_degenerate_vec3(k, Vec3::new(1e-4, 0.0, 0.0), None));
        assert!(are_parallel(Vec3::X, -Vec3::X, PARALLEL_ANGLE_RAD));
        assert!(!are_parallel(Vec3::X, Vec3::Y, PARALLEL_ANGLE_RAD));
        assert!(are_perpendicular(Vec3::X, Vec3::Y, 0.1));
        assert!(!are_perpendicular(Vec3::X, Vec3::ZERO, 0.1));
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
    fn coincidence_is_the_larger_tolerance() {
        let k = Resolution::DEFAULT;
        let contact = contact_tol_3d(k);
        assert_eq!(coincident(k, Accuracy::ZERO, Accuracy::ZERO).mm(), contact);
        assert_eq!(
            coincident(k, acc(4.0 * contact), acc(contact)).mm(),
            4.0 * contact
        );
        assert!(within_coincidence_tol(
            k,
            contact,
            Accuracy::ZERO,
            Accuracy::ZERO,
            1.0
        ));
        assert!(!within_coincidence_tol(
            k,
            2.0 * contact,
            acc(contact),
            acc(contact),
            1.0
        ));
        assert!(!within_coincidence_tol(
            k,
            f64::NAN,
            acc(contact),
            acc(contact),
            1.0
        ));
        assert_eq!(
            xsect_tol_3d_from_zones(acc(1.0e-5), acc(2.0e-5)),
            1.0e-5 + 2.0e-5
        );
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
    fn default_resolution_is_solidcores() {
        // solidcore's default `Tolerances`: zone 1e-5, xsect 2e-5, approx 5e-6, angle 0.5
        // degrees, model 50, ratio 1000
        let k = Resolution::DEFAULT;
        assert_eq!(k.mm(), ZONE_DEFAULT);
        assert_eq!(
            (
                zone_tol3d(k, None),
                xsect_tol3d(k, None),
                approx_tol3d(k, None)
            ),
            (1.0e-5, 2.0e-5, 5.0e-6)
        );
        assert_eq!(crate::consts::PARALLEL_ANGLE_DEG, 0.5);
        assert_eq!(
            (MODEL_SIZE_ESTIMATE, LARGE_SMALL_SIZE_RATIO),
            (50.0, 1000.0)
        );
    }
}

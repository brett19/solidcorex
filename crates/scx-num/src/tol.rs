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
// Then the tolerance helpers moved onto the newtypes (docs/research/57-idiom-audit.md M5, as
// its review amends it): `zone_tol3d`, `xsect_tol3d` and `approx_tol3d` became
// `Resolution::zone`, `xsect` and `approx` (an explicit tolerance is the caller's own
// `Length`); their look-alikes `zone_tol_3d`, `xsect_tol_3d` and `approx_tol_3d` became
// `Resolution::topology_zone` (the caller doubles or halves it); `contact_tol_3d` became
// `Resolution::contact`; `xsect_tol_3d_from_zones` became `Accuracy::sum`; `tol3d` and
// `tol_mul` removed (`f64::abs` and `*`); `eff_zero`, `eff_zero_sq`, `eff_zero_param`,
// `eff_zero_ang_rad`, `scaled_zero()`, `scaled_zero_sq()` and `scaled_zero_sqrt()` removed
// for the `math` constants they returned; `scaled_zero2`/`3`/`4`, their `_sq` and `_sqrt`
// forms, `scaled_zero_sq_of` and `scaled_zero_sqrt_of` became one `scaled_zero` over
// magnitudes (the caller squares it or takes its root); `is_degenerate_vec3(res, v, Option)`
// became `is_degenerate(v, Length)`; `is_zero_speed` lost its override (a caller with one
// compares); `zone_for_size`, `coincidence_reach_3d` and `zone_reach_3d` return a `Length`,
// `within_contact_tol` and `zone_reach_3d` take one, and `map_to_1d` takes a `Length` and
// returns a `ParamTol`. Tests read the new names.
//! Tolerance policy (`SmTol`), the scalar part: the tolerance constructors, model-size
//! based default tolerances, zone / intersection / approximation tolerances, the
//! contact and coincidence rules, scaled zeros and simple tolerance predicates.
//!
//! Upstream: `source/SMLib/inc/SmTol.h`, `source/SMLib/src/SmTol.cpp`.
//!
//! | Rust                                   | Upstream                                       |
//! |----------------------------------------|------------------------------------------------|
//! | (`f64::abs`)                           | `SmTol3d(double)`, `SmScaledZero(double)`, `SmTol2d(double)`, `SmTol1d(double)`, every `operator=(double)` of `SM_TOL_CALLS`, `SmTol3d::operator=(const SmTol3d)` |
//! | [`Resolution::zone`]                   | `SmZoneTol3d(SM_USE_DEFAULT)`                  |
//! | [`Resolution::xsect`]                  | `SmXSectTol3d(SM_USE_DEFAULT)`                 |
//! | [`Resolution::approx`]                 | `SmApproxTol3d(SM_USE_DEFAULT)`                |
//! | [`Length::from_mm`]                    | `SmZoneTol3d(double)`, `SmXSectTol3d(double)`, `SmApproxTol3d(double)` (an explicit tolerance) |
//! | (`*`)                                  | `operator*=(SmTol3d&, double)`                 |
//! | [`Length::mm`]                         | `operator double&`, `operator const double&` of every tolerance class |
//! | [`SmallTopoType`]                      | `SmSmallTopoType`                              |
//! | [`zone_for_size`]                      | `SmTol::SizeEstimateToZoneTol3d`               |
//! | [`in_tol`]                             | `SmTol::InTol(double, const SmTol3d&)`         |
//! | [`is_zero`], [`is_zero_squared`], [`is_zero_to_tol`] | `SM_IS_ZERO`, `SM_IS_ZERO_SQUARED`, `SM_IS_ZERO_TO_TOL` (`SmMath.h`) |
//! | [`are_same`], [`are_same_to_tol`]      | `SM_ARE_SAME`, `SM_ARE_SAME_TO_TOL` (`SmMath.h`) |
//! | [`EFF_ZERO`], [`EFF_ZERO_SQ`], [`EFF_ZERO_PARAM`], [`EFF_ZERO_RAD`] | `SmTol::GetEffZero`, `GetEffZeroSq`, `GetEffZeroParam`, `GetEffZeroAngRad` |
//! | [`Accuracy::sum`]                      | `SmTol::GetXSectTol3d(const SmZoneTol3d&, const SmZoneTol3d&)` |
//! | [`Resolution::contact`]                | not upstream (WR-787, #15429): when circles a hair less than tangent touch |
//! | [`within_contact_tol`]                 | not upstream: the one reading of a contact at the tolerance, shared by every contact site |
//! | [`crate::units::coincident`], [`coincidence_reach_3d`], [`within_coincidence_tol`] | not upstream: when two places are one, the larger of the contact tolerance and the two accuracies |
//! | [`zone_reach_3d`]                      | not upstream: when two places are one within a body's own tolerance, no contact floor |
//! | [`crate::consts::PARALLEL_ANGLE_RAD`]  | `SmTol::GetAngTolRad()`, `SM_ANG_TOL_RAD`      |
//! | [`MODEL_SIZE_ESTIMATE`]                | `SmTol::GetModelSizeEstimate()` (the default)  |
//! | [`are_parallel`]                       | `SmTol::AreParallel`                           |
//! | [`are_perpendicular`]                  | `SmTol::ArePerpendicular`                      |
//! | [`is_zero_speed`]                      | `SmTol::IsZeroSpeed`                           |
//! | [`is_degenerate`]                      | `SmTol::IsDegenerate(const SmVector3d, SmXSectTol3d*)` (the caller passes [`Resolution::xsect`] or its own) |
//! | [`Resolution::topology_zone`]          | `SmTol::GetZoneTol3d(SmSmallTopoType)` (system); `GetApproxTol3d` and `GetXSectTol3d(SmSmallTopoType)` are half and twice it |
//! | [`EFF_ZERO`], [`EFF_ZERO_SQ`], [`EFF_ZERO_SQRT`] | `SmTol::GetScaledZero()`, `GetScaledZeroSq()`, `GetScaledZeroSqrt()` |
//! | [`ScaledZero`] (`f64`, [`Vec2`], [`Vec3`], [`Extent1d`], `Extent2d`, [`Extent3d`]) | `SmTol::GetScaledZero(const double)`, `(const SmVector2d&)`, `(const SmVector3d&)`, `(const SmExtent1d&)`, `(const SmExtent2d&)` (impl in [`crate::extent2d`]), `(const SmExtent3d&)` |
//! | [`map_to_1d`]                          | `SmTol::MapTo1d(SmTol3d&, SmVector3d&, const SmExtent1d&)` |
//! | [`scaled_zero`]                        | the unary, binary, tertiary and quaternary `GetScaledZero<cA, ...>` templates (their `Sq` and `Sqrt` forms: the caller squares the result or takes its root) |
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
//! - The tolerance classes (`SmTol3d`, `SmZoneTol3d`, ...) are [`Length`] values (see
//!   [`crate::units`] for where a `Length` meets `f64`); upstream's `fabs` on conversion is
//!   `f64::abs` where the value can be negative, and nothing where it cannot.
//!   `SM_USE_DEFAULT` ("use the system value") is the [`Resolution`] method; a caller with
//!   its own tolerance passes it.
//! - Optional pointer arguments are gone: the caller passes the value it has, or the
//!   default by name.
//! - `SM_ASSERT*`, `WARN`, `ERR_MSG` and `SM_DEBUG_CODE` blocks are not ported (they do
//!   not change results in the release build).

use crate::certified::gamma;
use crate::extent1d::Extent1d;
use crate::extent3d::Extent3d;
use crate::math::{EFF_ZERO, EFF_ZERO_SQ, Vec3};
#[cfg(doc)]
use crate::math::{EFF_ZERO_PARAM, EFF_ZERO_RAD, EFF_ZERO_SQRT};
use crate::units::{Accuracy, Length, ParamTol, Resolution, coincident};
use crate::vector2d::Vec2;

/// `SM_MODEL_SIZE_ESTIMATE`: the model size [`zone_for_size`] reads when none is given.
pub const MODEL_SIZE_ESTIMATE: f64 = 50.0;
/// `SM_LARGE_SMALL_SIZE_RATIO`: the large/small topology size ratio, the factor of a
/// small-topology zone ([`Resolution::topology_zone`]). Owner: the tolerance policy. Its
/// own literal, as at the pin: the validity limit [`crate::consts::MAX_ACCURACY_RATIO`] has
/// the same value but is a separate decision.
pub const LARGE_SMALL_SIZE_RATIO: f64 = 1000.0;

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
/// `SM_USE_DEFAULT` size (no estimate) is the caller's [`MODEL_SIZE_ESTIMATE`]. A zero
/// size gives a zero zone and a NaN size a NaN one, neither of them a [`Resolution`].
#[must_use]
pub fn zone_for_size(size: f64) -> Length {
    Length::from_mm(if (0.5..=500.0).contains(&size) {
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
        (crate::fmath::powf(10.0, (crate::fmath::log10(size.abs()) / 2.0).floor()) * 1.0e-5).abs()
    })
}

impl Resolution {
    /// The zone tolerance for topology of size class `topo`
    /// (`SmTol::GetZoneTol3d(SmSmallTopoType)`): the zone, multiplied by
    /// [`LARGE_SMALL_SIZE_RATIO`] for small topology. Upstream's approximation and
    /// intersection tolerances of a size class (`GetApproxTol3d`, `GetXSectTol3d`) are half
    /// and twice it. (Upstream's uninitialised-tolerance branch has no counterpart: a
    /// resolution is above 0.)
    #[must_use]
    pub fn topology_zone(self, topo: SmallTopoType) -> Length {
        if topo.is_small() {
            self.zone() * LARGE_SMALL_SIZE_RATIO
        } else {
            self.zone()
        }
    }

    /// The tolerance up to which two circles or parallel cylinders, or a circle and a line
    /// in its plane, that overlap past touching touch on the boolean's path (read by
    /// [`within_contact_tol`]): the resolution, 1e-5 by default. Not upstream (WR-787,
    /// #15429, #14599; solidcore's `contact_tol_3d`): one value for every site that decides
    /// the contact (the walls, the rims, the curve/surface classifier), whatever tolerances
    /// earlier operations grew the edges and faces to, so the same two holes give the same
    /// topology whatever came before. The touching point midway lies δ/2 from each operand,
    /// so below it the point lies within the approximation tolerance (half the zone
    /// tolerance) of both, as the intersection curves through it do. At the intersection
    /// tolerance (twice the zone tolerance) two through holes overlapping by 1.02e-5 to
    /// 1.99e-5 came out a valid solid that kept the second tool whole, as a void.
    #[must_use]
    pub const fn contact(self) -> Length {
        self.zone()
    }
}

/// Whether `distance`, a length that decides a contact (how far two circles, or a circle
/// and a line in its plane, overlap past touching; how far a plane dips into a cylinder or
/// stands off a cone's generator; how far apart such curves lie all round), is within the
/// contact tolerance `contact`: not proved past it. Not upstream: the one reading of the
/// contact rule, shared by every site that decides a contact
/// (`circle_coplanar_circle_intersect_in_contact` and
/// `line_coplanar_circle_intersect_distinct` in `geom::geom_utility`, which the walls, the
/// rims and the curve/surface classifier call with [`Resolution::contact`]; a plane's section
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
pub fn within_contact_tol(distance: f64, contact: Length, size: f64) -> bool {
    distance <= (contact + rounding_of_copies(size)).mm()
}

/// How far a distance read off two copies of the same geometry can stand past the true
/// one by rounding alone: `γ_32` of `size`, the largest coordinate a point of either copy
/// has ([`within_contact_tol`] derives it). Shared by the contact and the coincidence rule,
/// so both read a length at their tolerance the same way.
fn rounding_of_copies(size: f64) -> Length {
    Length::from_mm(gamma(32) * size.abs())
}

/// The largest distance at which two places coincide (are one place), on two objects of
/// accuracies `a` and `b`: the tolerance [`crate::units::coincident`], the larger of the
/// contact tolerance (the resolution, [`Resolution::contact`]) and the two accuracies, and the
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
pub fn coincidence_reach_3d(res: Resolution, a: Accuracy, b: Accuracy, size: f64) -> Length {
    coincident(res, a, b) + rounding_of_copies(size)
}

/// The largest distance at which two places held to tolerance `zone` are one place: that
/// tolerance and the rounding of the copies past it, `size` the largest coordinate of either
/// place. Not upstream. For a site where the body's own tolerances decide, so the contact
/// tolerance ([`crate::units::coincident`]'s floor, which keeps a contact between two operands'
/// curves and a coincidence of them in agreement) does not apply: the side test of an edge
/// use, where a sample of the edge dropped onto its face's surface meets a side of that
/// surface's domain, `zone` the larger of the two tolerances. A NaN zone tolerance gives
/// NaN, within which nothing lies.
#[must_use]
pub fn zone_reach_3d(zone: Length, size: f64) -> Length {
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
    distance <= coincidence_reach_3d(res, a, b, size).mm()
}

// ---- predicates --------------------------------------------------------------------

/// `|val| <= tol` (`SmTol::InTol(double, const SmTol3d&)`).
#[must_use]
pub fn in_tol(val: f64, tol: f64) -> bool {
    val.abs() <= tol
}

/// `SM_IS_ZERO(a)`: `|a| <=` [`EFF_ZERO`] (`SmMath.h`).
#[must_use]
pub fn is_zero(a: f64) -> bool {
    in_tol(a, EFF_ZERO)
}

/// `SM_IS_ZERO_SQUARED(a)`: `|a| <=` [`EFF_ZERO_SQ`] (`SmMath.h`).
#[must_use]
pub fn is_zero_squared(a: f64) -> bool {
    in_tol(a, EFF_ZERO_SQ)
}

/// `SM_IS_ZERO_TO_TOL(a, tol)`: `|a| <= |tol|` (`SmMath.h`).
#[must_use]
pub fn is_zero_to_tol(a: f64, tol: f64) -> bool {
    in_tol(a, tol.abs())
}

/// `SM_ARE_SAME(a, b)`: `|a - b| <=` the larger scaled zero of `a` and `b`
/// (`SmMath.h`).
#[must_use]
pub fn are_same(a: f64, b: f64) -> bool {
    in_tol(a - b, scaled_zero([a, b]))
}

/// `SM_ARE_SAME_TO_TOL(a, b, tol)`: `|a - b| <= |tol|` (`SmMath.h`).
#[must_use]
pub fn are_same_to_tol(a: f64, b: f64, tol: f64) -> bool {
    in_tol(a - b, tol.abs())
}

/// True when `speed` is zero relative to the size `max_pos_dimension`:
/// `|speed| < SM_EFF_ZERO * (1 + |max|)` (`SmTol::IsZeroSpeed`; its override pointer,
/// which no caller passes, is gone).
#[must_use]
pub fn is_zero_speed(speed: f64, max_pos_dimension: f64) -> bool {
    speed.abs() < EFF_ZERO * (1.0 + max_pos_dimension.abs())
}

/// True when `v` is shorter than `tol` (`SmTol::IsDegenerate(const SmVector3d,
/// SmXSectTol3d*)`; solidcore's default, the section tolerance, is
/// [`Resolution::xsect`], which a caller now names).
#[must_use]
pub fn is_degenerate(v: Vec3, tol: Length) -> bool {
    v.is_zero(tol.mm())
}

/// True when `v1` and `v2` are parallel within `max_angle` radians
/// (`SmTol::AreParallel`; upstream's default angle is
/// [`crate::consts::PARALLEL_ANGLE_RAD`], which a caller now names).
#[must_use]
pub fn are_parallel(v1: Vec3, v2: Vec3, max_angle: f64) -> bool {
    v1.is_parallel_to(v2, max_angle)
}

/// True when `v1` and `v2` are perpendicular within `max_angle` radians
/// (`SmTol::ArePerpendicular`; upstream's default angle is
/// [`crate::consts::PARALLEL_ANGLE_RAD`]).
#[must_use]
pub fn are_perpendicular(v1: Vec3, v2: Vec3, max_angle: f64) -> bool {
    v1.is_perpendicular_to(v2, max_angle)
}

// ---- parameter-space mapping ------------------------------------------------------

/// Maps the 3D tolerance `tol` to a curve's parameter space at a point with first
/// derivative `deriv`: `tol / |deriv|`, or `natural_interval.length() / 100` where the
/// derivative is (near) zero (`SmTol::MapTo1d(SmTol3d&, SmVector3d&, const
/// SmExtent1d&)`). `tol` is read as its size, as an `SmTol3d` is.
#[must_use]
pub fn map_to_1d(tol: Length, deriv: Vec3, natural_interval: &Extent1d) -> ParamTol {
    let deriv_size = deriv.length();
    ParamTol::new(if deriv_size < EFF_ZERO {
        (natural_interval.length() / 100.0).abs()
    } else {
        (tol.mm() / deriv_size).abs()
    })
}

// ---- scaled zeros ------------------------------------------------------------------

/// The largest scaled zero of `magnitudes`: `SM_EFF_ZERO * (1 + max |m|)` (the
/// nullary to quaternary `SmTol::GetScaledZero` overloads in one). The maximum is
/// `f64::max`'s, so a NaN magnitude is passed over whatever its place; no magnitudes give
/// [`EFF_ZERO`]. A value's own magnitude is its
/// [`ScaledZero`] size (`max_dimension`).
#[must_use]
#[inline]
pub fn scaled_zero<I: IntoIterator<Item = f64>>(magnitudes: I) -> f64 {
    EFF_ZERO * (1.0 + magnitudes.into_iter().fold(0.0, |m: f64, x| m.max(x.abs())))
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
        scaled_zero([*self])
    }
}

impl ScaledZero for Vec2 {
    /// `SmTol::GetScaledZero(const SmVector2d&)`: `SM_EFF_ZERO * (1 + max |coord|)`.
    fn scaled_zero(&self) -> f64 {
        scaled_zero([self.x, self.y])
    }
}

impl ScaledZero for Vec3 {
    /// `SmTol::GetScaledZero(const SmVector3d&)`: `SM_EFF_ZERO * (1 + max |coord|)`.
    fn scaled_zero(&self) -> f64 {
        scaled_zero([self.x, self.y, self.z])
    }
}

impl ScaledZero for Extent1d {
    /// `SmTol::GetScaledZero(const SmExtent1d&)`: `SM_EFF_ZERO * (1 + max |end|)`.
    fn scaled_zero(&self) -> f64 {
        scaled_zero([self.max_dimension()])
    }
}

impl ScaledZero for Extent3d {
    /// `SmTol::GetScaledZero(const SmExtent3d&)`: `SM_EFF_ZERO * (1 + max |coord|)`
    /// over both corners.
    fn scaled_zero(&self) -> f64 {
        scaled_zero([self.max_dimension()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::{PARALLEL_ANGLE_RAD, ZONE_DEFAULT};
    use crate::math::Point3;

    fn res(mm: f64) -> Resolution {
        Resolution::new(Length::from_mm(mm)).unwrap_or(Resolution::DEFAULT)
    }
    fn acc(mm: f64) -> Accuracy {
        Accuracy::new(Length::from_mm(mm)).unwrap_or(Accuracy::ZERO)
    }

    #[test]
    fn size_estimate_table() {
        // SmTol.cpp sm_TemplateTrial.
        let z = |size: f64| zone_for_size(size).mm();
        assert_eq!(z(50.0), 1.0e-5);
        assert_eq!(z(50000.0), 1.0e-4);
        assert_eq!(z(0.005), 1.0e-6);
        assert_eq!(z(MODEL_SIZE_ESTIMATE), 1.0e-5);
        assert_eq!(z(1.0e8), 1.0e-1);
        assert_eq!(z(0.0), 0.0);
    }

    #[test]
    fn system_tolerances() {
        let k = Resolution::DEFAULT;
        assert_eq!(k.topology_zone(SmallTopoType::NormalSize).mm(), 1.0e-5);
        assert_eq!(
            k.topology_zone(SmallTopoType::SmallSize).mm(),
            1000.0 * 1.0e-5
        );
        let k = res(zone_for_size(50000.0).mm());
        assert_eq!(
            k.topology_zone(SmallTopoType::ForceSmallSize).mm(),
            1000.0 * 1.0e-4
        );
    }

    #[test]
    fn tol_constructors() {
        let k = Resolution::DEFAULT;
        assert_eq!(k.zone().mm(), 1.0e-5);
        assert_eq!(k.xsect().mm(), 2.0e-5);
        assert_eq!(k.approx().mm(), 5.0e-6);
        assert_eq!(k.contact(), k.zone());
        assert!(is_zero_to_tol(-2.0, -3.0));
        assert!(are_same_to_tol(1.0, 3.5, -3.0));
    }

    #[test]
    fn in_tol_and_predicates() {
        // SmTol.cpp sm_TemplateTrial: InTol(.001, .004) etc.
        assert!(in_tol(0.001, 0.004));
        assert!(in_tol(-0.001, 0.004));
        assert!(!in_tol(0.008, 0.004));
        assert!(is_zero_speed(1e-13, 0.0));
        assert!(!is_zero_speed(1e-11, 0.0));
        let k = Resolution::DEFAULT;
        assert!(is_degenerate(Vec3::new(1e-6, 0.0, 0.0), k.xsect()));
        assert!(!is_degenerate(Vec3::new(1e-4, 0.0, 0.0), k.xsect()));
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
        let e3 = Extent3d::spanning(
            Point3::new(0.0, -30.0, -600.0),
            Point3::new(2000.0, 100.0, 10000.0),
        );
        assert_eq!(v2.scaled_zero(), EFF_ZERO * 81.0);
        assert_eq!(v3.scaled_zero(), EFF_ZERO * 1001.0);
        assert_eq!(e3.scaled_zero(), EFF_ZERO * 10001.0);
        let (m2, m3) = (v2.max_dimension(), v3.max_dimension());
        assert_eq!(scaled_zero([m2, m3]), EFF_ZERO * 1001.0);
        assert_eq!(scaled_zero([m2, m3, 10000.0]), EFF_ZERO * 10001.0);
        assert_eq!(scaled_zero([1.0, m2, m3, -10000.0]), EFF_ZERO * 10001.0);
        assert_eq!(scaled_zero([-7.0]), 7.0f64.scaled_zero());
        assert_eq!(scaled_zero([]), EFF_ZERO);
        // `f64::max` passes a NaN over in either place
        assert_eq!(scaled_zero([f64::NAN, 3.0]), EFF_ZERO * 4.0);
        assert_eq!(scaled_zero([3.0, f64::NAN]), EFF_ZERO * 4.0);
    }

    #[test]
    fn coincidence_is_the_larger_tolerance() {
        let k = Resolution::DEFAULT;
        let contact = k.contact().mm();
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
        assert_eq!(acc(1.0e-5).sum(acc(2.0e-5)).mm(), 1.0e-5 + 2.0e-5);
    }

    #[test]
    fn zone_for_size_boundaries_keep_todays_answers() {
        // the checks run 0.5..=500, 500..=50000, 0.005..=0.5, 50000..=5e6,
        // 0.00005..=0.005: a boundary takes the first range listed that holds it
        let z = |size: f64| zone_for_size(size).mm();
        assert_eq!(z(0.5), 1.0e-5);
        assert_eq!(z(500.0), 1.0e-5);
        assert_eq!(z(50000.0), 1.0e-4);
        assert_eq!(z(0.005), 1.0e-6);
        assert_eq!(z(5e6), 1.0e-3);
        assert_eq!(z(0.00005), 1.0e-7);
        assert!(z(f64::NAN).is_nan());
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
        assert_eq!(k.zone().mm(), ZONE_DEFAULT);
        assert_eq!(
            (k.zone().mm(), k.xsect().mm(), k.approx().mm()),
            (1.0e-5, 2.0e-5, 5.0e-6)
        );
        assert_eq!(crate::consts::PARALLEL_ANGLE_DEG, 0.5);
        assert_eq!(
            (MODEL_SIZE_ESTIMATE, LARGE_SMALL_SIZE_RATIO),
            (50.0, 1000.0)
        );
    }
}

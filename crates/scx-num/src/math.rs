// Moved from solidcore `solidcore/crates/solidcore/src/math.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmVector3d.h`,
// `SmVector2d.h`, `SmCoreTypes.h`, `SmMath.h`, `SmTransform.h` / `src/SmTransform.cpp`, and
// `smgu_4x4Mult` in `src/SmGeomUtility.cpp`.
// Changes: `Transform::of_placement` stays in solidcore (it needs `Axis2Placement`); `det3`
// public (eval and blend use it); `EFF_ZERO_PARAM` defined from `consts::EPS_PARAM`;
// `gamma` from `crate::certified`; paths; adds the test `det3_is_the_integer_determinant`.
// No `UNDEF_DOUBLE` (`SM_UNDEF_DOUBLE`, `docs/01-architecture.md` §10.1): an unset value is
// an `Option`.
// Then the transcendental functions became `fmath`'s (correctly rounded, the same bits on every
// platform) in place of the C library's, so results may differ from the pin's in the last bit.
// `Transform::is_regular` reads its scale's exponent from the bits, not `log2(..).floor()`;
// for an entry `f64::MAX` it now answers `No` where the pin, whose `powi(2, -1024)` is 0, says
// `Undecided`.
// Then `max3`, `min3`, `max4` and `min4` removed: `smos_3Max(a, b, c)` is
// `max2(max2(a, b), c)` to the bit, NaN included, and `smos_4Max` was already nested `max2`.
// Then `Transform` (upstream's row-vector 4×4 with a public `m`) became `Affine`, column-vector,
// with no projective column: `concat` became `Mul` (`outer * inner`), `transform_point` and
// friends `apply*`, `inverse` answers `Option`, `is_rigid` lost its projective tolerance, and
// `transform_points` went (its one caller mapped points one by one). A composition's linear part
// no longer takes NaN from a translation not finite (`0 * inf` in `smgu_4x4Mult`).
// Then `max2` and `min2` (`smos_Max`, `smos_Min`: `a > b ? a : b`) removed for `f64::max` and
// `f64::min`. Those drop a NaN operand and answer the other, where `max2` answered its second
// operand: `max2(NaN, 1)` was 1 but `max2(1, NaN)` NaN, so a fold's answer hung on its order.
//! Small `f64` linear-algebra types: [`Vec3`]/[`Point3`], [`Vec2`]/[`Point2`],
//! [`Affine`], and the numeric constants of `SmMath.h`.
//!
//! Upstream: `source/SMLib/inc/SmVector3d.h`, `SmVector2d.h`, `SmCoreTypes.h`,
//! `SmMath.h`, `SmTransform.h` / `src/SmTransform.cpp`, and `smgu_4x4Mult` in
//! `src/SmGeomUtility.cpp`.
//!
//! | Rust                         | Upstream                                       |
//! |------------------------------|------------------------------------------------|
//! | [`Vec3`] / [`Point3`]        | `SmVector3d` / `SmPoint3d` (re-exported from [`crate::vector3d`]) |
//! | [`Vec2`] / [`Point2`]        | `SmVector2d` / `SmPoint2d` (re-exported from [`crate::vector2d`]) |
//! | [`Affine`]                   | `SmTransform` (`m_adT[4][4]`, affine only)     |
//! | `outer * inner` ([`Affine`]'s `Mul`) | `SmTransform::ConcatTransform` / `smgu_4x4Mult` |
//! | [`Affine::apply`]            | `SmTransform::TransformPoint`                  |
//! | [`Affine::apply_vec`]        | `SmTransform::TransformVector`                 |
//! | [`Affine::rotation`], [`Affine::uniform_scaling`], [`Affine::reflection`] | (not upstream) |
//! | [`EFF_ZERO`] etc.            | `SM_EFF_ZERO` etc.                             |
//! | [`is_infinite_param`]        | `SM_IS_INFINITE`                               |
//! | `f64::max`, `f64::min`       | `smos_Max`, `smos_Min`, `smos_3Max` etc. (a NaN operand is dropped) |
//! | [`rad_to_deg`]               | `SM_RAD2DEG`                                   |
//!
//! ## Why not `glam`/`nalgebra`
//!
//! The correctness gate compares against the C++ to `1e-9` relative, and the port keeps
//! upstream operation order. Third-party vector crates may reassociate sums, use SIMD
//! lanes or fused multiply-add, and do not match upstream degenerate-case behaviour
//! (e.g. `Unitize` of a zero vector). These hand-written types spell every expression
//! in upstream order, so results are bit-identical to the C++ (built with
//! `-ffp-contract=off`).

pub use crate::vector2d::{Point2, Vec2};
pub use crate::vector3d::{Point3, UnitVec3, Vec3};

/// `SM_PI` (the upstream literal rounds to exactly this `f64`).
pub const PI: f64 = core::f64::consts::PI;
/// `SM_HALFPI`.
pub const HALF_PI: f64 = core::f64::consts::FRAC_PI_2;
/// `SM_3HALFPI`.
pub const THREE_HALF_PI: f64 = 4.7123889803846897576939650749192543262957530990626;
/// `SM_2PI`.
pub const TWO_PI: f64 = core::f64::consts::TAU;
/// `SM_SQRT3`.
pub const SQRT3: f64 = 1.7320508075688772935274463415058723669428052538104;
/// `SM_EFF_ZERO`: numeric zero tolerance.
pub const EFF_ZERO: f64 = 1.0e-12;
/// `SM_EFF_ZERO_SQ`: `EFF_ZERO` squared.
pub const EFF_ZERO_SQ: f64 = 1.0e-24;
/// `SM_EFF_ZERO_SQRT`: square root of `EFF_ZERO`.
pub const EFF_ZERO_SQRT: f64 = 1.0e-6;
/// `SM_EFF_ZERO_PARAM`: numeric zero tolerance for knot/parameter values.
pub const EFF_ZERO_PARAM: f64 = crate::consts::EPS_PARAM;
/// `SM_EFF_ZERO_DEG`: numeric zero tolerance for angles in degrees.
pub const EFF_ZERO_DEG: f64 = 1.0e-4;
/// `SM_EFF_ZERO_RAD`: numeric zero tolerance for angles in radians.
pub const EFF_ZERO_RAD: f64 = EFF_ZERO_DEG * PI / 180.0;
/// `SM_BIG_DOUBLE`: a value larger than any coordinate (used for "inside out" extents).
pub const BIG_DOUBLE: f64 = 1.0e20;
/// `SM_INFINITE_PARAMETER`: the value that marks an unbounded interval end.
pub const INFINITE_PARAMETER: f64 = 1234567.0;
/// `SM_INFINITE_PARAMETER_SQRT`: unbounded marker for representations that square it.
pub const INFINITE_PARAMETER_SQRT: f64 = 1111.111;
/// `SM_BOUNDED_INFINITE_PARAM`: half size used to approximate an unbounded interval.
pub const BOUNDED_INFINITE_PARAM: f64 = 33.0;

/// `SM_IS_INFINITE(a)`: true when `a` is `±INFINITE_PARAMETER` or
/// `±INFINITE_PARAMETER_SQRT` (exact comparison, as upstream).
#[must_use]
pub fn is_infinite_param(a: f64) -> bool {
    a == INFINITE_PARAMETER
        || a == -INFINITE_PARAMETER
        || a == INFINITE_PARAMETER_SQRT
        || a == -INFINITE_PARAMETER_SQRT
}

/// `SM_RAD2DEG(a)` = `a * (180.0 / SM_PI)`: radians to degrees.
#[must_use]
pub fn rad_to_deg(a: f64) -> f64 {
    a * (180.0 / PI)
}

/// The determinant of the 3×3 matrix of the first three entries of `m`'s rows, expanded
/// along the first row.
#[must_use]
pub fn det3(m: &[[f64; 4]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// An affine 3D map in the column-vector convention: `p' = A p + t`, the linear part `A`
/// (whose columns are the images of the X, Y and Z axes) and the translation `t` (the image
/// of the origin). `a * b` applies `b` first.
///
/// It holds no projective part: [`TryFrom<[[f64; 4]; 4]>`](#impl-TryFrom%3C%5B%5Bf64;+4%5D;+4%5D%3E-for-Affine)
/// reads upstream's row-vector `SmTransform::m_adT` and refuses a fourth column other than
/// `(0, 0, 0, 1)`. Every map sums its terms in upstream's order (`TransformPoint`,
/// `TransformVector`, `smgu_4x4Mult`), so its results are upstream's to the bit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    /// The images of the X, Y and Z axes: the linear part's columns.
    axes: [Vec3; 3],
    /// The translation.
    shift: Vec3,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// The operation name of the refusals of the maps built from their numbers.
const OP_MAPS: &str = "math::Affine";

impl Affine {
    // ---- Constructors ----

    /// The identity map.
    pub const IDENTITY: Self = Self {
        axes: [Vec3::X, Vec3::Y, Vec3::Z],
        shift: Vec3::ZERO,
    };

    /// A pure translation by `d`.
    #[must_use]
    pub const fn translation(d: Vec3) -> Self {
        Self {
            axes: Self::IDENTITY.axes,
            shift: d,
        }
    }

    /// The map that takes the X, Y and Z axes to `x`, `y` and `z` and the origin to `origin`
    /// (`SetToXAxis`/`SetToYAxis`/`SetToZAxis`/`SetToDisp`).
    #[must_use]
    pub const fn from_axes(x: Vec3, y: Vec3, z: Vec3, origin: Point3) -> Self {
        Self {
            axes: [x, y, z],
            shift: Vec3::new(origin.x, origin.y, origin.z),
        }
    }

    /// The linear map with columns `x`, `y` and `z` about the point `fixed`, which it keeps:
    /// `p' = A (p - fixed) + fixed`, its translation `fixed - A fixed`. A `fixed` whose image
    /// overflows is taken at an eighth and the translation scaled back (exact, a power of
    /// two), so a point far out on the map's own axis keeps a translation.
    fn about(fixed: Point3, x: Vec3, y: Vec3, z: Vec3) -> Self {
        let shift = |f: Vec3| f - (x * f.x + y * f.y + z * f.z);
        let near = shift(fixed.to_vec());
        let t = if near.is_finite() {
            near
        } else {
            shift(fixed.to_vec() * 0.125) * 8.0
        };
        Self::from_axes(x, y, z, t.to_point())
    }

    /// Not upstream: the turn by `angle` radians, right-handed about `axis`, about the line
    /// through `point` along `axis` (any length, made unit here): Rodrigues' rotation formula,
    /// `R = c I + s [k]x + (1 - c) k kᵀ` with `k` the unit axis, `c` and `s` the cosine and
    /// sine of the angle.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for a point, axis or angle not finite, an axis whose squared
    /// length is below [`EFF_ZERO_SQ`], or a translation too large to represent.
    pub fn rotation(point: Point3, axis: Vec3, angle: f64) -> crate::error::Result<Self> {
        use crate::error::NumError;
        if !(point.is_finite() && axis.is_finite() && angle.is_finite()) {
            return Err(NumError::invalid_input(
                OP_MAPS,
                "a turn's numbers not finite",
            ));
        }
        let k = axis.normalized().ok_or(NumError::invalid_input(
            OP_MAPS,
            "a turn's axis of no length",
        ))?;
        let (s, c) = crate::fmath::sin_cos(angle);
        let v = 1.0 - c;
        let (x, y, z) = (k.x, k.y, k.z);
        Self::about(
            point,
            Vec3::new(c + x * x * v, x * y * v + z * s, x * z * v - y * s),
            Vec3::new(x * y * v - z * s, c + y * y * v, y * z * v + x * s),
            Vec3::new(x * z * v + y * s, y * z * v - x * s, c + z * z * v),
        )
        .represented()
    }

    /// Not upstream: the scale by `factor` in every direction about `centre`, which it keeps.
    /// A negative factor turns the scale through the centre as well.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for a centre or factor not finite, or a factor of zero (a map with
    /// no inverse).
    pub fn uniform_scaling(centre: Point3, factor: f64) -> crate::error::Result<Self> {
        use crate::error::NumError;
        if !(centre.is_finite() && factor.is_finite()) || factor == 0.0 {
            return Err(NumError::invalid_input(
                OP_MAPS,
                "a scale's numbers not finite, or a factor of zero",
            ));
        }
        Ok(Self::about(
            centre,
            Vec3::new(factor, 0.0, 0.0),
            Vec3::new(0.0, factor, 0.0),
            Vec3::new(0.0, 0.0, factor),
        ))
    }

    /// Not upstream: the mirror in the plane through `origin` square to `normal` (any length,
    /// made unit here): `p' = p - 2 ((p - origin)·n) n`, its linear part `I - 2 n nᵀ` and its
    /// translation `2 (origin·n) n`. For a normal along an axis every entry is exact, so the
    /// mirror twice is the identity exactly.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for an origin or normal not finite, a normal whose squared length
    /// is below [`EFF_ZERO_SQ`], or a translation too large to represent.
    pub fn reflection(origin: Point3, normal: Vec3) -> crate::error::Result<Self> {
        use crate::error::NumError;
        if !(origin.is_finite() && normal.is_finite()) {
            return Err(NumError::invalid_input(
                OP_MAPS,
                "a mirror's numbers not finite",
            ));
        }
        let n = normal
            .normalized()
            .ok_or(NumError::invalid_input(
                OP_MAPS,
                "a mirror's normal of no length",
            ))?
            .as_vec();
        let column = |e: Vec3, ni: f64| e - n * (2.0 * ni);
        Self::from_axes(
            column(Vec3::X, n.x),
            column(Vec3::Y, n.y),
            column(Vec3::Z, n.z),
            (n * far_dot(origin.to_vec(), n) * 2.0).to_point(),
        )
        .represented()
    }

    /// Not upstream: the rigid map from world coordinates into the frame of the plane through
    /// `origin` with x axis along `x` and y axis on `y`'s side in the plane of the two (each
    /// any length): the unit axes `x̂`, `n = unit(x̂ × y)` and `ŷ = n × x̂` become X, Z and Y,
    /// and `origin` the origin. A point's Z after the map is its level above the plane along
    /// its normal. The linear part's rows are `x̂`, `ŷ` and `n`, its translation
    /// `-(origin·x̂, origin·ŷ, origin·n)`.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for an origin or axis not finite, an `x` or `y` of no direction
    /// ([`Vec3::normalized`]), an `x̂ × ŷ` whose squared length is below [`EFF_ZERO_SQ`]
    /// (axes along one line), or a translation too large to represent.
    pub fn into_plane_frame(origin: Point3, x: Vec3, y: Vec3) -> crate::error::Result<Self> {
        use crate::error::NumError;
        if !(origin.is_finite() && x.is_finite() && y.is_finite()) {
            return Err(NumError::invalid_input(
                OP_MAPS,
                "a plane's numbers not finite",
            ));
        }
        let none =
            NumError::invalid_input(OP_MAPS, "a plane's axes of no length or along one line");
        let x = x.normalized().ok_or_else(|| none.clone())?.as_vec();
        let y = y.normalized().ok_or_else(|| none.clone())?;
        let n = x.cross(*y).normalized().ok_or(none)?.as_vec();
        let y = n.cross(x);
        Self::from_axes(
            Vec3::new(x.x, y.x, n.x),
            Vec3::new(x.y, y.y, n.y),
            Vec3::new(x.z, y.z, n.z),
            (-Vec3::new(
                far_dot(origin.to_vec(), x),
                far_dot(origin.to_vec(), y),
                far_dot(origin.to_vec(), n),
            ))
            .to_point(),
        )
        .represented()
    }

    /// `Ok(self)` when every entry is finite; otherwise the refusal of a map whose entries
    /// overflow (finite inputs far enough out that the translation is too large to represent).
    fn represented(self) -> crate::error::Result<Self> {
        if self.is_finite() {
            Ok(self)
        } else {
            Err(crate::error::NumError::invalid_input(
                OP_MAPS,
                "a map too large to represent",
            ))
        }
    }

    // ---- Queries ----

    /// Image of the X axis (`GetToXAxis`).
    #[must_use]
    pub const fn x_axis(&self) -> Vec3 {
        self.axes[0]
    }

    /// Image of the Y axis (`GetToYAxis`).
    #[must_use]
    pub const fn y_axis(&self) -> Vec3 {
        self.axes[1]
    }

    /// Image of the Z axis (`GetToZAxis`).
    #[must_use]
    pub const fn z_axis(&self) -> Vec3 {
        self.axes[2]
    }

    /// The translation, the image of the origin (`GetToDisp` / `GetToOrigin`).
    #[must_use]
    pub const fn translation_part(&self) -> Vec3 {
        self.shift
    }

    /// Whether every entry is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.axes.iter().all(|a| a.is_finite()) && self.shift.is_finite()
    }

    /// The lengths of the images of the axes (the linear part's columns), each within
    /// `gamma_4` of exact, relatively.
    #[must_use]
    pub fn axis_lengths(&self) -> [f64; 3] {
        self.axes
            .map(|a| (a.x * a.x + a.y * a.y + a.z * a.z).sqrt())
    }

    /// Not upstream: whether the map is regular, proved: every entry finite and its linear
    /// part's conditioning, the ratio of its smallest scale to its largest (singular values
    /// `s_min / s_max`), above `ratio`. `ratio` of 0 asks only for a non-zero determinant; a
    /// uniform scale of any size has conditioning 1, a map that flattens one direction against
    /// the others (a scale of `(1e-300, 1, 1)`) conditioning that small. `Undecided` when the
    /// rounding leaves it open.
    ///
    /// The conditioning is bounded below without a solve. With `r_i` the images of the axes,
    /// `F = sum |r_i|^2` and `C = sum_{i<j} |r_i|^2 |r_j|^2`: `s_max <= |A|_F = sqrt F`, and
    /// `s_max s_mid` is the largest singular value of the cofactor matrix, whose columns are
    /// the cross products `r_i x r_j` with `|r_i x r_j| <= |r_i| |r_j|`, so
    /// `s_max s_mid <= sqrt C`. Then `s_min = |det| / (s_max s_mid) >= |det| / sqrt C`, and
    /// `s_min / s_max >= |det| / sqrt(F C)`: the map is proved regular when `|det|` is
    /// proved above `ratio sqrt(F C)`. (The bound is within a factor 3 of the conditioning
    /// for a rigid motion; a stretch `(2, 3, 4)`, of conditioning 0.5, proves 0.28.)
    ///
    /// Rounding: the linear part is first scaled by one power of two, so its largest entry
    /// is in `[1, 2)` (exact, the ratio unchanged; a subnormal largest entry in two exact
    /// steps). The determinant's six products round within `gamma_5` of the sum of their
    /// sizes; `F` and `C` are sums of non-negative terms within `gamma_9` each, the root
    /// and the product with `ratio` two more roundings (`gamma_15` in all). Entries far
    /// below the largest underflow:
    /// - a scaled entry below `1e-100` (judged from the source entry, so an entry flushed
    ///   to 0 by the scaling, in an axis image spanning more than `2^1074`, is counted too)
    ///   moves each product of the determinant by a few subnormal units at most, which an
    ///   absolute term of 256 of them covers (the cofactors are at most 8, an entry moves by
    ///   half a unit);
    /// - a 2x2 minor that cancels to a subnormal, times an entry of at least `1e-100`, rounds
    ///   by at most half a subnormal unit, far inside `gamma_5 size` (that term of `size` is at
    ///   least `1e-300`);
    /// - `size` is itself computed: the exact one is within `(1 - u)^-5` of it, and the bound's
    ///   own sum rounds, which `gamma_5 size (1 + gamma_8)` covers;
    /// - squares below the smallest normal double move `F` and `C` by under `1e-320`
    ///   absolutely, so `sqrt(F C)` by under `1e-159` (`F` is at most 12), which the
    ///   absolute term `ratio * 1e-150` covers.
    ///
    /// SolidCraft's station-map check, `maps_ok`.
    #[must_use]
    pub fn is_regular(&self, ratio: f64) -> crate::certified::Verdict {
        use crate::certified::{gamma, Bounded, Verdict};
        if !self.is_finite() || ratio.is_nan() || ratio < 0.0 || ratio.is_infinite() {
            return Verdict::No;
        }
        let src = self.axes.map(Vec3::to_array);
        let big = src.iter().flatten().fold(0.0_f64, |b, v| b.max(v.abs()));
        if big == 0.0 {
            return Verdict::No;
        }
        // One power of two for the whole linear part (exact; two steps for a subnormal
        // largest entry, whose inverse power alone would overflow).
        let pre = if big < f64::MIN_POSITIVE {
            crate::fmath::powi(2.0, 600)
        } else {
            1.0
        };
        // The exponent read from the bits, not `log2(..).floor()`: the largest double's
        // `log2` is 1024 - 1.6e-16, which rounds correctly to 1024, one too many.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // 11 bits
        let e = ((big * pre).to_bits() >> 52) as i32 - 1023;
        let scale = crate::fmath::powi(2.0, -e);
        let mut m = [[0.0_f64; 3]; 3];
        let mut tiny_entry = false;
        for (row, src) in m.iter_mut().zip(&src) {
            for (r, &v) in row.iter_mut().zip(src) {
                *r = (v * pre) * scale;
                // from the source entry: one the scaling flushes to 0 counts too
                tiny_entry |= v != 0.0 && (v * pre).abs() < 1e-100 / scale;
            }
        }
        let det = m[0][0] * (m[1][1] * m[2][2] - m[2][1] * m[1][2])
            - m[1][0] * (m[0][1] * m[2][2] - m[2][1] * m[0][2])
            + m[2][0] * (m[0][1] * m[1][2] - m[1][1] * m[0][2]);
        let size = m[0][0].abs() * ((m[1][1] * m[2][2]).abs() + (m[2][1] * m[1][2]).abs())
            + m[1][0].abs() * ((m[0][1] * m[2][2]).abs() + (m[2][1] * m[0][2]).abs())
            + m[2][0].abs() * ((m[0][1] * m[1][2]).abs() + (m[1][1] * m[0][2]).abs());
        let q = |c: &[f64; 3]| c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
        let (q0, q1, q2) = (q(&m[0]), q(&m[1]), q(&m[2]));
        let f = q0 + q1 + q2;
        let c = q0 * q1 + q0 * q2 + q1 * q2;
        let floor = ratio * (f * c).sqrt();
        let value = det.abs() - floor;
        let tiny = if tiny_entry {
            256.0 * f64::from_bits(1)
        } else {
            0.0
        };
        let lower = Bounded {
            value,
            bound: gamma(5) * size * (1.0 + gamma(8))
                + gamma(15) * floor
                + ratio * 1e-150
                + gamma(1) * value.abs()
                + tiny,
        }
        .above(0.0);
        if lower == Verdict::Yes {
            return Verdict::Yes;
        }
        // Proved not regular: every product of the determinant exactly 0 (no entry
        // underflowed), or the conditioning's upper bound `|r_min| / |r_max|` (each axis
        // image is the image of a unit vector, between `s_min` and `s_max` long) at most
        // `ratio`, the squares' rounding and underflow counted against it.
        let (q_min, q_max) = (q0.min(q1).min(q2), q0.max(q1).max(q2));
        let flat = size == 0.0 && !tiny_entry;
        let thin =
            ratio * ratio * q_max * (1.0 - gamma(8)) - 1e-320 > q_min * (1.0 + gamma(4)) + 1e-320;
        if flat || thin {
            Verdict::No
        } else {
            Verdict::Undecided
        }
    }

    /// Not upstream: whether this finite map is a rotation or reflection and a translation:
    /// the images of the axes unit and square to each other within `axis_tol`, which must be
    /// finite and positive.
    #[must_use]
    pub fn is_rigid(&self, axis_tol: f64) -> bool {
        let [a, b, c] = self.axes;
        axis_tol.is_finite()
            && axis_tol > 0.0
            && self.is_finite()
            && self
                .axes
                .iter()
                .all(|v| (v.length() - 1.0).abs() < axis_tol)
            && a.dot(b).abs() < axis_tol
            && b.dot(c).abs() < axis_tol
            && c.dot(a).abs() < axis_tol
    }

    /// Not upstream: the orientation-preserving 2D map induced on an orthonormal plane frame.
    /// Translation normal to the plane is discarded, as for an orthogonal projection.
    /// Returns none for an identity within `identity_tol`, a changed normal, or a 2D
    /// determinant differing from one by at least `tol`. The caller establishes rigidity.
    #[must_use]
    pub fn plane_motion(
        &self,
        origin: Point3,
        x: Vec3,
        y: Vec3,
        tol: f64,
        identity_tol: f64,
    ) -> Option<([[f64; 2]; 2], Vec2)> {
        let near_identity = || {
            let id = Self::IDENTITY;
            self.axes
                .into_iter()
                .chain([self.shift])
                .zip(id.axes.into_iter().chain([id.shift]))
                .all(|(a, b)| {
                    let d = a - b;
                    d.x.abs() < identity_tol && d.y.abs() < identity_tol && d.z.abs() < identity_tol
                })
        };
        if !(tol.is_finite()
            && tol > 0.0
            && identity_tol.is_finite()
            && identity_tol > 0.0
            && origin.is_finite()
            && x.is_finite()
            && y.is_finite()
            && self.is_finite())
            || near_identity()
        {
            return None;
        }
        let n = x.cross(y);
        if (self.apply_vec(n) - n).length() > tol {
            return None;
        }
        let (rx, ry) = (self.apply_vec(x), self.apply_vec(y));
        let shift = self.apply(origin) - origin;
        let a = [[rx.dot(x), ry.dot(x)], [rx.dot(y), ry.dot(y)]];
        let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
        let b = Vec2::new(shift.dot(x), shift.dot(y));
        ((det - 1.0).abs() < tol && b.x.is_finite() && b.y.is_finite()).then_some((a, b))
    }

    // ---- Maps ----

    /// Maps a point (`SmTransform::TransformPoint`): `t + p.x X' + p.y Y' + p.z Z'`, summed
    /// in that order.
    #[must_use]
    pub fn apply(&self, p: Point3) -> Point3 {
        let ([tx, ty, tz], td) = (self.axes, self.shift);
        Point3::new(
            td.x + p.x * tx.x + p.y * ty.x + p.z * tz.x,
            td.y + p.x * tx.y + p.y * ty.y + p.z * tz.y,
            td.z + p.x * tx.z + p.y * ty.z + p.z * tz.z,
        )
    }

    /// Maps a vector, ignoring translation (`SmTransform::TransformVector`).
    #[must_use]
    pub fn apply_vec(&self, v: Vec3) -> Vec3 {
        let [tx, ty, tz] = self.axes;
        Vec3::new(
            v.x * tx.x + v.y * ty.x + v.z * tz.x,
            v.x * tx.y + v.y * ty.y + v.z * tz.y,
            v.x * tx.z + v.y * ty.z + v.z * tz.z,
        )
    }

    /// Not upstream: maps a point on a line and its direction. Translation affects only the
    /// point.
    #[must_use]
    pub fn apply_line(&self, point: Point3, direction: Vec3) -> (Point3, Vec3) {
        (self.apply(point), self.apply_vec(direction))
    }

    /// Not upstream: maps an undirected axis and orients its unit direction here.
    ///
    /// # Errors
    /// Refuses a non-finite point or a direction with length at most 1e-9.
    pub fn apply_axis(
        &self,
        point: Point3,
        direction: Vec3,
    ) -> crate::error::Result<(Point3, Vec3)> {
        let (point, direction) = self.apply_line(point, direction);
        let length = direction.length();
        if !point.is_finite() || !length.is_finite() || length <= 1e-9 {
            return Err(crate::error::NumError::InvalidInput {
                operation: OP_MAPS,
                reason: "the axis has no finite point and direction",
            });
        }
        Ok((point, (direction / length).canonical_axis_direction()))
    }

    /// Not upstream: the axis-aligned box around the eight corners of the box `lo`..`hi`
    /// mapped by [`Self::apply`]: it holds the exact image of every point of the box, and for
    /// a map that turns it is larger than the image itself. An empty box (`lo` above `hi` on
    /// any axis) maps to itself.
    ///
    /// Rounding: each mapped coordinate `t + x a + y b + z c` is four terms summed in order,
    /// within `gamma_4` of the sum of their sizes; the sizes' sum is itself rounded, so each
    /// side moves out by `gamma_5` of it, plus the smallest normal number for products that
    /// underflow, and then by one more double for the subtraction or addition.
    #[must_use]
    pub fn transformed_box(&self, lo: Point3, hi: Point3) -> (Point3, Point3) {
        use crate::certified::{gamma, next_down, next_up};
        if lo.x > hi.x || lo.y > hi.y || lo.z > hi.z {
            return (lo, hi);
        }
        let ([tx, ty, tz], td) = (self.axes, self.shift);
        let g = gamma(5);
        let mut min = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for i in 0..8 {
            let corner = Point3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let p = self.apply(corner);
            let size = |t: f64, a: f64, b: f64, c: f64| {
                g * (t.abs() + (corner.x * a).abs() + (corner.y * b).abs() + (corner.z * c).abs())
                    + f64::MIN_POSITIVE
            };
            let e = Vec3::new(
                size(td.x, tx.x, ty.x, tz.x),
                size(td.y, tx.y, ty.y, tz.y),
                size(td.z, tx.z, ty.z, tz.z),
            );
            min = Point3::new(
                min.x.min(next_down(p.x - e.x)),
                min.y.min(next_down(p.y - e.y)),
                min.z.min(next_down(p.z - e.z)),
            );
            max = Point3::new(
                max.x.max(next_up(p.x + e.x)),
                max.y.max(next_up(p.y + e.y)),
                max.z.max(next_up(p.z + e.z)),
            );
        }
        (min, max)
    }

    /// The map that undoes this one, when [`Self::is_regular`] proves it regular at ratio 0:
    /// `p = A⁻¹ (p' - t)`. With `a`, `b`, `c` the images of the axes (the columns of `A`),
    /// `A⁻¹`'s rows are `b × c`, `c × a` and `a × b` over `a · (b × c)`, its translation
    /// `-A⁻¹ t`. A mirror in a plane through the origin square to an axis is its own inverse
    /// exactly. `None` for an entry not finite or a determinant not proved apart from 0.
    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        if !self.is_regular(0.0).is_yes() {
            return None;
        }
        let [a, b, c] = self.axes;
        let (bc, ca, ab) = (b.cross(c), c.cross(a), a.cross(b));
        let s = 1.0 / a.dot(bc);
        let linear = Self::from_axes(
            Vec3::new(bc.x, ca.x, ab.x) * s,
            Vec3::new(bc.y, ca.y, ab.y) * s,
            Vec3::new(bc.z, ca.z, ab.z) * s,
            Point3::ORIGIN,
        );
        Some(Self {
            shift: -linear.apply_vec(self.shift),
            ..linear
        })
    }

    /// Not upstream: how far the linear part of this map (no translation) takes the point
    /// `pivot` from where it was, as the move it adds to the map's turn about `pivot`:
    /// `pivot - apply_vec(pivot)`. A turn about the origin followed by this move is the
    /// same turn about `pivot`.
    #[must_use]
    pub fn pivot_displacement(&self, pivot: Vec3) -> Vec3 {
        pivot - self.apply_vec(pivot)
    }

    /// Not upstream: carry a plane's axes by a rigid motion, preserving its UV scales and
    /// orthogonality. Source axes square to the same dot-product rounding as the plane
    /// constructor are reconstructed as an orthogonal pair after the move. Genuinely skew
    /// axes keep their parametrisation and are mapped directly. Call this only for a rigid
    /// motion.
    ///
    /// This prevents the rounding of successive rotations from accumulating into a skew
    /// plane. It does not widen the plane constructor's rounding bound.
    ///
    /// # Errors
    /// Non-finite coordinates, axes of no length, or a degenerate mapped pair.
    pub fn rigid_plane_axes(&self, u: Vec3, v: Vec3) -> crate::error::Result<[Vec3; 2]> {
        use crate::certified::gamma;
        use crate::error::NumError;
        let (a, b) = (u.length(), v.length());
        if !(u.is_finite()
            && v.is_finite()
            && a > 0.0
            && b > 0.0
            && (a * b).is_finite()
            && self.is_finite())
        {
            return Err(NumError::invalid_input(
                OP_MAPS,
                "invalid plane axes or motion",
            ));
        }
        let (mu, mv) = (self.apply_vec(u), self.apply_vec(v));
        if !(mu.is_finite() && mv.is_finite()) {
            return Err(NumError::invalid_input(
                OP_MAPS,
                "non-finite mapped plane axes",
            ));
        }
        if u.dot(v).abs() > gamma(3) * a * b {
            return Ok([mu, mv]);
        }
        mv.normalized().ok_or(NumError::invalid_input(
            OP_MAPS,
            "a mapped plane axis of no length",
        ))?;
        let [x, y, _] = mu.make_unit_ortho_vectors(Some(mv))?;
        Ok([x * a, y * b])
    }

    /// Not upstream: carry a plane given by three points through a rigid motion, returning
    /// its origin and axes. Keeping axes instead of reconstructing endpoints avoids
    /// cancellation when the plane's parametrisation is read back.
    ///
    /// # Errors
    /// Those of [`Self::rigid_plane_axes`], or a non-finite origin.
    pub fn rigid_plane_through(
        &self,
        origin: Point3,
        u: Point3,
        v: Point3,
    ) -> crate::error::Result<(Point3, [Vec3; 2])> {
        let axes = self.rigid_plane_axes(u - origin, v - origin)?;
        let origin = self.apply(origin);
        if !origin.is_finite() {
            return Err(crate::error::NumError::invalid_input(
                OP_MAPS,
                "non-finite plane origin",
            ));
        }
        Ok((origin, axes))
    }
}

/// `outer * inner`: `inner` first, then `outer` (`inner.ConcatTransform(outer)`,
/// `smgu_4x4Mult(inner, outer)`), with upstream's sums: each image of an axis is
/// `outer.apply_vec` of `inner`'s, the translation `outer.apply_vec(inner's) + outer's`.
/// Upstream's product also adds `0 * outer`'s translation into each entry of the linear part,
/// which makes it NaN for a translation not finite; the linear part of a composition does not
/// depend on a translation, and here it does not.
impl core::ops::Mul for Affine {
    type Output = Self;
    #[allow(clippy::suspicious_arithmetic_impl)] // the translation adds: an affine product
    fn mul(self, inner: Self) -> Self {
        Self {
            axes: inner.axes.map(|a| self.apply_vec(a)),
            shift: self.apply_vec(inner.shift) + self.shift,
        }
    }
}

/// Reads upstream's row-vector 4×4 matrix (`SmTransform::m_adT`, `out = [p.x p.y p.z 1] m`):
/// rows 0 to 2 the images of the axes, row 3 the translation. Exact.
impl TryFrom<[[f64; 4]; 4]> for Affine {
    type Error = crate::error::NumError;

    /// # Errors
    /// `InvalidInput` for a fourth column other than `(0, 0, 0, 1)` (NaN included): a
    /// projective map, which an `Affine` cannot hold.
    fn try_from(m: [[f64; 4]; 4]) -> Result<Self, Self::Error> {
        let [x, y, z, t] = m;
        if !(x[3] == 0.0 && y[3] == 0.0 && z[3] == 0.0 && t[3] == 1.0) {
            return Err(crate::error::NumError::invalid_input(
                OP_MAPS,
                "a projective column",
            ));
        }
        let v = |r: [f64; 4]| Vec3::new(r[0], r[1], r[2]);
        Ok(Self::from_axes(v(x), v(y), v(z), v(t).to_point()))
    }
}

/// Writes upstream's row-vector 4×4 matrix: the inverse of the [`TryFrom`] above. Exact.
impl From<Affine> for [[f64; 4]; 4] {
    fn from(a: Affine) -> Self {
        let r = |v: Vec3, w: f64| [v.x, v.y, v.z, w];
        let [x, y, z] = a.axes;
        [r(x, 0.0), r(y, 0.0), r(z, 0.0), r(a.shift, 1.0)]
    }
}

/// Not upstream: the unit, orthogonal frame axes nearest to `x` and `y`: `x` normalized, and
/// `y` made perpendicular to it and normalized (the kernel picks a perpendicular when `y` is
/// parallel to `x` or has no length). Restores a frame carried through a rigid motion to
/// exactly unit and orthogonal axes.
///
/// # Errors
/// Non-finite coordinates or an `x` of no length.
pub fn orthonormal_frame(x: Vec3, y: Vec3) -> crate::error::Result<[Vec3; 2]> {
    let [x, y, _] = x.make_unit_ortho_vectors(Some(y))?;
    Ok([x.as_vec(), y.as_vec()])
}

/// Not upstream: plane axes `x` and `y` with each coordinate smaller than `noise` dropped,
/// each made unit again. A flat face's plane read back from a body comes tilted by rounding
/// (about 2e-15); a cut by that plane meets the face only nearly, where the same plane with
/// the rounding dropped meets it exactly. Axes with nothing to drop come back unit and
/// otherwise unchanged. `None` when a coordinate or `noise` is not finite, `noise` is
/// negative, an axis has no length once cleaned, or the cleaned axes are parallel: the
/// caller then keeps the axes it has.
#[must_use]
pub fn axis_squared_frame(x: Vec3, y: Vec3, noise: f64) -> Option<[Vec3; 2]> {
    if !(x.is_finite() && y.is_finite() && noise.is_finite() && noise >= 0.0) {
        return None;
    }
    let clean = |v: Vec3| {
        let c = |a: f64| if a.abs() < noise { 0.0 } else { a };
        Vec3::new(c(v.x), c(v.y), c(v.z))
            .normalized()
            .map(UnitVec3::as_vec)
    };
    let (x, y) = (clean(x)?, clean(y)?);
    x.cross(y).normalized()?;
    Some([x, y])
}

/// Not upstream: the corners of a parallelogram on a plane, such as a reference image pinned to
/// points of a sketch. Its bottom edge runs from `bl` to `br`; its top-left corner is
/// `top_left` when the shape may stretch, and otherwise lies `aspect` (height over
/// width) times the bottom edge's length from `bl`, square to the edge and on its left looking
/// down `normal` (a unit normal of the plane). Returns bottom-left, bottom-right, top-right and
/// top-left; `None` when the corners give no area: an edge not finite or shorter than a
/// nanometre, or the two edges within a microradian of one line.
#[must_use]
pub fn rect_of_aspect(
    bl: Point3,
    br: Point3,
    top_left: Option<Point3>,
    normal: Vec3,
    aspect: f64,
) -> Option<[Point3; 4]> {
    let along = br - bl;
    let up = match top_left {
        Some(tl) => tl - bl,
        None => normal.cross(along) * aspect,
    };
    let (width, height) = (along.length(), up.length());
    let ok = width > 1e-9
        && height > 1e-9
        && width.is_finite()
        && height.is_finite()
        && along.cross(up).length() > 1e-6 * width * height;
    if !ok {
        return None;
    }
    Some([bl, br, br + up, bl + up])
}

/// Not upstream: how a rectangle `aspect` (height over width) times as high as wide fits a frame
/// pinned at `bl`, `br` and `tl`: its width (the bottom edge's length), and whether `tl` lies off
/// the rectangle's own top-left corner, square to the bottom edge on the frame's plane, by more
/// than a millionth of (1 + its larger side): the frame stretches it. `None` when the frame gives
/// no area, as [`rect_of_aspect`] refuses it.
#[must_use]
pub fn fit_aspect(bl: Point3, br: Point3, tl: Point3, aspect: f64) -> Option<(f64, bool)> {
    let (along, up) = (br - bl, tl - bl);
    let normal = along.cross(up).normalized()?.as_vec();
    rect_of_aspect(bl, br, Some(tl), normal, aspect)?;
    let width = along.length();
    let height = width * aspect;
    let own = bl + normal.cross(along) * aspect;
    let stretched = (tl - own).length() > 1e-6 * (1.0 + width.max(height));
    Some((width, stretched))
}

/// Not upstream: a quad offset from its own plane on both sides, such as a decal drawn just off
/// the face it lies on so the face never hides it. `corners` are bottom-left, bottom-right,
/// top-right and top-left; the normal is the unit cross product of the bottom edge with the left
/// one. Returns that normal, the corners moved `lift` along it and the corners moved `lift`
/// against it; `None` for a coordinate or lift not finite, an edge too long to represent or of
/// no direction, or edges along one line (their units' cross product of no length).
#[must_use]
pub fn offset_quad(corners: [Point3; 4], lift: f64) -> Option<(Vec3, [Point3; 4], [Point3; 4])> {
    if !(corners.iter().all(|p| p.is_finite()) && lift.is_finite()) {
        return None;
    }
    let [bl, br, _, tl] = corners;
    let (u, v) = (br - bl, tl - bl);
    // Crossing the units: edges too long to cross as given still have a normal.
    let normal = u
        .normalized()?
        .cross(*v.normalized()?)
        .normalized()?
        .as_vec();
    let off = normal * lift;
    Some((normal, corners.map(|p| p + off), corners.map(|p| p - off)))
}

/// Not upstream: the point `s` along the edge `u` and `t` along the edge `v` of the
/// parallelogram with a corner at `corner`, `corner + s u + t v`, as a picture's texture
/// coordinates place a point on the quad it is drawn on.
#[must_use]
pub fn parallelogram_point(corner: Point3, u: Vec3, v: Vec3, s: f64, t: f64) -> Point3 {
    corner + u * s + v * t
}

/// Why [`box_extent_along`], [`axis_along_own_axes`] and [`plane_offset_square_to`] refuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisRefusal {
    /// A number of the map, the axis or the box is not finite.
    NotFinite,
    /// The axis has no direction (it is zero, or too short to unitize).
    ZeroAxis,
    /// The map flattens space, so it has no inverse.
    FlatMap,
}

/// `axis` (a direction in the outer frame) as the placed body's own frame sees it, unit: the
/// linear part of the inverse of `placement` applied to it, unitized.
fn own_direction(placement: &Affine, axis: Vec3) -> Result<Vec3, AxisRefusal> {
    if !placement.is_finite() || !axis.is_finite() {
        return Err(AxisRefusal::NotFinite);
    }
    let inverse = placement.inverse().ok_or(AxisRefusal::FlatMap)?;
    // The unit axis: a long one would overflow through the map, and only its direction counts.
    let axis = axis.normalized().ok_or(AxisRefusal::ZeroAxis)?;
    let own = inverse.apply_vec(*axis);
    if !own.is_finite() {
        return Err(AxisRefusal::NotFinite);
    }
    own.normalized()
        .map(UnitVec3::as_vec)
        .ok_or(AxisRefusal::ZeroAxis)
}

/// Not upstream: how far a body's box `min`..`max` (in the body's own frame), placed in the outer
/// frame by `placement`, reaches along the outer direction `axis` (any length): with `d` the unit
/// direction that `axis` is in the body's own frame, `Σ |dᵢ| |maxᵢ - minᵢ|`, the width of the
/// box's [`crate::extent3d::Extent3d::span_along`] `d` (summed as widths, so a box far from the
/// origin does not overflow on its ends). Tight when `axis` runs along one of the body's own
/// axes ([`axis_along_own_axes`]).
///
/// # Errors
/// [`AxisRefusal`]: a number not finite (also the box's, or an extent not finite), a zero
/// `axis`, or a `placement` that flattens space.
pub fn box_extent_along(
    min: Point3,
    max: Point3,
    placement: &Affine,
    axis: Vec3,
) -> Result<f64, AxisRefusal> {
    let own = own_direction(placement, axis)?;
    if !min.is_finite() || !max.is_finite() {
        return Err(AxisRefusal::NotFinite);
    }
    let size = max - min;
    let extent = (size.x * own.x).abs() + (size.y * own.y).abs() + (size.z * own.z).abs();
    if extent.is_finite() {
        Ok(extent)
    } else {
        Err(AxisRefusal::NotFinite)
    }
}

/// Not upstream: whether the outer direction `axis` runs along one of the own axes of a body
/// placed by `placement` (so a box of the body is tight along it): the unit direction `axis` is
/// in the body's own frame has a component of at least `1 - 1e-9` in size.
///
/// # Errors
/// [`AxisRefusal`] for a number not finite, a zero `axis`, or a `placement` that flattens space.
pub fn axis_along_own_axes(placement: &Affine, axis: Vec3) -> Result<bool, AxisRefusal> {
    let own = own_direction(placement, axis)?;
    Ok([own.x, own.y, own.z].iter().any(|c| c.abs() >= 1.0 - 1e-9))
}

/// Not upstream: where a flat face, a plane through `origin` with normal `normal` in a body's own
/// frame, cuts the outer direction `axis` once the body is placed by `placement`, if it is square
/// to it: `Some` of the offset of the placed `origin` along the unit `axis` from the outer
/// origin when the placed normal is parallel to `axis` (the unit vectors' cross product no
/// longer than `1e-9`), `None` when it is not, or when `origin` or `normal` is not finite or
/// `normal` or the placed unit normal has no direction.
///
/// # Errors
/// [`AxisRefusal`] for a `placement` or `axis` with a number not finite, or a zero `axis`.
pub fn plane_offset_square_to(
    placement: &Affine,
    origin: Point3,
    normal: Vec3,
    axis: Vec3,
) -> Result<Option<f64>, AxisRefusal> {
    if !placement.is_finite() || !axis.is_finite() {
        return Err(AxisRefusal::NotFinite);
    }
    let axis = axis.normalized().ok_or(AxisRefusal::ZeroAxis)?.as_vec();
    if !origin.is_finite() || !normal.is_finite() {
        return Ok(None);
    }
    let Some(placed) = normal
        .normalized()
        .and_then(|n| placement.apply_vec(*n).normalized())
    else {
        return Ok(None);
    };
    if placed.cross(axis).length() > 1e-9 {
        return Ok(None);
    }
    Ok(Some(
        placement.apply(origin).distance_along(Point3::ORIGIN, axis),
    ))
}

/// The operation name of the refusals of [`centroid_and_mean_radius`].
const OP_CENTROID: &str = "math::centroid_and_mean_radius";

/// A point set's arithmetic centroid and mean Euclidean distance from that centroid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointSetSummary {
    /// The arithmetic mean of the input points.
    pub centroid: Point2,
    /// The arithmetic mean of each point's distance from `centroid`.
    pub mean_distance: f64,
}

/// Not upstream: the arithmetic centroid and mean radial distance of a non-empty point set.
///
/// # Errors
/// [`crate::error::NumError::InvalidInput`] for an empty set, a non-finite point, or a
/// non-finite intermediate or result.
pub fn centroid_and_mean_radius(points: &[Point2]) -> crate::error::Result<PointSetSummary> {
    use crate::error::NumError;
    if points.is_empty() {
        return Err(NumError::invalid_input(OP_CENTROID, "empty point set"));
    }
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(NumError::invalid_input(OP_CENTROID, "non-finite point"));
    }

    let (mut x, mut y) = (0.0, 0.0);
    for point in points {
        x += point.x;
        y += point.y;
        if !x.is_finite() || !y.is_finite() {
            return Err(NumError::invalid_input(
                OP_CENTROID,
                "point centroid overflows",
            ));
        }
    }
    let count = points.len() as f64;
    let centroid = Point2::new(x / count, y / count);
    if !centroid.x.is_finite() || !centroid.y.is_finite() {
        return Err(NumError::invalid_input(
            OP_CENTROID,
            "point centroid is not finite",
        ));
    }

    let mut total_distance = 0.0;
    for point in points {
        let distance = point.distance(centroid);
        if !distance.is_finite() {
            return Err(NumError::invalid_input(
                OP_CENTROID,
                "point distance overflows",
            ));
        }
        total_distance += distance;
        if !total_distance.is_finite() {
            return Err(NumError::invalid_input(
                OP_CENTROID,
                "mean point distance overflows",
            ));
        }
    }
    let mean_distance = total_distance / count;
    if !mean_distance.is_finite() {
        return Err(NumError::invalid_input(
            OP_CENTROID,
            "mean point distance is not finite",
        ));
    }
    Ok(PointSetSummary {
        centroid,
        mean_distance,
    })
}

/// `p · u` for a unit `u`, taken at an eighth of `p` and scaled back (exact, a power of two)
/// when a partial sum overflows: the dot of a point far out is finite while its terms' sum
/// passes `f64::MAX` on the way.
fn far_dot(p: Vec3, u: Vec3) -> f64 {
    let d = p.dot(u);
    if d.is_finite() {
        d
    } else {
        (p * 0.125).dot(u) * 8.0
    }
}

// ---- Not upstream: the 2D arithmetic of a sketch's arc snaps -------------------------------
//
// Plain `[f64; 2]` points and directions, for the applications' arc tools: the circle through
// three points, the arc a typed radius or sweep gives, the tangent arc, the perpendicular
// bisector. Each is written in the order the application wrote it, so a move changes no result.

/// `a - b`.
fn sub_2d([ax, ay]: [f64; 2], [bx, by]: [f64; 2]) -> [f64; 2] {
    [ax - bx, ay - by]
}

/// The dot product.
fn dot_2d([ax, ay]: [f64; 2], [bx, by]: [f64; 2]) -> f64 {
    ax * bx + ay * by
}

/// The length.
fn length_2d([x, y]: [f64; 2]) -> f64 {
    crate::fmath::hypot(x, y)
}

/// `v` turned a quarter turn counter-clockwise.
fn left_2d([x, y]: [f64; 2]) -> [f64; 2] {
    [-y, x]
}

/// `at + along * k`.
fn along_2d([x, y]: [f64; 2], [dx, dy]: [f64; 2], k: f64) -> [f64; 2] {
    [x + dx * k, y + dy * k]
}

/// The point halfway between `a` and `b`.
fn midpoint_2d([ax, ay]: [f64; 2], [bx, by]: [f64; 2]) -> [f64; 2] {
    [(ax + bx) * 0.5, (ay + by) * 0.5]
}

/// Not upstream: `v` made unit, `None` for a vector no longer than `1e-9` or not finite.
#[must_use]
pub fn unit_2d(v: [f64; 2]) -> Option<[f64; 2]> {
    let [x, y] = v;
    let length = length_2d(v);
    (length > 1e-9 && length.is_finite()).then_some([x / length, y / length])
}

/// Not upstream: the unit direction from `from` to `to`, `None` where they coincide.
#[must_use]
pub fn unit_between_2d(from: [f64; 2], to: [f64; 2]) -> Option<[f64; 2]> {
    unit_2d(sub_2d(to, from))
}

/// Not upstream: the unit direction from `from` to `to` turned a quarter turn counter-clockwise,
/// `None` where they coincide.
#[must_use]
pub fn left_normal_between_2d(from: [f64; 2], to: [f64; 2]) -> Option<[f64; 2]> {
    unit_between_2d(from, to).map(left_2d)
}

/// Not upstream: the centre and radius of the circle through `from` and `other` that touches the
/// unit direction `dir` at `from`. `None` when the points are within `1e-9` or `dir` runs along the
/// chord (no such circle).
#[must_use]
pub fn tangent_arc_centre_2d(
    from: [f64; 2],
    other: [f64; 2],
    dir: [f64; 2],
) -> Option<([f64; 2], f64)> {
    let w = sub_2d(from, other);
    let length = length_2d(w);
    if length <= 1e-9 {
        return None;
    }
    // The centre lies on the normal at `from`, as far from it as it is from `other`:
    // |from + n t - other|^2 = t^2.
    let n = left_2d(dir);
    let k = dot_2d(n, w);
    if k.abs() <= 1e-9 * length {
        return None;
    }
    let t = -dot_2d(w, w) / (2.0 * k);
    Some((along_2d(from, n, t), t.abs()))
}

/// Not upstream: the point of the perpendicular bisector of the chord `start`-`end` nearest `p`
/// (the centre of the arc through both ends that has it). `None` when the ends coincide.
#[must_use]
pub fn bisector_centre_2d(start: [f64; 2], end: [f64; 2], p: [f64; 2]) -> Option<[f64; 2]> {
    let mid = midpoint_2d(start, end);
    let n = left_2d(unit_between_2d(start, end)?);
    Some(along_2d(mid, n, dot_2d(n, sub_2d(p, mid))))
}

/// Not upstream: whether the unit direction `dir` at `p` is square to the radius from `centre`
/// (within a millionth). `false` where `p` is on `centre`.
#[must_use]
pub fn is_square_to_radius_2d(centre: [f64; 2], p: [f64; 2], dir: [f64; 2]) -> bool {
    unit_between_2d(centre, p).is_some_and(|radial| dot_2d(dir, radial).abs() <= 1e-6)
}

/// Not upstream: the centre of the circle through `a`, `b` and `c`. `None` for points on one
/// line (or not finite).
#[must_use]
pub fn circumcentre_2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<[f64; 2]> {
    let [abx, aby] = sub_2d(b, a);
    let [acx, acy] = sub_2d(c, a);
    let ab2 = abx * abx + aby * aby;
    let ac2 = acx * acx + acy * acy;
    let d = 2.0 * (abx * acy - aby * acx);
    if !d.is_finite() || d.abs() < 1e-12 {
        return None;
    }
    let [ax, ay] = a;
    let centre = [
        ax + (acy * ab2 - aby * ac2) / d,
        ay + (abx * ac2 - acx * ab2) / d,
    ];
    centre.iter().all(|v| v.is_finite()).then_some(centre)
}

/// Not upstream: the circle through `s`, `e` and `t`: its centre, radius and the sweep (radians)
/// of the arc from `s` to `e` that passes through `t`. `None` for points on one line (twice
/// the signed area of the triangle under `1e-12`), a chord or radius at most `1e-9`, a
/// result that is not finite, or an input that is not finite.
///
/// Computed exactly from the points, in expansions with unbounded exponents
/// (`crate::expansion`), each decision exact and each result rounded once at the end (from
/// a double-double), so every result is within a rounding error or two of the true
/// circle's however ill-conditioned the points (one at `1e300` beside two at `1e-300`,
/// three nearly on a line). For `a = s - t`, `b = e - t` and `d = 2 a x b`: the centre is
/// `(t d + (|a|^2 b - |b|^2 a)^perp) / d`; the radius is `|a| |b| |e - s| / |d|`; the sweep
/// is twice the angle at `t` away from the chord, `2 atan2(|a x b|, -(a . b))` (the
/// inscribed angle at `t` subtends the other arc). The pin computes the centre in plain
/// `f64` and the sweep as `2 asin` of the half chord over the radius, wrong by up to O(1)
/// on such points (`scx-difftest`'s `pin_defects::three_point_circle_2d`).
#[must_use]
pub fn three_point_circle_2d(
    s: [f64; 2],
    e: [f64; 2],
    t: [f64; 2],
) -> Option<([f64; 2], f64, f64)> {
    use crate::expansion::Expansion as X;
    if !s.iter().chain(&e).chain(&t).all(|v| v.is_finite()) {
        return None;
    }
    let diff = |p: [f64; 2], q: [f64; 2]| [X::difference(p[0], q[0]), X::difference(p[1], q[1])];
    let ([ax, ay], [bx, by], [cx, cy]) = (diff(s, t), diff(e, t), diff(e, s));
    let cross = ax.mul(&by).sub(&ay.mul(&bx));
    let d = cross.pow2(1);
    if d.abs().sub(&X::of(1e-12)).sign() < 0 {
        return None;
    }
    let sq = |x: &X, y: &X| x.mul(x).add(&y.mul(y));
    let (a2, b2, c2) = (sq(&ax, &ay), sq(&bx, &by), sq(&cx, &cy));
    let floor = X::of(1e-9).mul(&X::of(1e-9));
    let (a2a, b2a, c2a, da) = (a2.approx(), b2.approx(), c2.approx(), d.approx());
    // The chord above 1e-9 (|c|^2 above 1e-18 exactly: decided exactly when near it).
    let chord_long = c2a
        .clearly_below(floor.approx())
        .map_or_else(|| c2.sub(&floor).sign() > 0, |below| !below);
    if !chord_long || !c2a.sqrt().round().is_finite() {
        return None;
    }
    // r^2 = |a|^2 |b|^2 |c|^2 / d^2, above 1e-18 (decided exactly when near it).
    let r2 = a2a.mul(b2a).mul(c2a).div(da.mul(da));
    let radius_long = r2.clearly_below(floor.approx()).map_or_else(
        || {
            let num = a2.mul(&b2).mul(&c2);
            num.sub(&floor.mul(&d.mul(&d))).sign() > 0
        },
        |below| !below,
    );
    if !radius_long {
        return None;
    }
    let r = r2.sqrt().round();
    // The centre's offset from t, times d, exactly.
    let n = [by.mul(&a2).sub(&ay.mul(&b2)), ax.mul(&b2).sub(&bx.mul(&a2))];
    let mut centre = [0.0; 2];
    for ((c, &ti), ni) in centre.iter_mut().zip(&t).zip(&n) {
        *c = X::of(ti).mul(&d).add(ni).approx().div(da).round();
    }
    if !(r.is_finite() && centre.iter().all(|v| v.is_finite())) {
        return None;
    }
    let dot = ax.mul(&bx).add(&ay.mul(&by));
    let (y, x) = cross.abs().approx().together(dot.neg().approx());
    Some((centre, r, 2.0 * crate::fmath::atan2(y, x)))
}

/// Not upstream: the third point of the arc from `s` to `e` for the cursor `h` with a radius or a
/// sweep (radians; the sweep wins when both are given): on the arc of that size whose middle is
/// nearest the cursor. A radius picks among the minor and major arcs on either side of the chord;
/// a sweep takes the cursor's side. `None`: no such arc (a radius under half the chord, a sweep
/// outside `(0, 2 pi)`, neither given, or coincident ends).
#[must_use]
pub fn arc_third_point_2d(
    s: [f64; 2],
    e: [f64; 2],
    h: [f64; 2],
    radius: Option<f64>,
    sweep: Option<f64>,
) -> Option<[f64; 2]> {
    let chord = sub_2d(e, s);
    let len = length_2d(chord);
    let n = left_2d(unit_2d(chord)?);
    let mid = midpoint_2d(s, e);
    if let Some(sweep) = sweep {
        if !(sweep > 1e-9 && sweep < core::f64::consts::TAU - 1e-9) {
            return None;
        }
        let half = sweep * 0.5;
        let r = len / (2.0 * crate::fmath::sin(half));
        let side = if dot_2d(n, sub_2d(h, mid)) < 0.0 {
            -1.0
        } else {
            1.0
        };
        return Some(along_2d(
            mid,
            n,
            side * r * 2.0 * crate::fmath::powi(crate::fmath::sin(half * 0.5), 2),
        ));
    }
    let r = radius?;
    if !r.is_finite() || r * 2.0 < len - 1e-9 {
        return None;
    }
    let k = (r * r - len * len * 0.25).max(0.0).sqrt();
    [k + r, k - r, r - k, -k - r]
        .into_iter()
        .map(|d| along_2d(mid, n, d))
        .filter(|p| p.iter().all(|v| v.is_finite()))
        .min_by(|x, y| length_2d(sub_2d(*x, h)).total_cmp(&length_2d(sub_2d(*y, h))))
}

#[cfg(test)]
mod tests {
    #[test]
    fn det3_is_the_integer_determinant() {
        // Small integer entries: every product and sum is exact, so `det3` must equal the
        // determinant computed in integers, whatever the order of its terms. The fourth
        // column is ignored.
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 41) as i64 - 20
        };
        for _ in 0..2000 {
            let a: [[i64; 4]; 3] = core::array::from_fn(|_| core::array::from_fn(|_| next()));
            let exact = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
                - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
                + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
            let m = a.map(|r| r.map(|x| x as f64));
            assert_eq!(det3(&m), exact as f64, "{a:?}");
        }
        let id = [
            [1.0, 0.0, 0.0, 9.0],
            [0.0, 1.0, 0.0, 9.0],
            [0.0, 0.0, 1.0, 9.0],
        ];
        assert_eq!(det3(&id), 1.0);
        let swapped = [
            [0.0, 1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        assert_eq!(det3(&swapped), -1.0);
        let singular = [
            [1.0, 2.0, 3.0, 0.0],
            [2.0, 4.0, 6.0, 0.0],
            [7.0, 8.0, 9.0, 0.0],
        ];
        assert_eq!(det3(&singular), 0.0);
    }

    use super::*;

    /// Rounding below `noise` along a world axis is dropped and the axes come back unit; a plane
    /// tilted for real, and one with nothing to drop, keep their axes; parallel, empty or
    /// non-finite axes and a bad `noise` give `None`.
    #[test]
    fn an_axis_squared_frame_drops_only_the_rounding() {
        let tilted = axis_squared_frame(
            Vec3::new(1.0, 2e-15, 0.0),
            Vec3::new(-3e-16, 1.0, 1e-15),
            1e-12,
        );
        assert_eq!(tilted, Some([Vec3::X, Vec3::Y]));
        let (s, c) = (crate::fmath::sin(0.3), crate::fmath::cos(0.3));
        let real = [Vec3::new(c, s, 0.0), Vec3::new(-s, c, 0.0)];
        assert_eq!(axis_squared_frame(real[0], real[1], 1e-12), Some(real));
        assert_eq!(
            axis_squared_frame(Vec3::X, Vec3::new(1.0, 1e-15, 0.0), 1e-12),
            None,
            "parallel once cleaned"
        );
        assert_eq!(
            axis_squared_frame(Vec3::new(1e-13, 0.0, 0.0), Vec3::Y, 1e-12),
            None,
            "no length once cleaned"
        );
        assert_eq!(
            axis_squared_frame(Vec3::new(f64::NAN, 0.0, 0.0), Vec3::Y, 1e-12),
            None
        );
        for noise in [-1.0, f64::INFINITY, f64::NAN] {
            assert_eq!(axis_squared_frame(Vec3::X, Vec3::Y, noise), None);
        }
    }

    #[test]
    fn rigid_maps_include_reflections_but_reject_scale_shear_and_hostile_entries() {
        let rigid = |m: Affine| m.is_rigid(1e-6);
        assert!(rigid(Affine::IDENTITY));
        assert!(rigid(Affine::translation(Vec3::new(3.0, -4.0, 5.0))));
        assert!(rigid(
            Affine::rotation(Point3::ORIGIN, Vec3::Z, 0.7).unwrap()
        ));
        assert!(rigid(Affine::reflection(Point3::ORIGIN, Vec3::X).unwrap()));
        for (row, col, value) in [
            (0, 0, 2.0),
            (1, 0, 0.01),
            (3, 0, f64::INFINITY),
            (2, 1, f64::NAN),
        ] {
            let m = with_entry(Affine::IDENTITY, row, col, value);
            assert!(!rigid(m), "{row} {col} {value}");
        }
        assert!(!Affine::IDENTITY.is_rigid(f64::NAN));
        assert!(!Affine::IDENTITY.is_rigid(0.0));
    }

    #[test]
    fn plane_motion_uses_the_plane_origin_and_axes_and_rejects_tilts_and_reflections() {
        // Plane normal X, with local axes Y and Z, away from the world origin.
        // A quarter turn about X maps (10, 20, 30) to (10, -30, 20).
        let origin = Point3::new(10.0, 20.0, 30.0);
        let motion = |m: Affine| m.plane_motion(origin, Vec3::Y, Vec3::Z, 1e-9, 1e-12);
        let turn = Affine::from_axes(Vec3::X, Vec3::Z, -Vec3::Y, Point3::ORIGIN);
        let (a, b) = motion(turn).unwrap();
        assert_eq!(a, [[0.0, -1.0], [1.0, 0.0]]);
        assert_eq!(b, Vec2::new(-50.0, -10.0));
        assert!(motion(Affine::IDENTITY).is_none());
        assert!(motion(Affine::rotation(Point3::ORIGIN, Vec3::Y, 0.2).unwrap()).is_none());
        assert!(motion(Affine::reflection(Point3::ORIGIN, Vec3::Y).unwrap()).is_none());
        // A normal displacement leaves every projected coordinate unchanged.
        let (a, b) = motion(Affine::translation(Vec3::X * 5.0)).unwrap();
        assert_eq!(a, [[1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(b, Vec2::ZERO);
        assert!(motion(Affine::translation(Vec3::new(f64::NAN, 0.0, 0.0))).is_none());
    }

    /// `a` with upstream's matrix entry `[row][col]` (row-vector, `col` below 3) set to `value`.
    fn with_entry(a: Affine, row: usize, col: usize, value: f64) -> Affine {
        let mut m: [[f64; 4]; 4] = a.into();
        m[row][col] = value;
        Affine::try_from(m).unwrap()
    }

    #[test]
    fn upstream_matrices_read_back_exactly_and_a_projective_one_is_refused() {
        let m = [
            [2.0, 0.0, 0.0, 0.0],
            [1.0, -1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [10.0, 20.0, 30.0, 1.0],
        ];
        let map = Affine::try_from(m).unwrap();
        assert_eq!(<[[f64; 4]; 4]>::from(map), m);
        assert_eq!(
            map.apply(Point3::new(1.0, 2.0, 3.0)),
            Point3::new(14.0, 18.0, 33.0)
        );
        assert_eq!(map.x_axis(), Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(map.translation_part(), Vec3::new(10.0, 20.0, 30.0));
        for (row, value) in [(0, 1.0), (1, 1e-300), (2, f64::NAN), (3, 0.0), (3, 2.0)] {
            let mut p = m;
            p[row][3] = value;
            assert!(matches!(
                Affine::try_from(p),
                Err(crate::error::NumError::InvalidInput { operation, .. }) if operation == OP_MAPS
            ));
        }
    }

    #[test]
    fn transformed_box_holds_its_moved_corners_and_an_empty_box_stays_empty() {
        // A quarter turn about z, then a move by (10, 20, 30): x goes to y, y to -x.
        let map = Affine::from_axes(Vec3::Y, -Vec3::X, Vec3::Z, Point3::new(10.0, 20.0, 30.0));
        let (lo, hi) = map.transformed_box(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 2.0, 3.0));
        let (want_lo, want_hi) = (Point3::new(8.0, 20.0, 30.0), Point3::new(10.0, 21.0, 33.0));
        for (got, want, out) in [
            (lo.x, want_lo.x, -1.0),
            (lo.y, want_lo.y, -1.0),
            (lo.z, want_lo.z, -1.0),
            (hi.x, want_hi.x, 1.0),
            (hi.y, want_hi.y, 1.0),
            (hi.z, want_hi.z, 1.0),
        ] {
            // outside the exact image, by no more than its rounding
            assert!((got - want) * out > 0.0 && (got - want).abs() < 1e-13);
        }
        let empty = (Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 1.0));
        assert_eq!(map.transformed_box(empty.0, empty.1), empty);
        // 1 + 2^-53 rounds to 1: the box still holds it.
        let tiny = crate::fmath::powi(2.0, -53);
        let shift = Affine::translation(Vec3::new(1.0, 0.0, 0.0));
        let p = Point3::new(tiny, 0.0, 0.0);
        let (lo, hi) = shift.transformed_box(p, p);
        assert!(lo.x < 1.0 && hi.x > 1.0 && hi.x - lo.x < 1e-14);
    }

    #[test]
    fn repeated_rigid_turns_keep_plane_axes_square_and_keep_uv_scales() {
        use crate::certified::gamma;
        let tilt = Affine::rotation(Point3::ORIGIN, Vec3::X, 1.5e-6).unwrap();
        let turn = Affine::rotation(Point3::ORIGIN, Vec3::Z, 1_f64.to_radians()).unwrap();
        let [mut u, mut v] = tilt.rigid_plane_axes(Vec3::X * 2.0, Vec3::Y * 3.0).unwrap();
        for _ in 0..360 {
            [u, v] = turn.rigid_plane_axes(u, v).unwrap();
            assert!(u.dot(v).abs() <= gamma(3) * u.length() * v.length());
            assert!((u.length() - 2.0).abs() < 1e-12);
            assert!((v.length() - 3.0).abs() < 1e-12);
        }
    }

    #[test]
    fn a_rigid_turn_preserves_a_skew_plane_parametrisation() {
        let turn = Affine::rotation(Point3::ORIGIN, Vec3::Z, core::f64::consts::FRAC_PI_2).unwrap();
        let [u, v] = turn
            .rigid_plane_axes(Vec3::new(2.0, 0.0, 0.0), Vec3::new(1.0, 3.0, 0.0))
            .unwrap();
        assert!((u - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-14);
        assert!((v - Vec3::new(-3.0, 1.0, 0.0)).length() < 1e-14);
        assert!((u.dot(v) - 2.0).abs() < 1e-14);
    }

    /// The largest difference between two matrices' entries.
    fn apart(a: &Affine, b: &Affine) -> f64 {
        let (a, b): ([[f64; 4]; 4], [[f64; 4]; 4]) = ((*a).into(), (*b).into());
        a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn a_quarter_turn_about_z_takes_x_to_y_and_keeps_its_line() {
        use crate::certified::Verdict;
        let q = core::f64::consts::FRAC_PI_2;
        let turn = Affine::rotation(Point3::ORIGIN, Vec3::new(0.0, 0.0, 2.0), q).unwrap();
        let want = Affine::from_axes(Vec3::Y, -Vec3::X, Vec3::Z, Point3::ORIGIN);
        assert!(apart(&turn, &want) <= 1e-15, "{turn:?}");
        // about the line through (10, 0, 0) along z: that point stays, (11, 0, 0) goes to
        // (10, 1, 0)
        let p = Point3::new(10.0, 0.0, 0.0);
        let turn = Affine::rotation(p, Vec3::Z, q).unwrap();
        assert!(turn.apply(p).distance(p) <= 1e-14);
        let image = turn.apply(Point3::new(11.0, 0.0, 0.0));
        assert!(
            image.distance(Point3::new(10.0, 1.0, 0.0)) <= 1e-14,
            "{image:?}"
        );
        // a turn about a skew axis keeps lengths and its axis
        let axis = Vec3::new(1.0, 2.0, 2.0);
        let turn = Affine::rotation(p, axis, 0.7).unwrap();
        assert!(turn.apply(p + axis * 3.0).distance(p + axis * 3.0) <= 1e-12);
        for r in turn.axis_lengths() {
            assert!((r - 1.0).abs() <= 1e-14);
        }
        assert_eq!(turn.is_regular(1e-9), Verdict::Yes);
    }

    #[test]
    fn a_scale_keeps_its_centre_and_a_mirror_twice_is_the_identity() {
        let c = Point3::new(1.0, -2.0, 3.0);
        let scale = Affine::uniform_scaling(c, 2.5).unwrap();
        assert!(scale.apply(c).distance(c) <= 1e-14);
        let p = scale.apply(c + Vec3::X);
        assert!(p.distance(c + Vec3::X * 2.5) <= 1e-14, "{p:?}");
        // the mirror in y = 5, an axis normal of any length: every entry exact
        let mirror =
            Affine::reflection(Point3::new(7.0, 5.0, -1.0), Vec3::new(0.0, 3.0, 0.0)).unwrap();
        assert_eq!(
            mirror.apply(Point3::new(1.0, 2.0, 3.0)),
            Point3::new(1.0, 8.0, 3.0)
        );
        assert_eq!(mirror * mirror, Affine::IDENTITY);
        // a slanted mirror keeps its plane's points and reverses its normal
        let n = Vec3::new(1.0, 1.0, 0.0);
        let mirror = Affine::reflection(c, n).unwrap();
        assert!(mirror.apply(c).distance(c) <= 1e-14);
        let back = mirror.apply(c + n);
        assert!(back.distance(c - n) <= 1e-14, "{back:?}");
    }

    #[test]
    fn a_map_after_its_inverse_is_the_identity_and_a_singular_one_has_none() {
        // a turn about a line off the origin, then a move and a scale
        let turn =
            Affine::rotation(Point3::new(1.0, 2.0, 3.0), Vec3::new(1.0, -2.0, 2.0), 0.7).unwrap();
        let map = Affine::uniform_scaling(Point3::new(-1.0, 0.5, 2.0), 2.5).unwrap()
            * Affine::translation(Vec3::new(4.0, -5.0, 6.0))
            * turn;
        let inv = map.inverse().unwrap();
        let size = <[[f64; 4]; 4]>::from(map)
            .iter()
            .flatten()
            .fold(1.0_f64, |a, x| a.max(x.abs()));
        for (p, q) in [(map, inv), (inv, map)] {
            let back = q * p;
            assert!(apart(&back, &Affine::IDENTITY) <= 1e-15 * size, "{back:?}");
        }
        // a mirror in a plane square to an axis is its own inverse, exactly
        let mirror = Affine::reflection(Point3::new(7.0, 5.0, -1.0), Vec3::Y).unwrap();
        assert_eq!(mirror.inverse().unwrap(), mirror);
        // a flattening has none
        let flat = Affine::from_axes(Vec3::X, Vec3::Y, Vec3::ZERO, Point3::ORIGIN);
        assert_eq!(flat.inverse(), None);
    }

    #[test]
    fn maps_from_numbers_refuse_what_is_not_finite_or_has_no_size() {
        let bad = |r: crate::error::Result<Affine>| matches!(r, Err(crate::error::NumError::InvalidInput { operation, .. }) if operation == OP_MAPS);
        assert!(bad(Affine::rotation(Point3::ORIGIN, Vec3::ZERO, 1.0)));
        assert!(bad(Affine::rotation(Point3::ORIGIN, Vec3::Z, f64::NAN)));
        assert!(bad(Affine::rotation(
            Point3::new(f64::INFINITY, 0.0, 0.0),
            Vec3::Z,
            1.0
        )));
        assert!(bad(Affine::uniform_scaling(Point3::ORIGIN, 0.0)));
        assert!(bad(Affine::uniform_scaling(Point3::ORIGIN, f64::NAN)));
        assert!(bad(Affine::reflection(Point3::ORIGIN, Vec3::ZERO)));
        assert!(bad(Affine::reflection(
            Point3::ORIGIN,
            Vec3::new(f64::NAN, 1.0, 0.0)
        )));
        assert!(bad(Affine::into_plane_frame(
            Point3::ORIGIN,
            Vec3::ZERO,
            Vec3::Y
        )));
        assert!(bad(Affine::into_plane_frame(
            Point3::ORIGIN,
            Vec3::X,
            Vec3::new(-3.0, 0.0, 0.0)
        )));
        assert!(bad(Affine::into_plane_frame(
            Point3::new(f64::NAN, 0.0, 0.0),
            Vec3::X,
            Vec3::Y
        )));
    }

    /// Into a plane's frame: the plane's origin goes to the origin, a point along its x axis
    /// to X, along its normal to Z (its level above the plane), and the map is rigid. The
    /// plane through (3, 4, 5) with x = 2Z and y = (1, -1, 0) has the unit normal
    /// x̂ × ŷ = (1, 1, 0)/√2; a skew `y` in that plane gives the same frame.
    #[test]
    fn into_a_planes_frame_its_normal_is_z() {
        let o = Point3::new(3.0, 4.0, 5.0);
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let n = Vec3::new(s, s, 0.0);
        let skew = Vec3::new(1.0, -1.0, 7.0);
        for y in [Vec3::new(1.0, -1.0, 0.0), skew] {
            let t = Affine::into_plane_frame(o, Vec3::new(0.0, 0.0, 2.0), y).unwrap();
            let near = |a: Point3, b: Point3| (a - b).length() < 1e-12;
            assert!(near(t.apply(o), Point3::ORIGIN));
            assert!(near(t.apply(o + Vec3::Z * 2.5), Point3::new(2.5, 0.0, 0.0)));
            assert!(near(t.apply(o + n * 4.0), Point3::new(0.0, 0.0, 4.0)));
            assert!(near(
                t.apply(o + Vec3::new(s, -s, 0.0)),
                Point3::new(0.0, 1.0, 0.0)
            ));
            assert!(t.is_regular(0.0).is_yes());
        }
    }

    #[test]
    fn a_rigid_motion_is_regular_and_a_flattening_is_not() {
        use crate::certified::Verdict;
        let turn = Affine::from_axes(
            Vec3::new(0.6, 0.8, 0.0),
            Vec3::new(-0.8, 0.6, 0.0),
            Vec3::Z,
            Point3::new(1.0, 2.0, 3.0),
        );
        assert_eq!(turn.is_regular(1e-9), Verdict::Yes);
        // Scaled by 2, 3, 4: conditioning 0.5; the lower bound proves 24 / sqrt(29 * 244)
        // = 0.285, the rows' lengths cap it at 2 / 4.
        let stretch = Affine::from_axes(
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
            Vec3::new(0.0, 0.0, 4.0),
            Point3::ORIGIN,
        );
        assert_eq!(stretch.is_regular(0.28), Verdict::Yes);
        assert_eq!(stretch.is_regular(0.29), Verdict::Undecided);
        assert_eq!(stretch.is_regular(0.5), Verdict::Undecided);
        assert_eq!(stretch.is_regular(0.51), Verdict::No);
        // The identity's bound is 1 / 3 exactly: not proved above it.
        assert_eq!(Affine::IDENTITY.is_regular(1.0 / 3.0), Verdict::Undecided);
        // Every product of its determinant is exactly 0: proved singular.
        let flat = Affine::from_axes(Vec3::X, Vec3::Y, Vec3::new(1.0, 1.0, 0.0), Point3::ORIGIN);
        assert_eq!(flat.is_regular(0.0), Verdict::No);
        assert_eq!(flat.is_regular(1e-9), Verdict::No);
        // Nearly flat: a determinant of 1e-300 rounds relatively, so it is proved non-zero.
        let thin = Affine::from_axes(
            Vec3::X,
            Vec3::Y,
            Vec3::new(0.3, 0.7, 1e-300),
            Point3::ORIGIN,
        );
        assert_eq!(thin.is_regular(0.0), Verdict::Yes);
        let nan = Affine::translation(Vec3::new(f64::NAN, 0.0, 0.0));
        assert_eq!(nan.is_regular(0.0), Verdict::No);
        let zero_row = Affine::from_axes(Vec3::X, Vec3::ZERO, Vec3::Z, Point3::ORIGIN);
        assert_eq!(zero_row.is_regular(0.0), Verdict::No);
    }

    /// A map that flattens one direction against the others is not regular however its
    /// determinant compares with its rows' lengths (each row's scale apart, `(1e-300, 1, 1)`
    /// was proved regular); one that flattens by less than the ratio is.
    #[test]
    fn a_flattening_against_the_other_directions_is_refused() {
        use crate::certified::Verdict;
        let diag = |a: f64, b: f64, c: f64| {
            Affine::from_axes(
                Vec3::new(a, 0.0, 0.0),
                Vec3::new(0.0, b, 0.0),
                Vec3::new(0.0, 0.0, c),
                Point3::ORIGIN,
            )
        };
        assert_eq!(diag(1e-300, 1.0, 1.0).is_regular(1e-9), Verdict::No);
        assert_eq!(diag(1.0, 1e-10, 1.0).is_regular(1e-9), Verdict::No);
        assert_eq!(diag(1.0, 1.0, 1e-5).is_regular(1e-9), Verdict::Yes);
        // A row spanning more than 2^1022: the scaling flushes its small entry and the other
        // rows' products underflow; never a wrong answer (conditioning about 1e-300).
        let wide = Affine::from_axes(
            Vec3::new(1e300, 1e-300, 0.0),
            Vec3::Y,
            Vec3::Z,
            Point3::ORIGIN,
        );
        assert_eq!(wide.is_regular(1e-9), Verdict::No);
        assert_ne!(wide.is_regular(0.0), Verdict::No);
    }

    /// A uniform scale is regular however small or large, subnormal included: its
    /// conditioning is 1, which no underflow or overflow of the products may hide (a
    /// subnormal scale was `Undecided`: its row scale overflowed).
    #[test]
    fn a_tiny_or_huge_uniform_scale_is_regular() {
        for s in [1e-110, 1e110, 1e-300, 1e300, 1e-310, f64::from_bits(1)] {
            let m = Affine::from_axes(
                Vec3::new(s, 0.0, 0.0),
                Vec3::new(0.0, s, 0.0),
                Vec3::new(0.0, 0.0, s),
                Point3::ORIGIN,
            );
            assert_eq!(m.is_regular(1e-9), crate::certified::Verdict::Yes, "{s}");
        }
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn constants_match_upstream_literals() {
        // Upstream SmMath.h literals round to the std constants bit for bit.
        assert_eq!(PI, 3.1415926535897932384626433832795028841971693993751);
        assert_eq!(HALF_PI, 1.5707963267948965192313216916397514420985846996875);
        assert_eq!(TWO_PI, 6.2831853071795864769252867665590057683943387987502);
    }

    #[test]
    fn dot_cross_length() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(-4.0, 5.0, 0.5);
        assert_eq!(a.dot(b), -4.0 + 10.0 + 1.5);
        assert_eq!(
            a.cross(b),
            Vec3::new(2.0 * 0.5 - 3.0 * 5.0, 3.0 * -4.0 - 0.5, 5.0 + 8.0)
        );
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
        assert_eq!(Vec3::new(3.0, 4.0, 12.0).length(), 13.0);
        assert_eq!(a.cross(b).dot(a), 0.0);
    }

    #[test]
    fn apply_apply_vec_and_mul() {
        let rot_z = Affine::from_axes(Vec3::Y, -Vec3::X, Vec3::Z, Point3::ORIGIN);
        let tr = Affine::translation(Vec3::new(1.0, 2.0, 3.0));
        let p = Point3::new(1.0, 0.0, 0.0);
        assert_eq!(rot_z.apply(p), Point3::new(0.0, 1.0, 0.0));
        // `a * b` applies `b` first, then `a`.
        let both = tr * rot_z;
        assert_eq!(both.apply(p), Point3::new(1.0, 3.0, 3.0));
        assert_eq!(both.apply_vec(p.to_vec()), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(both * Affine::IDENTITY, both);
        assert_eq!(Affine::IDENTITY * both, both);
        assert_eq!(Affine::default(), Affine::IDENTITY);
    }

    #[test]
    fn pivot_displacement_makes_a_turn_about_the_origin_a_turn_about_the_pivot() {
        let rot_z = Affine::from_axes(Vec3::Y, -Vec3::X, Vec3::Z, Point3::ORIGIN);
        let pivot = Vec3::new(0.0, 10.0, 0.0);
        let d = rot_z.pivot_displacement(pivot);
        assert_eq!(d, Vec3::new(10.0, 10.0, 0.0));
        // The pivot stays where it is under the turn followed by the displacement.
        assert_eq!(rot_z.apply_vec(pivot) + d, pivot);
        // No turn, no displacement; a translation in the map is not read as a turn.
        assert_eq!(Affine::IDENTITY.pivot_displacement(pivot), Vec3::ZERO);
        let tr = Affine::translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(tr.pivot_displacement(pivot), Vec3::ZERO);
    }

    #[test]
    fn centroid_and_mean_radius_uses_the_arithmetic_mean_radius() {
        let points = [
            Point2::new(0.0, 0.0),
            Point2::new(6.0, 0.0),
            Point2::new(0.0, 6.0),
        ];
        let summary = centroid_and_mean_radius(&points).unwrap();
        assert_eq!(summary.centroid, Point2::new(2.0, 2.0));
        let expected = (2.0 * 2.0_f64.sqrt() + 4.0 * 5.0_f64.sqrt()) / 3.0;
        assert!((summary.mean_distance - expected).abs() <= 2.0 * f64::EPSILON * expected);
        assert_ne!(
            summary.mean_distance, 4.0,
            "the RMS radius is not the mean radius"
        );

        let moved = [
            Point2::new(0.0, 0.0),
            Point2::new(6.0, 0.0),
            Point2::new(0.0, 12.0),
        ];
        let moved_summary = centroid_and_mean_radius(&moved).unwrap();
        assert_eq!(moved_summary.centroid, Point2::new(2.0, 4.0));
        let moved_expected =
            (2.0 * 5.0_f64.sqrt() + 4.0 * 2.0_f64.sqrt() + 2.0 * 17.0_f64.sqrt()) / 3.0;
        assert!(
            (moved_summary.mean_distance - moved_expected).abs()
                <= 2.0 * f64::EPSILON * moved_expected
        );
    }

    #[test]
    fn centroid_and_mean_radius_accepts_single_and_duplicate_points() {
        let one = Point2::new(3.0, -4.0);
        assert_eq!(
            centroid_and_mean_radius(&[one]).unwrap(),
            PointSetSummary {
                centroid: one,
                mean_distance: 0.0,
            }
        );
        assert_eq!(
            centroid_and_mean_radius(&[one, one, one]).unwrap(),
            PointSetSummary {
                centroid: one,
                mean_distance: 0.0,
            }
        );
    }

    #[test]
    fn centroid_and_mean_radius_refuses_empty_non_finite_and_overflowing_sets() {
        assert!(centroid_and_mean_radius(&[]).is_err());
        assert!(centroid_and_mean_radius(&[Point2::new(f64::NAN, 0.0)]).is_err());
        assert!(centroid_and_mean_radius(&[Point2::new(0.0, f64::INFINITY)]).is_err());
        assert!(centroid_and_mean_radius(&[
            Point2::new(f64::MAX, 0.0),
            Point2::new(f64::MAX, 0.0),
        ])
        .is_err());
        assert!(centroid_and_mean_radius(&[
            Point2::new(-f64::MAX, 0.0),
            Point2::new(f64::MAX, 0.0),
        ])
        .is_err());
    }

    #[test]
    fn vec2_basics() {
        let a = Vec2::new(3.0, 4.0);
        assert_eq!(a.length(), 5.0);
        assert_eq!((a - a + a * 2.0).dot(-a), -50.0);
    }
}

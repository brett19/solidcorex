// Moved from solidcore `solidcore/crates/solidcore/src/geom/transform.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmTransform.h`,
// `source/SMLib/src/SmTransform.cpp`.
// Changes: the matrix part only (the volume, pseudo-box, solution-array, placement,
// database and type-code members stay in solidcore); adds `from_matrix`; the 3x3/4x4
// helpers come from `crate::small_matrix`. Tests: `concat_invert_scale` drops its
// `transform_simple` line (needs a placement); added `singular_axes_do_not_invert` and
// `predicates_and_domain`.
//! General linear (4x4 matrix) transforms from a `uvw` parameter space to an `xyz`
//! projection space, the simplest volume map.
//!
//! Upstream: `source/SMLib/inc/SmTransform.h`, `source/SMLib/src/SmTransform.cpp`.
//!
//! | Rust                                              | Upstream                                              |
//! |---------------------------------------------------|-------------------------------------------------------|
//! | [`Transform`]                                     | `SmTransform`                                         |
//! | [`Transform::IDENTITY`], `Default`                | `SmTransform(const SmContext* = NULL)` (identity)     |
//! | [`Transform::new`]                                | `SmTransform(cpContext, ToOrigin, ToX, ToY, cpToZ)`   |
//! | `Clone` / `Copy`                                  | `SmTransform(const SmTransform&, SmBoolean)`, `Copy`  |
//! | `Drop` (implicit)                                 | `~SmTransform`                                        |
//! | [`Transform::approx_eq`]                          | `operator==` (the transform's own members)            |
//! | [`Transform::matrix`]                             | `m_adT` (field access)                                |
//! | [`Transform::natural_param_domain`]               | `GetNaturalParamDomain`                               |
//! | [`Transform::to_origin`], [`Transform::to_disp`]  | `GetToOrigin`, `GetToDisp`                            |
//! | [`Transform::to_x_axis`], [`Transform::to_y_axis`], [`Transform::to_z_axis`] | `GetToXAxis`, `GetToYAxis`, `GetToZAxis` |
//! | [`Transform::set_to_origin`], [`Transform::set_to_disp`] | `SetToOrigin`, `SetToDisp` (vector and three-double forms) |
//! | [`Transform::set_to_x_axis`], [`Transform::set_to_y_axis`], [`Transform::set_to_z_axis`] | `SetToXAxis`, `SetToYAxis`, `SetToZAxis` (both forms) |
//! | [`Transform::init_ext`]                           | `InitExt`                                             |
//! | [`Transform::init`]                               | `Init`                                                |
//! | [`Transform::set_canonical`]                      | `SetCanonical`                                        |
//! | [`Transform::transform_vector`]                   | `TransformVector`                                     |
//! | [`Transform::inv_transform_point`]                | `InvTransformPoint`                                   |
//! | [`Transform::inv_transform_vector`]               | `InvTransformVector`                                  |
//! | [`Transform::concat_transform`]                   | `ConcatTransform`                                     |
//! | [`Transform::scale_simple`]                       | `ScaleSimple`                                         |
//! | [`Transform::mirror_simple`]                      | `MirrorSimple`                                        |
//! | [`Transform::invert_simple`]                      | `InvertSimple`                                        |
//! | [`Transform::evaluate_simple`]                    | `EvaluateSimple`                                      |
//! | [`Transform::inv_evaluate_guess_point_simple`]    | `InvEvaluateGuessPointSimple`                         |
//! | [`Transform::is_bounded_simple`]                  | `IsBoundedSimple`                                     |
//! | [`Transform::is_curve_in_param_domain_simple`]    | `IsCurveInParamDomainSimple`                          |
//!
//! Stay in solidcore (they need the volume base, pseudo boxes, solution arrays, a
//! placement or the database, all above this layer): `TransformSimple`,
//! `SetAxis2Placement`, `EvaluateBoundingBoxSimple`, `GlobalPointSolveSimple`,
//! `LocalPointSolveSimple`, `HasDiscontinuitiesSimple`, `ReadFromDB`, `WriteToDB` and the
//! `GetType` / `IsKindOf` family. [`Transform::from_matrix`] (not upstream) lets
//! solidcore's `ReadFromDB` build one from the rows it reads.
//!
//! Deviations:
//! - **Volume base.** `SmTransform` derives from `SmVolume` (context, attributes,
//!   compounding next/orient maps). Here a [`Transform`] is the matrix only: the
//!   constructors and `Copy` copy the matrix, and [`Transform::approx_eq`] is the member
//!   comparison `operator==` runs after `SmVolume::operator==` has passed.
//! - Out-params become return values. `InvTransformPoint`, `InvTransformVector`
//!   and `InvEvaluateGuessPointSimple` return `Err` (upstream `SM_ERR`, code 1001)
//!   for a singular axes sub-matrix, where upstream leaves the output unset.
//!   `EvaluateSimple` returns the `(n+1)^3` derivative array.
//! - `IsCurveInParamDomainSimple` ignores its curve upstream and takes none here.
//! - `GetToXAxis` and friends return copies; writes go through the setters.
//! - `GetMemoryUsed`, `Dump`, `DrawAxis`, `AssertValid`, `WriteToDB` and the members
//!   upstream never reaches from the kernel (`TransformPoint`, `MirrorSimple`,
//!   `RotateAboutAxis*`, `DecomposeToAngles`, ... listed in
//!   `harness/callgraph/unused_functions.txt`) are not part of this unit.

use crate::error::{NumError, Result};
use crate::extent3d::Extent3d;
use crate::math::INFINITE_PARAMETER;
use crate::small_matrix::{inverse_3x3, inverse_4x4, mult_4x4};
use crate::tol::is_zero;
use crate::vector3d::{Point3, Vec3};

/// `GW_MAX_DERIV`: the highest derivative `EvaluateSimple` accepts.
pub const MAX_DERIV: usize = 5;

/// A general linear transform, the 4x4 matrix `T` of `SmTransform`, mapping a
/// parameter-space point `[u v w 1]` to the projection-space point `[u v w 1] * T`.
///
/// Rows 0..2 hold the images of the `u`, `v`, `w` unit vectors (the "to" axes),
/// row 3 the image of the origin (the displacement); column 3 is the projective
/// part, `(0, 0, 0, 1)` for affine transforms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    m: [[f64; 4]; 4],
}

impl Default for Transform {
    /// The identity transform (`SmTransform()`).
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// The identity transform (`SmTransform()` / `Init`).
    pub const IDENTITY: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// The transform taking the parameter axes to `x_axis`, `y_axis`, `z_axis`
    /// (`x_axis × y_axis` when `None`) and the parameter origin to `origin`
    /// (`SmTransform(cpContext, ToOrigin, ToXAxis, ToYAxis, cpToZAxis)`).
    #[must_use]
    pub fn new(origin: Point3, x_axis: Vec3, y_axis: Vec3, z_axis: Option<Vec3>) -> Self {
        // SmVolume(cpContext) leaves m_adT uninitialised; SetCanonical writes every entry.
        let mut t = Self::IDENTITY;
        t.set_canonical(origin, x_axis, y_axis, z_axis);
        t
    }

    /// The 4x4 matrix `m_adT[row][col]`.
    #[must_use]
    pub const fn matrix(&self) -> &[[f64; 4]; 4] {
        &self.m
    }

    fn get_row(&self, i: usize) -> Vec3 {
        self.m
            .get(i)
            .map_or(Vec3::ZERO, |r| Vec3::new(r[0], r[1], r[2]))
    }

    fn set_row(&mut self, i: usize, v: Vec3) {
        if let Some(r) = self.m.get_mut(i) {
            r[0] = v.x;
            r[1] = v.y;
            r[2] = v.z;
        }
    }

    /// Tolerant equality of every matrix entry (`SM_IS_ZERO(a - b)`): the
    /// transform-specific part of `operator==`.
    #[must_use]
    pub fn approx_eq(&self, other: &Self) -> bool {
        self.m
            .iter()
            .flatten()
            .zip(other.m.iter().flatten())
            .all(|(a, b)| is_zero(a - b))
    }

    /// The parameter domain, unbounded in every direction (`GetNaturalParamDomain`).
    #[must_use]
    pub const fn natural_param_domain(&self) -> Extent3d {
        Extent3d::from_bounds(
            -INFINITE_PARAMETER,
            -INFINITE_PARAMETER,
            -INFINITE_PARAMETER,
            INFINITE_PARAMETER,
            INFINITE_PARAMETER,
            INFINITE_PARAMETER,
        )
    }

    /// The image of the parameter origin, row 3 (`GetToOrigin`).
    #[must_use]
    pub fn to_origin(&self) -> Point3 {
        self.get_row(3).to_point()
    }

    /// The image of the `u` unit vector, row 0 (`GetToXAxis`).
    #[must_use]
    pub fn to_x_axis(&self) -> Vec3 {
        self.get_row(0)
    }

    /// The image of the `v` unit vector, row 1 (`GetToYAxis`).
    #[must_use]
    pub fn to_y_axis(&self) -> Vec3 {
        self.get_row(1)
    }

    /// The image of the `w` unit vector, row 2 (`GetToZAxis`).
    #[must_use]
    pub fn to_z_axis(&self) -> Vec3 {
        self.get_row(2)
    }

    /// The displacement, row 3 as a vector: [`Transform::to_origin`] less the origin
    /// (`GetToDisp`).
    #[must_use]
    pub fn to_disp(&self) -> Vec3 {
        self.get_row(3)
    }

    /// Sets the projective column to `(0, 0, 0, 1)` (`InitExt`).
    pub fn init_ext(&mut self) {
        self.m[0][3] = 0.0;
        self.m[2][3] = 0.0;
        self.m[1][3] = 0.0;
        self.m[3][3] = 1.0;
    }

    /// Sets row 3 (`SetToOrigin`).
    pub fn set_to_origin(&mut self, origin: Point3) {
        self.set_row(3, origin.to_vec());
    }

    /// Sets row 0 (`SetToXAxis`).
    pub fn set_to_x_axis(&mut self, v: Vec3) {
        self.set_row(0, v);
    }

    /// Sets row 1 (`SetToYAxis`).
    pub fn set_to_y_axis(&mut self, v: Vec3) {
        self.set_row(1, v);
    }

    /// Sets row 2 (`SetToZAxis`).
    pub fn set_to_z_axis(&mut self, v: Vec3) {
        self.set_row(2, v);
    }

    /// Sets the displacement, row 3 (`SetToDisp`).
    pub fn set_to_disp(&mut self, v: Vec3) {
        self.set_row(3, v);
    }

    /// Resets to the identity transform (`Init`).
    pub fn init(&mut self) {
        self.set_to_x_axis(Vec3::new(1.0, 0.0, 0.0));
        self.set_to_y_axis(Vec3::new(0.0, 1.0, 0.0));
        self.set_to_z_axis(Vec3::new(0.0, 0.0, 1.0));
        self.set_to_disp(Vec3::ZERO);
        self.init_ext();
    }

    /// Sets the axes and displacement; `z_axis` defaults to `x_axis × y_axis`. Non-unit
    /// or non-orthogonal axes add scaling or shear (`SetCanonical`).
    pub fn set_canonical(
        &mut self,
        origin: Point3,
        x_axis: Vec3,
        y_axis: Vec3,
        z_axis: Option<Vec3>,
    ) {
        let z = z_axis.unwrap_or_else(|| x_axis.cross(y_axis));
        self.set_to_origin(origin);
        self.set_to_x_axis(x_axis);
        self.set_to_y_axis(y_axis);
        self.set_to_z_axis(z);
        self.init_ext();
    }

    /// Maps a parameter-space vector to projection space, ignoring the displacement
    /// (`TransformVector`).
    #[must_use]
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let x = self.to_x_axis();
        let y = self.to_y_axis();
        let z = self.to_z_axis();
        Vec3::new(
            v.x * x.x + v.y * y.x + v.z * z.x,
            v.x * x.y + v.y * y.y + v.z * z.y,
            v.x * x.z + v.y * y.z + v.z * z.z,
        )
    }

    /// Maps a projection-space point back to parameter space (`InvTransformPoint`).
    ///
    /// # Errors
    ///
    /// `SM_ERR` when the axes sub-matrix is singular.
    pub fn inv_transform_point(&self, p: Point3) -> Result<Point3> {
        let mid = p - self.to_origin();
        self.inv_transform_vector(mid)
            .map(Vec3::to_point)
            .map_err(NumError::call(
                "geom::transform::Transform::inv_transform_point",
            ))
    }

    /// Maps a projection-space vector back to parameter space through the inverse
    /// of the axes sub-matrix (`InvTransformVector`).
    ///
    /// # Errors
    ///
    /// `SM_ERR` when the axes sub-matrix is singular (`smgu_3x3Inverse` fails).
    pub fn inv_transform_vector(&self, v: Vec3) -> Result<Vec3> {
        let inv = inverse_3x3(
            &self.to_x_axis().to_array(),
            &self.to_y_axis().to_array(),
            &self.to_z_axis().to_array(),
        )
        .map_err(NumError::call(
            "geom::transform::Transform::inv_transform_vector",
        ))?;
        let [i0, i1, i2] = inv.map(Vec3::from_array);
        let t_inv = Self::new(Point3::ORIGIN, i0, i1, Some(i2));
        Ok(t_inv.transform_vector(v))
    }

    /// Appends `other` to this transform sequence: `T = T * other`
    /// (`ConcatTransform`).
    pub fn concat_transform(&mut self, other: &Self) {
        self.m = mult_4x4(&self.m, &other.m);
    }

    /// Scales the projection space by `scale` per coordinate, about `center` (the
    /// origin when `None`) (`ScaleSimple`).
    pub fn scale_simple(&mut self, scale: Vec3, center: Option<Point3>) {
        let mut x = self.to_x_axis();
        let mut y = self.to_y_axis();
        let mut z = self.to_z_axis();
        let mut d = self.to_disp();

        x.x *= scale.x;
        y.x *= scale.x;
        z.x *= scale.x;

        x.y *= scale.y;
        y.y *= scale.y;
        z.y *= scale.y;

        x.z *= scale.z;
        y.z *= scale.z;
        z.z *= scale.z;

        if let Some(c) = center {
            d.x += -x.x * c.x - y.x * c.y - z.x * c.z;
            d.y += -x.y * c.x - y.y * c.y - z.y * c.z;
            d.z += -x.z * c.x - y.z * c.y - z.z * c.z;
        }

        self.set_to_x_axis(x);
        self.set_to_y_axis(y);
        self.set_to_z_axis(z);
        self.set_to_disp(d);
    }

    /// Appends the mirror about the plane through `plane_pt` with normal
    /// `plane_normal` (projection space) to this transform (`MirrorSimple`).
    ///
    /// # Errors
    ///
    /// `InvalidInput` for a normal with no direction ([`Vec3::normalized`] is `None`:
    /// upstream's `SER` on `Unitize`, and here also a normal not finite), or a mirrored
    /// transform with an entry not finite (a plane far enough out that the displacement
    /// overflows; upstream answers it); the transform is then unchanged.
    pub fn mirror_simple(&mut self, plane_pt: Point3, plane_normal: Vec3) -> Result<()> {
        // unitize Plane Normal
        let n = plane_normal.normalized().ok_or(NumError::InvalidInput {
            operation: "geom::transform::Transform::mirror_simple",
            reason: "zero-length or non-finite normal",
        })?;

        // Build mirror transform            = [1-2NxNx  -2NxNy  -2NxNz  0]
        //                                     [ -2NyNy 1-2NyNy  -2NyNz  0]
        //  with D = crPlanePt.Dot(pNormal) ;  [ -2NzNx  -2NzNy 1-2NzNz  0]
        //                                     [  2D*Nx   2D*Ny   2D*Nz  1]
        // The plane's offset; a partial sum past `f64::MAX` is taken at an eighth (exact).
        let d = plane_pt.to_vec().dot(*n);
        let d = if d.is_finite() {
            d
        } else {
            (plane_pt.to_vec() * 0.125).dot(*n) * 8.0
        };
        let mut mirror = Self::IDENTITY;
        mirror.set_to_x_axis(Vec3::new(
            1.0 - 2.0 * n.x * n.x,
            -2.0 * n.x * n.y,
            -2.0 * n.x * n.z,
        ));
        mirror.set_to_y_axis(Vec3::new(
            -2.0 * n.y * n.x,
            1.0 - 2.0 * n.y * n.y,
            -2.0 * n.y * n.z,
        ));
        mirror.set_to_z_axis(Vec3::new(
            -2.0 * n.z * n.x,
            -2.0 * n.z * n.y,
            1.0 - 2.0 * n.z * n.z,
        ));
        // `(d n) 2`, not `(2 d) n`: a plane near `f64::MAX` would overflow on `2 d`.
        mirror.set_to_disp(*n * d * 2.0);

        // Mirror current transform; a mirror whose numbers overflow (a plane far enough out
        // that its displacement is too large to represent) is refused, the transform unchanged.
        let mut mirrored = *self;
        mirrored.concat_transform(&mirror);
        if !mirrored.m.iter().flatten().all(|v| v.is_finite()) {
            return Err(NumError::InvalidInput {
                operation: "geom::transform::Transform::mirror_simple",
                reason: "a mirror too large to represent",
            });
        }
        *self = mirrored;
        Ok(())
    }

    /// Inverts the 4x4 matrix in place (`InvertSimple`).
    ///
    /// # Errors
    ///
    /// `SM_ERR` when the matrix is singular; the transform is then unchanged.
    pub fn invert_simple(&mut self) -> Result<()> {
        self.m = inverse_4x4(&self.m)
            .map_err(NumError::call("geom::transform::Transform::invert_simple"))?;
        Ok(())
    }

    /// Position and partial derivatives at `param_point` up to order
    /// `highest_deriv`, as the `(n+1)^3` array `D[u][v][w]` (index
    /// `(u*(n+1)+v)*(n+1)+w`, `n = highest_deriv`). Only the position and the three
    /// first derivatives (the axes) are non-zero (`EvaluateSimple`).
    ///
    /// # Errors
    ///
    /// `SM_ERR_INVALID_INPUT` when `highest_deriv > MAX_DERIV`.
    pub fn evaluate_simple(&self, param_point: Point3, highest_deriv: usize) -> Result<Vec<Vec3>> {
        if highest_deriv > MAX_DERIV {
            return Err(NumError::InvalidInput {
                operation: "geom::transform::Transform::evaluate_simple",
                reason: "highest derivative exceeds GW_MAX_DERIV",
            });
        }
        let n1 = highest_deriv + 1;
        let mut ders = vec![Vec3::ZERO; n1 * n1 * n1];
        let sv = |u: usize, v: usize, w: usize| (u * n1 + v) * n1 + w;

        let x = self.to_x_axis();
        let y = self.to_y_axis();
        let z = self.to_z_axis();
        let d = self.to_disp();

        if let Some(p) = ders.get_mut(0) {
            *p = d + param_point.x * x + param_point.y * y + param_point.z * z;
        }
        if highest_deriv >= 1 {
            for (idx, axis) in [(sv(1, 0, 0), x), (sv(0, 1, 0), y), (sv(0, 0, 1), z)] {
                if let Some(slot) = ders.get_mut(idx) {
                    *slot = axis;
                }
            }
        }
        Ok(ders)
    }

    /// The parameter point mapping to `proj_point`, as the single guess of the
    /// Newton solvers (`InvEvaluateGuessPointSimple`).
    ///
    /// # Errors
    ///
    /// `SM_ERR` when the axes sub-matrix is singular.
    pub fn inv_evaluate_guess_point_simple(&self, proj_point: Point3) -> Result<Vec<Point3>> {
        Ok(vec![self.inv_transform_point(proj_point)?])
    }

    /// A transform is never bounded (`IsBoundedSimple`).
    #[must_use]
    pub const fn is_bounded_simple(&self) -> bool {
        false
    }

    /// Every curve lies in the (unbounded) parameter domain
    /// (`IsCurveInParamDomainSimple`).
    #[must_use]
    pub const fn is_curve_in_param_domain_simple(&self) -> bool {
        true
    }

    /// The transform with matrix `m` (not upstream: solidcore's `ReadFromDB` fills the
    /// matrix it reads into one).
    #[must_use]
    pub const fn from_matrix(m: [[f64; 4]; 4]) -> Self {
        Self { m }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z)
    }
    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn identity_and_canonical() {
        let t = Transform::default();
        assert_eq!(t, Transform::IDENTITY);
        let c = Transform::new(p(1.0, 2.0, 3.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), None);
        assert_eq!(c.to_z_axis(), v(0.0, 0.0, 1.0));
        assert_eq!(c.to_disp(), v(1.0, 2.0, 3.0));
        assert_eq!(c.matrix()[3], [1.0, 2.0, 3.0, 1.0]);
        let mut i = c;
        i.init();
        assert_eq!(i, Transform::IDENTITY);
    }

    #[test]
    fn evaluate_and_inverse_round_trip() {
        let t = Transform::new(p(1.0, -2.0, 0.5), v(2.0, 0.0, 0.0), v(0.0, 3.0, 1.0), None);
        let d = t.evaluate_simple(p(0.25, -1.0, 4.0), 1).unwrap();
        assert_eq!(d.len(), 8);
        assert_eq!(d[4], t.to_x_axis());
        assert_eq!(d[2], t.to_y_axis());
        assert_eq!(d[1], t.to_z_axis());
        let back = t.inv_transform_point(d[0].to_point()).unwrap();
        assert!((back - p(0.25, -1.0, 4.0)).length() < 1e-12);
        assert!(t.evaluate_simple(Point3::ORIGIN, MAX_DERIV + 1).is_err());
    }

    #[test]
    fn singular_axes_do_not_invert() {
        let flat = Transform::new(Point3::ORIGIN, v(1.0, 0.0, 0.0), v(2.0, 0.0, 0.0), None);
        assert!(flat.inv_transform_vector(v(1.0, 1.0, 1.0)).is_err());
        assert!(flat
            .inv_evaluate_guess_point_simple(Point3::ORIGIN)
            .is_err());
    }

    #[test]
    fn concat_invert_scale() {
        let mut a = Transform::new(p(1.0, 2.0, 3.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0), None);
        let b = a;
        a.invert_simple().unwrap();
        a.concat_transform(&b);
        assert!(a.approx_eq(&Transform::IDENTITY));

        let mut s = Transform::IDENTITY;
        s.scale_simple(v(2.0, 3.0, 4.0), Some(p(1.0, 1.0, 1.0)));
        assert_eq!(s.to_disp(), v(-2.0, -3.0, -4.0));
        let mut z = Transform::IDENTITY;
        z.scale_simple(Vec3::ZERO, None);
        assert!(z.invert_simple().is_err());
    }

    #[test]
    fn predicates_and_domain() {
        let t = Transform::IDENTITY;
        assert!(!t.is_bounded_simple());
        assert!(t.is_curve_in_param_domain_simple());
        let d = t.natural_param_domain();
        assert_eq!(d.u_min(), -INFINITE_PARAMETER);
        assert_eq!(Transform::from_matrix(*t.matrix()), t);
    }
}

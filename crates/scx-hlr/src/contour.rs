// Written for solidcorex. Apache-2.0.
//! A torus's silhouette, in closed form.
//!
//! On the torus of frame `(o; e1, e2, a)`, major radius `R` and minor radius `r`, write the
//! view `d = d1 e1 + d2 e2 + da a`, `ρ̂(θ) = cos θ e1 + sin θ e2`, `k(θ) = d · ρ̂(θ)` and
//! `s(θ) = √(da² + k²)`. The point of the meridian at `θ` whose outward normal is
//! `n = σ (−da ρ̂ + k a) / s` (`σ = ±1`) has `n · d = σ(−da k + k da)/s = 0`, so
//!
//! ```text
//! p_σ(θ) = o + R ρ̂(θ) + r σ (−da ρ̂(θ) + k(θ) a) / s(θ)
//! ```
//!
//! is the whole silhouette: two closed branches, one per `σ`, each a point per meridian.
//! (On the screen it is the projected spine circle, an ellipse, offset by `r` along its
//! normal: the torus is the envelope of spheres of radius `r` on its spine, and a sphere's
//! outline is a circle of radius `r` about its centre's image.) `s` is never zero unless
//! `da = 0`, where the branches become four circles: the top and bottom circles (radius `R`
//! at heights `±r`) and the two meridians in the plane normal to `d` through the axis. With
//! `d1 = d2 = 0` they are the two equators, radius `R ± r`. Both special forms are taken only
//! on exact zeros, so no tolerance decides the form; near them the general form is exact
//! too.
//!
//! **Boundary crossings, in closed form.** A torus face is bounded by meridians (circles in
//! a plane through the axis, `θ = θ0`) and parallels (circles about the axis at height `h`
//! and radius `ρc`, `w = v0`). A branch meets the meridian at `θ0` at `θ0` itself. It meets
//! the parallel where its normal's slope is the parallel's: `cos w = −σ da / s` and
//! `sin w = σ k / s` give `k(θ) = −da tan v0`, so with `k(θ) = |dρ| cos(θ − φd)`,
//!
//! ```text
//! θ = φd ± acos(q),   q = −da h / (|dρ| (ρc − R))
//! ```
//!
//! (on one branch only, but both are split: an extra split changes nothing). A boundary edge
//! of any other shape is named ([`Undecided::TorusEdge`]).

use core::f64::consts::{PI, TAU};

use scx_brep::FaceId;
use scx_curve::{Circle, Curve};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::Torus;

use crate::error::HlrError;
use crate::silhouette::{Candidate, Rim, SilhouetteCurve, Splitter, Undecided, whole_turn};
use crate::tolerance::PARALLEL;
use crate::view::{View, place_point, place_vec};

/// Which branch of a torus's contour: the sign `σ` of its normal `σ (−da ρ̂ + k a) / s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Branch {
    /// `σ = +1`.
    Plus,
    /// `σ = −1`.
    Minus,
}

impl Branch {
    const fn sign(self) -> f64 {
        match self {
            Self::Plus => 1.0,
            Self::Minus => -1.0,
        }
    }
}

/// One branch of a torus's silhouette under a view the torus's axis is neither along nor
/// across, parametrised by the meridian angle `θ ∈ [0, 2π]` (see the module notes).
#[derive(Debug, Clone, PartialEq)]
pub struct TorusContour {
    frame: Frame,
    major: f64,
    minor: f64,
    sight: [f64; 3],
    branch: Branch,
}

impl TorusContour {
    /// The torus's frame: its centre and axes, `z` the axis.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The major radius.
    #[must_use]
    pub const fn major(&self) -> Length {
        Length::from_mm(self.major)
    }

    /// The minor radius.
    #[must_use]
    pub const fn minor(&self) -> Length {
        Length::from_mm(self.minor)
    }

    /// The direction of sight's components along the frame's `x`, `y` and `z`.
    #[must_use]
    pub const fn sight(&self) -> [f64; 3] {
        self.sight
    }

    /// The branch.
    #[must_use]
    pub const fn branch(&self) -> Branch {
        self.branch
    }

    /// The radial direction `ρ̂(θ)` and the branch's surface normal at `θ`.
    fn at(&self, theta: f64) -> (Vec3, Vec3) {
        let (s, c) = fmath::sin_cos(theta);
        let f = &self.frame;
        let rho = *f.x() * c + *f.y() * s;
        let [d1, d2, da] = self.sight;
        let k = d1 * c + d2 * s;
        let len = fmath::hypot(da, k);
        let n = (rho * -da + *f.z() * k) * (self.branch.sign() / len);
        (rho, n)
    }

    /// The point at `θ`.
    #[must_use]
    pub fn point(&self, theta: f64) -> Point3 {
        let (rho, n) = self.at(theta);
        self.frame.origin() + rho * self.major + n * self.minor
    }

    /// The torus's outward normal at the point at `θ` (normal to the direction of sight).
    #[must_use]
    pub fn normal(&self, theta: f64) -> Vec3 {
        self.at(theta).1
    }

    /// The meridian angle of `p`, in `[0, 2π)`.
    pub(crate) fn angle_of(&self, p: Point3) -> f64 {
        let r = p - self.frame.origin();
        fmath::atan2(r.dot(*self.frame.y()), r.dot(*self.frame.x())).rem_euclid(TAU)
    }

    /// The same contour on the torus placed by `place` (the sight's components in the
    /// torus's frame do not change).
    pub(crate) fn placed(&self, place: &Frame) -> Option<Self> {
        let f = &self.frame;
        let frame = Frame::from_axes(
            place_point(place, f.origin()),
            place_vec(place, *f.x()),
            place_vec(place, *f.z()),
        )
        .ok()?;
        Some(Self {
            frame,
            ..self.clone()
        })
    }
}

/// What a torus face's boundary edge is to the torus.
enum Kind {
    /// The meridian at this angle.
    Meridian(f64),
    /// The parallel at this height and radius.
    Parallel { h: f64, rho: f64 },
}

/// The kind of `rim` on the torus `(f, major, minor)`, within its `near`.
fn kind(rim: &Rim<'_>, f: &Frame, major: f64, minor: f64) -> Result<Kind, Undecided> {
    let c = match rim.curve {
        Curve::Circle(c) => c,
        Curve::Nurbs(_) => return Err(Undecided::NurbsEdge { edge: rim.edge }),
        _ => return Err(Undecided::TorusEdge { edge: rim.edge }),
    };
    let near = rim.near;
    let a = *f.z();
    let rad = c.radius().mm();
    let nz = *c.frame().z();
    let rel = c.frame().origin() - f.origin();
    let h = rel.dot(a);
    let radial = rel - a * h;
    let dist = radial.length();
    if dist <= near && nz.cross(a).length() * rad <= near {
        return Ok(Kind::Parallel { h, rho: rad });
    }
    let in_axis_plane = radial
        .normalized()
        .is_some_and(|u| nz.dot(*u).abs() * rad <= near);
    if h.abs() <= near
        && (dist - major).abs() <= near
        && (rad - minor).abs() <= near
        && nz.dot(a).abs() * rad <= near
        && in_axis_plane
    {
        return Ok(Kind::Meridian(fmath::atan2(
            rel.dot(*f.y()),
            rel.dot(*f.x()),
        )));
    }
    Err(Undecided::TorusEdge { edge: rim.edge })
}

/// The silhouette candidates of a torus face bounded by `rims`.
pub(crate) fn candidates(
    face: FaceId,
    torus: &Torus,
    view: &View,
    rims: &[Rim<'_>],
) -> Result<Vec<Candidate>, HlrError> {
    let f = torus.frame();
    let (major, minor) = (torus.major().mm(), torus.minor().mm());
    let d = *view.d();
    let (d1, d2, da) = (d.dot(*f.x()), d.dot(*f.y()), d.dot(*f.z()));
    let m = fmath::hypot(d1, d2);
    let kinds: Result<Vec<Kind>, Undecided> =
        rims.iter().map(|r| kind(r, f, major, minor)).collect();
    let meridians = |kinds: &[Kind]| -> Vec<f64> {
        kinds
            .iter()
            .filter_map(|k| match k {
                Kind::Meridian(t) => Some(*t),
                Kind::Parallel { .. } => None,
            })
            .collect()
    };
    let circle = |frame: Frame, radius: f64, splits: Result<Vec<f64>, Undecided>| {
        Circle::new(frame, Length::from_mm(radius), Sense::Forward)
            .map_err(|error| HlrError::Curve { face, error })
            .and_then(|c| {
                Ok(Candidate {
                    curve: SilhouetteCurve::Circle(c),
                    range: whole_turn()?,
                    closed: true,
                    splitter: Splitter::Found(splits),
                })
            })
    };
    let frame_at = |origin: Point3, x: Vec3, z: Vec3| {
        Frame::from_axes(origin, x, z).map_err(|_| HlrError::Defect {
            reason: "a torus circle's axes give no frame",
        })
    };
    let (o, e1, a) = (f.origin(), *f.x(), *f.z());
    let mut out = Vec::new();
    if m == 0.0 {
        // Along the axis: the equators, parametrised by θ.
        let splits = kinds.as_deref().map(meridians).map_err(|e| *e);
        out.push(circle(frame_at(o, e1, a)?, major + minor, splits.clone())?);
        if major > minor {
            out.push(circle(frame_at(o, e1, a)?, major - minor, splits)?);
        }
        return Ok(out);
    }
    if da == 0.0 {
        // Across the axis: the top and bottom circles, parametrised by θ, and the two
        // meridians normal to the view, parametrised by the minor angle w.
        let splits = kinds.as_deref().map(meridians).map_err(|e| *e);
        for z in [minor, -minor] {
            out.push(circle(frame_at(o + a * z, e1, a)?, major, splits.clone())?);
        }
        let star = (*f.x() * -d2 + *f.y() * d1) * (1.0 / m);
        let parallels = kinds.as_deref().map_err(|e| *e).map(|ks| {
            ks.iter()
                .filter_map(|k| match k {
                    Kind::Parallel { h, rho } => Some(fmath::atan2(*h, rho - major)),
                    Kind::Meridian(_) => None,
                })
                .collect::<Vec<_>>()
        });
        for rho in [star, -star] {
            let frame = frame_at(o + rho * major, rho, rho.cross(a))?;
            out.push(circle(frame, minor, parallels.clone())?);
        }
        return Ok(out);
    }
    let phi = fmath::atan2(d2, d1);
    let splits = kinds.map(|ks| {
        let mut s = Vec::new();
        for k in &ks {
            match *k {
                Kind::Meridian(t) => s.push(t),
                Kind::Parallel { h, rho } => {
                    let q = -da * h / (m * (rho - major));
                    if q.abs() <= 1.0 + PARALLEL {
                        let g = fmath::acos(q.clamp(-1.0, 1.0));
                        s.push(phi + g);
                        s.push(phi - g);
                    }
                }
            }
        }
        s
    });
    for branch in [Branch::Plus, Branch::Minus] {
        out.push(Candidate {
            curve: SilhouetteCurve::Torus(TorusContour {
                frame: *f,
                major,
                minor,
                sight: [d1, d2, da],
                branch,
            }),
            range: whole_turn()?,
            closed: true,
            splitter: Splitter::Found(splits.clone()),
        });
    }
    Ok(out)
}

/// Planes that cut the boundary edge `rim` of a torus face wherever the face's normal is
/// square to the view (`n · d = 0`): the edge's crossings with the contour, found as
/// crossings with planes (the edge lies on the torus, so a plane's crossings with it are
/// among the planes' own; an extra plane crossing changes nothing).
///
/// - A meridian at angle `θ` (the outward `ρ̂ = cos θ x + sin θ y`): the normal at the
///   minor angle `φ` is `cos φ ρ̂ + sin φ a`, square to `d` at `φ0 = atan2(ρ̂·d, -d_a)` and
///   `φ0 + π`; the planes are the horizontal ones at the heights `r sin φ`.
/// - A parallel at height `h`, radius `ρ`: `n · d` is `((ρ - R) m cos(θ - φ_d) + h d_a) / r`
///   with `m = |d_ρ|` and `φ_d = atan2(d_y, d_x)`, so zero at `θ = φ_d ± acos q`,
///   `q = -d_a h / (m (ρ - R))` (the torus code's own); the planes are the ones through the
///   axis holding those meridians. None where the factor is zero (the sign is then constant
///   along the parallel).
///
/// # Errors
///
/// [`Undecided`] for a NURBS edge or one that is neither a meridian nor a parallel.
pub(crate) fn edge_planes(
    rim: &Rim<'_>,
    torus: &Torus,
    view: &View,
) -> Result<Vec<Frame>, Undecided> {
    let f = torus.frame();
    let (major, minor) = (torus.major().mm(), torus.minor().mm());
    let d = *view.d();
    let (d1, d2, da) = (d.dot(*f.x()), d.dot(*f.y()), d.dot(*f.z()));
    let (o, a) = (f.origin(), *f.z());
    let m = fmath::hypot(d1, d2);
    let mut out = Vec::new();
    match kind(rim, f, major, minor)? {
        Kind::Meridian(theta) => {
            let rho_d = fmath::cos(theta) * d1 + fmath::sin(theta) * d2;
            if fmath::hypot(rho_d, da) > PARALLEL {
                let phi = fmath::atan2(rho_d, -da);
                for p in [phi, phi + PI] {
                    if let Some(fr) = Frame::new(o + a * (minor * fmath::sin(p)), a, *f.x()) {
                        out.push(fr);
                    }
                }
            }
        }
        Kind::Parallel { h, rho } => {
            let k = m * (rho - major);
            if m > PARALLEL && k.abs() > PARALLEL * major.max(minor) {
                let q = -da * h / k;
                if q.abs() <= 1.0 + PARALLEL {
                    let g = fmath::acos(q.clamp(-1.0, 1.0));
                    let phi = fmath::atan2(d2, d1);
                    for t in [phi + g, phi - g] {
                        let n = *f.x() * -fmath::sin(t) + *f.y() * fmath::cos(t);
                        if let Some(fr) = Frame::new(o, n, a) {
                            out.push(fr);
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

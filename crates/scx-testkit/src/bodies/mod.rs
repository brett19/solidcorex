// Written for solidcorex. Apache-2.0.
//! Seeded body generators (research 63 §6.4, BR8): valid bodies of any size, and broken
//! variants that each break one structure invariant, all built through `scx-brep`'s public
//! [`Draft`] API and deterministic from a seed and a size.
//!
//! Behind the `bodies` feature: this module alone uses the geometry and topology types, so
//! the truth references above stay on plain data and the geometry layers' tests do not
//! build `scx-brep` (layers.txt `[testkit]`; 63 §6.10).
//!
//! - [`Kind`] names the valid generators; [`generate`] builds one. Each gives a
//!   [`Generated`]: the draft, and the counts a closed form says it has.
//! - [`check::violations`] is the structure oracle (63 §6.4): the invariants a draft breaks.
//!   Every valid body breaks none.
//! - [`loose`] builds a valid body whose trims carry a recorded accuracy, for the gap closure
//!   of a measure (MA2).
//! - [`broken`] builds, for each [`Invariant`] but [`Invariant::UvGap`], a body that breaks
//!   it and no other.
//!
//! Seed 0 is the canonical body: the world frame, round dimensions and no jitter, so a test
//! can compare it with the pin's primitive of the same size. Every other seed places the
//! body in a random right-handed frame and draws its dimensions and jitter.
//!
//! The size is the subdivision: faces per side for a block, a sheet and a torus, wall
//! faces for a cylinder, a cone and a sphere, sides past three for a prism and a hole, and
//! blocks past one for a stack, and cells past one along a side for a checkerboard. Size
//! 0 is size 1.

pub mod check;
mod curved;
mod loose;
pub mod made;
mod mesh;
mod planar;
mod shell;
mod wreck;

use core::fmt;

use scx_brep::{Draft, TableFull};
use scx_curve::{Curve2, CurveError, Line2};
use scx_num::domain::UvDomain;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{PcurveError, SurfaceError};

pub use check::Invariant;
pub use loose::loose;
pub use shell::quarter_shell;
pub use wreck::broken;

/// The resolution the generators build pcurves to: its approximation tolerance is
/// 1e-9 mm, far inside any resolution.
#[must_use]
pub fn build_resolution() -> Resolution {
    // 2e-9 is finite and above zero.
    Resolution::new(Length::from_mm(2e-9)).unwrap_or_default()
}

/// A valid generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A box, each face `n x n` planar quads.
    Block,
    /// A prism over a star-shaped polygon of `n + 3` sides.
    Prism,
    /// A planar sheet of `n x n` quads, both sides the outside.
    QuadSheet,
    /// A planar sheet of `2 n^2` triangles.
    TriSheet,
    /// A capped cylinder of `n` wall faces; one wall face crosses a seam.
    Cylinder,
    /// A cone to its apex over a planar base, `n` wall faces, each with a collapsed edge
    /// at the apex (a pole); one wall face crosses a seam.
    Cone,
    /// A sphere of `n` lunes, each with a collapsed edge at either pole; one lune crosses
    /// a seam.
    Sphere,
    /// A torus of `n x n` patches; one patch crosses both seams.
    Torus,
    /// A slab with a through hole of `n + 3` sides: faces with inner loops, genus 1.
    Holed,
    /// A block with a block-shaped void inside: one solid region of two shells.
    Cavity,
    /// `n + 1` blocks stacked, each its own solid region, sharing faces: edges of three
    /// fins in radial order.
    Stack,
    /// An `(n + 1) x (n + 1)` checkerboard of blocks, each its own solid region, diagonal
    /// neighbours sharing an edge: `n^2` edges of four fins in radial order.
    Checker,
    /// The checkerboard's blocks as one solid region of one shell, touching along the
    /// edges of four fins: sectors the solid and the outside by turns, so only the
    /// geometry fixes the radial order.
    Touching,
}

impl Kind {
    /// Every generator.
    pub const ALL: [Self; 13] = [
        Self::Block,
        Self::Prism,
        Self::QuadSheet,
        Self::TriSheet,
        Self::Cylinder,
        Self::Cone,
        Self::Sphere,
        Self::Torus,
        Self::Holed,
        Self::Cavity,
        Self::Stack,
        Self::Checker,
        Self::Touching,
    ];

    /// A stable name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Prism => "prism",
            Self::QuadSheet => "quad_sheet",
            Self::TriSheet => "tri_sheet",
            Self::Cylinder => "cylinder",
            Self::Cone => "cone",
            Self::Sphere => "sphere",
            Self::Torus => "torus",
            Self::Holed => "holed",
            Self::Cavity => "cavity",
            Self::Stack => "stack",
            Self::Checker => "checker",
            Self::Touching => "touching",
        }
    }

    /// The face count at `size`, from the closed form.
    pub fn faces(self, size: usize) -> usize {
        counts(self, size.max(1)).faces
    }
}

/// What a generator built, and what it should hold.
#[derive(Debug, Clone)]
pub struct Generated {
    /// The body, unfinished.
    pub draft: Draft,
    /// The counts the closed form gives.
    pub counts: Counts,
}

/// The entity counts of a body, and its Euler characteristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Regions, the outside included.
    pub regions: usize,
    /// Shells.
    pub shells: usize,
    /// Faces.
    pub faces: usize,
    /// Loops.
    pub loops: usize,
    /// Uses.
    pub uses: usize,
    /// Edges, collapsed ones included.
    pub edges: usize,
    /// Collapsed edges (poles).
    pub collapsed: usize,
    /// Vertices.
    pub vertices: usize,
    /// `V - (E - C) + F - (L - F)`: a collapsed edge bounds nothing, so it is not counted.
    /// For one closed shell of genus `g`, `2 - 2g`; summed over a body's closed shells
    /// where every face bounds one solid; for a sheet with one boundary, 1.
    pub euler: i64,
}

/// Why a generator failed: always a bug in the generator or in the layer below it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GenError {
    /// A table ran out of ids.
    Table(TableFull),
    /// A curve constructor refused.
    Curve(CurveError),
    /// A surface constructor refused.
    Surface(SurfaceError),
    /// No exact pcurve.
    Pcurve(PcurveError),
    /// A frame or an interval with no extent, named.
    Degenerate(&'static str),
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Table(e) => write!(f, "generator: {e}"),
            Self::Curve(e) => write!(f, "generator: curve: {e}"),
            Self::Surface(e) => write!(f, "generator: surface: {e}"),
            Self::Pcurve(e) => write!(f, "generator: pcurve: {e}"),
            Self::Degenerate(what) => write!(f, "generator: degenerate {what}"),
        }
    }
}

impl std::error::Error for GenError {}

impl From<TableFull> for GenError {
    fn from(e: TableFull) -> Self {
        Self::Table(e)
    }
}
impl From<CurveError> for GenError {
    fn from(e: CurveError) -> Self {
        Self::Curve(e)
    }
}
impl From<SurfaceError> for GenError {
    fn from(e: SurfaceError) -> Self {
        Self::Surface(e)
    }
}
impl From<PcurveError> for GenError {
    fn from(e: PcurveError) -> Self {
        Self::Pcurve(e)
    }
}

/// The result of a generator.
pub type Result<T> = core::result::Result<T, GenError>;

/// Builds the `kind` body for `seed` at `size`.
///
/// # Errors
///
/// [`GenError`] only on a bug: every seed and size is meant to build.
pub fn generate(kind: Kind, seed: u64, size: usize) -> Result<Generated> {
    let n = size.max(1);
    let mut dice = Dice::new(seed);
    let draft = match kind {
        Kind::Block => planar::block(&mut dice, n),
        Kind::Prism => planar::prism(&mut dice, n),
        Kind::QuadSheet => planar::sheet(&mut dice, n, false),
        Kind::TriSheet => planar::sheet(&mut dice, n, true),
        Kind::Holed => planar::holed(&mut dice, n),
        Kind::Cavity => planar::cavity(&mut dice, n),
        Kind::Stack => planar::stack(&mut dice, n),
        Kind::Checker => planar::checker(&mut dice, n, false),
        Kind::Touching => planar::checker(&mut dice, n, true),
        Kind::Cylinder => curved::cylinder(&mut dice, n),
        Kind::Cone => curved::cone(&mut dice, n),
        Kind::Sphere => curved::sphere(&mut dice, n),
        Kind::Torus => curved::torus(&mut dice, n),
    }?;
    Ok(Generated {
        draft,
        counts: counts(kind, n),
    })
}

/// The closed-form counts of `kind` at `n >= 1`.
fn counts(kind: Kind, n: usize) -> Counts {
    // (regions, shells, faces, loops, edges, collapsed, vertices); uses follow.
    let (r, s, f, l, e, c, v, uses) = match kind {
        Kind::Block => (
            2,
            2,
            6 * n * n,
            6 * n * n,
            12 * n * n,
            0,
            6 * n * n + 2,
            24 * n * n,
        ),
        Kind::Prism => {
            let m = n + 3;
            (2, 2, m + 2, m + 2, 3 * m, 0, 2 * m, 6 * m)
        }
        Kind::QuadSheet => (
            1,
            1,
            n * n,
            n * n,
            2 * n * (n + 1),
            0,
            (n + 1) * (n + 1),
            4 * n * n,
        ),
        Kind::TriSheet => (
            1,
            1,
            2 * n * n,
            2 * n * n,
            2 * n * (n + 1) + n * n,
            0,
            (n + 1) * (n + 1),
            6 * n * n,
        ),
        Kind::Cylinder => (2, 2, n + 2, n + 2, 3 * n, 0, 2 * n, 6 * n),
        Kind::Cone => (2, 2, n + 1, n + 1, 3 * n, n, n + 1, 5 * n),
        Kind::Sphere => (2, 2, n, n, 3 * n, 2 * n, 2, 4 * n),
        Kind::Torus => (2, 2, n * n, n * n, 2 * n * n, 0, n * n, 4 * n * n),
        Kind::Holed => {
            let m = n + 3;
            (2, 2, m + 6, m + 8, 12 + 3 * m, 0, 8 + 2 * m, 24 + 6 * m)
        }
        Kind::Cavity => (
            3,
            4,
            12 * n * n,
            12 * n * n,
            24 * n * n,
            0,
            12 * n * n + 4,
            48 * n * n,
        ),
        Kind::Stack => (
            n + 2,
            n + 2,
            5 * n + 6,
            5 * n + 6,
            8 * n + 12,
            0,
            4 * n + 8,
            20 * n + 24,
        ),
        Kind::Checker | Kind::Touching => {
            // The black cells, the inner lattice edges they share, and the lattice
            // points they reach: all but the two corners on white cells when `n` is odd.
            let blocks = ((n + 1) * (n + 1)).div_ceil(2);
            let r = if kind == Kind::Checker { blocks + 1 } else { 2 };
            let v = 2 * ((n + 2) * (n + 2) - 2 * (n % 2));
            (
                r,
                r,
                6 * blocks,
                6 * blocks,
                12 * blocks - n * n,
                0,
                v,
                24 * blocks,
            )
        }
    };
    let euler = v as i64 - (e - c) as i64 + f as i64 - (l - f) as i64;
    Counts {
        regions: r,
        shells: s,
        faces: f,
        loops: l,
        uses,
        edges: e,
        collapsed: c,
        vertices: v,
        euler,
    }
}

/// The seeded draws: none for seed 0, which takes every canonical value.
pub(crate) struct Dice {
    rng: Option<Rng>,
}

impl Dice {
    pub(crate) const fn new(seed: u64) -> Self {
        Self {
            rng: if seed == 0 {
                None
            } else {
                Some(Rng::new(seed))
            },
        }
    }

    /// Uniform in `[lo, hi)`, or `canonical` for seed 0.
    pub(crate) fn range(&mut self, lo: f64, hi: f64, canonical: f64) -> f64 {
        match &mut self.rng {
            Some(r) => lo + (hi - lo) * r.next_f64(),
            None => canonical,
        }
    }

    /// Uniform in `[-a, a)`, or zero for seed 0.
    pub(crate) fn jitter(&mut self, a: f64) -> f64 {
        self.range(-a, a, 0.0)
    }

    /// A random right-handed frame at an origin in `[-10, 10)^3`; the world frame for
    /// seed 0.
    pub(crate) fn frame(&mut self) -> Frame {
        let Some(r) = &mut self.rng else {
            return Frame::WORLD;
        };
        for _ in 0..64 {
            let o = Point3::new(
                10.0 * r.next_signed_unit(),
                10.0 * r.next_signed_unit(),
                10.0 * r.next_signed_unit(),
            );
            let z = Vec3::new(
                r.next_signed_unit(),
                r.next_signed_unit(),
                r.next_signed_unit(),
            );
            let x = Vec3::new(
                r.next_signed_unit(),
                r.next_signed_unit(),
                r.next_signed_unit(),
            );
            if z.length() > 0.1
                && let Some(f) = Frame::new(o, z, x)
                && z.cross(x).length() > 0.1 * z.length() * x.length()
            {
                return f;
            }
        }
        Frame::WORLD
    }
}

/// A rigid placement: local coordinates to world.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Place {
    pub(crate) frame: Frame,
}

impl Place {
    pub(crate) fn point(&self, l: [f64; 3]) -> Point3 {
        let f = &self.frame;
        f.origin() + (f.x() * l[0] + f.y() * l[1] + f.z() * l[2])
    }

    pub(crate) fn vec(&self, l: [f64; 3]) -> Vec3 {
        let f = &self.frame;
        f.x() * l[0] + f.y() * l[1] + f.z() * l[2]
    }

    /// The frame at local `origin` with local axis `z` and x hint `x`.
    pub(crate) fn frame_at(&self, origin: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Result<Frame> {
        Frame::new(self.point(origin), self.vec(z), self.vec(x))
            .ok_or(GenError::Degenerate("frame"))
    }
}

/// The uv point where a use in `sense` starts or ends on `curve` over `[lo, hi]`.
pub(crate) fn use_ends(curve: &Curve2, lo: f64, hi: f64, sense: Sense) -> [Point2; 2] {
    let (a, b) = (curve.point(lo), curve.point(hi));
    match sense {
        Sense::Forward => [a, b],
        Sense::Reversed => [b, a],
    }
}

/// `curve` moved by whole periods of `domain` so that a use in `sense` over `[lo, hi]`
/// starts at `at`: the period window a face's loop needs (pcurve module docs, 07 §3
/// rule 5). Unmoved where no whole period closes the gap.
pub(crate) fn aligned(
    curve: Curve2,
    lo: f64,
    hi: f64,
    sense: Sense,
    at: Point2,
    domain: &UvDomain,
) -> Result<Curve2> {
    let [start, _] = use_ends(&curve, lo, hi, sense);
    let gap = at - start;
    let whole = |g: f64, p: Option<f64>| match p {
        Some(p) => (g / p).round(),
        None => 0.0,
    };
    let (pu, pv) = (domain.u.period(), domain.v.period());
    let (mu, mv) = (whole(gap.x, pu), whole(gap.y, pv));
    if mu == 0.0 && mv == 0.0 {
        return Ok(curve);
    }
    shifted(&curve, [mu, mv], [pu.unwrap_or(0.0), pv.unwrap_or(0.0)])
}

/// `curve` moved by `m[i]` whole periods `p[i]` along u and v.
pub(crate) fn shifted(curve: &Curve2, m: [f64; 2], p: [f64; 2]) -> Result<Curve2> {
    let d = Vec2::new(m[0] * p[0], m[1] * p[1]);
    match curve {
        Curve2::Line(l) => Ok(Curve2::Line(Line2::new(l.origin() + d, l.direction())?)),
        Curve2::AngleLine(a) => {
            use scx_curve::{AngleAxis, AngleLine2};
            let tau = core::f64::consts::TAU;
            // The angle coordinate moves by whole turns of the angle; the other by the
            // period itself.
            let (turns, other) = match a.axis() {
                AngleAxis::U => (m[0], d.y),
                AngleAxis::V => (m[1], d.x),
            };
            let line = AngleLine2::new(
                a.arcs(),
                a.axis(),
                a.start() + turns * tau,
                a.sense(),
                a.other() + other,
                a.rate(),
            )?;
            Ok(Curve2::AngleLine(line))
        }
        _ => Err(GenError::Degenerate("pcurve kind to shift")),
    }
}

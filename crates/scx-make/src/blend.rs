// Derived from solidcore `solidcore/crates/solidcore/src/sweep/section_blend.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: none (not in SMLib). SolidCraft's
// `loft_blend` (`crates/kernel/src/loft_blend.rs` at 52afe21) and the Hermite rows of its
// `bridge::section`, moved into the kernel so no application builds the surface.
// Changes: the surface is CN4's skin through the sections with the first and last section's
// Hermite conditions as `Edge::Held`, so interior sections are interpolated, not given
// finite-difference conditions, and the u degree is a global one; typed errors; no
// section/station caps; certified `through` and a rail certificate from `scx_surface`; the
// definition record, frames builders and open-section turns are MK12b's.

//! The multi-section blend (research 75 §7, task MK12): one B-spline surface through ordered
//! cross-sections that leaves the first and the last of them with the continuity their frames
//! ask for, G0 to G3.
//!
//! **The surface.** The sections run along `u`, each sampled at the same stations along `v`.
//! Section `i` sits at `u = i / (n - 1)` and the surface passes through every station of it.
//! Each section is the curve through its stations (degree `min(3, stations - 1)`, parameters
//! `s / (stations - 1)`, averaged knots), so all sections share one knot vector; the surface is
//! [`scx_nurbs_surface::skin::skin`] across them (CN4: the global interpolation along `u` at
//! the sections' parameters, with `Edge::Held` conditions at the first and last section).
//!
//! **The ends.** The first and last section hold what their [`EndOrder`] asks, from the
//! caller's frames: the tangent `leave * speed`, the curvature `bend * speed^2` and the change
//! of curvature from `bend_change` (only its part square to the tangent holds the condition;
//! the rest is a reparametrisation and is left zero). The speed is the station's chord to the
//! neighbouring section over 3, times `1 + crown`, over the interval `1 / (n - 1)`. The last
//! section's frames point back into the blend, so its odd derivatives turn round. Each derivative
//! is given at the stations; the fit's linearity turns it into the derivative control column
//! `Edge::Held` takes (the same interpolation through the derivative vectors).
//!
//! **The degree along `u`** is `min(max(c + c' - 1, 3), n + c + c' - 3)` for `c`, `c'` the
//! conditions at the two ends (G0 is 1): the pin's degree of a two-section span, at least cubic
//! when the sections allow it, and never above what the rows and conditions can fix. Two G0
//! sections are degree 1, the ruled surface.
//!
//! **What it is not.** The pin held finite-difference conditions at every interior section and
//! joined Bezier spans. That was SolidCraft's definition of the blend, not a surface with
//! a truth of its own. Here the interior sections are only interpolated, so the surface is as
//! smooth as the skin's simple knots (`C^(q-1)` along `u`), and a quadratic family is the
//! quadratic exactly (the skin holds degree `q >= 2` polynomials).
//!
//! **Refused**, naming why: fewer than two sections; fewer than 2 stations, or sections of
//! different station counts; a frame that is not finite; a crown outside `-0.9 ..= 3`; a
//! `leave` of no length at an end that uses it; two neighbouring sections that meet at a
//! station. The two floors (a tangent's length and a chord) are exact tests, never an adjusted
//! input.

use core::fmt;

use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::fmath;
use scx_num::units::Accuracy;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::fit::{self, End, FitError};
use scx_nurbs::{CompatibleCurves, NurbsCurve, NurbsError};
use scx_nurbs_surface::NurbsSurface;
use scx_nurbs_surface::fit::Edge;
use scx_nurbs_surface::skin::{Skin, SkinError, skin};
use scx_surface::SurfaceError;
use scx_surface::certificate::{Node, SpanBound, span_bound};

/// Fewest stations a cross-section is sampled at (a straight line across it).
pub const MIN_STATIONS: usize = 2;

/// One station of a cross-section: its point and, at the first and last section, how the blend
/// leaves it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionFrame {
    /// The point.
    pub p: Point3,
    /// The direction the blend leaves in, towards the next section (at the last section, back
    /// into the blend, towards the section before). Its length is ignored.
    pub leave: Vec3,
    /// The curvature vector the blend takes along `leave` (zero for a straight start).
    pub bend: Vec3,
    /// The arc-length derivative of `bend` along `leave`.
    pub bend_change: Vec3,
}

impl SectionFrame {
    /// A station that holds nothing beyond its point (for G0 ends and interior sections).
    #[must_use]
    pub const fn at(p: Point3) -> Self {
        Self {
            p,
            leave: Vec3::new(0.0, 0.0, 0.0),
            bend: Vec3::new(0.0, 0.0, 0.0),
            bend_change: Vec3::new(0.0, 0.0, 0.0),
        }
    }
}

/// The continuity a blend holds at its first or last section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndOrder {
    /// Position.
    G0,
    /// Position and tangent direction.
    G1,
    /// And curvature.
    G2,
    /// And change of curvature.
    G3,
}

impl EndOrder {
    /// The conditions held: 1 to 4 (position, tangent, curvature, change of curvature).
    #[must_use]
    pub const fn conditions(self) -> usize {
        match self {
            Self::G0 => 1,
            Self::G1 => 2,
            Self::G2 => 3,
            Self::G3 => 4,
        }
    }
}

/// Which input is wrong.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum BlendRefusal {
    /// Fewer than two sections.
    Sections {
        /// How many were given.
        count: usize,
    },
    /// A section with fewer than [`MIN_STATIONS`] stations.
    Stations {
        /// How many the first section has.
        count: usize,
    },
    /// A section whose station count differs from the first's.
    Uneven {
        /// The section.
        section: usize,
        /// Its stations.
        count: usize,
        /// The first section's.
        expected: usize,
    },
    /// A crown not in `-0.9 ..= 3` (or not finite).
    Crown {
        /// 0 for the first section's, 1 for the last's.
        end: usize,
        /// The value.
        value: f64,
    },
    /// A frame with a coordinate that is not finite.
    NotFinite {
        /// The section.
        section: usize,
        /// The station.
        station: usize,
    },
    /// An end's `leave` has no length where its order uses it.
    NoDirection {
        /// 0 for the first section, 1 for the last.
        end: usize,
        /// The station.
        station: usize,
    },
    /// Two neighbouring sections meet at a station: no room for a blend there.
    Meet {
        /// The earlier section.
        section: usize,
        /// The station.
        station: usize,
    },
    /// A derivative the end holds is not finite (a tangent without room for the change of
    /// curvature, or an overflow).
    Derivative {
        /// 0 for the first section, 1 for the last.
        end: usize,
        /// The station.
        station: usize,
    },
}

/// Why no blend was made.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum BlendError {
    /// The input is wrong.
    Refused(BlendRefusal),
    /// A fit through a section's stations or derivatives failed. A context frame.
    Station(FitError),
    /// The fitted sections are not compatible curves. A context frame.
    Curves(NurbsError),
    /// The skin across the sections failed. A context frame.
    Skin(SkinError),
    /// A step failed through the module's own fault. A bug.
    Defect {
        /// The step.
        why: &'static str,
    },
}

impl BlendError {
    /// What the caller can do about it.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::Station(e) => e.kind(),
            Self::Curves(e) => e.kind(),
            Self::Skin(e) => e.kind(),
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(BlendRefusal::Sections { .. }) => "blend_sections",
            Self::Refused(BlendRefusal::Stations { .. }) => "blend_stations",
            Self::Refused(BlendRefusal::Uneven { .. }) => "blend_uneven",
            Self::Refused(BlendRefusal::Crown { .. }) => "blend_crown",
            Self::Refused(BlendRefusal::NotFinite { .. }) => "blend_not_finite",
            Self::Refused(BlendRefusal::NoDirection { .. }) => "blend_no_direction",
            Self::Refused(BlendRefusal::Meet { .. }) => "blend_meet",
            Self::Refused(BlendRefusal::Derivative { .. }) => "blend_derivative",
            Self::Station(_) => "blend_station_fit",
            Self::Curves(_) => "blend_curves",
            Self::Skin(_) => "blend_skin",
            Self::Defect { .. } => "blend_defect",
        }
    }
}

impl fmt::Display for BlendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(r) => match r {
                BlendRefusal::Sections { count } => {
                    write!(f, "a blend runs through at least 2 sections, not {count}")
                }
                BlendRefusal::Stations { count } => {
                    write!(f, "a cross-section takes at least 2 stations, not {count}")
                }
                BlendRefusal::Uneven {
                    section,
                    count,
                    expected,
                } => write!(
                    f,
                    "section {section} has {count} stations, the first has {expected}"
                ),
                BlendRefusal::Crown { end, value } => {
                    write!(f, "the crown {value} of end {end} is not in -0.9 ..= 3")
                }
                BlendRefusal::NotFinite { section, station } => {
                    write!(f, "station {station} of section {section} is not finite")
                }
                BlendRefusal::NoDirection { end, station } => {
                    write!(f, "end {end} leaves station {station} in no direction")
                }
                BlendRefusal::Meet { section, station } => write!(
                    f,
                    "sections {section} and {} meet at station {station}",
                    section + 1
                ),
                BlendRefusal::Derivative { end, station } => write!(
                    f,
                    "end {end} holds a derivative at station {station} that is not finite"
                ),
            },
            Self::Station(_) => f.write_str("a section's stations could not be fitted"),
            Self::Curves(_) => f.write_str("the sections are not compatible curves"),
            Self::Skin(_) => f.write_str("the skin across the sections failed"),
            Self::Defect { why } => write!(f, "internal defect: {why}"),
        }
    }
}

impl std::error::Error for BlendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Station(e) => Some(e),
            Self::Curves(e) => Some(e),
            Self::Skin(e) => Some(e),
            _ => None,
        }
    }
}

/// A blend and how far it misses its stations.
#[derive(Debug, Clone, PartialEq)]
pub struct Blended {
    /// The surface: section `i` of `n` is `S(i / (n - 1), .)`, the stations along `v` at
    /// `s / (stations - 1)`.
    pub surface: NurbsSurface<3>,
    /// A certified bound of `|S(u_i, v) - C_i(v)|` over every section curve `C_i` and every
    /// `v` (CN4's `through`).
    pub through: f64,
    sections: CompatibleCurves<3>,
}

impl Blended {
    /// The curve through section `i`'s stations (the surface's `u = i / (n - 1)` row is within
    /// [`Blended::through`] of it), or `None` past the last.
    #[must_use]
    pub fn section(&self, i: usize) -> Option<NurbsCurve<3>> {
        self.sections.curve(i)
    }

    /// The sections' count.
    #[must_use]
    pub fn count(&self) -> usize {
        self.sections.count()
    }

    /// A proved bound of the distance between the surface and section `i`'s curve as a rail
    /// (`scx_surface::certificate::span_bound`, the correspondence `u = i / (n - 1)`, `v = t`):
    /// the same quantity [`Blended::through`] bounds, from the surface's own patches, so it
    /// also holds for a surface that was edited since. `None` for `i` past the last section.
    ///
    /// # Errors
    ///
    /// As `span_bound`.
    pub fn rail_bound(
        &self,
        i: usize,
        tol: Accuracy,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Option<SpanBound>, SurfaceError> {
        let Some(rail) = self.section(i) else {
            return Ok(None);
        };
        let n = self.count();
        #[allow(clippy::cast_precision_loss)] // a count of sections
        let u = if i + 1 == n {
            1.0
        } else {
            i as f64 / (n - 1) as f64
        };
        let (t0, t1) = (rail.domain().min(), rail.domain().max());
        let a = Node {
            t: t0,
            uv: Point2::new(u, t0),
        };
        let b = Node {
            t: t1,
            uv: Point2::new(u, t1),
        };
        span_bound(&self.surface, &rail, a, b, tol, budget, stop).map(Some)
    }
}

type Arr = [f64; 3];

fn arr(v: Vec3) -> Arr {
    [v.x, v.y, v.z]
}

fn parr(p: Point3) -> Arr {
    [p.x, p.y, p.z]
}

fn finite(a: Arr) -> bool {
    a.iter().all(|x| x.is_finite())
}

fn add(a: Arr, b: Arr) -> Arr {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Arr, b: Arr) -> Arr {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: Arr, s: f64) -> Arr {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: Arr, b: Arr) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: Arr) -> f64 {
    dot(a, a).sqrt()
}

/// The third derivative a curve needs at an end so that the arc-length derivative of its
/// curvature vector is `target`, from its first two derivatives `d1`, `d2` there: only its part
/// square to the tangent holds the condition, the rest is a reparametrisation and is left zero.
/// `None` for a tangent of no length or a result that is not finite.
fn third_derivative(d1: Arr, d2: Arr, target: Arr) -> Option<Arr> {
    let sigma = length(d1);
    if !(sigma.is_finite() && sigma > 0.0) {
        return None;
    }
    let t = scale(d1, 1.0 / sigma);
    let tp = sub(
        scale(d2, 1.0 / sigma),
        scale(d1, dot(d1, d2) / (sigma * sigma * sigma)),
    );
    let nn = sub(d2, scale(t, dot(d2, t)));
    let r = sub(scale(tp, -dot(d2, t)), scale(t, dot(d2, tp)));
    let x = scale(
        add(
            sub(
                scale(target, fmath::powi(sigma, 5)),
                scale(r, sigma * sigma),
            ),
            scale(nn, 2.0 * dot(d1, d2)),
        ),
        1.0 / (sigma * sigma),
    );
    if !finite(x) {
        return None;
    }
    Some(sub(x, scale(t, dot(x, t))))
}

/// The derivatives one end holds at every station: `d[k]` the `(k + 1)`-th.
struct EndData {
    d: [Vec<Arr>; 3],
}

fn end_data(
    frames: &[SectionFrame],
    next: &[SectionFrame],
    order: EndOrder,
    crown: f64,
    step: f64,
    last: bool,
    end: usize,
) -> Result<EndData, BlendError> {
    let c = order.conditions();
    let mut d: [Vec<Arr>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (station, (f, n)) in frames.iter().zip(next).enumerate() {
        let leave = arr(f.leave);
        let len = length(leave);
        if c >= 2 && !(len.is_finite() && len > 0.0) {
            return Err(BlendError::Refused(BlendRefusal::NoDirection {
                end,
                station,
            }));
        }
        let unit = if len > 0.0 {
            scale(leave, 1.0 / len)
        } else {
            leave
        };
        let sign = if last { -1.0 } else { 1.0 };
        let chord = length(sub(parr(f.p), parr(n.p)));
        let speed = chord / 3.0 * (1.0 + crown) / step;
        let d1 = scale(unit, sign * speed);
        let d2 = scale(arr(f.bend), speed * speed);
        let d3 = if c >= 4 {
            let target = arr(f.bend_change);
            let got = if last {
                third_derivative(scale(d1, -1.0), d2, target).map(|x| scale(x, -1.0))
            } else {
                third_derivative(d1, d2, target)
            };
            got.ok_or(BlendError::Refused(BlendRefusal::Derivative {
                end,
                station,
            }))?
        } else {
            [0.0; 3]
        };
        if !(finite(d1) && finite(d2) && finite(d3)) {
            return Err(BlendError::Refused(BlendRefusal::Derivative {
                end,
                station,
            }));
        }
        let [a, b, e] = &mut d;
        a.push(d1);
        b.push(d2);
        e.push(d3);
    }
    Ok(EndData { d })
}

/// The control column of the curve through `data` at `t` on `knots`: the derivative control
/// polygon a [`Edge::Held`] takes.
fn polygon(knots: &scx_nurbs::KnotVector, data: &[Arr], t: &[f64]) -> Result<Vec<Arr>, BlendError> {
    fit::interpolate_on(knots, data, t, &End::Point, &End::Point)
        .map(|s| s.points().to_vec())
        .map_err(BlendError::Station)
}

fn edge(
    knots: &scx_nurbs::KnotVector,
    t: &[f64],
    order: EndOrder,
    data: Option<&EndData>,
) -> Result<Edge<3>, BlendError> {
    let Some(data) = data.filter(|_| order.conditions() >= 2) else {
        return Ok(Edge::Point);
    };
    let [d1, d2, d3] = &data.d;
    let p1 = polygon(knots, d1, t)?;
    let p2 = if order.conditions() >= 3 {
        polygon(knots, d2, t)?
    } else {
        Vec::new()
    };
    let p3 = if order.conditions() >= 4 {
        polygon(knots, d3, t)?
    } else {
        Vec::new()
    };
    let held = p1
        .iter()
        .enumerate()
        .map(|(i, &a)| match order {
            EndOrder::G1 => Ok(End::Tangent { d1: a }),
            EndOrder::G2 => {
                p2.get(i)
                    .map(|&b| End::Curvature { d1: a, d2: b })
                    .ok_or(BlendError::Defect {
                        why: "a second derivative column is short",
                    })
            }
            _ => match (p2.get(i), p3.get(i)) {
                (Some(&b), Some(&c)) => Ok(End::Hermite {
                    d1: a,
                    d2: b,
                    d3: c,
                }),
                _ => Err(BlendError::Defect {
                    why: "a derivative column is short",
                }),
            },
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Edge::Held(held))
}

/// The blend through `sections` (each a list of stations), holding `orders` at the first and
/// last section with the crowns `crown` (`-0.9 ..= 3`: the speed leaving each end is the
/// chord to the neighbouring section over 3, times `1 + crown`).
///
/// # Errors
///
/// [`BlendError::Refused`] for input that is not a blend (see the module), the frames of a G0
/// end being ignored; the fits' and the skin's own errors as context frames.
pub fn section_blend(
    sections: &[Vec<SectionFrame>],
    orders: [EndOrder; 2],
    crown: [f64; 2],
) -> Result<Blended, BlendError> {
    let run = sections.len();
    let Some(first) = sections.first().filter(|_| run >= 2) else {
        return Err(BlendError::Refused(BlendRefusal::Sections { count: run }));
    };
    let samples = first.len();
    if samples < MIN_STATIONS {
        return Err(BlendError::Refused(BlendRefusal::Stations {
            count: samples,
        }));
    }
    for (end, &c) in crown.iter().enumerate() {
        if !(c.is_finite() && (-0.9..=3.0).contains(&c)) {
            return Err(BlendError::Refused(BlendRefusal::Crown { end, value: c }));
        }
    }
    for (section, s) in sections.iter().enumerate() {
        if s.len() != samples {
            return Err(BlendError::Refused(BlendRefusal::Uneven {
                section,
                count: s.len(),
                expected: samples,
            }));
        }
        for (station, f) in s.iter().enumerate() {
            let all = [f.p.x, f.p.y, f.p.z]
                .into_iter()
                .chain(arr(f.leave))
                .chain(arr(f.bend))
                .chain(arr(f.bend_change));
            if !all.into_iter().all(f64::is_finite) {
                return Err(BlendError::Refused(BlendRefusal::NotFinite {
                    section,
                    station,
                }));
            }
        }
    }
    for (section, w) in sections.windows(2).enumerate() {
        if let [a, b] = w {
            for (station, (x, y)) in a.iter().zip(b).enumerate() {
                // exact: no room for a speed of zero, and no tangent to divide by next
                if length(sub(parr(x.p), parr(y.p))) <= 0.0 {
                    return Err(BlendError::Refused(BlendRefusal::Meet { section, station }));
                }
            }
        }
    }
    #[allow(clippy::cast_precision_loss)] // counts of sections and stations
    let (step, stations_t, section_t) = {
        let step = 1.0 / (run - 1) as f64;
        let st: Vec<f64> = (0..samples)
            .map(|s| s as f64 / (samples - 1) as f64)
            .collect();
        let su: Vec<f64> = (0..run).map(|i| i as f64 / (run - 1) as f64).collect();
        (step, st, su)
    };
    // every section's curve through its stations, on one knot vector
    let degree_v = 3.min(samples - 1);
    let mut curves: Vec<NurbsCurve<3>> = Vec::with_capacity(run);
    let mut knots = None;
    for s in sections {
        let pts: Vec<Arr> = s.iter().map(|f| parr(f.p)).collect();
        let spline = fit::interpolate(&pts, &stations_t, degree_v, &End::Point, &End::Point)
            .map_err(BlendError::Station)?;
        if knots.is_none() {
            knots = Some(spline.knot_vector().clone());
        }
        curves.push(spline.into_curve().map_err(BlendError::Curves)?);
    }
    let knots = knots.ok_or(BlendError::Defect {
        why: "no section was fitted",
    })?;
    let rows = CompatibleCurves::from_curves(&curves).map_err(BlendError::Curves)?;
    // the ends, from the first and last sections' frames and their neighbours'
    let (Some(second), Some(before), Some(last)) =
        (sections.get(1), sections.get(run - 2), sections.last())
    else {
        return Err(BlendError::Defect {
            why: "a section is missing",
        });
    };
    let start_data = end_data(first, second, orders[0], crown[0], step, false, 0)?;
    let end_data = end_data(last, before, orders[1], crown[1], step, true, 1)?;
    let start = edge(&knots, &stations_t, orders[0], Some(&start_data))?;
    let end = edge(&knots, &stations_t, orders[1], Some(&end_data))?;
    let (a, b) = (orders[0].conditions(), orders[1].conditions());
    let degree = (a + b - 1).max(3).min(run + a + b - 3);
    let skinned =
        skin(&rows, &section_t, degree, &Skin::Open { start, end }).map_err(BlendError::Skin)?;
    Ok(Blended {
        surface: skinned.surface.swapped(),
        through: skinned.through,
        sections: rows,
    })
}

#[cfg(test)]
mod tests;

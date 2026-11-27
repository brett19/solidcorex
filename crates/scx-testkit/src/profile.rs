// Written for solidcorex. Apache-2.0.
//! Profile fixtures: closed, open-by-a-hair, self-touching, crossing, nested and reversed
//! loops of lines and circular arcs, each with the verdict `Region::new` must give (research
//! 72 §4.4, T11).
//!
//! `scx-planar` does not exist yet and a testkit depends on no library type, so a fixture is
//! plain data: points as `[f64; 2]`, a [`Seg`] as a line or an arc, a [`Fixture`] as an outer
//! loop, holes, a resolution and a [`Verdict`]. The planar layer's tests adapt these by a thin
//! function and compare outcomes.
//!
//! Every coordinate is a small integer or a power-of-two fraction and every arc is on a
//! Pythagorean circle, so each fixture's endpoints are exact in `f64`: the closure gaps, the
//! touching and the crossing are exactly as stated, not rounding artefacts. The verdicts are
//! decided by hand from the picture drawn in each fixture's doc comment. [`loop_area`] and
//! [`self_check`] check the data against closed forms (shoelace plus the circular segment).

use std::f64::consts::PI;

use scx_num::fmath;

/// A segment of a loop, from `start` to `end`: the one segment type of the testkit's
/// line-and-arc profiles, shared by these fixtures, by [`crate::solid`]'s extrusions and by
/// [`crate::csg`]'s profile oracle (research 72 §14.10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    /// The straight segment.
    Line {
        /// First end.
        start: [f64; 2],
        /// Second end.
        end: [f64; 2],
    },
    /// The circular arc about `centre`, from `start` to `end`, counter-clockwise when `ccw`
    /// and clockwise otherwise. Both ends lie on the circle. Equal ends are the whole circle
    /// (the solid and CSG modules take it so; these fixtures spell a circle as two arcs).
    Arc {
        /// The circle's centre.
        centre: [f64; 2],
        /// First end.
        start: [f64; 2],
        /// Second end.
        end: [f64; 2],
        /// The sense of travel about the centre.
        ccw: bool,
    },
}

impl Seg {
    /// Where the segment starts.
    pub fn start(&self) -> [f64; 2] {
        match *self {
            Seg::Line { start, .. } | Seg::Arc { start, .. } => start,
        }
    }

    /// Where the segment ends.
    pub fn end(&self) -> [f64; 2] {
        match *self {
            Seg::Line { end, .. } | Seg::Arc { end, .. } => end,
        }
    }
}

/// Why `Region::new` must refuse a fixture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cause {
    /// Loop `loop_index` (0 is the outer loop, `1 + i` is hole `i`) does not close: its last
    /// segment ends `gap` away from its first start, more than the resolution.
    Open {
        /// Which loop.
        loop_index: usize,
        /// The distance left open.
        gap: f64,
    },
    /// Two segments cross at a point interior to both.
    Crossing,
    /// Two segments touch away from their shared ends: a vertex on another segment or a
    /// vertex shared by non-adjacent segments, within a loop or between loops.
    Touching,
    /// Hole `hole` is not inside the outer loop.
    HoleOutside {
        /// Index into the fixture's holes.
        hole: usize,
    },
    /// Hole `inner` lies inside hole `outer`.
    HoleInHole {
        /// The enclosing hole.
        outer: usize,
        /// The enclosed hole.
        inner: usize,
    },
}

/// The answer a fixture expects.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// A region is built. The outer loop is counter-clockwise and the holes clockwise in it;
    /// a loop given the other way round is reversed, not refused (72 §4.3).
    Accepted {
        /// The region's area: the outer loop's, less the holes'. Exact to rounding of the
        /// closed form.
        area: f64,
        /// The outer loop was given clockwise, so it is reversed.
        outer_reversed: bool,
        /// Per hole, given counter-clockwise, so reversed.
        holes_reversed: Vec<bool>,
    },
    /// No region is built, for this cause.
    Refused(Cause),
}

/// A profile and what building a region from it must answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixture {
    /// A name for failure messages.
    pub name: &'static str,
    /// The outer loop, as given.
    pub outer: Vec<Seg>,
    /// The holes, as given.
    pub holes: Vec<Vec<Seg>>,
    /// The resolution: a gap up to this closes a loop.
    pub resolution: f64,
    /// The expected answer.
    pub verdict: Verdict,
}

fn ln(a: [f64; 2], b: [f64; 2]) -> Seg {
    Seg::Line { start: a, end: b }
}

fn arc(c: [f64; 2], a: [f64; 2], b: [f64; 2], ccw: bool) -> Seg {
    Seg::Arc {
        centre: c,
        start: a,
        end: b,
        ccw,
    }
}

/// A closed polygon through `pts`, one line per consecutive pair and back to the first.
fn polygon(pts: &[[f64; 2]]) -> Vec<Seg> {
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(&a, &b)| ln(a, b))
        .collect()
}

/// The square `[x0, x0 + s] x [y0, y0 + s]` counter-clockwise.
fn square(x0: f64, y0: f64, s: f64) -> Vec<Seg> {
    polygon(&[[x0, y0], [x0 + s, y0], [x0 + s, y0 + s], [x0, y0 + s]])
}

/// The circle of radius `r` about `c`, counter-clockwise, as two half arcs from `(c.x + r, c.y)`.
fn circle(c: [f64; 2], r: f64) -> Vec<Seg> {
    let (e, w) = ([c[0] + r, c[1]], [c[0] - r, c[1]]);
    vec![arc(c, e, w, true), arc(c, w, e, true)]
}

/// The loop traversed the other way: segments in reverse order, each reversed.
pub fn reversed(l: &[Seg]) -> Vec<Seg> {
    l.iter()
        .rev()
        .map(|s| match *s {
            Seg::Line { start, end } => ln(end, start),
            Seg::Arc {
                centre,
                start,
                end,
                ccw,
            } => arc(centre, end, start, !ccw),
        })
        .collect()
}

fn accepted(area: f64, outer_reversed: bool, holes_reversed: Vec<bool>) -> Verdict {
    Verdict::Accepted {
        area,
        outer_reversed,
        holes_reversed,
    }
}

/// The signed area a loop encloses: positive counter-clockwise. The shoelace sum of the
/// chords plus, per arc, the signed circular segment between arc and chord, `r^2 (t - sin t) / 2`
/// for the swept angle `t` (positive counter-clockwise), which is closed form and
/// independent of any library.
pub fn loop_area(l: &[Seg]) -> f64 {
    let mut twice = 0.0;
    let mut extra = 0.0;
    for s in l {
        let (a, b) = (s.start(), s.end());
        twice += a[0] * b[1] - b[0] * a[1];
        if let Seg::Arc {
            centre, ccw, start, ..
        } = *s
        {
            let t = swept(s);
            let r = fmath::hypot(start[0] - centre[0], start[1] - centre[1]);
            // `t` is positive in the direction of travel; a clockwise arc's segment lies on
            // the right of its chord, so it subtracts.
            let seg = 0.5 * r * r * (t - fmath::sin(t));
            extra += if ccw { seg } else { -seg };
        }
    }
    0.5 * twice + extra
}

/// An arc's swept angle in `(0, 2 pi]`, positive in the direction of travel.
fn swept(s: &Seg) -> f64 {
    let Seg::Arc {
        centre,
        start,
        end,
        ccw,
    } = *s
    else {
        return 0.0;
    };
    let a0 = fmath::atan2(start[1] - centre[1], start[0] - centre[0]);
    let a1 = fmath::atan2(end[1] - centre[1], end[0] - centre[0]);
    let mut t = if ccw { a1 - a0 } else { a0 - a1 };
    while t <= 0.0 {
        t += 2.0 * PI;
    }
    while t > 2.0 * PI {
        t -= 2.0 * PI;
    }
    t
}

/// The fixtures, in a fixed order.
#[allow(clippy::vec_init_then_push)]
pub fn fixtures() -> Vec<Fixture> {
    let res = 1e-9;
    let mut v = Vec::new();

    // Closed.
    // A square, counter-clockwise: area 16.
    v.push(Fixture {
        name: "closed square",
        outer: square(0.0, 0.0, 4.0),
        holes: vec![],
        resolution: res,
        verdict: accepted(16.0, false, vec![]),
    });
    // A D: the diameter from (-5, 0) to (5, 0), then the upper half circle back. Area 25 pi / 2.
    v.push(Fixture {
        name: "closed D of a line and an arc",
        outer: vec![
            ln([-5.0, 0.0], [5.0, 0.0]),
            arc([0.0, 0.0], [5.0, 0.0], [-5.0, 0.0], true),
        ],
        holes: vec![],
        resolution: res,
        verdict: accepted(12.5 * PI, false, vec![]),
    });
    // A circle of two half arcs. Area 25 pi.
    v.push(Fixture {
        name: "closed circle of two arcs",
        outer: circle([0.0, 0.0], 5.0),
        holes: vec![],
        resolution: res,
        verdict: accepted(25.0 * PI, false, vec![]),
    });
    // A 6 x 4 rectangle whose top edge bulges to (0, 5): a counter-clockwise arc about the
    // origin through the 3-4-5 points (3, 4) and (-3, 4).
    v.push(Fixture {
        name: "closed rounded top",
        outer: vec![
            ln([-3.0, 0.0], [3.0, 0.0]),
            ln([3.0, 0.0], [3.0, 4.0]),
            arc([0.0, 0.0], [3.0, 4.0], [-3.0, 4.0], true),
            ln([-3.0, 4.0], [-3.0, 0.0]),
        ],
        holes: vec![],
        resolution: res,
        // Rectangle 6 x 4 = 24, plus the segment of chord 6 at radius 5: swept angle
        // 2 atan2(3, 4), area (25 / 2) (t - sin t) with sin t = 24 / 25.
        verdict: accepted(
            24.0 + 12.5 * (2.0 * fmath::atan2(3.0, 4.0) - 24.0 / 25.0),
            false,
            vec![],
        ),
    });
    // Closed within the resolution: the last segment stops 2^-40 short of the start.
    let tiny = 1.0 / 1_099_511_627_776.0;
    v.push(Fixture {
        name: "closed within resolution",
        outer: vec![
            ln([0.0, 0.0], [4.0, 0.0]),
            ln([4.0, 0.0], [4.0, 4.0]),
            ln([4.0, 4.0], [0.0, 4.0]),
            ln([0.0, 4.0], [0.0, tiny]),
        ],
        holes: vec![],
        resolution: res,
        verdict: accepted(16.0, false, vec![]),
    });

    // Open by a hair: the last segment stops 2^-20 (about 1e-6, a thousand resolutions) short.
    let hair = 1.0 / 1_048_576.0;
    v.push(Fixture {
        name: "open by a hair",
        outer: vec![
            ln([0.0, 0.0], [4.0, 0.0]),
            ln([4.0, 0.0], [4.0, 4.0]),
            ln([4.0, 4.0], [0.0, 4.0]),
            ln([0.0, 4.0], [0.0, hair]),
        ],
        holes: vec![],
        resolution: res,
        verdict: Verdict::Refused(Cause::Open {
            loop_index: 0,
            gap: hair,
        }),
    });
    // The D whose arc stops a hair above the diameter's first end.
    v.push(Fixture {
        name: "open by a hair at an arc end",
        outer: vec![
            ln([-5.0, 0.0], [5.0, 0.0]),
            arc([0.0, 0.0], [5.0, 0.0], [-5.0, 0.0], true),
            ln([-5.0, 0.0], [-5.0, hair]),
        ],
        holes: vec![],
        resolution: res,
        // The loop ends at (-5, hair), a hair from its start (-5, 0). Closure is judged first.
        verdict: Verdict::Refused(Cause::Open {
            loop_index: 0,
            gap: hair,
        }),
    });
    // A hole open by a hair, inside a good outer loop.
    v.push(Fixture {
        name: "hole open by a hair",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![vec![
            ln([3.0, 3.0], [3.0, 6.0]),
            ln([3.0, 6.0], [6.0, 6.0]),
            ln([6.0, 6.0], [6.0, 3.0]),
            ln([6.0, 3.0], [3.0 + hair, 3.0]),
        ]],
        resolution: res,
        verdict: Verdict::Refused(Cause::Open {
            loop_index: 1,
            gap: hair,
        }),
    });

    // Self-touching.
    // A pentagon whose fourth vertex (3, 0) lies on the first edge's interior.
    v.push(Fixture {
        name: "vertex on an edge",
        outer: polygon(&[[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [3.0, 0.0], [0.0, 4.0]]),
        holes: vec![],
        resolution: res,
        verdict: Verdict::Refused(Cause::Touching),
    });
    // A figure of eight through (2, 0): the vertex is shared by non-adjacent segments.
    v.push(Fixture {
        name: "figure of eight at a vertex",
        outer: polygon(&[
            [2.0, 0.0],
            [0.0, 2.0],
            [0.0, -2.0],
            [2.0, 0.0],
            [4.0, -2.0],
            [4.0, 2.0],
        ]),
        holes: vec![],
        resolution: res,
        verdict: Verdict::Refused(Cause::Touching),
    });
    // A loop whose arc touches a line tangentially: the circle about (0, 5) of radius 5 touches
    // y = 0 at the origin. The loop goes along y = 0 through the origin and the circle's lower
    // half comes down to touch it.
    v.push(Fixture {
        name: "arc tangent to a line",
        outer: vec![
            ln([-5.0, 0.0], [5.0, 0.0]),
            ln([5.0, 0.0], [5.0, 5.0]),
            // The circle about (0, 5), radius 5: from (5, 5) over the bottom, touching (0, 0),
            // clockwise to (-5, 5).
            arc([0.0, 5.0], [5.0, 5.0], [-5.0, 5.0], false),
            ln([-5.0, 5.0], [-5.0, 0.0]),
        ],
        holes: vec![],
        resolution: res,
        verdict: Verdict::Refused(Cause::Touching),
    });
    // A hole's corner touching the outer loop.
    v.push(Fixture {
        name: "hole touches the outer loop",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![reversed(&polygon(&[[0.0, 4.0], [3.0, 3.0], [3.0, 6.0]]))],
        resolution: res,
        verdict: Verdict::Refused(Cause::Touching),
    });

    // Crossing.
    // A bow tie: the diagonals of a square cross at (2, 2).
    v.push(Fixture {
        name: "bow tie",
        outer: polygon(&[[0.0, 0.0], [4.0, 4.0], [4.0, 0.0], [0.0, 4.0]]),
        holes: vec![],
        resolution: res,
        verdict: Verdict::Refused(Cause::Crossing),
    });
    // An arc crossed by a line: the quarter arc about the origin from (5, 0) to (0, 5), then
    // the line from (0, 5) to (6, 2), which meets the circle again at (4, 3), a point interior
    // to the arc and to the line, then the line back to (5, 0).
    v.push(Fixture {
        name: "line crossing an arc",
        outer: vec![
            arc([0.0, 0.0], [5.0, 0.0], [0.0, 5.0], true),
            ln([0.0, 5.0], [6.0, 2.0]),
            ln([6.0, 2.0], [5.0, 0.0]),
        ],
        holes: vec![],
        resolution: res,
        verdict: Verdict::Refused(Cause::Crossing),
    });

    // Nested.
    // A square with a square hole 3 to 6, clockwise as a hole is: area 100 - 9.
    v.push(Fixture {
        name: "square with a square hole",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![reversed(&square(3.0, 3.0, 3.0))],
        resolution: res,
        verdict: accepted(91.0, false, vec![false]),
    });
    // A square with a round hole of radius 2 about (5, 5): area 100 - 4 pi.
    v.push(Fixture {
        name: "square with a round hole",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![reversed(&circle([5.0, 5.0], 2.0))],
        resolution: res,
        verdict: accepted(100.0 - 4.0 * PI, false, vec![false]),
    });
    // Two holes side by side.
    v.push(Fixture {
        name: "two holes",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![
            reversed(&square(1.0, 1.0, 2.0)),
            reversed(&square(5.0, 5.0, 3.0)),
        ],
        resolution: res,
        verdict: accepted(100.0 - 4.0 - 9.0, false, vec![false, false]),
    });
    // A hole wholly outside the outer loop.
    v.push(Fixture {
        name: "hole outside",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![reversed(&square(11.0, 11.0, 2.0))],
        resolution: res,
        verdict: Verdict::Refused(Cause::HoleOutside { hole: 0 }),
    });
    // A hole in a hole: an island inside the first hole is not a region of this kind.
    v.push(Fixture {
        name: "hole in a hole",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![
            reversed(&square(2.0, 2.0, 6.0)),
            reversed(&square(4.0, 4.0, 2.0)),
        ],
        resolution: res,
        verdict: Verdict::Refused(Cause::HoleInHole { outer: 0, inner: 1 }),
    });

    // Reversed.
    // A square given clockwise.
    v.push(Fixture {
        name: "square given clockwise",
        outer: reversed(&square(0.0, 0.0, 4.0)),
        holes: vec![],
        resolution: res,
        verdict: accepted(16.0, true, vec![]),
    });
    // A D given clockwise.
    v.push(Fixture {
        name: "D given clockwise",
        outer: reversed(&[
            ln([-5.0, 0.0], [5.0, 0.0]),
            arc([0.0, 0.0], [5.0, 0.0], [-5.0, 0.0], true),
        ]),
        holes: vec![],
        resolution: res,
        verdict: accepted(12.5 * PI, true, vec![]),
    });
    // A hole given counter-clockwise, the outer loop right.
    v.push(Fixture {
        name: "hole given counter-clockwise",
        outer: square(0.0, 0.0, 10.0),
        holes: vec![square(3.0, 3.0, 3.0)],
        resolution: res,
        verdict: accepted(91.0, false, vec![true]),
    });
    // Everything the wrong way round.
    v.push(Fixture {
        name: "outer clockwise and hole counter-clockwise",
        outer: reversed(&square(0.0, 0.0, 10.0)),
        holes: vec![circle([5.0, 5.0], 2.0)],
        resolution: res,
        verdict: accepted(100.0 - 4.0 * PI, true, vec![true]),
    });
    v
}

/// What is wrong with a fixture's own data, found without any library: a loop whose segments
/// do not chain within a quarter of the gap it declares, an arc whose ends are not on its
/// circle, an accepted fixture whose stated area or orientation flags disagree with
/// [`loop_area`]. Empty for every fixture of [`fixtures`].
pub fn self_check(f: &Fixture) -> Vec<String> {
    let mut bad = Vec::new();
    let loops: Vec<&Vec<Seg>> = std::iter::once(&f.outer).chain(&f.holes).collect();
    for (k, l) in loops.iter().enumerate() {
        for s in *l {
            if let Seg::Arc {
                centre, start, end, ..
            } = *s
            {
                let r0 = fmath::hypot(start[0] - centre[0], start[1] - centre[1]);
                let r1 = fmath::hypot(end[0] - centre[0], end[1] - centre[1]);
                if (r0 - r1).abs() > 1e-12 * r0 {
                    bad.push(format!("loop {k}: an arc's ends are not on its circle"));
                }
            }
        }
        // The closing gap is the last end to the first start; the others chain exactly.
        let chained = l
            .iter()
            .zip(l.iter().skip(1))
            .all(|(a, b)| a.end() == b.start());
        if !chained {
            bad.push(format!("loop {k}: segments do not chain"));
        }
        let gap = match (l.first(), l.last()) {
            (Some(first), Some(last)) => {
                let (a, b) = (first.start(), last.end());
                fmath::hypot(a[0] - b[0], a[1] - b[1])
            }
            _ => f64::INFINITY,
        };
        let declared = match f.verdict {
            Verdict::Refused(Cause::Open { loop_index, gap }) if loop_index == k => gap,
            _ => 0.0,
        };
        let closed_in_accepted = matches!(f.verdict, Verdict::Accepted { .. });
        let within = gap <= f.resolution;
        if closed_in_accepted && !within {
            bad.push(format!("loop {k}: accepted but open by {gap}"));
        }
        if declared > 0.0 && (gap != declared || within) {
            bad.push(format!(
                "loop {k}: declared gap {declared} but measured {gap}"
            ));
        }
        if matches!(f.verdict, Verdict::Refused(c) if !matches!(c, Cause::Open{..})) && !within {
            bad.push(format!("loop {k}: open, but the cause is not Open"));
        }
    }
    if let Verdict::Accepted {
        area,
        outer_reversed,
        ref holes_reversed,
    } = f.verdict
    {
        let outer = loop_area(&f.outer);
        if (outer < 0.0) != outer_reversed {
            bad.push("outer orientation flag disagrees with the signed area".into());
        }
        if holes_reversed.len() != f.holes.len() {
            bad.push("one flag per hole".into());
        }
        let mut net = outer.abs();
        for (h, &flag) in f.holes.iter().zip(holes_reversed) {
            let a = loop_area(h);
            if (a > 0.0) != flag {
                bad.push("a hole's orientation flag disagrees with the signed area".into());
            }
            net -= a.abs();
        }
        if (net - area).abs() > 1e-12 * area.max(1.0) {
            bad.push(format!("area {area} but the closed form gives {net}"));
        }
    }
    bad
}

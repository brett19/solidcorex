// Written for solidcorex.
//! `read(write(g)) == g` bit for bit, for Part 42 geometry in millimetres and radians: every
//! curve and surface kind by hand, pcurves of each 2D kind, rational and implied-knot splines,
//! and generated geometry; and the writer's refusals of values that have no Part 21 form.
//!
//! Comparison is by `Debug`, which prints each real in its shortest round-trip form (and keeps
//! `-0.`), so equal text is equal bits.

#![allow(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_p21::error::{NoPart42Form, Refusal, Unwritable};
use scx_p21::part42::geom::*;
use scx_p21::part42::read::Reader;
use scx_p21::part42::write;
use scx_p21::units::Units;
use scx_p21::{parse, Exchange, Header, P21Error, Writer};

const MM: Units = Units { len: 1.0, ang: 1.0 };

/// What `w` holds, written as text and parsed back.
fn reread(w: Writer) -> Exchange {
    let ex = Exchange::new(Header::default(), w.finish(None));
    let text = scx_p21::write(&ex).unwrap();
    let back = parse(&text).unwrap_or_else(|e| panic!("{}\n{text}", e.one_line()));
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    back.exchange
}

fn same<T: std::fmt::Debug>(back: &T, g: &T) {
    assert_eq!(format!("{back:?}"), format!("{g:?}"));
}

fn curve_round_trip(c: &Curve) {
    let mut w = Writer::new();
    let id = write::curve(&mut w, c).unwrap_or_else(|e| panic!("{}\n{c:?}", e.one_line()));
    let ex = reread(w);
    let back = Reader::new(&ex, MM)
        .curve(id)
        .unwrap_or_else(|e| panic!("{}\n{c:?}", e.one_line()));
    same(&*back, c);
}

fn surface_round_trip(s: &Surface) {
    let mut w = Writer::new();
    let id = write::surface(&mut w, s).unwrap_or_else(|e| panic!("{}\n{s:?}", e.one_line()));
    let ex = reread(w);
    let back = Reader::new(&ex, MM)
        .surface(id)
        .unwrap_or_else(|e| panic!("{}\n{s:?}", e.one_line()));
    same(&*back, s);
}

fn pcurve_round_trip(pc: &Pcurve) {
    let mut w = Writer::new();
    let id = write::pcurve(&mut w, pc).unwrap_or_else(|e| panic!("{}\n{pc:?}", e.one_line()));
    let ex = reread(w);
    let back = Reader::new(&ex, MM)
        .pcurve(id)
        .unwrap_or_else(|e| panic!("{}\n{pc:?}", e.one_line()));
    same(&back, pc);
}

fn at(origin: P3) -> Placement {
    Placement {
        origin,
        axis: Some([0.0, 0.0, 1.0]),
        ref_direction: Some([1.0, 0.0, 0.0]),
    }
}

fn at_2d(origin: P3) -> Placement {
    Placement {
        origin,
        axis: None,
        ref_direction: Some([0.6, -0.8, 0.0]),
    }
}

fn vector(orientation: P3, magnitude: f64) -> Vector {
    Vector {
        orientation,
        magnitude,
        scale: [1.0; 3],
    }
}

fn spline(points: Vec<P3>, weights: Option<Vec<f64>>) -> BSplineCurve {
    let n = points.len();
    BSplineCurve {
        degree: 2,
        points,
        weights,
        knots: (0..n - 1).map(|i| i as f64 * 0.5).collect(),
        multiplicities: {
            let mut m = vec![1; n - 1];
            m[0] = 3;
            m[n - 2] = 3;
            m
        },
        form: CurveForm::Unspecified,
        closed: Logical::False,
        self_intersect: Logical::Unknown,
        knot_spec: KnotSpec::Unspecified,
    }
}

fn plane() -> Arc<Surface> {
    Arc::new(Surface::Plane {
        position: at([1.0, 2.0, 3.0]),
    })
}

fn line() -> Arc<Curve> {
    Arc::new(Curve::Line {
        origin: [0.1, 0.2, 0.3],
        vector: vector([0.0, 1.0, 0.0], 2.5),
    })
}

#[test]
fn every_curve_kind() {
    let o = [1.5, -2.25, 1e-300];
    let mut cs = vec![
        (*line()).clone(),
        Curve::Circle {
            position: at(o),
            radius: 0.1,
        },
        Curve::Circle {
            // An omitted axis and reference direction stay omitted.
            position: Placement {
                origin: o,
                axis: None,
                ref_direction: None,
            },
            radius: 7.0,
        },
        Curve::Ellipse {
            position: at(o),
            semi_axis_1: 3.0,
            semi_axis_2: 1.0 / 3.0,
        },
        Curve::Hyperbola {
            position: at(o),
            semi_axis: 2.0,
            semi_imag_axis: 5e-7,
        },
        Curve::Parabola {
            position: at(o),
            focal_dist: -0.5,
        },
        Curve::Polyline(vec![[0.0, 0.0, 0.0], [-0.0, 1.0, 2.0], [3.0, 4.0, 5.0]]),
        Curve::BSpline(Box::new(spline(
            vec![[0.0; 3], [1.0, 1.0, 0.0], [2.0, 0.0, 0.0], [3.0, 1.0, 0.0]],
            None,
        ))),
        Curve::BSpline(Box::new(spline(
            vec![[0.0; 3], [1.0, 1.0, 0.0], [2.0, 0.0, 0.0], [3.0, 1.0, 0.0]],
            Some(vec![1.0, FRAC, 0.25, 1.0]),
        ))),
        Curve::Trimmed {
            basis: Arc::new(Curve::Circle {
                position: at(o),
                radius: 2.0,
            }),
            trim_1: vec![Trim::Parameter(0.5), Trim::Point([1.0, 2.0, 3.0])],
            trim_2: vec![Trim::Parameter(-1.25)],
            sense: false,
            preference: TrimPreference::Parameter,
        },
        Curve::OffsetCurve3d {
            basis: line(),
            distance: -0.75,
            self_intersect: Logical::False,
            ref_direction: [0.0, 0.0, 2.0],
        },
    ];
    for kind in [
        SurfaceCurveKind::Surface,
        SurfaceCurveKind::Seam,
        SurfaceCurveKind::Intersection,
        SurfaceCurveKind::BoundedSurface,
    ] {
        cs.push(Curve::SurfaceCurve {
            kind,
            curve_3d: line(),
            associated: vec![
                Associated::Pcurve(Pcurve {
                    surface: plane(),
                    curve: Arc::new(Curve::Line {
                        origin: [0.5, 0.25, 0.0],
                        vector: vector([1.0, 0.0, 0.0], 1.0),
                    }),
                }),
                Associated::Surface(Arc::new(Surface::Cylinder {
                    position: at(o),
                    radius: 4.0,
                })),
            ],
            master: PreferredRep::PcurveS1,
        });
    }
    for c in &cs {
        curve_round_trip(c);
    }
}

const FRAC: f64 = 0.1 + 0.2;

#[test]
fn every_surface_kind() {
    let o = [-1.0, 0.0, 9.5];
    let net = vec![
        vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[1.0, 0.0, 1.0], [1.0, 1.0, 1.0]],
        vec![[2.0, 0.0, 0.0], [2.0, 1.0, 0.0]],
    ];
    let bs = |weights| BSplineSurface {
        u_degree: 2,
        v_degree: 1,
        points: net.clone(),
        weights,
        u_knots: vec![0.0, 1.0],
        u_multiplicities: vec![3, 3],
        v_knots: vec![0.0, 2.0],
        v_multiplicities: vec![2, 2],
        form: SurfaceForm::QuadricSurf,
        u_closed: Logical::False,
        v_closed: Logical::Unknown,
        self_intersect: Logical::True,
        knot_spec: KnotSpec::PiecewiseBezierKnots,
    };
    let ss = [
        Surface::Plane { position: at(o) },
        Surface::Cylinder {
            position: at(o),
            radius: 3.0,
        },
        Surface::Cone {
            position: at(o),
            radius: 0.0,
            semi_angle: 0.5,
        },
        Surface::Sphere {
            position: at(o),
            radius: 12.0,
        },
        Surface::Torus {
            position: at(o),
            major_radius: 10.0,
            minor_radius: 2.0,
        },
        Surface::DegenerateTorus {
            position: at(o),
            major_radius: 1.0,
            minor_radius: 2.0,
            select_outer: true,
        },
        Surface::BSpline(Box::new(bs(None))),
        Surface::BSpline(Box::new(bs(Some(vec![
            vec![1.0, 2.0],
            vec![FRAC, 1.0],
            vec![1.0, 1.0],
        ])))),
        Surface::Revolution {
            profile: line(),
            axis: Axis1 {
                origin: o,
                direction: Some([0.0, 0.0, 1.0]),
            },
        },
        Surface::Revolution {
            profile: line(),
            axis: Axis1 {
                origin: o,
                direction: None,
            },
        },
        Surface::Extrusion {
            profile: line(),
            vector: vector([1.0, 1.0, 0.0], 4.0),
        },
        Surface::Offset {
            basis: plane(),
            distance: -2.0,
            self_intersect: Logical::Unknown,
        },
        Surface::RectangularTrimmed {
            basis: plane(),
            u1: -1.0,
            u2: 1.0,
            v1: 0.0,
            v2: 2.5,
            u_sense: true,
            v_sense: false,
        },
    ];
    for s in &ss {
        surface_round_trip(s);
    }
}

#[test]
fn every_2d_curve_kind_as_a_pcurve() {
    let o = [0.25, -0.5, 0.0];
    let basis = Arc::new(Curve::Ellipse {
        position: at_2d(o),
        semi_axis_1: 2.0,
        semi_axis_2: 1.0,
    });
    let cs = [
        Curve::Line {
            origin: o,
            vector: vector([3.0, 4.0, 0.0], 0.5),
        },
        Curve::Circle {
            position: at_2d(o),
            radius: 1.0,
        },
        Curve::Circle {
            position: Placement {
                origin: o,
                axis: None,
                ref_direction: None,
            },
            radius: 1.0,
        },
        (*basis).clone(),
        Curve::Hyperbola {
            position: at_2d(o),
            semi_axis: 1.0,
            semi_imag_axis: 2.0,
        },
        Curve::Parabola {
            position: at_2d(o),
            focal_dist: 0.125,
        },
        Curve::Polyline(vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]),
        Curve::BSpline(Box::new(spline(
            vec![[0.0; 3], [1.0, 1.0, 0.0], [2.0, 0.0, 0.0]],
            Some(vec![1.0, 0.5, 1.0]),
        ))),
        Curve::Trimmed {
            basis,
            trim_1: vec![Trim::Point([2.25, -0.5, 0.0]), Trim::Parameter(0.0)],
            trim_2: vec![Trim::Parameter(1.5)],
            sense: true,
            preference: TrimPreference::Cartesian,
        },
    ];
    let surfaces = [
        plane(),
        Arc::new(Surface::Cylinder {
            position: at([0.0; 3]),
            radius: 2.0,
        }),
        Arc::new(Surface::Sphere {
            position: at([0.0; 3]),
            radius: 2.0,
        }),
    ];
    for s in &surfaces {
        for c in &cs {
            pcurve_round_trip(&Pcurve {
                surface: s.clone(),
                curve: Arc::new(c.clone()),
            });
        }
    }
}

#[test]
fn implied_knots_read_back_as_their_explicit_form() {
    // A uniform spline written with its knots spelt out (as the reader gives them) reads back
    // the same, its knot_spec kept.
    curve_round_trip(&Curve::BSpline(Box::new(BSplineCurve {
        degree: 2,
        points: vec![[0.0; 3], [1.0, 0.0, 0.0], [2.0, 1.0, 0.0], [3.0, 0.0, 0.0]],
        weights: None,
        knots: vec![-2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0],
        multiplicities: vec![1; 7],
        form: CurveForm::Unspecified,
        closed: Logical::False,
        self_intersect: Logical::False,
        knot_spec: KnotSpec::UniformKnots,
    })));
    // And a spline read from a UNIFORM_CURVE writes and reads back as itself.
    let text = "ISO-10303-21;HEADER;ENDSEC;DATA;\
        #1=CARTESIAN_POINT('',(0.,0.,0.));#2=CARTESIAN_POINT('',(1.,0.,0.));\
        #3=CARTESIAN_POINT('',(2.,1.,0.));\
        #10=QUASI_UNIFORM_CURVE('',2,(#1,#2,#3),.UNSPECIFIED.,.F.,.F.);ENDSEC;END-ISO-10303-21;";
    let ex = parse(text).unwrap().exchange;
    let c = Reader::new(&ex, MM).curve(10).unwrap();
    curve_round_trip(&c);
}

/// A xorshift64 stream: deterministic, the same on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A real with all 53 bits of mantissa in play, of size below 2^10, either sign; now and
    /// then an exact small value or a signed zero.
    fn real(&mut self) -> f64 {
        match self.below(8) {
            0 => [0.0, -0.0, 1.0, -1.0][self.below(4) as usize],
            _ => {
                let m = (self.next() >> 11) as f64; // [0, 2^53)
                let x = m * f64::from_bits((1023 - 43 - self.below(40)) << 52);
                if self.below(2) == 0 {
                    x
                } else {
                    -x
                }
            }
        }
    }

    fn positive(&mut self) -> f64 {
        loop {
            let x = self.real().abs();
            if x > 0.0 {
                return x;
            }
        }
    }

    fn point(&mut self) -> P3 {
        [self.real(), self.real(), self.real()]
    }

    fn point_2d(&mut self) -> P3 {
        [self.real(), self.real(), 0.0]
    }

    fn direction(&mut self, p: fn(&mut Rng) -> P3) -> P3 {
        loop {
            let d = p(self);
            if d.iter().any(|x| *x != 0.0) {
                return d;
            }
        }
    }

    fn logical(&mut self) -> Logical {
        [Logical::True, Logical::False, Logical::Unknown][self.below(3) as usize]
    }

    fn placement(&mut self, two_d: bool) -> Placement {
        let p: fn(&mut Rng) -> P3 = if two_d { Rng::point_2d } else { Rng::point };
        Placement {
            origin: p(self),
            axis: (!two_d && self.below(4) != 0).then(|| self.direction(p)),
            ref_direction: (self.below(4) != 0).then(|| self.direction(p)),
        }
    }

    /// A clamped or unclamped knot vector for `n` points of degree `d`: distinct knots, the
    /// end multiplicities `d + 1` or 1 and the interior ones from 1 to `d`.
    fn knots(&mut self, n: usize, d: usize) -> (Vec<f64>, Vec<usize>) {
        let end = if self.below(2) == 0 { d + 1 } else { 1 };
        let mut interior = n + d + 1 - 2 * end;
        let mut mults = vec![end];
        while interior > 0 {
            let m = (1 + self.below(d as u64) as usize).min(interior);
            mults.push(m);
            interior -= m;
        }
        mults.push(end);
        let mut t = self.real();
        let knots = mults
            .iter()
            .map(|_| {
                let k = t;
                t += 1.0 + self.positive();
                k
            })
            .collect();
        (knots, mults)
    }

    fn bspline(&mut self, two_d: bool) -> BSplineCurve {
        let d = 1 + self.below(4) as usize;
        let n = d + 1 + self.below(6) as usize;
        let p: fn(&mut Rng) -> P3 = if two_d { Rng::point_2d } else { Rng::point };
        let points = (0..n).map(|_| p(self)).collect();
        let weights = (self.below(2) == 0).then(|| (0..n).map(|_| self.positive()).collect());
        let (knots, multiplicities) = self.knots(n, d);
        BSplineCurve {
            degree: d,
            points,
            weights,
            knots,
            multiplicities,
            form: CurveForm::Unspecified,
            closed: self.logical(),
            self_intersect: self.logical(),
            knot_spec: KnotSpec::Unspecified,
        }
    }

    fn curve(&mut self, two_d: bool, depth: u32) -> Curve {
        let p: fn(&mut Rng) -> P3 = if two_d { Rng::point_2d } else { Rng::point };
        let top = if depth == 0 || two_d { 8 } else { 10 };
        match self.below(top) {
            0 => Curve::Line {
                origin: p(self),
                vector: vector(self.direction(p), self.positive()),
            },
            1 => Curve::Circle {
                position: self.placement(two_d),
                radius: self.positive(),
            },
            2 => Curve::Ellipse {
                position: self.placement(two_d),
                semi_axis_1: self.positive(),
                semi_axis_2: self.positive(),
            },
            3 => Curve::Hyperbola {
                position: self.placement(two_d),
                semi_axis: self.positive(),
                semi_imag_axis: self.positive(),
            },
            4 => Curve::Parabola {
                position: self.placement(two_d),
                focal_dist: self.positive(),
            },
            5 => Curve::Polyline((0..2 + self.below(5)).map(|_| p(self)).collect()),
            6 => Curve::BSpline(Box::new(self.bspline(two_d))),
            7 => {
                if depth == 0 {
                    return self.curve(two_d, depth);
                }
                let mut trims = || {
                    let mut t = vec![Trim::Parameter(self.real())];
                    if self.below(2) == 0 {
                        t.insert(self.below(2) as usize, Trim::Point(p(self)));
                    }
                    t
                };
                let (trim_1, trim_2) = (trims(), trims());
                Curve::Trimmed {
                    basis: Arc::new(self.curve(two_d, depth - 1)),
                    trim_1,
                    trim_2,
                    sense: self.below(2) == 0,
                    preference: [
                        TrimPreference::Cartesian,
                        TrimPreference::Parameter,
                        TrimPreference::Unspecified,
                    ][self.below(3) as usize],
                }
            }
            8 => Curve::OffsetCurve3d {
                basis: Arc::new(self.curve(false, depth - 1)),
                distance: self.real(),
                self_intersect: self.logical(),
                ref_direction: self.direction(Rng::point),
            },
            _ => {
                let associated = (0..1 + self.below(2))
                    .map(|_| {
                        if self.below(2) == 0 {
                            Associated::Surface(Arc::new(self.surface(0)))
                        } else {
                            Associated::Pcurve(self.pcurve())
                        }
                    })
                    .collect();
                Curve::SurfaceCurve {
                    kind: [
                        SurfaceCurveKind::Surface,
                        SurfaceCurveKind::Seam,
                        SurfaceCurveKind::Intersection,
                        SurfaceCurveKind::BoundedSurface,
                    ][self.below(4) as usize],
                    curve_3d: Arc::new(self.curve(false, depth - 1)),
                    associated,
                    master: [
                        PreferredRep::Curve3d,
                        PreferredRep::PcurveS1,
                        PreferredRep::PcurveS2,
                    ][self.below(3) as usize],
                }
            }
        }
    }

    fn pcurve(&mut self) -> Pcurve {
        Pcurve {
            surface: Arc::new(self.surface(0)),
            curve: Arc::new(self.curve(true, 1)),
        }
    }

    fn surface(&mut self, depth: u32) -> Surface {
        let top = if depth == 0 { 7 } else { 12 };
        match self.below(top) {
            0 => Surface::Plane {
                position: self.placement(false),
            },
            1 => Surface::Cylinder {
                position: self.placement(false),
                radius: self.positive(),
            },
            2 => Surface::Cone {
                position: self.placement(false),
                radius: self.positive(),
                // Inside the reader's (1e-12, π/2 - 1e-12).
                semi_angle: 1e-6 + self.positive() / 1024.0 * 1.5,
            },
            3 => Surface::Sphere {
                position: self.placement(false),
                radius: self.positive(),
            },
            4 => Surface::Torus {
                position: self.placement(false),
                major_radius: self.real(),
                minor_radius: self.positive(),
            },
            5 => Surface::DegenerateTorus {
                position: self.placement(false),
                major_radius: self.real(),
                minor_radius: self.positive(),
                select_outer: self.below(2) == 0,
            },
            6 => {
                let (du, dv) = (1 + self.below(3) as usize, 1 + self.below(3) as usize);
                let (nu, nv) = (
                    du + 1 + self.below(3) as usize,
                    dv + 1 + self.below(3) as usize,
                );
                let points = (0..nu)
                    .map(|_| (0..nv).map(|_| self.point()).collect())
                    .collect();
                let weights = (self.below(2) == 0).then(|| {
                    (0..nu)
                        .map(|_| (0..nv).map(|_| self.positive()).collect())
                        .collect()
                });
                let (u_knots, u_multiplicities) = self.knots(nu, du);
                let (v_knots, v_multiplicities) = self.knots(nv, dv);
                Surface::BSpline(Box::new(BSplineSurface {
                    u_degree: du,
                    v_degree: dv,
                    points,
                    weights,
                    u_knots,
                    u_multiplicities,
                    v_knots,
                    v_multiplicities,
                    form: SurfaceForm::Unspecified,
                    u_closed: self.logical(),
                    v_closed: self.logical(),
                    self_intersect: self.logical(),
                    knot_spec: KnotSpec::Unspecified,
                }))
            }
            7 => Surface::Revolution {
                profile: Arc::new(self.curve(false, 1)),
                axis: Axis1 {
                    origin: self.point(),
                    direction: (self.below(4) != 0).then(|| self.direction(Rng::point)),
                },
            },
            8 => Surface::Extrusion {
                profile: Arc::new(self.curve(false, 1)),
                vector: vector(self.direction(Rng::point), self.positive()),
            },
            9 => Surface::Offset {
                basis: Arc::new(self.surface(depth - 1)),
                distance: self.real(),
                self_intersect: self.logical(),
            },
            _ => Surface::RectangularTrimmed {
                basis: Arc::new(self.surface(depth - 1)),
                u1: self.real(),
                u2: self.real(),
                v1: self.real(),
                v2: self.real(),
                u_sense: self.below(2) == 0,
                v_sense: self.below(2) == 0,
            },
        }
    }
}

#[test]
fn generated_geometry() {
    let mut g = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..400 {
        let c = g.curve(false, 3);
        curve_round_trip(&c);
        let s = g.surface(2);
        surface_round_trip(&s);
        let pc = g.pcurve();
        pcurve_round_trip(&pc);
    }
}

/// `r`, which must refuse geometry with no ISO 10303-42 form, for `cause`; the refusal is
/// transparent, so its text is the cause's and it has no further source.
fn refused(r: Result<u64, P21Error>, cause: NoPart42Form) {
    let e = r.unwrap_err();
    assert_eq!(
        e,
        P21Error::Refused(Refusal::Unwritable {
            id: None,
            cause: Unwritable::Geometry(cause),
        })
    );
    // Transparent all the way (research 62 D12): the error's one frame is the cause's.
    assert_eq!(e.to_string(), cause.to_string());
    assert!(std::error::Error::source(&e).is_none());
}

#[test]
fn the_writer_refuses_what_has_no_part_21_form() {
    let pc = |curve| Pcurve {
        surface: plane(),
        curve: Arc::new(curve),
    };
    let mut w = Writer::new();
    let r = write::pcurve(
        &mut w,
        &pc(Curve::Polyline(vec![[0.0; 3], [1.0, 1.0, 1.0]])),
    );
    refused(r, NoPart42Form::OffParameterPlane);

    let mut w = Writer::new();
    let r = write::pcurve(
        &mut w,
        &pc(Curve::Circle {
            position: at([0.0; 3]),
            radius: 1.0,
        }),
    );
    refused(r, NoPart42Form::AxisInParameterSpace);

    let mut w = Writer::new();
    let r = write::pcurve(
        &mut w,
        &pc(Curve::OffsetCurve3d {
            basis: line(),
            distance: 1.0,
            self_intersect: Logical::False,
            ref_direction: [0.0, 0.0, 1.0],
        }),
    );
    refused(r, NoPart42Form::SpaceCurveInParameterSpace);

    // A vector holds one magnitude, so its components must share a unit factor: in a model
    // space all three, in a parameter space the two it has.
    for (scale, ok_in_2d) in [([1.0, 1.0, 2.0], true), ([1.0, 2.0, 1.0], false)] {
        let v = Curve::Line {
            origin: [0.0; 3],
            vector: Vector {
                orientation: [1.0, 0.0, 0.0],
                magnitude: 1.0,
                scale,
            },
        };
        let mut w = Writer::new();
        let r = write::curve(&mut w, &v);
        refused(r, NoPart42Form::MixedUnitVector);
        let mut w = Writer::new();
        let r = write::pcurve(&mut w, &pc(v));
        if ok_in_2d {
            assert!(r.is_ok());
        } else {
            refused(r, NoPart42Form::MixedUnitVector);
        }
    }

    // A 3D point is fine where it belongs.
    assert!(write::point(&mut Writer::new(), [1.0, 1.0, 1.0]).is_ok());
}

// Written for solidcorex (docs/research/74-step6-idiom-prep.md §29).
//! The analytic build (SP2) on small files written here entity by entity: boxes in each
//! style, a cylinder with its seam, a ball and a cone through their poles, a plate with a
//! through hole, a void, sheets, conic edges, units, the gaps the build raises, the kinds it
//! leaves pending and the files it refuses. Ported from the pin's topology and assembly
//! tests; the truths are closed forms (a box's volume, a cone's third of its cylinder, a
//! parabolic segment's area), never the pin's output.

#![allow(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use std::f64::consts::PI;
use std::fmt::Write as _;

use scx_brep::Document;
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_check::Class;
use scx_mass::{RelAccuracy, area, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_p21::{Exchange, parse};
use scx_step::{
    BuildError, Built, GapKind, Note, ReadOptions, SolidOrSheet, Waits, build, import, read,
};

type V = [f64; 3];

fn real(x: f64) -> String {
    format!("{x:.17E}")
}

fn sub3(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn neg(a: V) -> V {
    [-a[0], -a[1], -a[2]]
}

fn b(x: bool) -> &'static str {
    if x { ".T." } else { ".F." }
}

fn list(ids: &[u64]) -> String {
    ids.iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Writes a STEP data section one entity at a time.
#[derive(Default)]
struct W {
    s: String,
    n: u64,
}

impl W {
    fn add(&mut self, body: impl AsRef<str>) -> u64 {
        self.n += 1;
        let _ = writeln!(self.s, "#{}={};", self.n, body.as_ref());
        self.n
    }
    fn pt(&mut self, p: V) -> u64 {
        self.add(format!(
            "CARTESIAN_POINT('',({},{},{}))",
            real(p[0]),
            real(p[1]),
            real(p[2])
        ))
    }
    fn dir(&mut self, d: V) -> u64 {
        self.add(format!(
            "DIRECTION('',({},{},{}))",
            real(d[0]),
            real(d[1]),
            real(d[2])
        ))
    }
    fn axis(&mut self, o: V, z: V, x: V) -> u64 {
        let (o, z, x) = (self.pt(o), self.dir(z), self.dir(x));
        self.add(format!("AXIS2_PLACEMENT_3D('',#{o},#{z},#{x})"))
    }
    fn vertex(&mut self, p: V) -> u64 {
        let p = self.pt(p);
        self.add(format!("VERTEX_POINT('',#{p})"))
    }
    /// A line edge from `a` to `b`; with `same_sense` false its LINE runs from b to a.
    fn line_edge(&mut self, (va, pa): (u64, V), (vb, pb): (u64, V), same_sense: bool) -> u64 {
        let (o, d) = if same_sense {
            (pa, sub3(pb, pa))
        } else {
            (pb, sub3(pa, pb))
        };
        let (o, d) = (self.pt(o), self.dir(d));
        let v = self.add(format!("VECTOR('',#{d},1.)"));
        let l = self.add(format!("LINE('',#{o},#{v})"));
        self.add(format!("EDGE_CURVE('',#{va},#{vb},#{l},{})", b(same_sense)))
    }
    fn circle(&mut self, c: V, z: V, x: V, r: f64) -> u64 {
        let a = self.axis(c, z, x);
        self.add(format!("CIRCLE('',#{a},{})", real(r)))
    }
    fn oe(&mut self, edge: u64, orientation: bool) -> u64 {
        self.add(format!("ORIENTED_EDGE('',*,*,#{edge},{})", b(orientation)))
    }
    fn bound(&mut self, oes: &[u64], outer: bool, orientation: bool) -> u64 {
        let l = self.add(format!("EDGE_LOOP('',({}))", list(oes)));
        let kind = if outer {
            "FACE_OUTER_BOUND"
        } else {
            "FACE_BOUND"
        };
        self.add(format!("{kind}('',#{l},{})", b(orientation)))
    }
    fn plane(&mut self, o: V, z: V, x: V) -> u64 {
        let a = self.axis(o, z, x);
        self.add(format!("PLANE('',#{a})"))
    }
    fn face(&mut self, bounds: &[u64], surface: u64, same_sense: bool) -> u64 {
        self.add(format!(
            "ADVANCED_FACE('',({}),#{surface},{})",
            list(bounds),
            b(same_sense)
        ))
    }
    /// The file: millimetres, radians, an uncertainty of 1e-7 mm, every item in one
    /// representation.
    fn file(self, items: &[u64]) -> Exchange {
        self.file_in(items, 1e-7, false)
    }

    /// [`W::file`] with the file's uncertainty (in the file's unit) and its length unit the
    /// inch when `inch`.
    fn file_in(mut self, items: &[u64], uncertainty: f64, inch: bool) -> Exchange {
        self.rep(items, uncertainty, inch);
        exchange(&self.s)
    }

    /// [`W::file`] with the representation the shape of one part.
    fn part(mut self, items: &[u64]) -> Exchange {
        let rep = self.rep(items, 1e-7, false);
        let app = self.add("APPLICATION_CONTEXT('')");
        let ctx = self.add(format!(
            "PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')"
        ));
        let product = self.add("PRODUCT('p','p','',())");
        let formation = self.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{product})"));
        let pd = self.add(format!(
            "PRODUCT_DEFINITION('design','',#{formation},#{ctx})"
        ));
        let pds = self.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));
        self.add(format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));
        exchange(&self.s)
    }

    /// The shape representation of `items`, with its context; its id.
    fn rep(&mut self, items: &[u64], uncertainty: f64, inch: bool) -> u64 {
        let mm = self.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))");
        let l = if inch {
            let m = self.add(format!(
                "LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#{mm})"
            ));
            let d = self.add("DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.)");
            self.add(format!(
                "(CONVERSION_BASED_UNIT('INCH',#{m})LENGTH_UNIT()NAMED_UNIT(#{d}))"
            ))
        } else {
            mm
        };
        let a = self.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))");
        let u = self.add(format!(
            "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({}),#{l},'distance_accuracy_value','')",
            real(uncertainty)
        ));
        let c = self.add(format!(
            "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{u}))\
             GLOBAL_UNIT_ASSIGNED_CONTEXT((#{l},#{a}))REPRESENTATION_CONTEXT('',''))"
        ));
        self.add(format!(
            "ADVANCED_BREP_SHAPE_REPRESENTATION('',({}),#{c})",
            list(items)
        ))
    }
}

fn exchange(data: &str) -> Exchange {
    let t = format!(
        "ISO-10303-21;\nHEADER;FILE_DESCRIPTION(('t'),'2;1');FILE_NAME('t','',(''),(''),'','','');\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;\nDATA;\n{data}ENDSEC;\nEND-ISO-10303-21;\n"
    );
    match parse(&t) {
        Ok(p) => p.exchange,
        Err(e) => panic!("{}\n{t}", e.one_line()),
    }
}

/// How a box is written.
#[derive(Clone, Copy, Default)]
struct BoxStyle {
    /// Every other face on a plane facing into the box, with same_sense .F.
    flip_faces: bool,
    /// Every other edge with same_sense .F. (its line running from its end to its start).
    flip_edges: bool,
    /// Every face's bound written the other way round with orientation .F.
    flip_bounds: bool,
    /// One face's same_sense written wrong (its plane facing in, same_sense .T.).
    wrong_face: bool,
    /// One edge's same_sense written wrong (its line running from start to end, same_sense .F.).
    wrong_edge: bool,
    /// The first edge starts at a second VERTEX_POINT at its first corner's point moved by
    /// the given distance along x (0 for the same point).
    dup_vertex: Option<f64>,
    /// Corner 0's VERTEX_POINT written the given distance along x off the corner its edges'
    /// lines run through.
    off_vertex: Option<f64>,
}

/// The box from `lo` to `hi`: its faces (outward normals), with the extra inner bounds of
/// its top and bottom faces written by `inner` (given the face's axis sign).
fn box_faces(
    w: &mut W,
    lo: V,
    hi: V,
    style: BoxStyle,
    inner: &mut dyn FnMut(&mut W, f64) -> Vec<u64>,
) -> Vec<u64> {
    let corner = |i: usize| {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    let vs: Vec<(u64, V)> = (0..8)
        .map(|i| {
            let mut at = corner(i);
            if i == 0 {
                at[0] += style.off_vertex.unwrap_or(0.0);
            }
            (w.vertex(at), corner(i))
        })
        .collect();
    let mut edges: std::collections::BTreeMap<(usize, usize), u64> = Default::default();
    let mut faces = Vec::new();
    let mut count = 0;
    for a in 0..3 {
        for side in [false, true] {
            let (bx, cx) = ((a + 1) % 3, (a + 2) % 3);
            let bit = |axis: usize| 1usize << axis;
            let base = if side { bit(a) } else { 0 };
            let mut ring = [
                base,
                base | bit(bx),
                base | bit(bx) | bit(cx),
                base | bit(cx),
            ];
            if !side {
                ring.reverse();
            }
            let mut oes = Vec::new();
            for k in 0..4 {
                let (p, q) = (ring[k], ring[(k + 1) % 4]);
                let key = (p.min(q), p.max(q));
                let e = if let Some(e) = edges.get(&key) {
                    *e
                } else {
                    let n = edges.len();
                    let flip = style.flip_edges && n % 2 == 1;
                    let e = if style.wrong_edge && n == 3 {
                        // Its line runs from start to end, but it says it runs against it.
                        let (va, pa) = vs[key.0];
                        let (vb, pb) = vs[key.1];
                        let (o, d) = (w.pt(pa), w.dir(sub3(pb, pa)));
                        let v = w.add(format!("VECTOR('',#{d},1.)"));
                        let l = w.add(format!("LINE('',#{o},#{v})"));
                        w.add(format!("EDGE_CURVE('',#{va},#{vb},#{l},.F.)"))
                    } else if let Some(off) = style.dup_vertex.filter(|_| n == 0) {
                        let at = [vs[key.0].1[0] + off, vs[key.0].1[1], vs[key.0].1[2]];
                        let twin = (w.vertex(at), at);
                        w.line_edge(twin, vs[key.1], !flip)
                    } else {
                        w.line_edge(vs[key.0], vs[key.1], !flip)
                    };
                    edges.insert(key, e);
                    e
                };
                oes.push(w.oe(e, p == key.0));
            }
            let mut normal = [0.0; 3];
            normal[a] = if side { 1.0 } else { -1.0 };
            let mut x = [0.0; 3];
            x[bx] = 1.0;
            let mut center = [
                0.5 * (lo[0] + hi[0]),
                0.5 * (lo[1] + hi[1]),
                0.5 * (lo[2] + hi[2]),
            ];
            center[a] = if side { hi[a] } else { lo[a] };
            let mut bounds = if style.flip_bounds {
                let rev: Vec<u64> = oes
                    .iter()
                    .rev()
                    .map(|&oe| {
                        // The same edges, the other way round.
                        let line =
                            w.s.lines()
                                .find(|l| l.starts_with(&format!("#{oe}=")))
                                .unwrap()
                                .to_string();
                        let flipped = if line.ends_with(".T.);") {
                            line.replace(".T.);", ".F.);")
                        } else {
                            line.replace(".F.);", ".T.);")
                        };
                        let body = flipped.split_once('=').unwrap().1.trim_end_matches(';');
                        w.add(body)
                    })
                    .collect();
                vec![w.bound(&rev, true, false)]
            } else {
                vec![w.bound(&oes, true, true)]
            };
            if a == 2 {
                bounds.extend(inner(w, normal[2]));
            }
            let flip = style.flip_faces && count % 2 == 1;
            let wrong = style.wrong_face && count == 4;
            let plane = w.plane(center, if flip || wrong { neg(normal) } else { normal }, x);
            faces.push(w.face(&bounds, plane, !flip));
            count += 1;
        }
    }
    faces
}

fn closed_shell(w: &mut W, faces: &[u64]) -> u64 {
    w.add(format!("CLOSED_SHELL('',({}))", list(faces)))
}

fn cube(style: BoxStyle) -> Exchange {
    let mut w = W::default();
    let faces = box_faces(&mut w, [0.0; 3], [10.0, 20.0, 30.0], style, &mut |_, _| {
        Vec::new()
    });
    let shell = closed_shell(&mut w, &faces);
    let solid = w.add(format!("MANIFOLD_SOLID_BREP('cube',#{shell})"));
    w.file(&[solid])
}

/// The file's one body, built in a document at the default resolution.
fn try_built(ex: &Exchange) -> Result<Built, BuildError> {
    let r = read(ex, &ReadOptions::default()).unwrap();
    assert!(r.skipped.is_empty(), "{:?}", r.skipped);
    assert_eq!(r.bodies.len(), 1);
    let doc = Document::default();
    let (settings, stop) = (Settings::new(), Stop::new());
    let mut scratch = Scratch::default();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    build(ex, &r.bodies[0], &mut cx)
}

/// The file's one body, built, with no findings and nothing raised.
#[track_caller]
fn built(ex: &Exchange) -> Built {
    let b = try_built(ex).unwrap_or_else(|e| panic!("{e} ({})", e.name()));
    assert!(b.findings.list.is_empty(), "{:?}", b.findings.list);
    assert!(
        b.findings.undecided.is_empty(),
        "{:?}",
        b.findings.undecided
    );
    b
}

/// [regions, shells, faces, edges, vertices]. A solid's closed shell is two shells, one
/// each side: the outside's and the material's (a void's, the cavity's and the lining).
fn counts(b: &Built) -> [usize; 5] {
    let body = &b.body;
    [
        body.regions().len(),
        body.shells().len(),
        body.faces().len(),
        body.edges().len(),
        body.vertices().len(),
    ]
}

/// The solid regions' volume; `volume` refuses a body set inside out, so a value is also
/// the proof every shell faces away from its material.
fn vol(b: &Built) -> f64 {
    volume(
        &b.body,
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::default(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn sheet_area(b: &Built) -> f64 {
    area(
        &b.body,
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::default(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

#[track_caller]
fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-7 * want.abs().max(1.0),
        "{got} against {want}"
    );
}

#[test]
fn a_cube_in_each_style_is_six_faces_twelve_edges_eight_vertices() {
    for style in [
        BoxStyle::default(),
        BoxStyle {
            flip_faces: true,
            ..Default::default()
        },
        BoxStyle {
            flip_edges: true,
            ..Default::default()
        },
        BoxStyle {
            flip_bounds: true,
            ..Default::default()
        },
        BoxStyle {
            flip_faces: true,
            flip_edges: true,
            flip_bounds: true,
            ..Default::default()
        },
    ] {
        let s = built(&cube(style));
        assert_eq!((s.name.as_str(), s.kind), ("cube", SolidOrSheet::Solid));
        assert!(s.notes.is_empty(), "{:?}", s.notes);
        assert!(s.raised.is_empty(), "{:?}", s.raised);
        assert_eq!(counts(&s), [2, 2, 6, 12, 8]);
        assert_eq!(s.findings.class, Class::Solid);
        assert_eq!((s.faces.len(), s.edges.len(), s.vertices.len()), (6, 12, 8));
        near(vol(&s), 6000.0);
        // Every edge is used twice, and no accuracy is recorded: every gap is in the zone.
        for e in s.body.edges() {
            assert_eq!(e.uses().count(), 2);
            assert!(e.accuracy().is_none());
        }
    }
}

/// The pin turned these without saying which entity; the build keeps the loops and the
/// vertices and names each entity it turned.
#[test]
fn faces_and_edges_written_against_their_sense_are_read_by_their_loops_and_vertices() {
    let s = built(&cube(BoxStyle {
        wrong_face: true,
        wrong_edge: true,
        ..Default::default()
    }));
    assert_eq!(counts(&s), [2, 2, 6, 12, 8]);
    assert_eq!(
        s.notes
            .iter()
            .filter(|n| matches!(n, Note::TurnedFace { .. }))
            .count(),
        1,
        "{:?}",
        s.notes
    );
    assert_eq!(
        s.notes
            .iter()
            .filter(|n| matches!(n, Note::TurnedEdge { .. }))
            .count(),
        1,
        "{:?}",
        s.notes
    );
    near(vol(&s), 6000.0);
}

/// Deliberate difference from the pin, which joins two vertices at one point (R3: the
/// importer makes no change to the file's topology). Two `VERTEX_POINT`s at one corner stay
/// two, the loops through them do not close, and the body is refused: a stitch (SP4) or
/// the file's writer joins them.
#[test]
fn two_vertices_at_one_point_are_not_joined() {
    for off in [0.0, 1.0] {
        let e = try_built(&cube(BoxStyle {
            dup_vertex: Some(off),
            ..Default::default()
        }))
        .unwrap_err();
        // At one point the loops do not join and the records do not finish; a vertex 1 mm
        // off its line leaves the line edge off the face's loop, a pcurve the closed forms
        // do not give.
        let refused = match off {
            0.0 => matches!(e, BuildError::Unfinished(_)),
            _ => matches!(
                e,
                BuildError::Pending {
                    waits: Waits::PcurveFit,
                    ..
                }
            ),
        };
        assert!(refused, "{off}: {e:?}");
    }
}

/// A corner written off the lines through it: the vertex's accuracy is raised to its
/// certified gap, once, and recorded.
#[test]
fn a_vertex_off_its_edges_is_raised_to_its_gap() {
    let off = 1e-3;
    let s = try_built(&cube(BoxStyle {
        off_vertex: Some(off),
        ..Default::default()
    }))
    .unwrap();
    let raised: Vec<_> = s
        .raised
        .iter()
        .filter(|r| r.gap == GapKind::VertexToEdge)
        .collect();
    assert_eq!(raised.len(), 1, "{:?}", s.raised);
    let r = raised[0];
    assert_eq!(r.before, 1e-5, "the zone, wider than the file's 1e-7 mm");
    // The certified bound encloses the gap and is within half the zone of it.
    assert!(r.after >= off && r.after <= off + 1e-5, "{r:?}");
    let v = s.body.vertices().find(|v| v.accuracy().is_some()).unwrap();
    assert!(v.accuracy().unwrap().acc.length().mm() >= off);
    // No edge moved: the lines meet the faces' planes.
    assert!(s.raised.iter().all(|r| r.gap != GapKind::EdgeToFace));
    assert!(!s.notes.iter().any(|n| matches!(n, Note::PastRatio { .. })));
}

/// A gap past `MAX_ACCURACY_RATIO` zones is still raised (R3), with a note.
#[test]
fn a_raise_past_the_accuracy_ratio_is_made_and_noted() {
    let s = try_built(&cube(BoxStyle {
        off_vertex: Some(0.05),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(s.raised.len(), 1, "{:?}", s.raised);
    let ratio = s
        .notes
        .iter()
        .find_map(|n| match n {
            Note::PastRatio { ratio, .. } => Some(*ratio),
            _ => None,
        })
        .unwrap();
    assert!((5000.0..5001.0).contains(&ratio), "{ratio}");
}

/// The cylinder of radius `r` from z0 to z1 about (cx, cy): its vertices on the seam at
/// angle 0, its circles (counter-clockwise about +z) and its seam line (upwards).
struct Cyl {
    circles: [u64; 2],
    seam: u64,
    surface: u64,
}

fn cylinder(w: &mut W, c: [f64; 2], r: f64, z: [f64; 2]) -> Cyl {
    let p = |h: f64| [c[0] + r, c[1], h];
    let (v0, v1) = (w.vertex(p(z[0])), w.vertex(p(z[1])));
    let circle = |w: &mut W, v: u64, h: f64| {
        let k = w.circle([c[0], c[1], h], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], r);
        w.add(format!("EDGE_CURVE('',#{v},#{v},#{k},.T.)"))
    };
    let circles = [circle(w, v0, z[0]), circle(w, v1, z[1])];
    let seam = w.line_edge((v0, p(z[0])), (v1, p(z[1])), true);
    let a = w.axis([c[0], c[1], 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let surface = w.add(format!("CYLINDRICAL_SURFACE('',#{a},{})", real(r)));
    Cyl {
        circles,
        seam,
        surface,
    }
}

/// The cylinder's side as a face whose normal points out of the cylinder (`outward`) or
/// into it.
fn side(w: &mut W, c: &Cyl, outward: bool) -> u64 {
    let [b, t] = c.circles;
    // Counter-clockwise in (angle, height): about the outward normal.
    let ring = [(b, true), (c.seam, true), (t, false), (c.seam, false)];
    let oes: Vec<u64> = if outward {
        ring.iter().map(|&(e, o)| w.oe(e, o)).collect()
    } else {
        ring.iter().rev().map(|&(e, o)| w.oe(e, !o)).collect()
    };
    let bound = w.bound(&oes, true, true);
    w.face(&[bound], c.surface, outward)
}

#[test]
fn a_cylinder_has_a_seam_used_twice_by_its_side() {
    let mut w = W::default();
    let c = cylinder(&mut w, [1.0, 2.0], 5.0, [0.0, 10.0]);
    let side_face = side(&mut w, &c, true);
    let ob = w.oe(c.circles[0], false);
    let bottom_bound = w.bound(&[ob], true, true);
    let bottom_plane = w.plane([1.0, 2.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let bottom = w.face(&[bottom_bound], bottom_plane, false);
    let ot = w.oe(c.circles[1], true);
    let top_bound = w.bound(&[ot], true, true);
    let top_plane = w.plane([1.0, 2.0, 10.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let top = w.face(&[top_bound], top_plane, true);
    let shell = closed_shell(&mut w, &[bottom, side_face, top]);
    let solid = w.add(format!("MANIFOLD_SOLID_BREP('',#{shell})"));
    let s = built(&w.file(&[solid]));
    assert_eq!(counts(&s), [2, 2, 3, 3, 2]);
    // The seam's two uses are in the side face; each circle closes on its one vertex.
    let seam = s.edges.iter().find(|&&(e, _)| e == c.seam).unwrap().1;
    let seam = s.body.edge(seam).unwrap();
    let faces: Vec<_> = seam.uses().map(|u| u.face().id()).collect();
    assert_eq!(faces.len(), 2);
    assert_eq!(faces[0], faces[1]);
    near(vol(&s), PI * 25.0 * 10.0);
}

#[test]
fn a_ball_bounded_by_its_seam_alone() {
    for same_sense in [true, false] {
        let mut w = W::default();
        let (n, south) = ([0.0, 0.0, 5.0], [0.0, 0.0, -5.0]);
        let (vn, vs) = (w.vertex(n), w.vertex(south));
        // The meridian through +x: angle 0 at the north pole, pi at the south.
        let axis = if same_sense {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, -1.0, 0.0]
        };
        let k = w.circle([0.0; 3], axis, [0.0, 0.0, 1.0], 5.0);
        let e = w.add(format!("EDGE_CURVE('',#{vn},#{vs},#{k},{})", b(same_sense)));
        let (o1, o2) = (w.oe(e, true), w.oe(e, false));
        let bound = w.bound(&[o1, o2], true, true);
        let a = w.axis([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let sphere = w.add(format!("SPHERICAL_SURFACE('',#{a},5.)"));
        let face = w.face(&[bound], sphere, true);
        let shell = closed_shell(&mut w, &[face]);
        let solid = w.add(format!("MANIFOLD_SOLID_BREP('ball',#{shell})"));
        let s =
            try_built(&w.file(&[solid])).unwrap_or_else(|e| panic!("same_sense {same_sense}: {e}"));
        // One face, the meridian and the two vertices of the file; the writer's collapsed
        // edges at the poles are the only other edges.
        let [regions, shells, faces, _, vertices] = counts(&s);
        assert_eq!(
            [regions, shells, faces, vertices],
            [2, 2, 1, 2],
            "{same_sense}"
        );
        assert_eq!(s.edges.len(), 1);
        near(vol(&s), 4.0 / 3.0 * PI * 125.0);
    }
}

/// The cone from its apex at z = 10 widening to radius 5 at z = 0, and its base disk.
fn cone_with(w: &mut W, apex_loop: bool) -> u64 {
    let (p, top) = ([5.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
    let (v, apex) = (w.vertex(p), w.vertex(top));
    let k = w.circle([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 5.0);
    let e = w.add(format!("EDGE_CURVE('',#{v},#{v},#{k},.T.)"));
    let a = w.axis([0.0; 3], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    let cone = w.add(format!(
        "CONICAL_SURFACE('',#{a},5.,{})",
        real(fmath::atan(0.5))
    ));
    let side = if apex_loop {
        let o = w.oe(e, true);
        let outer = w.bound(&[o], true, true);
        let vl = w.add(format!("VERTEX_LOOP('',#{apex})"));
        let point = w.add(format!("FACE_BOUND('',#{vl},.T.)"));
        w.face(&[outer, point], cone, true)
    } else {
        let seam = w.line_edge((v, p), (apex, top), true);
        let uses = [w.oe(e, true), w.oe(seam, true), w.oe(seam, false)];
        let outer = w.bound(&uses, true, true);
        w.face(&[outer], cone, true)
    };
    let o = w.oe(e, false);
    let base_bound = w.bound(&[o], true, true);
    let plane = w.plane([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let base = w.face(&[base_bound], plane, false);
    let shell = closed_shell(w, &[side, base]);
    w.add(format!("MANIFOLD_SOLID_BREP('cone',#{shell})"))
}

#[test]
fn a_cone_with_its_seam_to_the_apex_is_a_third_of_its_cylinder() {
    let mut w = W::default();
    let solid = cone_with(&mut w, false);
    let s = built(&w.file(&[solid]));
    let [regions, shells, faces, _, vertices] = counts(&s);
    assert_eq!([regions, shells, faces, vertices], [2, 2, 2, 2]);
    assert_eq!(s.edges.len(), 2);
    assert!(s.notes.is_empty(), "{:?}", s.notes);
    near(vol(&s), PI * 25.0 * 10.0 / 3.0);
}

/// The pin adds a seam to the apex for a `VERTEX_LOOP`; that is a change to the file's
/// topology, so it waits, by name.
#[test]
fn a_cone_reaching_its_apex_through_a_vertex_loop_waits() {
    let mut w = W::default();
    let solid = cone_with(&mut w, true);
    let e = try_built(&w.file(&[solid])).unwrap_err();
    assert!(
        matches!(
            e,
            BuildError::Pending {
                waits: Waits::VertexLoop,
                ..
            }
        ),
        "{e:?}"
    );
}

fn plate(marked: bool) -> Exchange {
    let mut w = W::default();
    let c = cylinder(&mut w, [10.0, 10.0], 3.0, [0.0, 5.0]);
    let mut inner = |w: &mut W, up: f64| {
        // About the face's outward normal the hole runs clockwise.
        let oe = if up > 0.0 {
            w.oe(c.circles[1], false)
        } else {
            w.oe(c.circles[0], true)
        };
        vec![w.bound(&[oe], false, true)]
    };
    let mut faces = box_faces(
        &mut w,
        [0.0; 3],
        [20.0, 20.0, 5.0],
        BoxStyle::default(),
        &mut inner,
    );
    if !marked {
        w.s = w.s.replace("FACE_OUTER_BOUND", "FACE_BOUND");
        // The hole first.
        let mut lines: Vec<String> = w.s.lines().map(str::to_string).collect();
        for l in &mut lines {
            if l.contains("ADVANCED_FACE") && l.matches('#').count() == 4 {
                let (head, rest) = l.split_once("',(").unwrap();
                let (bounds, tail) = rest.split_once(')').unwrap();
                let parts: Vec<&str> = bounds.split(',').rev().collect();
                *l = format!("{head}',({}){tail}", parts.join(","));
            }
        }
        w.s = lines.join("\n") + "\n";
    }
    faces.push(side(&mut w, &c, false));
    let shell = closed_shell(&mut w, &faces);
    let solid = w.add(format!("MANIFOLD_SOLID_BREP('plate',#{shell})"));
    w.file(&[solid])
}

#[test]
fn a_plate_with_a_through_hole() {
    for marked in [true, false] {
        let s = built(&plate(marked));
        assert_eq!(counts(&s), [2, 2, 7, 15, 10], "marked {marked}");
        let holed = s.body.faces().filter(|f| f.loops().len() == 2).count();
        assert_eq!(holed, 2);
        near(vol(&s), 20.0 * 20.0 * 5.0 - PI * 9.0 * 5.0);
    }
}

#[test]
fn a_solid_with_a_void() {
    let mut w = W::default();
    let none = &mut |_: &mut W, _: f64| Vec::new();
    let outer = box_faces(&mut w, [0.0; 3], [10.0; 3], BoxStyle::default(), none);
    let outer = closed_shell(&mut w, &outer);
    let style = BoxStyle {
        flip_faces: true,
        ..Default::default()
    };
    let inner = box_faces(&mut w, [4.0; 3], [6.0; 3], style, none);
    let inner = closed_shell(&mut w, &inner);
    let void = w.add(format!("ORIENTED_CLOSED_SHELL('',*,#{inner},.F.)"));
    let solid = w.add(format!("BREP_WITH_VOIDS('',#{outer},(#{void}))"));
    let s = built(&w.file(&[solid]));
    assert_eq!(counts(&s), [3, 4, 12, 24, 16]);
    assert!(s.notes.is_empty(), "{:?}", s.notes);
    near(vol(&s), 1000.0 - 8.0);
}

/// A void written with its faces pointing into the cavity and no `ORIENTED_CLOSED_SHELL`:
/// the certified volume sign turns it, and says so.
#[test]
fn a_void_written_inside_out_is_turned_by_its_volume_sign() {
    let mut w = W::default();
    let none = &mut |_: &mut W, _: f64| Vec::new();
    let outer = box_faces(&mut w, [0.0; 3], [10.0; 3], BoxStyle::default(), none);
    let outer = closed_shell(&mut w, &outer);
    let inner = box_faces(&mut w, [4.0; 3], [6.0; 3], BoxStyle::default(), none);
    let inner = closed_shell(&mut w, &inner);
    let solid = w.add(format!("BREP_WITH_VOIDS('',#{outer},(#{inner}))"));
    let s = built(&w.file(&[solid]));
    assert_eq!(s.notes, vec![Note::TurnedShell { shell: inner }]);
    near(vol(&s), 1000.0 - 8.0);
}

#[test]
fn a_surface_model_of_open_shells_is_a_sheet() {
    let mut w = W::default();
    let none = &mut |_: &mut W, _: f64| Vec::new();
    let mut faces = box_faces(&mut w, [0.0; 3], [1.0; 3], BoxStyle::default(), none);
    faces.pop();
    let open = w.add(format!("OPEN_SHELL('',({}))", list(&faces)));
    let single = box_faces(&mut w, [5.0; 3], [6.0; 3], BoxStyle::default(), none);
    let one = w.add(format!("OPEN_SHELL('',(#{}))", single[0]));
    let model = w.add(format!(
        "SHELL_BASED_SURFACE_MODEL('sheet',(#{open},#{one}))"
    ));
    let s = built(&w.file(&[model]));
    assert_eq!(s.kind, SolidOrSheet::Sheet);
    assert_eq!(s.findings.class, Class::Sheet);
    assert_eq!(counts(&s)[2..], [6, 12 + 4, 8 + 4]);
    let once = s.body.edges().filter(|e| e.uses().count() == 1).count();
    assert_eq!(once, 4 + 4);
    near(sheet_area(&s), 6.0);
}

/// A plane face bounded by a conic arc from t = -1.5 to 1.5 and its chord: the area is the
/// segment's closed form, for the parabola `(t^2 / 2, t)` and the hyperbola `(cosh t, sinh t)`.
#[test]
fn parabola_and_hyperbola_edges_bound_faces() {
    let u: f64 = 1.5;
    for (kind, want) in [
        // 2 * integral over [0, u] of (u^2 / 2 - y^2 / 2).
        (
            "PARABOLA('',#{a},0.5)",
            2.0 * (fmath::powi(u, 3) / 2.0 - fmath::powi(u, 3) / 6.0),
        ),
        // The triangle to the origin less the hyperbolic sector.
        (
            "HYPERBOLA('',#{a},1.,1.)",
            fmath::cosh(u) * fmath::sinh(u) - u,
        ),
    ] {
        let mut w = W::default();
        let at = |t: f64| {
            if kind.starts_with("PARABOLA") {
                [0.5 * t * t, t, 0.0]
            } else {
                [fmath::cosh(t), fmath::sinh(t), 0.0]
            }
        };
        let (p1, p2) = (at(-u), at(u));
        let (v1, v2) = (w.vertex(p1), w.vertex(p2));
        let a = w.axis([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let conic = w.add(kind.replace("{a}", &a.to_string()));
        let e1 = w.add(format!("EDGE_CURVE('',#{v1},#{v2},#{conic},.T.)"));
        let e2 = w.line_edge((v2, p2), (v1, p1), true);
        let (o1, o2) = (w.oe(e1, true), w.oe(e2, true));
        let bound = w.bound(&[o1, o2], true, true);
        let plane = w.plane([0.0; 3], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
        let face = w.face(&[bound], plane, true);
        let shell = w.add(format!("OPEN_SHELL('',(#{face}))"));
        let model = w.add(format!("SHELL_BASED_SURFACE_MODEL('',(#{shell}))"));
        let s = built(&w.file(&[model]));
        assert_eq!(counts(&s), [1, 1, 1, 2, 2], "{kind}");
        near(sheet_area(&s), want);
    }
}

#[test]
fn an_inch_file_is_built_in_millimetres() {
    let mut w = W::default();
    let faces = box_faces(
        &mut w,
        [0.0; 3],
        [1.0, 2.0, 3.0],
        BoxStyle::default(),
        &mut |_, _| Vec::new(),
    );
    let shell = closed_shell(&mut w, &faces);
    let solid = w.add(format!("MANIFOLD_SOLID_BREP('cube',#{shell})"));
    let s = built(&w.file_in(&[solid], 1e-7, true));
    near(vol(&s), 6.0 * fmath::powi(25.4, 3));
}

#[test]
fn building_twice_gives_the_same_body() {
    let ex = plate(true);
    let (a, b) = (built(&ex), built(&ex));
    assert_eq!(format!("{:?}", a.body), format!("{:?}", b.body));
    assert_eq!(
        (&a.faces, &a.edges, &a.vertices),
        (&b.faces, &b.edges, &b.vertices)
    );
    assert_eq!(vol(&a).to_bits(), vol(&b).to_bits());
}

/// A unit square sheet whose edge from (1, 0) to (1, 1) is a B-spline: SP5's kind, by name.
#[test]
fn a_spline_edge_waits_for_its_task() {
    let mut w = W::default();
    let p = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let vs: Vec<(u64, V)> = p.iter().map(|&q| (w.vertex(q), q)).collect();
    let (c0, c1) = (w.pt(p[1]), w.pt(p[2]));
    let spline = w.add(format!(
        "B_SPLINE_CURVE_WITH_KNOTS('',1,(#{c0},#{c1}),.UNSPECIFIED.,.F.,.F.,(2,2),(0.,1.),.UNSPECIFIED.)"
    ));
    let (v1, v2) = (vs[1].0, vs[2].0);
    let edges = [
        w.line_edge(vs[0], vs[1], true),
        w.add(format!("EDGE_CURVE('',#{v1},#{v2},#{spline},.T.)")),
        w.line_edge(vs[2], vs[3], true),
        w.line_edge(vs[3], vs[0], true),
    ];
    let oes: Vec<u64> = edges.iter().map(|&e| w.oe(e, true)).collect();
    let bound = w.bound(&oes, true, true);
    let plane = w.plane([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let face = w.face(&[bound], plane, true);
    let shell = w.add(format!("OPEN_SHELL('',(#{face}))"));
    let model = w.add(format!("SHELL_BASED_SURFACE_MODEL('',(#{shell}))"));
    let e = try_built(&w.file(&[model])).unwrap_err();
    assert_eq!(
        e,
        BuildError::Pending {
            entity: spline,
            waits: Waits::BSplineCurve
        }
    );
    assert_eq!(e.name(), "step_waits_bspline_curve");
}

/// `import` builds each body once and leaves the placing to the product tree; a body that
/// cannot be built is listed with its cause and the rest are built.
#[test]
fn import_builds_each_body_once_and_lists_the_rest() {
    let mut w = W::default();
    let none = &mut |_: &mut W, _: f64| Vec::new();
    let faces = box_faces(&mut w, [0.0; 3], [1.0; 3], BoxStyle::default(), none);
    let shell = closed_shell(&mut w, &faces);
    let good = w.add(format!("MANIFOLD_SOLID_BREP('good',#{shell})"));
    let bad = cone_with(&mut w, true);
    let ex = w.part(&[good, bad]);
    let doc = Document::default();
    let (settings, stop) = (Settings::new(), Stop::new());
    let mut scratch = Scratch::default();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let got = import(&ex, &ReadOptions::default(), &mut cx).unwrap();
    assert_eq!(got.built.len(), 1, "{:?}", got.unbuilt);
    assert_eq!(got.built[0].entity, good);
    assert_eq!(got.unbuilt.len(), 1);
    assert_eq!(got.unbuilt[0].entity, bad);
    assert_eq!(got.read.products.placements_of(good).count(), 1);
    assert!(got.read.report.raised.is_empty());
    near(vol(&got.built[0]), 1.0);
}

/// An edge used by three faces is not a manifold edge: it waits.
#[test]
fn an_edge_of_three_faces_waits() {
    let mut w = W::default();
    let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
    let (va, vb) = ((w.vertex(p[0]), p[0]), (w.vertex(p[1]), p[1]));
    let shared = w.line_edge(va, vb, true);
    let mut faces = Vec::new();
    for far in [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]] {
        let q = [[1.0, far[1], far[2]], [0.0, far[1], far[2]]];
        let (vc, vd) = ((w.vertex(q[0]), q[0]), (w.vertex(q[1]), q[1]));
        let edges = [
            w.line_edge(vb, vc, true),
            w.line_edge(vc, vd, true),
            w.line_edge(vd, va, true),
        ];
        let mut oes = vec![w.oe(shared, true)];
        oes.extend(edges.iter().map(|&e| w.oe(e, true)));
        let bound = w.bound(&oes, true, true);
        let n = if far[2] == 0.0 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, -1.0, 0.0]
        };
        let plane = w.plane([0.0; 3], n, [1.0, 0.0, 0.0]);
        faces.push(w.face(&[bound], plane, true));
    }
    let shell = w.add(format!("OPEN_SHELL('',({}))", list(&faces)));
    let model = w.add(format!("SHELL_BASED_SURFACE_MODEL('',(#{shell}))"));
    let e = try_built(&w.file(&[model])).unwrap_err();
    assert_eq!(
        e,
        BuildError::Pending {
            entity: shared,
            waits: Waits::NonManifold
        }
    );
}

#[test]
fn an_edge_of_no_length_is_refused() {
    let mut w = W::default();
    let none = &mut |_: &mut W, _: f64| Vec::new();
    let faces = box_faces(&mut w, [0.0; 3], [1.0; 3], BoxStyle::default(), none);
    let shell = closed_shell(&mut w, &faces);
    let solid = w.add(format!("MANIFOLD_SOLID_BREP('',#{shell})"));
    // The first LINE edge made to start and end at one vertex.
    let text = {
        let (head, rest) = w.s.split_once("EDGE_CURVE('',#").unwrap();
        let (first, rest) = rest.split_once(",#").unwrap();
        let (_, rest) = rest.split_once(',').unwrap();
        format!("{head}EDGE_CURVE('',#{first},#{first},{rest}")
    };
    w.s = text;
    let e = try_built(&w.file(&[solid])).unwrap_err();
    assert!(matches!(e, BuildError::Degenerate { .. }), "{e:?}");
}

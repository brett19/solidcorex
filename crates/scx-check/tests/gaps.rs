// Written for solidcorex. Apache-2.0.
//! The certified gaps of `check` (research 74 §28, CF2a), on testkit bodies only. Each
//! recipe has one defect, of a size `delta` set against the accuracy `acc` it is allowed:
//! at `10 acc` it is found, at `acc` it is the allowance (found, undecided or clean, never
//! anything else), at `acc / 10` it is clean. The truths are constructions: a block's edge
//! moved by `delta` along one face's normal is `delta` off that face and on the other.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Draft, EdgeId, EdgeShape, FaceId, UseGeom};
use scx_check::{CheckError, Finding, Findings, Scope, Undecided, Why};
use scx_curve::{Curve, Line};
use scx_num::budget::Stop;
use scx_num::cert::{Basis, Cert, Claim, EdgeEnd, Method, Reads, Recorded};
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Vec3;
use scx_nurbs::Approach;
use scx_testkit::bodies::{Kind, generate, loose};

fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn kind(k: Kind, seed: u64, size: usize) -> Brep {
    finish(generate(k, seed, size).unwrap().draft)
}

fn run(b: &Brep, settings: &Settings, stop: &Stop, scope: Scope) -> Result<Findings, CheckError> {
    let doc = Document::with_resolution(b.resolution());
    let mut scratch = Scratch::default();
    let mut cx = Cx::new(&doc, settings, &mut scratch, stop, &NoTrace);
    scx_check::check(b, &mut cx, scope)
}

fn found(b: &Brep, scope: Scope) -> Findings {
    run(b, &Settings::new(), &Stop::new(), scope).unwrap()
}

fn recorded(mm: f64) -> Option<Arc<Recorded>> {
    Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(mm)).unwrap(),
        basis: Basis::Measured,
    }))
}

/// The block's first edge, the first face it bounds and that face's unit normal.
fn first_edge(b: &Brep) -> (EdgeId, FaceId, Vec3) {
    let e = b.edges().next().unwrap();
    let u = e.uses().next().unwrap();
    let UseGeom::Pcurve { curve, interval } = u.geom() else {
        panic!("a block's use is a pcurve");
    };
    let uv = curve.point(0.5 * (interval.min() + interval.max()));
    let n = u
        .face()
        .surface()
        .normal(uv, [Approach::Above; 2])
        .unwrap()
        .as_vec();
    (e.id(), u.face().id(), n)
}

/// A block (seed `seed`) with its first edge moved by `delta` along its first face's
/// normal: off that face by `delta`, still on the other (the faces are square). The edge is
/// given the accuracy `acc`; its vertices and uses are given `delta` and a little, so the
/// body finishes: only the face gap is past its allowance.
fn off_face(seed: u64, delta: f64, acc: Option<f64>) -> (Brep, EdgeId, FaceId) {
    let (id, face, n) = first_edge(&kind(Kind::Block, seed, 1));
    let mut d = generate(Kind::Block, seed, 1).unwrap().draft;
    let slack = delta * 1.01 + 1e-12;
    let g = d.edge_mut(id).unwrap();
    let EdgeShape::Curve {
        curve: Curve::Line(l),
        ..
    } = &mut g.shape
    else {
        panic!("a block's edge is a line");
    };
    *l = Line::new(l.origin() + n * delta, l.direction()).unwrap();
    g.accuracy = acc.and_then(recorded);
    let ends = g.ends;
    for v in ends {
        d.vertex_mut(v).unwrap().accuracy = recorded(slack);
    }
    let faces: Vec<FaceId> = d.faces().map(|f| f.id()).collect();
    for f in faces {
        let rec = d.face_mut(f).unwrap();
        for lp in &mut rec.loops {
            for u in &mut lp.uses {
                if u.edge == id {
                    u.bound = recorded(slack);
                }
            }
        }
    }
    (finish(d), id, face)
}

const ACC: f64 = 1e-5;

/// The recipe at `10 acc`, `acc` and `acc / 10` under each scope.
#[test]
fn an_edge_off_its_face_is_found_past_its_reach_only() {
    for seed in 0..3 {
        for scope in [Scope::Import, Scope::Stitched, Scope::Full] {
            // 10 acc: the one finding, its bracket holding delta.
            let delta = 10.0 * ACC;
            let (b, edge, face) = off_face(seed, delta, Some(ACC));
            let f = found(&b, scope);
            let [
                Finding::EdgeOffFace {
                    edge: e,
                    face: g,
                    gap,
                    allowed,
                },
            ] = f.list[..]
            else {
                panic!("seed {seed} {scope:?}: {:?} {:?}", f.list, f.undecided);
            };
            assert_eq!((e, g), (edge, face));
            assert_eq!(allowed.mm(), ACC);
            assert!(gap.lower() <= delta * (1.0 + 1e-9) && delta * (1.0 - 1e-9) <= gap.upper());
            assert!(f.undecided.is_empty(), "{:?}", f.undecided);

            // acc: the gap is the allowance. Whatever is said is about that edge and face,
            // and a bracket left open is within NEAR of it.
            let (b, edge, face) = off_face(seed, ACC, Some(ACC));
            let f = found(&b, scope);
            for x in &f.list {
                assert!(
                    matches!(x, Finding::EdgeOffFace { edge: e, face: g, gap, .. }
                        if (*e, *g) == (edge, face) && gap.lower() > ACC),
                    "{x:?}"
                );
            }
            for x in &f.undecided {
                assert!(
                    matches!(x, Undecided::EdgeOffFace { edge: e, face: g, gap, why, .. }
                        if (*e, *g) == (edge, face)
                        && gap.width() <= 1e-3 * ACC
                        && matches!(why, Why::AtAllowance | Why::Precision)),
                    "{x:?}"
                );
            }

            // acc / 10: clean.
            let (b, _, _) = off_face(seed, ACC / 10.0, Some(ACC));
            let f = found(&b, scope);
            assert!(f.is_clean(), "seed {seed}: {:?} {:?}", f.list, f.undecided);
        }
    }
}

/// A record of `acc` carrying a certificate of `claim` with `bound`.
fn cert(acc: f64, claim: Claim, bound: f64) -> Option<Arc<Recorded>> {
    Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(acc)).unwrap(),
        basis: Basis::Certified(Cert {
            claim,
            bound: Length::from_mm(bound),
            method: Method::Interval,
            reads: Reads::NONE,
        }),
    }))
}

/// The edge off its face by `10 acc` (as above) with the edge's record replaced by `edge`
/// and its uses' by `uses`, where given.
fn off_face_recorded(
    edge: Option<Arc<Recorded>>,
    uses: Option<Arc<Recorded>>,
) -> (Brep, EdgeId, FaceId) {
    let (b, id, face) = off_face(0, 10.0 * ACC, Some(ACC));
    let mut d = b.into_draft();
    if edge.is_some() {
        d.edge_mut(id).unwrap().accuracy = edge;
    }
    if uses.is_some() {
        let faces: Vec<FaceId> = d.faces().map(|f| f.id()).collect();
        for f in faces {
            for lp in &mut d.face_mut(f).unwrap().loops {
                for u in &mut lp.uses {
                    if u.edge == id {
                        u.bound.clone_from(&uses);
                    }
                }
            }
        }
    }
    (finish(d), id, face)
}

/// Under `Import` and `Stitched` a gap recorded with a certificate of its question's claim,
/// within the question's allowance, is taken as decided (04 SP2): the check does not prove
/// it again, so a certificate that lies (here one that says an edge `10 acc` off its face is
/// within `acc`) is believed, and `Full` still finds the edge. A certificate of another
/// claim, past the allowance, or a record that is not certified is proved as before.
#[test]
fn a_recorded_certificate_decides_its_question_outside_full_only() {
    let slack = 10.0 * ACC * 1.01 + 1e-12;
    let off = |b: &Brep, scope| {
        let f = found(b, scope);
        matches!(f.list[..], [Finding::EdgeOffFace { .. }]) && f.undecided.is_empty()
    };
    // Believed: the edge's own certificate, or its uses' within re.
    for (edge, uses) in [
        (cert(ACC, Claim::CurveToIntersection, ACC), None),
        (None, cert(slack, Claim::PcurveOnSurface, ACC)),
    ] {
        let (b, ..) = off_face_recorded(edge, uses);
        for scope in [Scope::Import, Scope::Stitched] {
            let f = found(&b, scope);
            assert!(f.is_clean(), "{scope:?}: {:?} {:?}", f.list, f.undecided);
        }
        assert!(off(&b, Scope::Full));
    }
    // Proved: a wrong claim, a bound past the allowance, an unproven or measured basis, none.
    let unproven = Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(ACC)).unwrap(),
        basis: Basis::Unproven(Cert {
            claim: Claim::CurveToIntersection,
            bound: Length::from_mm(ACC),
            method: Method::Interval,
            reads: Reads::NONE,
        }),
    }));
    for (edge, uses) in [
        (cert(ACC, Claim::EndsAtVertex(EdgeEnd::Start), ACC), None),
        (cert(ACC, Claim::PcurveOnSurface, ACC), None),
        (cert(ACC, Claim::CurveToIntersection, 2.0 * ACC), None),
        (unproven, None),
        (recorded(ACC), None),
        (None, cert(slack, Claim::CurveToIntersection, ACC)),
        (None, cert(slack, Claim::PcurveOnSurface, 2.0 * slack)),
    ] {
        let (b, ..) = off_face_recorded(edge.clone(), uses.clone());
        for scope in [Scope::Import, Scope::Stitched, Scope::Full] {
            assert!(off(&b, scope), "{scope:?} {edge:?} {uses:?}");
        }
    }
}

/// On a valid curved body whose uses carry true certificates, `Import` takes them and
/// spends less than `Full`, with the same (clean) answer.
#[test]
fn true_certificates_save_the_proof_and_change_no_answer() {
    for k in [Kind::Cylinder, Kind::Sphere, Kind::Torus] {
        let mut d = kind(k, 0, 1).into_draft();
        let faces: Vec<FaceId> = d.faces().map(|f| f.id()).collect();
        for f in faces {
            for lp in &mut d.face_mut(f).unwrap().loops {
                for u in &mut lp.uses {
                    u.bound = cert(1e-9, Claim::PcurveOnSurface, 1e-9);
                }
            }
        }
        let b = finish(d);
        let (import, full) = (found(&b, Scope::Import), found(&b, Scope::Full));
        assert!(import.is_clean() && full.is_clean(), "{k:?}");
        assert!(
            import.spent.work < full.spent.work,
            "{k:?}: {} {}",
            import.spent.work,
            full.spent.work
        );
    }
}

/// The pin's `offset_edge_curve_fails_test_13`: a box's first edge moved 0.01 off a face
/// at the default accuracy; the gap is 0.01.
#[test]
fn an_edge_moved_a_hundredth_off_a_face_is_a_hundredth_off() {
    let (b, edge, face) = off_face(0, 0.01, None);
    let f = found(&b, Scope::Import);
    let [
        Finding::EdgeOffFace {
            edge: e,
            face: g,
            gap,
            ..
        },
    ] = f.list[..]
    else {
        panic!("{:?} {:?}", f.list, f.undecided);
    };
    assert_eq!((e, g), (edge, face));
    assert!(
        gap.lower() <= 0.01 && 0.01 <= gap.upper() + 1e-15,
        "{gap:?}"
    );
    assert!(gap.width() <= 1e-3 * b.resolution().zone().mm() + 1e-15);
}

/// A defect finish samples (a pcurve or an end off by `10 acc`) never reaches the check:
/// finish refuses the body. The check repeats those tests certified, and at `acc` and
/// `acc / 10` they are clean.
#[test]
fn a_moved_vertex_is_clean_within_its_ball_and_refused_by_finish_past_it() {
    for (delta, ok) in [(10.0 * ACC, false), (ACC, true), (ACC / 10.0, true)] {
        let b = kind(Kind::Block, 1, 1);
        let v = b.vertices().next().unwrap().id();
        let p = b.vertex(v).unwrap().point();
        let mut d = generate(Kind::Block, 1, 1).unwrap().draft;
        let rec = d.vertex_mut(v).unwrap();
        rec.point = p + Vec3::new(1.0, 2.0, 2.0) * (delta / 3.0);
        // The edges keep the zone; the vertex is allowed acc.
        rec.accuracy = recorded(ACC);
        let finished = d.finish(&Document::default());
        assert_eq!(finished.is_ok(), ok, "delta {delta}");
        if let Ok((b, _)) = finished {
            let f = found(&b, Scope::Full);
            assert!(
                f.is_clean(),
                "delta {delta}: {:?} {:?}",
                f.list,
                f.undecided
            );
        }
    }
}

/// Every generator and its loose variant (pcurves moved into their faces by up to
/// `1e-6`, each with the bound it measured) is clean under every scope: the gaps add no
/// finding to a valid body, and decide every question on it. (A stack's two blocks are not
/// a solid, CF1's structural finding, so only the gap findings are counted.)
fn gaps(f: &Findings) -> Vec<&Finding> {
    f.list
        .iter()
        .filter(|x| {
            !matches!(
                x,
                Finding::NotSolid { .. } | Finding::VertexNotManifold { .. }
            )
        })
        .collect()
}

/// One generator's bodies at every seed and size: valid under every scope, and loose.
fn valid_and_loose_are_clean(k: Kind) {
    for seed in 0..2 {
        for size in [1, 3] {
            let b = kind(k, seed, size);
            for scope in [Scope::Import, Scope::Stitched, Scope::Full] {
                let f = found(&b, scope);
                assert!(
                    gaps(&f).is_empty() && f.undecided.is_empty(),
                    "{} {seed} {size} {scope:?}: {:?} {:?}",
                    k.name(),
                    f.list,
                    f.undecided
                );
            }
            for offset in [1e-9, 1e-6] {
                let b = finish(loose(k, seed, size, offset).unwrap());
                let f = found(&b, Scope::Import);
                assert!(
                    gaps(&f).is_empty() && f.undecided.is_empty(),
                    "loose {} {seed} {size} {offset}: {:?} {:?}",
                    k.name(),
                    f.list,
                    f.undecided
                );
            }
        }
    }
}

/// One test per generator, so the runner checks them in parallel (together the 13 checks
/// of one test that ran 95 s on one core). `TESTED` lists them for the guard below.
macro_rules! per_kind {
    ($($name:ident => $kind:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                valid_and_loose_are_clean(Kind::$kind);
            }
        )*
        const TESTED: &[Kind] = &[$(Kind::$kind),*];
    };
}

per_kind! {
    valid_and_loose_block_is_clean => Block,
    valid_and_loose_prism_is_clean => Prism,
    valid_and_loose_quad_sheet_is_clean => QuadSheet,
    valid_and_loose_tri_sheet_is_clean => TriSheet,
    valid_and_loose_cylinder_is_clean => Cylinder,
    valid_and_loose_cone_is_clean => Cone,
    valid_and_loose_sphere_is_clean => Sphere,
    valid_and_loose_torus_is_clean => Torus,
    valid_and_loose_holed_is_clean => Holed,
    valid_and_loose_cavity_is_clean => Cavity,
    valid_and_loose_stack_is_clean => Stack,
    valid_and_loose_checker_is_clean => Checker,
    valid_and_loose_touching_is_clean => Touching,
}

/// A generator added to `Kind::ALL` gets a test above.
#[test]
fn every_generator_has_a_clean_body_test() {
    assert_eq!(TESTED, &Kind::ALL[..]);
}

/// The pin's `issue_1419`: a vertex wide past the ratio limit is a defect of our own
/// results (`Full`) and a recorded fact of imported or stitched data, not a finding.
#[test]
fn a_wide_vertex_is_found_under_full_only() {
    let b = kind(Kind::Block, 0, 1);
    let zone = b.resolution().zone().mm();
    let v = b.vertices().next().unwrap().id();
    for (ratio, wide) in [(999.0, false), (1000.0, true), (2050.0, true)] {
        let mut d = generate(Kind::Block, 0, 1).unwrap().draft;
        d.vertex_mut(v).unwrap().accuracy = recorded(ratio * zone);
        let b = finish(d);
        let f = found(&b, Scope::Full);
        if wide {
            let [
                Finding::WideVertex {
                    vertex,
                    accuracy,
                    limit,
                },
            ] = f.list[..]
            else {
                panic!("{ratio}: {:?}", f.list);
            };
            assert_eq!(vertex, v);
            assert_eq!(accuracy.mm(), ratio * zone);
            assert_eq!(limit.mm(), 1000.0 * zone);
        } else {
            assert!(f.list.is_empty(), "{ratio}: {:?}", f.list);
        }
        for scope in [Scope::Import, Scope::Stitched] {
            assert!(found(&b, scope).list.is_empty(), "{ratio} {scope:?}");
        }
    }
}

/// An edge whose share of the budget runs out is listed undecided with why, never passed
/// or failed; with a share as large as the whole unlimited run spent the same body decides.
#[test]
fn a_budget_too_small_leaves_the_edge_off_its_face_undecided() {
    let (b, edge, face) = off_face(0, 10.0 * ACC, Some(ACC));
    let full = found(&b, Scope::Import);
    let spent = full.spent.work;
    let uses: u64 = b
        .faces()
        .map(|f| f.loops().map(|l| l.uses().len() as u64).sum::<u64>())
        .sum();
    let mut seen_exhausted = false;
    for limit in [uses + 1, uses + b.edges().len() as u64 * 4, spent / 2] {
        let f = run(
            &b,
            &Settings::new().with_budget(limit),
            &Stop::new(),
            Scope::Import,
        )
        .unwrap();
        assert!(f.spent.work <= limit);
        // Never a false finding; what is not found is undecided.
        for x in &f.list {
            assert!(
                matches!(x, Finding::EdgeOffFace { edge: e, .. } if *e == edge),
                "{x:?}"
            );
        }
        for x in &f.undecided {
            // Any edge's share may run out (the moved edge's other face still holds it,
            // and the others are clean but not yet proved); the moved edge's must at some
            // limit.
            if let Undecided::EdgeOffFace {
                why: Why::Exhausted,
                edge: e,
                face: g,
                ..
            } = x
            {
                seen_exhausted |= (*e, *g) == (edge, face);
            }
        }
        if f.list.is_empty() {
            assert!(!f.undecided.is_empty(), "limit {limit}");
        }
    }
    assert!(seen_exhausted);
    let f = run(
        &b,
        &Settings::new().with_budget(uses + spent * b.edges().len() as u64),
        &Stop::new(),
        Scope::Import,
    )
    .unwrap();
    assert_eq!(f.list, full.list);
}

#[test]
fn a_set_stop_cancels() {
    let (b, _, _) = off_face(0, 10.0 * ACC, Some(ACC));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        run(&b, &Settings::new(), &stop, Scope::Import),
        Err(CheckError::Cancelled)
    );
}

#[test]
fn the_same_body_gives_the_same_findings() {
    for k in [Kind::Cylinder, Kind::Sphere, Kind::Torus, Kind::Cone] {
        let b = finish(loose(k, 3, 2, 1e-7).unwrap());
        let a = found(&b, Scope::Full);
        assert_eq!(a, found(&b, Scope::Full));
        assert_eq!(a.spent, found(&b, Scope::Full).spent);
    }
    let (b, _, _) = off_face(2, 10.0 * ACC, Some(ACC));
    assert_eq!(found(&b, Scope::Full), found(&b, Scope::Full));
}

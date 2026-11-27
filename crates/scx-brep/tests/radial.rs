// Written for solidcorex. Apache-2.0.
//! The radial order of an edge's fins (research 63 §6.15): `finish` refuses an edge of more
//! than two fins whose faces do not leave it counter-clockwise in the fins' order, and only
//! such an edge. Judged against the testkit's oracle, which orders fins by its own angles.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use scx_brep::{Document, Draft, EdgeId, FinishCause, GeometryCause};
use scx_testkit::bodies::check::{Tolerance, violations};
use scx_testkit::bodies::{Invariant, Kind, generate};

fn draft(kind: Kind, seed: u64, size: usize) -> Draft {
    generate(kind, seed, size).unwrap().draft
}

fn four_fin_edges(d: &Draft) -> Vec<EdgeId> {
    d.edges()
        .filter(|e| e.fins().len() == 4)
        .map(scx_brep::Edge::id)
        .collect()
}

fn cause(d: Draft) -> Result<(), FinishCause> {
    d.finish(&Document::default())
        .map(|_| ())
        .map_err(|u| u.cause)
}

/// Every order of four, as indices.
fn orders() -> Vec<[usize; 4]> {
    let mut out = Vec::new();
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let o = [a, b, c, d];
                    if (0..4).all(|i| o.contains(&i)) {
                        out.push(o);
                    }
                }
            }
        }
    }
    out
}

#[test]
fn edges_of_four_fins_in_order_finish() {
    for kind in [Kind::Checker, Kind::Touching] {
        for seed in 0..6 {
            for size in 1..=3 {
                let d = draft(kind, seed, size);
                assert_eq!(four_fin_edges(&d).len(), size * size);
                assert_eq!(cause(d), Ok(()), "{} seed {seed} size {size}", kind.name());
            }
        }
    }
}

#[test]
fn two_blocks_sharing_an_edge_out_of_order_disagree_on_sides() {
    // Two solid regions: the sectors about the edge are the outside, one block, the outside,
    // the other. Any order but a rotation puts a block's sector against the wrong face.
    let mut d = draft(Kind::Checker, 3, 1);
    let e = four_fin_edges(&d)[0];
    d.adjacency_mut(e).unwrap().fins.reverse();
    assert_eq!(cause(d).unwrap_err().name(), "sides_disagree");
}

#[test]
fn touching_blocks_out_of_order_are_refused_by_the_geometry() {
    // One solid region: the sectors are the solid and the outside by turns, and a reversal
    // or a swap of opposite fins keeps them so. Only the faces' directions show the order.
    for seed in 0..6 {
        for size in 1..=2 {
            let base = draft(Kind::Touching, seed, size);
            for (i, &e) in four_fin_edges(&base).iter().enumerate() {
                let mut d = base.clone();
                let fins = &mut d.adjacency_mut(e).unwrap().fins;
                if i % 2 == 0 {
                    fins.reverse();
                } else {
                    fins.swap(0, 2);
                }
                assert_eq!(
                    cause(d),
                    Err(FinishCause::Geometry(GeometryCause::RadialOrder {
                        edge: e,
                        at: 0.5
                    })),
                    "seed {seed} size {size} edge {e}"
                );
            }
        }
    }
}

#[test]
fn every_order_of_four_fins_is_judged_as_the_oracle_judges_it() {
    // Truth: the oracle's radial order, from angles it computes alone. A rotation of the
    // list is the same order and finishes; every other order is refused, by the sides
    // where they show it and by the geometry where they do not.
    let mut tally = [0usize; 3];
    for kind in [Kind::Checker, Kind::Touching] {
        for seed in [0, 1, 4] {
            let base = draft(kind, seed, 1);
            let e = four_fin_edges(&base)[0];
            let fins = base.edge(e).unwrap().fins().collect::<Vec<_>>();
            for o in orders() {
                let mut d = base.clone();
                d.adjacency_mut(e).unwrap().fins = o.iter().map(|&k| fins[k]).collect();
                let rotation = (0..4).any(|r| (0..4).all(|i| o[i] == (i + r) % 4));
                let oracle = violations(&d, Tolerance::default());
                assert_eq!(
                    oracle.contains(&Invariant::RadialOrder),
                    !rotation,
                    "{} seed {seed} order {o:?}",
                    kind.name()
                );
                let label = format!("{} seed {seed} order {o:?}", kind.name());
                match cause(d) {
                    Ok(()) => {
                        assert!(rotation, "{label}: finished");
                        tally[0] += 1;
                    }
                    Err(FinishCause::Structure(s)) => {
                        assert!(!rotation, "{label}: {s}");
                        assert_eq!(s.name(), "sides_disagree", "{label}");
                        tally[1] += 1;
                    }
                    Err(FinishCause::Geometry(GeometryCause::RadialOrder { edge, .. })) => {
                        assert!(!rotation && edge == e, "{label}");
                        tally[2] += 1;
                    }
                    Err(other) => panic!("{label}: {other}"),
                }
            }
        }
    }
    eprintln!("finished, sides, radial: {tally:?}");
    // Each way of judging is reached.
    assert!(tally.iter().all(|&n| n > 0), "{tally:?}");
}

#[test]
fn edges_of_two_fins_are_not_the_rules() {
    // Two fins have one cyclic order: either list is it, and the rule never reads them.
    for kind in [Kind::Block, Kind::Cylinder, Kind::Sphere, Kind::QuadSheet] {
        let mut d = draft(kind, 2, 2);
        let edges: Vec<EdgeId> = d.edges().map(scx_brep::Edge::id).collect();
        for e in edges {
            d.adjacency_mut(e).unwrap().fins.reverse();
        }
        assert_eq!(cause(d), Ok(()), "{}", kind.name());
    }
}

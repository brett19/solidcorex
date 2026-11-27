// Written for solidcorex. Apache-2.0.
//! `betti` against the testkit generators' known topology and the makers' primitives.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use scx_bodyq::{Betti, BodyqError, BodyqRefusal, betti};
use scx_brep::{Brep, Document, ShellId};
use scx_testkit::bodies::{Kind, generate};

fn finish(kind: Kind, seed: u64, size: usize) -> (Brep, usize) {
    let g = generate(kind, seed, size).unwrap();
    let body = g
        .draft
        .finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0;
    (body, g.counts.shells)
}

const BALL: Betti = Betti {
    b0: 1,
    b1: 0,
    b2: 1,
};
const TORUS: Betti = Betti {
    b0: 1,
    b1: 2,
    b2: 1,
};

fn all(body: &Brep) -> Vec<(ShellId, Betti)> {
    body.shells()
        .map(|s| (s.id(), betti(body, s.id()).unwrap()))
        .collect()
}

/// Every shell of each simply connected closed generator is a sphere, and each of the
/// handled ones a torus, at several seeds and sizes.
#[test]
fn closed_generators_give_their_genus() {
    let want = [
        (Kind::Block, BALL),
        (Kind::Prism, BALL),
        (Kind::Cylinder, BALL),
        (Kind::Cone, BALL),
        (Kind::Sphere, BALL),
        (Kind::Torus, TORUS),
        (Kind::Holed, TORUS),
        (Kind::Cavity, BALL),
        (Kind::Stack, BALL),
    ];
    for (kind, b) in want {
        for seed in [0, 1, 2] {
            for size in [1, 2, 4] {
                let (body, shells) = finish(kind, seed, size);
                let got = all(&body);
                assert_eq!(got.len(), shells, "{} {seed} {size}: shells", kind.name());
                // The outside shell of a stack is a box's skin, the handled kinds' is
                // theirs too: every shell has the same numbers.
                for (s, got) in got {
                    assert_eq!(got, b, "{} seed {seed} size {size} {s}", kind.name());
                }
            }
        }
    }
}

/// A cavity is one solid region of two shells: both spheres.
#[test]
fn a_cavity_has_two_solid_shells_of_a_sphere() {
    let (body, _) = finish(Kind::Cavity, 0, 2);
    let solid: Vec<_> = body.regions().filter(|r| r.is_solid()).collect();
    assert_eq!(solid.len(), 1);
    let shells: Vec<_> = solid[0].shells().collect();
    assert_eq!(shells.len(), 2);
    for s in shells {
        assert_eq!(betti(&body, s.id()).unwrap(), BALL);
    }
}

/// A sheet is a disc: one component, no loop, no void, whatever its mesh.
#[test]
fn sheets_are_discs() {
    for kind in [Kind::QuadSheet, Kind::TriSheet] {
        for size in [1, 2, 5] {
            let (body, _) = finish(kind, 0, size);
            for (s, got) in all(&body) {
                if body.shell(s).unwrap().faces().next().is_some() {
                    assert_eq!(
                        got,
                        Betti {
                            b0: 1,
                            b1: 0,
                            b2: 0
                        },
                        "{} {size}",
                        kind.name()
                    );
                }
            }
        }
    }
}

/// Blocks touching along an edge. As the checkerboard, each block's own shell is a sphere.
/// As one shell (`Touching`), the `K` blocks are joined at the `n^2` shared edges of four
/// fins: a connected graph of blocks, so `b0 = 1`, `b2 = K` (each block a void), and
/// `b1 = n^2 - K + 1` (the graph's independent cycles).
#[test]
fn touching_blocks_count_each_shells_cells_once() {
    for size in [1, 2, 3, 4] {
        let (body, _) = finish(Kind::Checker, 0, size);
        let solid: Vec<_> = body.regions().filter(|r| r.is_solid()).collect();
        for r in &solid {
            for s in r.shells() {
                assert_eq!(betti(&body, s.id()).unwrap(), BALL, "checker {size}");
            }
        }
        let k = solid.len() as u32;
        let (body, _) = finish(Kind::Touching, 0, size);
        let solid: Vec<_> = body.regions().filter(|r| r.is_solid()).collect();
        assert_eq!(solid.len(), 1);
        let s = solid[0].shells().next().unwrap().id();
        let n = size as u32;
        let want = Betti {
            b0: 1,
            b1: n * n + 1 - k,
            b2: k,
        };
        assert_eq!(betti(&body, s).unwrap(), want, "touching {size}");
    }
}

/// The makers' primitives, with their poles and seams.
#[test]
fn the_makers_bodies() {
    use scx_num::frame::Frame;
    use scx_num::units::Length;
    let f = Frame::WORLD;
    let mm = Length::from_mm;
    let doc = Document::default();
    let bodies = [
        (
            scx_testkit::bodies::made::block(&f, [mm(1.0), mm(2.0), mm(3.0)], &doc)
                .unwrap()
                .body,
            BALL,
        ),
        (
            scx_testkit::bodies::made::cylinder(&f, mm(1.0), mm(2.0), &doc).unwrap().body,
            BALL,
        ),
        (
            scx_testkit::bodies::made::cone(&f, [mm(1.0), mm(0.5)], mm(2.0), &doc)
                .unwrap()
                .body,
            BALL,
        ),
        (
            scx_testkit::bodies::made::cone(&f, [mm(1.0), mm(0.0)], mm(2.0), &doc)
                .unwrap()
                .body,
            BALL,
        ),
        (scx_testkit::bodies::made::sphere(&f, mm(1.0), &doc).unwrap().body, BALL),
        (
            scx_testkit::bodies::made::torus(&f, mm(3.0), mm(1.0), &doc).unwrap().body,
            TORUS,
        ),
    ];
    for (i, (body, b)) in bodies.iter().enumerate() {
        for (s, got) in all(body) {
            assert_eq!(got, *b, "maker {i} {s}");
        }
    }
}

/// Two runs give the same numbers, and a shell the body lacks is refused by name.
#[test]
fn deterministic_and_refuses_a_missing_shell() {
    let (body, _) = finish(Kind::Torus, 1, 3);
    assert_eq!(all(&body), all(&body));
    let missing = ShellId::from_raw(1000);
    assert_eq!(
        betti(&body, missing),
        Err(BodyqError::Refused(BodyqRefusal::NoShell {
            shell: missing
        }))
    );
}

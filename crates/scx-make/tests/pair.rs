// Written for solidcorex. Apache-2.0.
//! `loft::pair` (research 75 §4.1, Q5; task MK9): the exact search against brute force, the
//! band against the exact search, and the pin's loft-pairing outcomes (`loft_sections`'s
//! square to diamond, 5000-gons, split rectangles and combs) as outcomes of the alignment.
//!
//! The brute force here shares no code with the search: it recomputes the cost of a step
//! from the polygons' own vertices (centroid by the shoelace sums, scale to unit area) and
//! enumerates every monotone walk from every start.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use std::f64::consts::{SQRT_2, TAU};

use scx_make::loft::{
    Alignment, Event, MAX_CELLS, Optimality, PairError, PairRefusal, align, align_banded, pair,
};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::vector2d::Point2;
use scx_planar::Segment;

type P = (f64, f64);

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn poly(pts: &[P]) -> Vec<Segment> {
    (0..pts.len())
        .map(|k| {
            let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
            Segment::line(p(a.0, a.1), p(b.0, b.1))
        })
        .collect()
}

fn next(seed: &mut u64) -> f64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (*seed >> 11) as f64 / (1u64 << 53) as f64
}

/// A counter-clockwise star-shaped polygon of `n` vertices about `c`.
fn star(seed: &mut u64, n: usize, c: P) -> Vec<P> {
    (0..n)
        .map(|k| {
            let a = (k as f64 + 0.8 * next(seed)) * TAU / n as f64;
            let r = 5.0 + 10.0 * next(seed);
            (c.0 + r * fmath::cos(a), c.1 + r * fmath::sin(a))
        })
        .collect()
}

fn exact(a: &[Segment], b: &[Segment]) -> Alignment {
    align(a, b, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

// --- An independent cost --------------------------------------------------------------------

/// A polygon at its centroid and unit area, as the cost reads it.
struct Shape {
    at: Vec<P>,
    before: Vec<P>,
    after: Vec<P>,
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn dot(a: P, b: P) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

fn norm(a: P) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: P) -> P {
    let l = norm(a);
    (a.0 / l, a.1 / l)
}

impl Shape {
    fn new(pts: &[P]) -> Self {
        let n = pts.len();
        let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
        for k in 0..n {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            let c = a.0 * b.1 - b.0 * a.1;
            area += c / 2.0;
            cx += (a.0 + b.0) * c / 6.0;
            cy += (a.1 + b.1) * c / 6.0;
        }
        let (c, s) = ((cx / area, cy / area), area.abs().sqrt());
        let at: Vec<P> = pts
            .iter()
            .map(|&q| ((q.0 - c.0) / s, (q.1 - c.1) / s))
            .collect();
        let after: Vec<P> = (0..n).map(|k| unit(sub(at[(k + 1) % n], at[k]))).collect();
        let before = (0..n).map(|k| after[(k + n - 1) % n]).collect();
        Self { at, before, after }
    }

    fn n(&self) -> usize {
        self.at.len()
    }
}

fn turn(a: P, b: P) -> f64 {
    1.0 - dot(a, b)
}

fn paired(x: &Shape, i: usize, y: &Shape, j: usize) -> f64 {
    norm(sub(x.at[i], y.at[j]))
        + 0.5 * (turn(x.before[i], y.before[j]) + turn(x.after[i], y.after[j]))
}

/// Vertex `i` of `x` alone on segment `e` of `y`.
fn alone(x: &Shape, i: usize, y: &Shape, e: usize) -> f64 {
    let (a, b) = (y.at[e], y.at[(e + 1) % y.n()]);
    let d = sub(b, a);
    let t = (dot(sub(x.at[i], a), d) / dot(d, d)).clamp(0.0, 1.0);
    let q = (a.0 + d.0 * t, a.1 + d.1 * t);
    let tan = unit(d);
    norm(sub(x.at[i], q)) + 0.5 * (turn(x.before[i], tan) + turn(x.after[i], tan)) + 0.1
}

/// The cost of an alignment's events, recomputed; and a check that they are a walk round both
/// loops once, from vertex 0 of `x` and vertex `start` of `y`, landing lone vertices on the
/// segment before the other loop's place.
fn cost_of(x: &Shape, y: &Shape, al: &Alignment) -> f64 {
    let (m, n) = (x.n(), y.n());
    let (mut i, mut j, mut c) = (0usize, al.start, 0.0);
    for e in &al.events {
        match *e {
            Event::Pair { first, second } => {
                assert_eq!((first, second), (i, j % n));
                c += paired(x, i, y, j % n);
                (i, j) = (i + 1, j + 1);
            }
            Event::FirstAlone { vertex, on } => {
                assert_eq!((vertex, on), (i, (j + n - 1) % n));
                c += alone(x, i, y, on);
                i += 1;
            }
            Event::SecondAlone { vertex, on } => {
                assert_eq!((vertex, on), (j % n, (i + m - 1) % m));
                c += alone(y, j % n, x, on);
                j += 1;
            }
            _ => panic!("unknown event"),
        }
    }
    assert_eq!((i, j), (m, al.start + n));
    c
}

/// The least cost over every start and every monotone walk.
fn brute(x: &Shape, y: &Shape) -> f64 {
    fn go(x: &Shape, y: &Shape, i: usize, j: usize, s: usize, sofar: f64, best: &mut f64) {
        let (m, n) = (x.n(), y.n());
        if (i, j) == (m, s + n) {
            *best = best.min(sofar);
            return;
        }
        if i < m && j < s + n {
            go(x, y, i + 1, j + 1, s, sofar + paired(x, i, y, j % n), best);
        }
        if i < m {
            let c = alone(x, i, y, (j + n - 1) % n);
            go(x, y, i + 1, j, s, sofar + c, best);
        }
        if j < s + n {
            let c = alone(y, j % n, x, (i + m - 1) % m);
            go(x, y, i, j + 1, s, sofar + c, best);
        }
    }
    let mut best = f64::INFINITY;
    for s in 0..y.n() {
        go(x, y, 0, s, s, 0.0, &mut best);
    }
    best
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * a.abs().max(b.abs()).max(1.0)
}

/// 600 random pairs of star polygons of 3 to 7 vertices: the search's answer is optimal, its
/// cost is the brute-force least to 1e-12, and its events are a walk whose recomputed cost is
/// the reported one.
#[test]
fn the_exact_search_is_the_brute_force_optimum() {
    let mut seed = 0x5eed_u64;
    for case in 0..600 {
        let m = 3 + (next(&mut seed) * 5.0) as usize;
        let n = 3 + (next(&mut seed) * 5.0) as usize;
        let pa = star(&mut seed, m, (0.0, 0.0));
        let off = (next(&mut seed) * 9.0, next(&mut seed) * 9.0);
        let pb = star(&mut seed, n, off);
        let al = exact(&poly(&pa), &poly(&pb));
        assert_eq!(al.optimality, Optimality::Optimal);
        let (x, y) = (Shape::new(&pa), Shape::new(&pb));
        let truth = brute(&x, &y);
        assert!(
            close(al.cost, truth, 1e-12),
            "case {case}, {m} by {n}: {} against {truth}",
            al.cost
        );
        assert!(close(cost_of(&x, &y, &al), al.cost, 1e-12), "case {case}");
    }
}

/// Turning either loop's numbering round leaves the least cost: the search does not depend on
/// where a section's loop begins.
#[test]
fn the_least_cost_does_not_hang_on_where_a_loop_begins() {
    let mut seed = 99u64;
    for _ in 0..100 {
        let (m, n) = (
            3 + (next(&mut seed) * 30.0) as usize,
            3 + (next(&mut seed) * 30.0) as usize,
        );
        let pa = star(&mut seed, m, (0.0, 0.0));
        let pb = star(&mut seed, n, (2.0, -1.0));
        let base = exact(&poly(&pa), &poly(&pb)).cost;
        let (ka, kb) = (
            (next(&mut seed) * m as f64) as usize,
            (next(&mut seed) * n as f64) as usize,
        );
        let mut ra = pa.clone();
        ra.rotate_left(ka);
        let mut rb = pb.clone();
        rb.rotate_left(kb);
        let turned = exact(&poly(&ra), &poly(&rb)).cost;
        assert!(close(base, turned, 1e-9), "{base} against {turned}");
    }
}

/// The band against the exact search on 200 random pairs of 20 to 120 vertices, each band
/// a fifth of the grid wide: the band's cost is never below the optimum (it is an upper bound), and
/// its status says so. How often it equals the optimum and by how much it misses are printed
/// (`--nocapture`); research 75 §13 records them.
#[test]
fn the_band_is_an_upper_bound_on_the_exact_search() {
    let mut seed = 2024u64;
    let (mut equal, mut worst, mut total) = (0, 0.0f64, 0);
    for _ in 0..200 {
        let m = 20 + (next(&mut seed) * 100.0) as usize;
        let n = 20 + (next(&mut seed) * 100.0) as usize;
        let (a, b) = (
            poly(&star(&mut seed, m, (0.0, 0.0))),
            poly(&star(&mut seed, n, (1.0, 3.0))),
        );
        let best = exact(&a, &b);
        // A band of half-width n / 10 columns (at least one).
        let cells = (m + 1) * (2 * (n / 10).max(1) + 1) + 2 * n;
        let band = align_banded(&a, &b, cells, &mut Budget::unlimited(), &Stop::new()).unwrap();
        assert_eq!(band.optimality, Optimality::Unproven);
        assert!(
            band.cost >= best.cost * (1.0 - 1e-12),
            "{} below {}",
            band.cost,
            best.cost
        );
        total += 1;
        if close(band.cost, best.cost, 1e-12) {
            equal += 1;
        }
        worst = worst.max(band.cost / best.cost - 1.0);
    }
    println!(
        "band = exact in {equal} of {total}; worst excess {:.3}%",
        worst * 100.0
    );
    assert!(equal > 0);
}

/// On the shapes the pin's band tests use, at sizes the exact search also runs (regular
/// polygons, split rectangles, combs), a band of a tenth of the grid's columns finds the
/// optimum: the band is not wrong on these, only unproven.
#[test]
fn on_regular_shapes_the_band_finds_the_optimum() {
    let cases = [
        (gon(1000, 1.0), gon(1000, 3.0)),
        (gon(600, 1.0), gon(1000, 1.0)),
        (split_rectangle(false, 997), split_rectangle(true, 997)),
        (comb(10.0, 300, 1.0), comb(1.5, 300, 1.0)),
    ];
    for (k, (a, b)) in cases.iter().enumerate() {
        let best = exact(a, b);
        assert_eq!(best.optimality, Optimality::Optimal);
        let h = b.len() / 10;
        let cells = (a.len() + 1) * (2 * h + 1) + 2 * b.len();
        let band = align_banded(a, b, cells, &mut Budget::unlimited(), &Stop::new()).unwrap();
        assert!(
            close(band.cost, best.cost, 1e-12),
            "case {k}: band {} exact {}",
            band.cost,
            best.cost
        );
    }
}

/// Equal shapes at different sizes and places, numbered alike, pair vertex for vertex from
/// start 0 at no distance: the cost is invariant under moving and scaling a section.
#[test]
fn a_section_and_its_moved_scaled_copy_pair_vertex_for_vertex() {
    let mut seed = 5u64;
    let pa = star(&mut seed, 40, (0.0, 0.0));
    let pb: Vec<P> = pa
        .iter()
        .map(|q| (3.0 * q.0 + 7.0, 3.0 * q.1 - 2.0))
        .collect();
    let al = exact(&poly(&pa), &poly(&pb));
    assert_eq!(al.start, 0);
    assert!(al.cost.abs() < 1e-12, "{}", al.cost);
    for (k, e) in al.events.iter().enumerate() {
        assert_eq!(
            *e,
            Event::Pair {
                first: k,
                second: k
            }
        );
    }
}

// --- The pin's outcomes ------------------------------------------------------------------------

/// The second section's vertex paired with the first's vertex `i`, if it is paired.
fn partner(al: &Alignment, i: usize) -> Option<usize> {
    al.events.iter().find_map(|e| match *e {
        Event::Pair { first, second } if first == i => Some(second),
        _ => None,
    })
}

fn near(a: Point2, b: P) -> bool {
    fmath::hypot(a.x - b.0, a.y - b.1) < 1e-9
}

/// A square to a diamond with a vertex in the middle of each side (pin:
/// `a_square_to_a_diamond_runs_its_corners_straight_up`): each corner of the square pairs with
/// the diamond's vertex in its direction, not with a corner of the diamond.
#[test]
fn a_square_to_a_diamond_runs_its_corners_straight_up() {
    let (r, h) = (SQRT_2, SQRT_2 / 2.0);
    let square = poly(&[(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]);
    let diamond = poly(&[
        (r, 0.0),
        (h, h),
        (0.0, r),
        (-h, h),
        (-r, 0.0),
        (-h, -h),
        (0.0, -r),
        (h, -h),
    ]);
    let al = exact(&square, &diamond);
    for (i, s) in square.iter().enumerate() {
        let c = s.start();
        let j = partner(&al, i).unwrap();
        assert!(
            near(diamond[j].start(), (c.x * h, c.y * h)),
            "{c:?} runs to {j}"
        );
    }
}

/// The regular `n`-gon of radius `r` about the origin, from (r, 0).
fn gon(n: usize, r: f64) -> Vec<Segment> {
    let pts: Vec<P> = (0..n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64;
            (r * fmath::cos(a), r * fmath::sin(a))
        })
        .collect();
    poly(&pts)
}

/// Past [`MAX_CELLS`], in the band (pin: `sections_too_large_for_the_whole_grid_are_paired_in_a_band`):
/// two 5000-gons pair vertex for vertex, and a 3000-gon with a 5000-gon pairs each of the
/// 3000's vertices with a vertex within one of its steps round.
#[test]
fn sections_past_the_limit_are_paired_in_the_band() {
    let al = exact(&gon(5000, 10.0), &gon(5000, 20.0));
    assert_eq!(al.optimality, Optimality::Unproven);
    assert!(al.cost.abs() < 1e-9, "{}", al.cost);
    let start = al.start;
    for (k, e) in al.events.iter().enumerate() {
        assert_eq!(
            *e,
            Event::Pair {
                first: k,
                second: (k + start) % 5000
            }
        );
    }
    assert_eq!(start, 0);
    let (a, b) = (gon(3000, 10.0), gon(5000, 10.0));
    let al = exact(&a, &b);
    assert_eq!(al.optimality, Optimality::Unproven);
    for (i, s) in a.iter().enumerate() {
        let Some(j) = partner(&al, i) else { continue };
        let (u, v) = (s.start(), b[j].start());
        let t = (fmath::atan2(v.y, v.x) - fmath::atan2(u.y, u.x)).rem_euclid(TAU);
        assert!(t.min(TAU - t) < TAU / 3000.0, "{i} runs to {j}");
    }
}

/// The 40 x 20 rectangle in 5000 segments, `split` of them along its bottom or its top.
fn split_rectangle(top: bool, split: usize) -> Vec<Segment> {
    let mut pts = vec![(0.0, 0.0)];
    let along = |k: usize| 40.0 * k as f64 / split as f64;
    if top {
        pts.extend([(40.0, 0.0), (40.0, 20.0)]);
        pts.extend((1..split).map(|k| (40.0 - along(k), 20.0)));
        pts.push((0.0, 20.0));
    } else {
        pts.extend((1..split).map(|k| (along(k), 0.0)));
        pts.extend([(40.0, 0.0), (40.0, 20.0), (0.0, 20.0)]);
    }
    poly(&pts)
}

fn vertex_at(s: &[Segment], c: P) -> usize {
    s.iter().position(|q| near(q.start(), c)).unwrap()
}

/// Two 5000-segment rectangles, one split along its bottom and one along its top (pin:
/// `corners_pair_in_sections_too_large_for_the_whole_grid`), pair all four corners in the band.
#[test]
fn split_rectangles_past_the_limit_pair_their_corners() {
    let (a, b) = (split_rectangle(false, 4997), split_rectangle(true, 4997));
    let al = exact(&a, &b);
    assert_eq!(al.optimality, Optimality::Unproven);
    for c in [(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0)] {
        assert_eq!(
            partner(&al, vertex_at(&a, c)),
            Some(vertex_at(&b, c)),
            "{c:?}"
        );
    }
}

/// A `w` x 1 rectangle whose bottom is a comb of `teeth` teeth 0.1 deep.
fn comb(w: f64, teeth: usize, height: f64) -> Vec<Segment> {
    let mut pts: Vec<P> = (0..2 * teeth)
        .map(|k| {
            (
                w * k as f64 / (2 * teeth) as f64,
                if k % 2 == 1 { -0.1 } else { 0.0 },
            )
        })
        .collect();
    pts.extend([(w, 0.0), (w, height), (0.0, height)]);
    poly(&pts)
}

/// Combs of 1100 teeth (2203 corners each, so even the corner polygons are past the limit)
/// pair tooth by tooth and corner by corner (pin: `combs_of_more_corners_than_the_grid_bound_pair_tooth_by_tooth`
/// and `combs_of_different_proportions_pair_tooth_by_tooth`, the second with sides 200 high,
/// where the shares put the teeth some 1300 vertices from their partners).
#[test]
fn combs_past_the_limit_pair_tooth_by_tooth() {
    let teeth = 1100;
    for height in [1.0, 200.0] {
        let (a, b) = (comb(10.0, teeth, 1.0), comb(1.5, teeth, height));
        assert!((a.len() + 1) * (b.len() + 1) > MAX_CELLS);
        let al = exact(&a, &b);
        assert_eq!(al.optimality, Optimality::Unproven);
        let tip = |w: f64, k: usize| (w * (2 * k + 1) as f64 / (2 * teeth) as f64, -0.1);
        let pairs = (0..teeth).map(|k| (tip(10.0, k), tip(1.5, k))).chain([
            ((10.0, 0.0), (1.5, 0.0)),
            ((10.0, 1.0), (1.5, height)),
            ((0.0, 1.0), (0.0, height)),
        ]);
        for (x, y) in pairs {
            assert_eq!(
                partner(&al, vertex_at(&a, x)),
                Some(vertex_at(&b, y)),
                "{x:?}, height {height}"
            );
        }
    }
}

// --- Arcs, several sections, refusals ------------------------------------------------------------

/// A disc as one circle and as four quarter arcs: at its centroid and unit area either way, so
/// a circle paired with a circle costs nothing, and four quarters with four quarters likewise.
#[test]
fn circles_and_arcs_pair_at_no_cost_with_their_copies() {
    let c = p(3.0, 4.0);
    let one = [Segment::circle(c, 2.0, Sense::Forward)];
    let big = [Segment::circle(p(-1.0, 0.0), 5.0, Sense::Forward)];
    let al = exact(&one, &big);
    assert!(al.cost.abs() < 1e-12, "{}", al.cost);
    assert_eq!(
        al.events,
        vec![Event::Pair {
            first: 0,
            second: 0
        }]
    );
    let quarters = |c: Point2, r: f64| -> Vec<Segment> {
        let at = |k: usize| {
            let a = TAU * k as f64 / 4.0;
            p(c.x + r * fmath::cos(a), c.y + r * fmath::sin(a))
        };
        (0..4)
            .map(|k| Segment::arc(c, at(k), at(k + 1), Sense::Forward))
            .collect()
    };
    let al = exact(&quarters(c, 2.0), &quarters(p(0.0, 0.0), 7.0));
    assert!(al.cost.abs() < 1e-9, "{}", al.cost);
    assert_eq!(al.start, 0);
}

/// Three sections pair each with the next, in order.
#[test]
fn a_loft_of_three_sections_pairs_each_with_the_next() {
    let (a, b, c) = (gon(4, 1.0), gon(8, 2.0), gon(6, 1.0));
    let got = pair(&[&a, &b, &c], &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(got.alignments.len(), 2);
    assert!(got.is_optimal());
    assert_eq!(got.alignments[0], exact(&a, &b));
    assert_eq!(got.alignments[1], exact(&b, &c));
    let one = pair(&[&a], &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert!(one.alignments.is_empty());
}

#[test]
fn wrong_sections_are_refused_by_cause() {
    let (mut budget, stop) = (Budget::unlimited(), Stop::new());
    let sq = gon(4, 1.0);
    let refused = |r: Result<_, PairError>| match r {
        Err(PairError::Refused(why)) => why,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        refused(align(&[], &sq, &mut budget, &stop)),
        PairRefusal::Empty { section: 0 }
    );
    let mut bad = sq.clone();
    bad[2] = Segment::line(p(f64::NAN, 0.0), p(1.0, 1.0));
    assert_eq!(
        refused(align(&sq, &bad, &mut budget, &stop)),
        PairRefusal::NotFinite {
            section: 1,
            segment: 2
        }
    );
    let mut point = sq.clone();
    point.insert(1, Segment::line(sq[1].start(), sq[1].start()));
    assert_eq!(
        refused(align(&point, &sq, &mut budget, &stop)),
        PairRefusal::Degenerate {
            section: 0,
            segment: 1
        }
    );
    let flat = poly(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]);
    assert_eq!(
        refused(align(&sq, &flat, &mut budget, &stop)),
        PairRefusal::NoArea { section: 1 }
    );
    let back: Vec<Segment> = sq.iter().rev().map(Segment::reversed).collect();
    assert_eq!(
        refused(align(&sq, &back, &mut budget, &stop)),
        PairRefusal::Winding { section: 1 }
    );
    let e = pair(&[&sq, &sq, &back], &mut budget, &stop).unwrap_err();
    assert_eq!(e, PairError::Refused(PairRefusal::Winding { section: 2 }));
    assert_eq!((e.kind(), e.name()), (ErrorKind::Refused, "winding"));
}

/// The budget is charged a unit a cell, and the flag ends the search.
#[test]
fn the_budget_and_the_flag_end_the_search() {
    let (a, b) = (gon(30, 1.0), gon(40, 1.0));
    let mut budget = Budget::unlimited();
    align(&a, &b, &mut budget, &Stop::new()).unwrap();
    // At least the whole grid once; at most once for the first start and once a level of the
    // halving of the starts (6 levels for 40), each level's walks overlapping by a row each.
    let (cells, used) = (31u64 * 41, budget.used());
    assert!(used >= cells && used <= cells * 8, "{used}");
    let e = align(&a, &b, &mut Budget::new(cells), &Stop::new()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Exhausted);
    let stop = Stop::new();
    stop.set();
    let e = align(&a, &b, &mut Budget::unlimited(), &stop).unwrap_err();
    assert_eq!(e, PairError::Stopped(scx_num::budget::Stopped));
    assert_eq!(e.kind(), ErrorKind::Cancelled);
}

/// A band one column wide does not fit: refused as too large, not answered wrongly.
#[test]
fn a_band_too_narrow_to_fit_is_too_large() {
    let (a, b) = (gon(100, 1.0), gon(100, 1.0));
    let e = align_banded(&a, &b, 300, &mut Budget::unlimited(), &Stop::new()).unwrap_err();
    assert_eq!(
        e,
        PairError::TooLarge {
            first: 100,
            second: 100
        }
    );
    assert_eq!(e.kind(), ErrorKind::Unsupported);
}

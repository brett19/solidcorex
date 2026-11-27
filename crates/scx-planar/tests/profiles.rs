// Written for solidcorex. Apache-2.0.
//! Profiles beyond the fixtures: tangent joints, collinear edges, same-circle arcs,
//! contacts between loops, distances at the resolution, the order of the checks, and
//! large loops.

use core::f64::consts::PI;

use scx_num::fmath;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_planar::{Degenerate, LoopId, PlanarError, Region, Segment, Sense};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn res(r: f64) -> Resolution {
    Resolution::new(Length::from_mm(r)).unwrap_or(Resolution::DEFAULT)
}

fn polygon(pts: &[Point2]) -> Vec<Segment> {
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(a, b)| Segment::line(*a, *b))
        .collect()
}

fn reversed(l: &[Segment]) -> Vec<Segment> {
    l.iter().rev().map(Segment::reversed).collect()
}

fn circle(c: Point2, r: f64) -> Vec<Segment> {
    let (e, w) = (p(c.x + r, c.y), p(c.x - r, c.y));
    vec![
        Segment::arc(c, e, w, Sense::Forward),
        Segment::arc(c, w, e, Sense::Forward),
    ]
}

fn build(outer: Vec<Segment>, holes: Vec<Vec<Segment>>) -> Result<Region, PlanarError> {
    Region::new(outer, holes, res(1e-9))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * b.abs().max(1.0)
}

/// A `w` x `h` rectangle at the origin with corners rounded to radius `r`: lines tangent
/// to quarter arcs at every joint.
fn rounded_rectangle(w: f64, h: f64, r: f64) -> Vec<Segment> {
    let q = |c: Point2, a: Point2, b: Point2| Segment::arc(c, a, b, Sense::Forward);
    vec![
        Segment::line(p(r, 0.0), p(w - r, 0.0)),
        q(p(w - r, r), p(w - r, 0.0), p(w, r)),
        Segment::line(p(w, r), p(w, h - r)),
        q(p(w - r, h - r), p(w, h - r), p(w - r, h)),
        Segment::line(p(w - r, h), p(r, h)),
        q(p(r, h - r), p(r, h), p(0.0, h - r)),
        Segment::line(p(0.0, h - r), p(0.0, r)),
        q(p(r, r), p(0.0, r), p(r, 0.0)),
    ]
}

#[test]
fn tangent_joints_are_joints() {
    let r = build(rounded_rectangle(10.0, 6.0, 1.5), vec![]);
    let r = r.unwrap_or_else(|e| unreachable!("{e}"));
    assert!(
        close(r.area(), 60.0 - (4.0 - PI) * 1.5 * 1.5),
        "{}",
        r.area()
    );
    // A slot: two half circles joined by tangent lines.
    let slot = vec![
        Segment::line(p(0.0, 0.0), p(8.0, 0.0)),
        Segment::arc(p(8.0, 2.0), p(8.0, 0.0), p(8.0, 4.0), Sense::Forward),
        Segment::line(p(8.0, 4.0), p(0.0, 4.0)),
        Segment::arc(p(0.0, 2.0), p(0.0, 4.0), p(0.0, 0.0), Sense::Forward),
    ];
    let s = build(slot, vec![]).unwrap_or_else(|e| unreachable!("{e}"));
    assert!(close(s.area(), 32.0 + 4.0 * PI), "{}", s.area());
    // Two arcs tangent at their joint, bulging opposite ways: an S-bend closed by lines.
    let bend = vec![
        Segment::arc(p(0.0, 5.0), p(-5.0, 5.0), p(0.0, 0.0), Sense::Forward),
        Segment::arc(p(0.0, -5.0), p(0.0, 0.0), p(5.0, -5.0), Sense::Reversed),
        Segment::line(p(5.0, -5.0), p(5.0, -10.0)),
        Segment::line(p(5.0, -10.0), p(-10.0, -10.0)),
        Segment::line(p(-10.0, -10.0), p(-10.0, 5.0)),
        Segment::line(p(-10.0, 5.0), p(-5.0, 5.0)),
    ];
    assert!(build(bend, vec![]).is_ok());
}

#[test]
fn collinear_edges_apart_are_apart() {
    // A notch in the top edge leaves two collinear pieces of it, apart.
    let notched = polygon(&[
        p(0.0, 0.0),
        p(10.0, 0.0),
        p(10.0, 10.0),
        p(7.0, 10.0),
        p(7.0, 6.0),
        p(3.0, 6.0),
        p(3.0, 10.0),
        p(0.0, 10.0),
    ]);
    assert!(build(notched, vec![]).is_ok());
    // A straight joint: three collinear points in a row.
    let straight = polygon(&[
        p(0.0, 0.0),
        p(5.0, 0.0),
        p(10.0, 0.0),
        p(10.0, 10.0),
        p(0.0, 10.0),
    ]);
    assert!(build(straight, vec![]).is_ok());
    // A spike folding back along its own edge.
    let folded = polygon(&[p(0.0, 0.0), p(10.0, 0.0), p(5.0, 0.0), p(5.0, 10.0)]);
    assert!(matches!(
        build(folded, vec![]),
        Err(PlanarError::Touching { .. })
    ));
}

#[test]
fn arcs_on_one_circle() {
    // Four quarter arcs of one circle: adjacent arcs share a circle, opposite ones too.
    let q = |a: Point2, b: Point2| Segment::arc(p(0.0, 0.0), a, b, Sense::Forward);
    let four = vec![
        q(p(3.0, 0.0), p(0.0, 3.0)),
        q(p(0.0, 3.0), p(-3.0, 0.0)),
        q(p(-3.0, 0.0), p(0.0, -3.0)),
        q(p(0.0, -3.0), p(3.0, 0.0)),
    ];
    let r = build(four, vec![]).unwrap_or_else(|e| unreachable!("{e}"));
    assert!(close(r.area(), 9.0 * PI));
    // An arc retraced: there and back along the same half circle.
    let back = vec![
        Segment::arc(p(0.0, 0.0), p(3.0, 0.0), p(-3.0, 0.0), Sense::Forward),
        Segment::arc(p(0.0, 0.0), p(-3.0, 0.0), p(3.0, 0.0), Sense::Reversed),
    ];
    assert!(matches!(
        build(back, vec![]),
        Err(PlanarError::Touching { .. })
    ));
    // An annulus: a concentric round hole.
    let ring = build(
        circle(p(0.0, 0.0), 5.0),
        vec![reversed(&circle(p(0.0, 0.0), 2.0))],
    );
    let ring = ring.unwrap_or_else(|e| unreachable!("{e}"));
    assert!(close(ring.area(), 21.0 * PI));
    // A hole on the outer loop's own circle, inside it by a hair less than the resolution.
    let near = circle(p(0.0, 0.0), 5.0 - 5e-10);
    assert!(matches!(
        build(circle(p(0.0, 0.0), 5.0), vec![near]),
        Err(PlanarError::Touching { .. })
    ));
}

#[test]
fn contacts_between_loops() {
    let plate = || polygon(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]);
    let sq = |x: f64, y: f64, s: f64| {
        reversed(&polygon(&[
            p(x, y),
            p(x + s, y),
            p(x + s, y + s),
            p(x, y + s),
        ]))
    };
    // Two holes sharing an edge's stretch.
    let e = build(plate(), vec![sq(1.0, 1.0, 3.0), sq(4.0, 2.0, 3.0)]).err();
    assert!(
        matches!(
            e,
            Some(PlanarError::Touching { a, b }) if a.in_loop == LoopId::Hole(0) && b.in_loop == LoopId::Hole(1)
        ),
        "{e:?}"
    );
    // Two holes overlapping: their edges cross.
    let e = build(plate(), vec![sq(1.0, 1.0, 3.0), sq(3.0, 3.0, 3.0)]).err();
    assert!(matches!(e, Some(PlanarError::Crossing { .. })), "{e:?}");
    // A hole across the outer loop.
    let e = build(plate(), vec![sq(8.0, 4.0, 4.0)]).err();
    assert!(matches!(e, Some(PlanarError::Crossing { .. })), "{e:?}");
    // A round hole tangent to the outer loop's edge from inside.
    let e = build(plate(), vec![reversed(&circle(p(5.0, 3.0), 3.0))]).err();
    assert!(matches!(e, Some(PlanarError::Touching { .. })), "{e:?}");
}

#[test]
fn a_vertex_at_the_resolution_from_an_edge() {
    // A notch whose bottom vertex comes `d` from the bottom edge.
    let notch = |d: f64| {
        polygon(&[
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 10.0),
            p(5.0, d),
            p(0.0, 10.0),
        ])
    };
    assert!(build(notch(2e-9), vec![]).is_ok());
    assert!(matches!(
        build(notch(5e-10), vec![]),
        Err(PlanarError::Touching { .. })
    ));
    // Exactly on it, in coordinates that are not dyadic.
    let t = 0.1;
    let on = polygon(&[
        p(0.0, 0.0),
        p(1.0, 3.0),
        p(1.0, 5.0),
        p(t, 3.0 * t),
        p(-2.0, 1.0),
    ]);
    assert!(matches!(
        build(on, vec![]),
        Err(PlanarError::Touching { .. })
    ));
    // Two edges crossing by less than the resolution: proved crossing, exactly.
    let x = polygon(&[
        p(0.0, 0.0),
        p(10.0, 0.0),
        p(10.0, 10.0),
        p(5.0, -1e-12),
        p(0.0, 10.0),
    ]);
    assert!(matches!(
        build(x, vec![]),
        Err(PlanarError::Crossing { .. })
    ));
}

#[test]
fn the_checks_run_in_order() {
    // A NaN in an open loop: the number is judged before closure.
    let mut l = polygon(&[p(0.0, 0.0), p(4.0, 0.0), p(4.0, 4.0)]);
    l.pop();
    l.push(Segment::line(p(4.0, 4.0), p(f64::NAN, 1.0)));
    assert!(matches!(
        build(l, vec![]),
        Err(PlanarError::NotFinite { .. })
    ));
    // A bow tie left open: Open, not Crossing.
    let mut bow = polygon(&[p(0.0, 0.0), p(4.0, 4.0), p(4.0, 0.0), p(0.0, 4.0)]);
    if let Some(last) = bow.last_mut() {
        *last = Segment::line(p(0.0, 4.0), p(0.0, 1e-3));
    }
    assert!(matches!(
        build(bow.clone(), vec![]),
        Err(PlanarError::Open { joint: 3, .. })
    ));
    // The same with a zero-length segment as well: still Open.
    bow.insert(1, Segment::line(p(4.0, 4.0), p(4.0, 4.0)));
    assert!(matches!(
        build(bow, vec![]),
        Err(PlanarError::Open { joint: 4, .. })
    ));
    // Degenerate before contacts: a crossing loop with a zero-length segment.
    let mut d = polygon(&[p(0.0, 0.0), p(4.0, 4.0), p(4.0, 0.0), p(0.0, 4.0)]);
    d.insert(1, Segment::line(p(4.0, 4.0), p(4.0, 4.0)));
    if let Some(first) = d.first_mut() {
        *first = Segment::line(p(0.0, 0.0), p(4.0, 4.0));
    }
    assert!(matches!(
        build(d, vec![]),
        Err(PlanarError::Degenerate {
            why: Degenerate::Short,
            ..
        })
    ));
    // An arc whose ends are on different circles.
    let off = vec![
        Segment::arc(p(0.0, 0.0), p(5.0, 0.0), p(-5.5, 0.0), Sense::Forward),
        Segment::line(p(-5.5, 0.0), p(5.0, 0.0)),
    ];
    assert!(matches!(
        build(off, vec![]),
        Err(PlanarError::Degenerate {
            why: Degenerate::EndOffCircle,
            ..
        })
    ));
    // A coordinate out of range, and an empty hole.
    let far = polygon(&[p(0.0, 0.0), p(2e9, 0.0), p(0.0, 1.0)]);
    assert!(matches!(
        build(far, vec![]),
        Err(PlanarError::OutOfRange { .. })
    ));
    let sq = polygon(&[p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)]);
    assert!(matches!(
        build(sq, vec![vec![]]),
        Err(PlanarError::Empty {
            in_loop: LoopId::Hole(0)
        })
    ));
}

#[test]
fn joints_are_made_exact() {
    let gap = f64::from_bits(0x3D70_0000_0000_0000);
    let l = polygon(&[p(0.0, 0.0), p(4.0, 0.0), p(4.0, 4.0), p(0.0, 4.0)]);
    let mut l = l;
    if let Some(last) = l.last_mut() {
        *last = Segment::line(p(0.0, 4.0), p(0.0, gap));
    }
    let r = build(l, vec![]).unwrap_or_else(|e| unreachable!("{e}"));
    let s = r.outer().segments();
    for (a, b) in s.iter().zip(s.iter().cycle().skip(1)) {
        assert_eq!(a.end(), b.start());
    }
}

/// A regular `n`-gon of radius `r`, the vertices from `fmath`.
fn ngon(n: usize, r: f64) -> Vec<Segment> {
    let pts: Vec<Point2> = (0..n)
        .map(|i| {
            let t = 2.0 * PI * i as f64 / n as f64;
            p(r * fmath::cos(t), r * fmath::sin(t))
        })
        .collect();
    polygon(&pts)
}

/// A comb of `teeth` teeth, each 1 wide and 10 tall with gaps of 1: `4 teeth + 2` segments,
/// every tooth's long edges overlapping every other's in `x` range only near it.
fn comb(teeth: usize) -> Vec<Segment> {
    let mut pts = vec![p(0.0, 0.0)];
    let w = 2.0 * teeth as f64;
    pts.push(p(w, 0.0));
    for i in (0..teeth).rev() {
        let x = 2.0 * i as f64;
        pts.extend([
            p(x + 2.0, 10.0),
            p(x + 1.0, 10.0),
            p(x + 1.0, 1.0),
            p(x, 1.0),
        ]);
    }
    pts.pop();
    polygon(&pts)
}

#[test]
fn large_loops() {
    let r = build(ngon(1000, 50.0), vec![]).unwrap_or_else(|e| unreachable!("{e}"));
    let want = 0.5 * 1000.0 * 2500.0 * fmath::sin(2.0 * PI / 1000.0);
    assert!(
        (r.area() - want).abs() <= 1e-9 * want,
        "{} {want}",
        r.area()
    );
    let c = build(comb(250), vec![]);
    assert!(c.is_ok(), "{c:?}");
    // The same comb with one tooth bent onto its neighbour.
    let mut bent = comb(250);
    if let Some(s) = bent.get_mut(500) {
        let (a, b) = (s.start(), s.end());
        *s = Segment::line(a, p(b.x - 1.0 - 1e-12, b.y));
    }
    assert!(build(bent, vec![]).is_err());
}

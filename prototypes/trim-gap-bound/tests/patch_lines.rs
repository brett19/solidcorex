//! Pieces that cross a patch line of a C^1 net (review-55, P1 and P2), and trims that touch
//! the domain's boundary off an iso line (P4).
//!
//! The net has two patches in `u`, `[0, p]` and `[p, 2]`, with `x = u`, `y = v` and
//! `z = 0` on the left patch, `z = -k (u - p)^2` on the right (cubic Hermite reproduces the
//! quadratic). It is C^1 across `u = p`, and its `z_uu` jumps from 0 to `-2k` there. The trim
//! is the straight line `(t, 1/2)` on `[0, 2]`, one span, so every piece the bisection starts
//! from crosses the line.

use core::f64::consts::PI;

use scx_trim_gap_bound::curve::Hermite;
use scx_trim_gap_bound::gap::{frame_f64, offset_point, Method, Problem, Stop};
use scx_trim_gap_bound::jet::Jet;
use scx_trim_gap_bound::surface::HermiteNet;
use scx_trim_gap_bound::cases;

const P: f64 = 1.1;

fn kinked_net(k: f64) -> HermiteNet {
    let ku = vec![0.0, P, 2.0];
    let kv = vec![0.0, 1.0];
    let mut node = Vec::new();
    for &u in &ku {
        for &v in &kv {
            let w = (u - P).max(0.0);
            node.push([[u, v, -k * w * w], [1.0, 0.0, -2.0 * k * w], [0.0, 1.0, 0.0], [0.0; 3]]);
        }
    }
    HermiteNet { ku, kv, node }
}

/// `(t, 1/2)` on `[0, t1]`, one span.
fn straight_trim(t1: f64) -> Hermite<2> {
    Hermite { t: vec![0.0, t1], p: vec![[0.0, 0.5], [t1, 0.5]], m: vec![[1.0, 0.0], [1.0, 0.0]] }
}

/// `(t, 1/2, -l t)` on `[0, t1]`, one span: on the trim's image `g_z = z(t) + l t`.
fn sloped_edge(l: f64, t1: f64) -> Hermite<3> {
    Hermite { t: vec![0.0, t1], p: vec![[0.0, 0.5, 0.0], [t1, 0.5, -l * t1]], m: vec![[1.0, 0.0, -l]; 2] }
}

/// The edge as the straight line between two points, on `[0, 2]`.
fn chord(a: [f64; 3], b: [f64; 3]) -> Hermite<3> {
    let s = [(b[0] - a[0]) / 2.0, (b[1] - a[1]) / 2.0, (b[2] - a[2]) / 2.0];
    Hermite { t: vec![0.0, 2.0], p: vec![a, b], m: vec![s, s] }
}

/// Pieces `[a, b]` around the line: wide and narrow, the middle on either side of it.
fn straddling(t1: f64) -> Vec<[f64; 2]> {
    let mut out = vec![[0.0, t1]];
    for a in [0.0, 0.5, 0.8, 0.9, 1.0, 1.05, 1.07, 1.075, 1.08, 1.09, 1.099] {
        for b in [1.101, 1.11, 1.112, 1.115, 1.12, 1.15, 1.2, 1.3, 1.4, 1.6, 2.0] {
            if b <= t1 {
                out.push([a, b]);
            }
        }
    }
    out
}

fn every_piece_holds(name: &str, p: &Problem<HermiteNet>, pieces: &[[f64; 2]], methods: &[Method]) {
    for &[a, b] in pieces {
        let s = p.sampled_on(a, b, 4000);
        for &m in methods {
            let up = p.piece_bound(a, b, m);
            assert!(up >= s, "{name} {m:?} on [{a}, {b}]: piece bound {up} below sampled {s}");
        }
    }
}

/// P1: review-55's counter-example. `g_z(t) = l t - (t - p)_+^2` with `l = (2 - p)^2 / 2`, so
/// `g(0) = g(2) = 0` and the true gap is `l p + l^2 / 4 = 0.4865` at `t = p + l / 2`. On a
/// piece whose middle is on the flat side, `|g''(m)| = 0` and `g'''` is 0 on both sides, so
/// the centred form `|g''(m)| + h/2 sup|g'''|` misses the jump of `g''` at the line.
#[test]
fn centred_form_holds_across_a_c1_line() {
    let l = (2.0 - P) * (2.0 - P) / 2.0;
    let p = Problem {
        surf: kinked_net(1.0),
        d: 0.0,
        trim: straight_trim(2.0),
        edge: sloped_edge(l, 2.0),
    };
    let (s, at) = p.sampled_max(50_000);
    let expected = l * P + l * l / 4.0;
    assert!((s - expected).abs() < 1e-9 && (at - (P + l / 2.0)).abs() < 1e-4, "sampled {s} at {at}");
    every_piece_holds("P1", &p, &straddling(2.0), &[Method::Centred, Method::Second]);
    for m in [Method::Centred, Method::Second] {
        let o = p.certify(m, Stop::Tight);
        assert!(o.bound >= s && o.lower <= s * (1.0 + 1e-9), "P1 {m:?}: bound {} lower {} sampled {s}", o.bound, o.lower);
        assert!(o.resolved && o.bound <= 1.01 * s, "P1 {m:?}: {o:?}");
    }
}

/// P1 with the peak close to the line, which is where the prototype's own enclosures show
/// the flaw (on the construction above the jets' overestimation covers it). `l = 0.002`, so
/// the gap peaks at `t = p + delta`, `delta = l/2 = 0.001`; the trim stops at `t = 1.15`, so
/// that peak is the largest gap. A piece `[p - w1, p + w2]` with `delta < w2 < w1` holds the
/// peak and has its middle on the flat side. Its maximum is inside, `(w2 - delta)^2` above
/// its larger end, while the centred form adds `h^2/8 (|g''(m)| + h/2 sup|g'''|)` with
/// `g''(m) = 0` and `sup|g'''|` only the jets' overestimation, `O(h)`: a fourth-order term.
/// The old code (centred form on every piece) fails on these pieces.
#[test]
fn centred_form_holds_across_a_c1_line_near_its_peak() {
    let (l, t1) = (0.002, 1.15);
    let p = Problem { surf: kinked_net(1.0), d: 0.0, trim: straight_trim(t1), edge: sloped_edge(l, t1) };
    let (s, at) = p.sampled_max(50_000);
    assert!((s - (l * P + l * l / 4.0)).abs() < 1e-12 && (at - (P + l / 2.0)).abs() < 1e-5, "sampled {s} at {at}");
    let mut pieces = Vec::new();
    for w1 in [2.5e-3, 3e-3, 4e-3, 6e-3] {
        for w2 in [1.5e-3, 2e-3, 2.4e-3, 3e-3] {
            if w2 < w1 {
                pieces.push([P - w1, P + w2]);
            }
        }
    }
    every_piece_holds("P1 near peak", &p, &pieces, &[Method::Centred, Method::Second]);
    for m in [Method::Centred, Method::Second] {
        let o = p.certify(m, Stop::Tight);
        assert!(o.bound >= s && o.lower <= s * (1.0 + 1e-9), "P1 near peak {m:?}: bound {} lower {} sampled {s}", o.bound, o.lower);
        assert!(o.resolved && o.bound <= 1.01 * s, "P1 near peak {m:?}: {o:?}");
    }
}

/// P2: the offset of the same net at distance `d`. The normal's `u` derivative jumps with
/// `z_uu` at the line, so `g'` jumps (a kink in `g_x`), and no second-order form holds on a
/// piece across it. The edge is the chord of `Q(c(t))` between the ends.
#[test]
fn offset_across_a_c1_line_holds() {
    let (k, d) = (0.1, 2.0);
    let surf = kinked_net(k);
    let q = |u: f64| {
        let f = frame_f64(&surf, Jet::of(u), Jet::of(0.5));
        let q = offset_point(&f, d);
        [q[0].v, q[1].v, q[2].v]
    };
    let edge = chord(q(0.0), q(2.0));
    let p = Problem { surf: surf.clone(), d, trim: straight_trim(2.0), edge };
    let (s, at) = p.sampled_max(50_000);
    assert!(s > 0.1 && (at - P).abs() < 1e-3, "the gap peaks at the kink: {s} at {at}");
    every_piece_holds("P2", &p, &straddling(2.0), &[Method::Centred, Method::Second, Method::Lipschitz]);
    for m in [Method::Centred, Method::Second] {
        let o = p.certify(m, Stop::Tight);
        assert!(o.bound >= s && o.lower <= s * (1.0 + 1e-9), "P2 {m:?}: bound {} lower {} sampled {s}", o.bound, o.lower);
        assert!(o.resolved && o.bound <= 1.01 * s, "P2 {m:?}: {o:?}");
    }
}

/// P4: a trim that starts on `u = 0` and ends on `u = 1` of the wavy patch, off any iso line.
/// The interval evaluation of the trim near either end leaves the domain by its
/// overestimation, at every depth. The bound must still resolve.
#[test]
fn trim_touching_the_boundary_resolves() {
    let gamma = |t: f64| ([t, 0.3 + 0.4 * t + 0.1 * (PI * t).sin()], [1.0, 0.4 + 0.1 * PI * (PI * t).cos()]);
    let p = Problem::fitted(cases::wavy_patch(), 0.05, gamma, [0.0, 1.0], 5, 7);
    let (s, _) = p.sampled_max(50_000);
    for m in [Method::Centred, Method::Second] {
        let o = p.certify(m, Stop::Tight);
        assert!(o.resolved && o.bound >= s && o.bound <= 1.01 * s, "P4 {m:?}: {o:?}, sampled {s}");
    }
}

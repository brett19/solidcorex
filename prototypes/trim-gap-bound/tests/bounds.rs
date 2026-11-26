//! The bound is never below the sampled gap, is tight where it should be, refuses at a pole,
//! and composes with a net's certified distance.

use scx_trim_gap_bound::cases;
use scx_trim_gap_bound::eps::{net_gap_bound, net_gap_sampled};
use scx_trim_gap_bound::gap::{Method, Problem, Stop};
use scx_trim_gap_bound::surface::Surface;

const SAMPLES: usize = 50_000;

fn check<S: Surface>(name: &str, p: &Problem<S>, tight_ratio: f64) {
    let (s, _) = p.sampled_max(SAMPLES);
    assert!(s > 0.0, "{name}: zero gap");
    for m in [Method::Centred, Method::Second, Method::Lipschitz, Method::Direct] {
        let o = p.certify(m, Stop::Tight);
        assert!(o.bound >= s, "{name} {m:?}: bound {} below sampled {}", o.bound, s);
        assert!(o.lower <= s * (1.0 + 1e-9), "{name} {m:?}: proved-attained {} above sampled {}", o.lower, s);
        if matches!(m, Method::Second | Method::Centred) {
            assert!(o.resolved, "{name}: not resolved");
            assert!(o.bound <= tight_ratio * s, "{name}: bound {} not within {tight_ratio} of {}", o.bound, s);
        }
    }
    for m in [Method::Centred, Method::Second] {
        let w = p.certify(m, Stop::Within(1.1 * s));
        assert!(w.resolved && w.bound <= 1.1 * s && w.bound >= s, "{name} {m:?}: within");
    }
}

#[test]
fn bezier_offsets() {
    check("bezier +0.05", &cases::bezier_offset(0.05, 5, 7), 1.01);
    check("bezier fine", &cases::bezier_offset(0.05, 20, 28), 1.01);
    check("bezier -0.2", &cases::bezier_offset(-0.2, 5, 7), 1.01);
}

#[test]
fn torus_offsets_including_the_cusp() {
    check("torus +0.5", &cases::torus_offset(0.5, 6, 9), 1.01);
    check("torus -0.99", &cases::torus_offset(-0.99, 6, 9), 1.01);
    check("torus -1 cusp", &cases::torus_offset(-1.0, 6, 9), 1.01);
}

#[test]
fn near_a_pole_certifies_and_at_it_refuses() {
    for delta in [1e-1, 1e-3] {
        let p = cases::sphere_offset(0.1, delta, 8, 11);
        let (s, _) = p.sampled_max(SAMPLES);
        for m in [Method::Centred, Method::Second] {
            let o = p.certify(m, Stop::Tight);
            assert!(o.resolved && o.bound >= s, "delta {delta} {m:?}");
        }
    }
    let p = cases::sphere_offset(0.1, 0.0, 8, 11);
    let o = p.certify(Method::Second, Stop::Tight);
    assert!(!o.resolved || o.bound.is_infinite() || o.bound >= p.sampled_max(SAMPLES).0);
}

#[test]
fn blend_composition_holds() {
    for n in [2, 4] {
        let (on_net, on_def, net) = cases::blend(n, 6, 9);
        let area = net.domain();
        let e = net_gap_bound(&net, &on_def.surf, area);
        assert!(e.bound >= net_gap_sampled(&net, &on_def.surf, area, 200), "eps n={n}");
        let b = on_net.certify(Method::Centred, Stop::Tight);
        let (sd, _) = on_def.sampled_max(SAMPLES);
        assert!(b.bound + e.bound >= sd, "composed n={n}");
        let (sn, _) = on_net.sampled_max(SAMPLES);
        assert!(b.bound >= sn, "net n={n}");
    }
}

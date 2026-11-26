//! The measurements of report 55, printed as TSV sections.

use scx_trim_gap_bound::cases;
use scx_trim_gap_bound::eps::{net_gap_bound, net_gap_sampled};
use scx_trim_gap_bound::gap::{Method, Outcome, Problem, Stop};
use scx_trim_gap_bound::surface::Surface;

const SAMPLES: usize = 200_000;

fn row<S: Surface>(case: &str, p: &Problem<S>, sampled: f64, method: Method, stop: Stop) -> Outcome {
    let o = p.certify(method, stop);
    let stop_s = match stop {
        Stop::Tight => "tight".to_string(),
        Stop::Within(x) => format!("within:{:.3e}", x),
    };
    println!(
        "{case}\t{method:?}\t{stop_s}\t{:.4e}\t{:.4e}\t{:.4}\t{}\t{}\t{:.3}\t{}",
        o.bound,
        sampled,
        o.bound / sampled,
        o.pieces,
        o.ops,
        o.secs * 1e3,
        o.resolved
    );
    o
}

fn offset_rows<S: Surface>(case: &str, p: &Problem<S>) {
    let (s, _) = p.sampled_max(SAMPLES);
    for m in [Method::Centred, Method::Second, Method::Lipschitz, Method::Direct] {
        row(case, p, s, m, Stop::Tight);
    }
    for m in [Method::Centred, Method::Second] {
        row(case, p, s, m, Stop::Within(1.1 * s));
        row(case, p, s, m, Stop::Within(10.0 * s));
    }
}

fn main() {
    println!("# section offset");
    println!("case\tmethod\tstop\tbound\tsampled\tratio\tpieces\tops\tms\tresolved");
    offset_rows("bezier d=0.05 trim5 edge7", &cases::bezier_offset(0.05, 5, 7));
    offset_rows("bezier d=0.05 trim20 edge28", &cases::bezier_offset(0.05, 20, 28));
    offset_rows("bezier d=-0.2 trim5 edge7", &cases::bezier_offset(-0.2, 5, 7));
    offset_rows("torus d=0.5 trim6 edge9", &cases::torus_offset(0.5, 6, 9));
    offset_rows("torus d=-0.99 trim6 edge9", &cases::torus_offset(-0.99, 6, 9));
    offset_rows("torus d=-1 (cusp) trim6 edge9", &cases::torus_offset(-1.0, 6, 9));

    println!("# section pole");
    println!("case\tmethod\tstop\tbound\tsampled\tratio\tpieces\tops\tms\tresolved");
    for delta in [1e-1, 1e-2, 1e-3, 1e-4, 1e-6, 0.0] {
        let p = cases::sphere_offset(0.1, delta, 8, 11);
        let (s, _) = p.sampled_max(SAMPLES);
        for m in [Method::Centred, Method::Second] {
            row(&format!("sphere d=0.1 delta={delta:e}"), &p, s, m, Stop::Tight);
        }
    }

    println!("# section blend");
    println!("case\teps_resolved\tnet_bound\tnet_lower\teps_bound\teps_sampled\teps_boxes\teps_ops\teps_ms\tcomposed\tdirect_def\tsampled_def\tratio_composed\tratio_direct\tnet_pieces\tnet_ops\tnet_ms");
    for n in [2, 4, 8] {
        let (on_net, on_def, net) = cases::blend(n, 6, 9);
        let area = net.domain();
        let e = net_gap_bound(&net, &on_def.surf, area);
        let es = net_gap_sampled(&net, &on_def.surf, area, 400);
        let b = on_net.certify(Method::Centred, Stop::Tight);
        let direct = on_def.certify(Method::Centred, Stop::Tight);
        let (sd, _) = on_def.sampled_max(SAMPLES);
        let composed = (b.bound + e.bound) * (1.0 + 4.0 * f64::EPSILON);
        println!(
            "torus net {n}x{n}\t{}\t{:.4e}\t{:.4e}\t{:.4e}\t{:.4e}\t{}\t{}\t{:.3}\t{:.4e}\t{:.4e}\t{:.4e}\t{:.4}\t{:.4}\t{}\t{}\t{:.3}",
            e.resolved,
            b.bound,
            b.lower,
            e.bound,
            es,
            e.boxes,
            e.ops,
            e.secs * 1e3,
            composed,
            direct.bound,
            sd,
            composed / sd,
            direct.bound / sd,
            b.pieces,
            b.ops,
            b.secs * 1e3
        );
    }
}

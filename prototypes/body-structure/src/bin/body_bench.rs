//! Measurements for report 51. Prints TSV sections to stdout.
//!   body-bench edits   : cost of an edit of k of n faces, per layout
//!   body-bench reads   : read cost per layout against the plain baseline
//!   body-bench memory  : bytes of an unshared body per layout
//!   body-bench all

use std::hint::black_box;
use std::time::Instant;

use scx_body_structure::body::*;
use scx_body_structure::counting;
use scx_body_structure::model::{Lcg, Source};

const SIZES: [(usize, usize); 5] = [(4, 3), (10, 10), (32, 32), (100, 100), (320, 320)];

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Live bytes of a freshly built body (generator included in the window; its garbage freed).
fn body_bytes<B: Body>(w: usize, h: usize) -> (B, i64) {
    let r0 = counting::read();
    let b = B::build(Source::torus(w, h));
    (b, counting::read().live_since(&r0))
}

fn ks(n: usize) -> Vec<usize> {
    let mut v = vec![1, 10, n / 100, n / 10];
    v.retain(|&k| k >= 1 && k <= (n / 10).max(1));
    v.sort_unstable();
    v.dedup();
    v
}

fn edits<B: Body>() {
    for (w, h) in SIZES {
        let src = Source::torus(w, h);
        let (b, bytes) = body_bytes::<B>(w, h);
        let n = b.n_faces();
        for k in ks(n) {
            for (pat, patch) in [("patch", src.patch(k)), ("scatter", src.scattered(k))] {
                let iters = if n >= 10_000 { 9 } else { 51 };
                let mut times = Vec::new();
                let mut fin_times = Vec::new();
                let (mut alloc, mut live, mut count, mut fin_alloc, mut touched) = (0, 0, 0, 0, 0);
                for it in 0..iters {
                    let r0 = counting::read();
                    let t0 = Instant::now();
                    let mut d = b.edit();
                    d.apply_patch(&patch, it as f64);
                    let t1 = Instant::now();
                    let r1 = counting::read();
                    let (nb, out) = d.finish();
                    let t2 = Instant::now();
                    let r2 = counting::read();
                    times.push((t2 - t0).as_secs_f64());
                    fin_times.push((t2 - t1).as_secs_f64());
                    alloc = r2.allocated_since(&r0);
                    live = r2.live_since(&r0);
                    count = r2.count_since(&r0);
                    fin_alloc = r2.allocated_since(&r1);
                    touched = out.fates.len();
                    black_box(&nb);
                    drop(nb);
                }
                println!(
                    "edit\t{}\t{n}\t{k}\t{pat}\t{touched}\t{:.2}\t{alloc}\t{live}\t{count}\t{bytes}\t{:.4}\t{:.2}\t{fin_alloc}",
                    B::name(),
                    median(times) * 1e6,
                    1.0 - live as f64 / bytes as f64,
                    median(fin_times) * 1e6,
                );
            }
        }
        // transient mode: the draft owns the only reference, writes go in place
        let k = (n / 100).max(1);
        let patch = src.patch(k);
        let mut times = Vec::new();
        let (mut alloc, mut count) = (0, 0);
        for it in 0..5 {
            let fresh = B::build(Source::torus(w, h));
            let r0 = counting::read();
            let t0 = Instant::now();
            let mut d = fresh.into_draft();
            d.apply_patch(&patch, it as f64);
            let (nb, _) = d.finish();
            times.push(t0.elapsed().as_secs_f64());
            let r1 = counting::read();
            alloc = r1.allocated_since(&r0);
            count = r1.count_since(&r0);
            black_box(&nb);
        }
        println!("transient\t{}\t{n}\t{k}\t{:.2}\t{alloc}\t{count}", B::name(), median(times) * 1e6);
    }
}

fn content_key<B: Body>(b: &B, f: u32, buf: &mut Vec<u32>) -> u64 {
    // what a content-identity check of a face's cells must read: its surface, trims, edge tolerances
    let mut k = std::sync::Arc::as_ptr(b.surface(f)) as *const f64 as usize as u64;
    for t in b.trims(f) {
        k = k.rotate_left(5) ^ (t.as_ptr() as usize as u64);
    }
    buf.clear();
    b.face_edges(f, buf);
    for &e in buf.iter() {
        k = k.rotate_left(5) ^ b.edge(e).tol.to_bits();
    }
    k
}

fn reads<B: Body>() {
    for (w, h) in [(32, 32), (320, 320)] {
        let b = B::build(Source::torus(w, h));
        let n = b.n_faces();
        // an edited body: reads after a few edits (sharing in place)
        let mut e = b.clone();
        for it in 0..10 {
            let mut d = e.edit();
            d.apply_patch(&Source::torus(w, h).scattered(n / 100 + 1), it as f64);
            e = d.finish().0;
        }
        for (label, body) in [("fresh", &b), ("edited10", &e)] {
            let lookups = 4_000_000;
            let mut best = [f64::MAX; 5];
            for _ in 0..5 {
                let mut rng = Lcg(7);
                let t = Instant::now();
                let mut s = 0.0;
                for _ in 0..lookups {
                    s += body.face_tol(rng.next(n) as u32);
                }
                black_box(s);
                best[0] = best[0].min(t.elapsed().as_secs_f64() * 1e9 / lookups as f64);

                let reps = (4_000_000 / n).max(1);
                let t = Instant::now();
                for _ in 0..reps {
                    black_box(body.traverse());
                }
                best[1] = best[1].min(t.elapsed().as_secs_f64() * 1e9 / (reps * n) as f64);

                let steps = 4_000_000;
                let t = Instant::now();
                black_box(body.walk(0, steps));
                best[2] = best[2].min(t.elapsed().as_secs_f64() * 1e9 / steps as f64);

                for f in 0..n as u32 {
                    body.bbox(f);
                }
                let mut rng = Lcg(9);
                let t = Instant::now();
                let mut s = 0.0;
                for _ in 0..lookups {
                    s += body.bbox(rng.next(n) as u32)[0];
                }
                black_box(s);
                best[3] = best[3].min(t.elapsed().as_secs_f64() * 1e9 / lookups as f64);

                let mut rng = Lcg(9);
                let mut buf = Vec::new();
                let t = Instant::now();
                let mut s = 0u64;
                for _ in 0..lookups / 4 {
                    let f = rng.next(n) as u32;
                    s ^= content_key(body, f, &mut buf);
                    s ^= body.bbox(f)[0].to_bits();
                }
                black_box(s);
                best[4] = best[4].min(t.elapsed().as_secs_f64() * 1e9 / (lookups / 4) as f64);
            }
            println!(
                "read\t{}\t{n}\t{label}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                B::name(),
                best[0],
                best[1],
                best[2],
                best[3],
                best[4]
            );
        }
    }
}

fn memory<B: Body>() {
    for (w, h) in SIZES {
        let (b, bytes) = body_bytes::<B>(w, h);
        let n = b.n_faces();
        // what a plain deep copy of this layout costs (the draft of today's Clone)
        let r0 = counting::read();
        let t0 = Instant::now();
        let c = b.clone();
        let t = t0.elapsed().as_secs_f64();
        let cb = counting::read().allocated_since(&r0);
        black_box(&c);
        println!("memory\t{}\t{n}\t{bytes}\t{:.1}\t{cb}\t{:.2}", B::name(), bytes as f64 / n as f64, t * 1e6);
    }
}

macro_rules! each {
    ($f:ident) => {
        $f::<TableBody<PlainF>>();
        $f::<TableBody<WholeF>>();
        $f::<TableBody<ChunkF>>();
        $f::<TableBody<TrieF>>();
        $f::<FaceBody<PlainF>>();
        $f::<FaceBody<ChunkF>>();
        $f::<FaceBody<Chunk16F>>();
        $f::<FaceBody<TrieF>>();
    };
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    if mode == "memory" || mode == "all" {
        println!("# memory\tlayout\tn\tbody_bytes\tbytes_per_face\tclone_alloc\tclone_us");
        each!(memory);
    }
    if mode == "edits" || mode == "all" {
        println!("# edit\tlayout\tn\tk\tpattern\tfaces_touched\tus\talloc\tretained\tallocs\tbody_bytes\tshared_frac\tfinish_us\tfinish_alloc");
        println!("# transient\tlayout\tn\tk\tus\talloc\tallocs");
        each!(edits);
    }
    if mode == "reads" || mode == "all" {
        println!("# read\tlayout\tn\tbody\tlookup_ns\ttraverse_ns_per_face\twalk_ns_per_step\tcell_read_ns\tcell_read_with_content_key_ns");
        each!(reads);
    }
}

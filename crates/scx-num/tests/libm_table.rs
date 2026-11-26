// Written for solidcorex. Apache-2.0.
//! The results of the functions scx-num calls that are not correctly rounded (libm's
//! transcendentals, `powi`, `hypot`, `cbrt`), pinned bit for bit on the build platform
//! (x86_64 Linux, Rust 1.99.1, the table below). Each function's results at a fixed set of
//! inputs are hashed (FNV-1a over the result bits) and compared with the pinned hash. On
//! another platform the test is ignored; run with `--ignored` there and it names each
//! function whose libm differs: bit-identity across platforms is not shown for these
//! (report 53, "Cross-platform"). Not solved here.
//!
//! `SCX_PRINT_LIBM_TABLE=1 cargo test -p scx-num --test libm_table -- --nocapture` prints
//! the table for the platform it runs on.

use scx_num::ring::acos_clamped;
use scx_num::tol::zone_for_size;

/// Inputs spread over each function's domain, its special points and the edges of its
/// range reduction (multiples of pi/4 and their neighbours, huge arguments).
fn inputs() -> Vec<f64> {
    let mut v = vec![
        0.0,
        -0.0,
        1e-300,
        1e-17,
        1e-8,
        0.1,
        0.5,
        0.7071067811865476,
        0.9999999999999999,
        1.0,
        1.0000000000000002,
        1.5707963267948966,
        2.0,
        3.141592653589793,
        4.71238898038469,
        6.283185307179586,
        10.0,
        50.0,
        1e5,
        1e15,
        1e22,
        1.7976931348623157e308,
        f64::INFINITY,
        f64::NAN,
    ];
    let more: Vec<f64> = v.iter().map(|x| -x).collect();
    v.extend(more);
    // a deterministic spread of ordinary values
    let mut s = 0x2545_f491_4f6c_dd1d_u64;
    for _ in 0..400 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let mant = (s >> 12) as f64 / (1u64 << 52) as f64; // [0, 1)
        let exp = ((s >> 4) % 40) as i32 - 20;
        let sign = if s & 1 == 0 { 1.0 } else { -1.0 };
        v.push(sign * (1.0 + mant) * 2f64.powi(exp));
    }
    v
}

fn fnv(h: &mut u64, bits: u64) {
    for b in bits.to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x0100_0000_01b3);
    }
}

fn table() -> Vec<(&'static str, u64)> {
    type F1 = fn(f64) -> f64;
    let unary: [(&str, F1); 12] = [
        ("sin", f64::sin),
        ("cos", f64::cos),
        ("asin", f64::asin),
        ("acos", f64::acos),
        ("cbrt", f64::cbrt),
        ("log2", f64::log2),
        ("log10", f64::log10),
        ("sin_cos.0", |x| x.sin_cos().0),
        ("sin_cos.1", |x| x.sin_cos().1),
        ("acos_clamped", acos_clamped),
        ("zone_for_size", zone_for_size),
        ("powi(2)", |x| x.powi(2)),
    ];
    let xs = inputs();
    let mut out = Vec::new();
    for (name, f) in unary {
        let mut h = 0xcbf2_9ce4_8422_2325_u64;
        for &x in &xs {
            fnv(&mut h, f(x).to_bits());
        }
        out.push((name, h));
    }
    type F2 = fn(f64, f64) -> f64;
    let binary: [(&str, F2); 4] = [
        ("atan2", f64::atan2),
        ("hypot", f64::hypot),
        ("powf", f64::powf),
        ("powi(n)", |x, y| x.powi((y.abs() % 64.0) as i32 - 32)),
    ];
    for (name, f) in binary {
        let mut h = 0xcbf2_9ce4_8422_2325_u64;
        for (i, &x) in xs.iter().enumerate() {
            for &y in xs.iter().skip(i % 7).step_by(7) {
                fnv(&mut h, f(x, y).to_bits());
            }
        }
        out.push((name, h));
    }
    out
}

/// The pinned hashes, x86_64-unknown-linux-gnu, Rust 1.99.1.
const PINNED: &[(&str, u64)] = &[
    ("sin", 0xf94c8f46d889a26c),
    ("cos", 0x1e6d8fea58673408),
    ("asin", 0x6e3bdbb02a4b2498),
    ("acos", 0xadd4af4984be8f46),
    ("cbrt", 0xfa1fed3be91ccc35),
    ("log2", 0x78b7954c215f6fa4),
    ("log10", 0x034021b005a3230f),
    ("sin_cos.0", 0xf94c8f46d889a26c),
    ("sin_cos.1", 0x1e6d8fea58673408),
    ("acos_clamped", 0x5940ff5fe435ece3),
    ("zone_for_size", 0xf583226ed19859ff),
    ("powi(2)", 0xc937b8737ca22c35),
    ("atan2", 0x435f6ae3add47b98),
    ("hypot", 0x65af180928ff261f),
    ("powf", 0x393ff551b990323d),
    ("powi(n)", 0x4d11d37d87f2f461),
];

#[test]
#[cfg_attr(
    not(all(target_arch = "x86_64", target_os = "linux")),
    ignore = "pinned on x86_64 Linux; run with --ignored to see which functions differ here"
)]
fn libm_results_are_the_build_platforms() {
    let got = table();
    if std::env::var_os("SCX_PRINT_LIBM_TABLE").is_some() {
        for (name, h) in &got {
            println!("    (\"{name}\", 0x{h:016x}),");
        }
    }
    let differ: Vec<&str> = got
        .iter()
        .filter(|(n, h)| PINNED.iter().find(|(m, _)| m == n).map(|(_, p)| p) != Some(h))
        .map(|(n, _)| *n)
        .collect();
    assert!(
        differ.is_empty(),
        "these functions' results differ from the pinned platform's: {differ:?}"
    );
}

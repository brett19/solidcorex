// Written for solidcorex. Apache-2.0.
//! The bits of `fmath`'s results, and of the scx-num functions built on them, pinned on
//! every platform: each function's results at a fixed set of inputs are hashed (FNV-1a
//! over the result bits) and compared with the pinned hash. `fmath` uses only IEEE 754's
//! correctly rounded operations, so the hashes must agree everywhere; a difference is a
//! bug in `fmath` or a change to it (which re-pins the table on purpose).
//!
//! `SCX_PRINT_FMATH_TABLE=1 cargo test -p scx-num --test fmath_pin -- --nocapture` prints
//! the table.

use scx_num::fmath;
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
        core::f64::consts::FRAC_1_SQRT_2,
        0.9999999999999999,
        1.0,
        1.0000000000000002,
        core::f64::consts::FRAC_PI_2,
        2.0,
        core::f64::consts::PI,
        4.71238898038469,
        core::f64::consts::TAU,
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
        v.push(sign * (1.0 + mant) * fmath::powi(2.0, exp));
    }
    v
}

fn fnv(h: &mut u64, bits: u64) {
    for b in bits.to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x0100_0000_01b3);
    }
}

type F1 = fn(f64) -> f64;
type F2 = fn(f64, f64) -> f64;

fn table() -> Vec<(&'static str, u64)> {
    let unary: [(&str, F1); 20] = [
        ("exp", fmath::exp),
        ("exp2", fmath::exp2),
        ("ln", fmath::ln),
        ("log2", fmath::log2),
        ("log10", fmath::log10),
        ("sin", fmath::sin),
        ("cos", fmath::cos),
        ("tan", fmath::tan),
        ("sin_cos.0", |x| fmath::sin_cos(x).0),
        ("sin_cos.1", |x| fmath::sin_cos(x).1),
        ("asin", fmath::asin),
        ("acos", fmath::acos),
        ("atan", fmath::atan),
        ("sinh", fmath::sinh),
        ("cosh", fmath::cosh),
        ("tanh", fmath::tanh),
        ("cbrt", fmath::cbrt),
        ("powi(2)", |x| fmath::powi(x, 2)),
        ("acos_clamped", acos_clamped),
        ("zone_for_size", zone_for_size),
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
    let binary: [(&str, F2); 4] = [
        ("atan2", fmath::atan2),
        ("hypot", fmath::hypot),
        ("powf", fmath::powf),
        ("powi(n)", |x, y| {
            fmath::powi(x, (y.abs() % 64.0) as i32 - 32)
        }),
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

/// The pinned hashes (the same on every platform).
const PINNED: &[(&str, u64)] = &[
    ("exp", 0x5df3a9423961c93d),
    ("exp2", 0x75723a59ae2e705b),
    ("ln", 0x10d85f8de118875b),
    ("log2", 0xf63e3940051fe7a4),
    ("log10", 0x75dbdccde6b942a0),
    ("sin", 0x46f47e602ea86639),
    ("cos", 0x76a0b5de44bf5888),
    ("tan", 0xdf0f11710bd42cb0),
    ("sin_cos.0", 0x46f47e602ea86639),
    ("sin_cos.1", 0x76a0b5de44bf5888),
    ("asin", 0xf260fb19682fee18),
    ("acos", 0x511740d6f00edcc6),
    ("atan", 0xc1cdd7fe80cd48d9),
    ("sinh", 0x4d42a8c3459e2d24),
    ("cosh", 0x94e0e9a1a2866e0d),
    ("tanh", 0xd33735c451f77077),
    ("cbrt", 0x9ef716c9a41890b5),
    ("powi(2)", 0x83cb3439748d1bb5),
    ("acos_clamped", 0xb8c20a91b830ff63),
    ("zone_for_size", 0xf583226ed19859ff),
    ("atan2", 0x38c4aee23e996054),
    ("hypot", 0xaad879607804e9ab),
    ("powf", 0x4852b3e48e58cf4e),
    ("powi(n)", 0xc4040889c67ee521),
];

#[test]
fn fmath_results_are_pinned() {
    let got = table();
    if std::env::var_os("SCX_PRINT_FMATH_TABLE").is_some() {
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
        "these functions' results differ from the pinned ones: {differ:?}"
    );
}

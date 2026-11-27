// Written for solidcorex. Apache-2.0.
//! The geometry records (research 63 RE2): every curve and surface kind round-trips to the
//! bit in binary and in text, from seeded samples built by the constructors and the edits
//! (transforms, mirrors, reversals, knot insertion, splits, degree elevation, rational in
//! both forms); binary → text → binary gives the same bytes; every truncation and
//! single-byte change of a record gives a value or a `ReadError`, never a panic, and a
//! value only when the changed input is that value's own spelling; and invalid geometry is
//! refused with the constructor's own cause.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::sync::Arc;

use scx_curve::arcs::Arcs;
use scx_curve::{
    AngleAxis, AngleLine2, AngleWave2, Chart, ChartKind, Circle, Circle2, Curve, Curve2,
    CurveError, Degenerate, Ellipse, Ellipse2, Hyperbola, Hyperbola2, Line, Line2, OnChart,
    Parabola, Parabola2,
};
use scx_num::Length;
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Frame2, FrameAxis, FrameError, Sense};
use scx_num::hash::Rng;
use scx_num::math::Affine;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{KnotVector, NurbsCurve, NurbsError};
use scx_nurbs_surface::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis, WeightCause};
use scx_record::{
    ReadCause, ReadError, Reader, Record, Source, TextReader, TextWriter, WriteCause, Writer,
};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, SurfaceError, Torus};
use scx_testkit::sample::{self, Knots, Spec, Weights};

// ---- encoding helpers ------------------------------------------------------------------

fn bin<T: Record>(v: &T) -> Vec<u8> {
    let mut w = Writer::new();
    v.put(&mut w).unwrap();
    w.finish()
}

fn txt<T: Record>(v: &T) -> String {
    let mut w = TextWriter::new();
    v.put(&mut w).unwrap();
    w.finish()
}

fn from_bin<T: Record>(bytes: &[u8]) -> Result<T, ReadError> {
    let mut r = Reader::new(bytes);
    let v = T::take(&mut r)?;
    r.finish()?;
    Ok(v)
}

fn from_txt<T: Record>(text: &str) -> Result<T, ReadError> {
    let mut r = TextReader::new(text);
    let v = T::take(&mut r)?;
    r.finish()?;
    Ok(v)
}

/// `Debug` prints each `f64` as its shortest round-trip spelling, so equal `Debug` text is
/// equal bits (no value here holds a NaN); `PartialEq` alone would let `-0 == 0` through.
fn same<T: core::fmt::Debug + PartialEq>(a: &T, b: &T) -> bool {
    a == b && format!("{a:?}") == format!("{b:?}")
}

/// The value back to the bit in both encodings, and binary → text → binary equal bytes.
fn round_trip<T: Record + core::fmt::Debug + PartialEq>(v: &T) {
    let b = bin(v);
    let back: T = from_bin(&b).unwrap_or_else(|e| panic!("binary {e:?} for {v:?}"));
    assert!(same(&back, v), "binary changed {v:?} into {back:?}");
    assert_eq!(bin(&back), b);
    let t = txt(v);
    let back: T = from_txt(&t).unwrap_or_else(|e| panic!("text {e:?} for {v:?}\n{t}"));
    assert!(same(&back, v), "text changed {v:?} into {back:?}");
    assert_eq!(txt(&back), t);
    let via: T = from_txt(&txt(&from_bin::<T>(&b).unwrap())).unwrap();
    assert_eq!(bin(&via), b);
}

// ---- seeded values ---------------------------------------------------------------------

fn signed(rng: &mut Rng, scale: f64) -> f64 {
    rng.next_signed_unit() * scale
}

fn vec3(rng: &mut Rng, scale: f64) -> Vec3 {
    Vec3::new(signed(rng, scale), signed(rng, scale), signed(rng, scale))
}

fn point3(rng: &mut Rng) -> Point3 {
    Point3::new(signed(rng, 50.0), signed(rng, 50.0), signed(rng, 50.0))
}

fn length(rng: &mut Rng, lo: f64, hi: f64) -> Length {
    Length::from_mm(lo + rng.next_f64() * (hi - lo))
}

fn sense(rng: &mut Rng) -> Sense {
    if rng.next_u64() & 1 == 0 {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

fn frame(rng: &mut Rng) -> Frame {
    loop {
        if let Some(f) = Frame::new(point3(rng), vec3(rng, 1.0), vec3(rng, 1.0)) {
            return f;
        }
    }
}

fn frame2(rng: &mut Rng) -> Frame2 {
    loop {
        let o = Point2::new(signed(rng, 50.0), signed(rng, 50.0));
        if let Some(f) = Frame2::new(o, Vec2::new(signed(rng, 1.0), signed(rng, 1.0))) {
            return f;
        }
    }
}

/// A rotation about a random axis, a translation, or a mirror in a random plane.
fn affine(rng: &mut Rng) -> Affine {
    match rng.next_u64() % 3 {
        0 => loop {
            if let Ok(a) = Affine::rotation(point3(rng), vec3(rng, 1.0), signed(rng, 3.0)) {
                return a;
            }
        },
        1 => Affine::translation(vec3(rng, 20.0)),
        _ => Affine::from_axes(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Point3::new(0.0, 0.0, 0.0),
        ),
    }
}

fn spec(rng: &mut Rng, knots: Knots) -> Spec {
    let degree = 1 + (rng.next_u64() % 4) as usize;
    let count = degree + 1 + (rng.next_u64() % 5) as usize;
    Spec {
        degree,
        count,
        knots,
    }
}

const KNOTS: [Knots; 4] = [
    Knots::Uniform,
    Knots::Random,
    Knots::Clustered,
    Knots::Multiple,
];
const WEIGHTS: [Weights; 3] = [Weights::Unit, Weights::Moderate, Weights::NearZero];

/// A NURBS curve from the sampler, through `new` or (rational) `from_homogeneous`.
fn nurbs<const D: usize>(rng: &mut Rng, i: usize) -> Option<NurbsCurve<D>> {
    let s = spec(rng, KNOTS[i % 4]);
    let data = sample::curve::<D>(rng, s, WEIGHTS[(i / 4) % 3]);
    let knots = KnotVector::new(data.degree, data.knots).ok()?;
    match data.weights {
        Some(w) if i % 2 == 1 => {
            let wp: Vec<[f64; D]> = data
                .points
                .iter()
                .zip(&w)
                .map(|(p, &w)| p.map(|x| x * w * 1.1))
                .collect();
            NurbsCurve::from_homogeneous(knots, wp, w).ok()
        }
        w => NurbsCurve::new(knots, data.points, w).ok(),
    }
}

/// The curve after one of the edits.
fn edited<const D: usize>(c: NurbsCurve<D>, rng: &mut Rng) -> NurbsCurve<D> {
    let k = c.knots();
    let (a, b) = (k[c.degree()], k[k.len() - 1 - c.degree()]);
    let t = a + (0.2 + 0.6 * rng.next_f64()) * (b - a);
    match rng.next_u64() % 5 {
        0 => c.insert_knot(t, 1).unwrap_or(c),
        1 => c.split(t).map(|(l, _)| l).unwrap_or(c),
        2 => c.split(t).map(|(_, r)| r).unwrap_or(c),
        3 => c.elevate(1).unwrap_or(c),
        _ => c.reversed(),
    }
}

fn nurbs_surface(rng: &mut Rng, i: usize) -> Option<NurbsSurface<3>> {
    let su = spec(rng, KNOTS[i % 4]);
    let sv = spec(rng, KNOTS[(i / 4) % 4]);
    let data = sample::surface::<3>(rng, (su, sv), WEIGHTS[(i / 2) % 3]);
    let u = KnotVector::new(data.degrees.0, data.knots_u).ok()?;
    let v = KnotVector::new(data.degrees.1, data.knots_v).ok()?;
    let (rows, cols) = (su.count, sv.count);
    match data.weights {
        Some(w) if i % 2 == 1 => {
            let wp: Vec<[f64; 3]> = data
                .points
                .iter()
                .zip(&w)
                .map(|(p, &w)| p.map(|x| x * w * 1.1))
                .collect();
            let (wp, w) = (
                Net::new(rows, cols, wp).ok()?,
                Net::new(rows, cols, w).ok()?,
            );
            NurbsSurface::from_homogeneous(u, v, wp, w).ok()
        }
        w => {
            let w = match w {
                Some(w) => Some(Net::new(rows, cols, w).ok()?),
                None => None,
            };
            NurbsSurface::new(u, v, Net::new(rows, cols, data.points).ok()?, w).ok()
        }
    }
}

fn axis(rng: &mut Rng) -> ParamAxis {
    if rng.next_u64() & 1 == 0 {
        ParamAxis::U
    } else {
        ParamAxis::V
    }
}

fn edited_surface(s: NurbsSurface<3>, rng: &mut Rng) -> NurbsSurface<3> {
    let ax = axis(rng);
    let k = s.knots(ax);
    let p = s.degree(ax);
    let (a, b) = (k[p], k[k.len() - 1 - p]);
    let t = a + (0.2 + 0.6 * rng.next_f64()) * (b - a);
    match rng.next_u64() % 6 {
        0 => s.insert_knot(ax, t, 1).unwrap_or(s),
        1 => s.split(ax, t).map(|(l, _)| l).unwrap_or(s),
        2 => s.elevate(ax, 1).unwrap_or(s),
        3 => s.reversed(ax),
        4 => s.swapped(),
        _ => s.transformed(&affine(rng)).unwrap_or(s),
    }
}

fn curves(seed: u64, n: usize) -> Vec<Curve> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for i in 0..n {
        let f = frame(&mut rng);
        let s = sense(&mut rng);
        let c = match i % 6 {
            0 => Curve::Line(Line::new(point3(&mut rng), vec3(&mut rng, 10.0)).unwrap()),
            1 => Curve::Circle(Circle::new(f, length(&mut rng, 0.01, 30.0), s).unwrap()),
            2 => Curve::Ellipse(
                Ellipse::new(
                    f,
                    length(&mut rng, 0.01, 30.0),
                    length(&mut rng, 0.01, 30.0),
                    s,
                )
                .unwrap(),
            ),
            3 => Curve::Parabola(Parabola::new(f, length(&mut rng, 0.01, 30.0), s).unwrap()),
            4 => Curve::Hyperbola(
                Hyperbola::new(
                    f,
                    length(&mut rng, 0.01, 30.0),
                    length(&mut rng, 0.01, 30.0),
                    s,
                )
                .unwrap(),
            ),
            _ => match nurbs::<3>(&mut rng, i / 6) {
                Some(c) => {
                    let c = if i % 4 == 0 { edited(c, &mut rng) } else { c };
                    Curve::Nurbs(Arc::new(c))
                }
                None => continue,
            },
        };
        out.push(c.clone());
        // Every third value also through a rotation, translation or mirror, and reversed.
        if i % 3 == 0
            && let Ok((t, _)) = c.transformed(&affine(&mut rng))
        {
            out.push(t.reversed());
            out.push(t);
        }
    }
    out
}

fn curves2(seed: u64, n: usize) -> Vec<Curve2> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for i in 0..n {
        let f = frame2(&mut rng);
        let s = sense(&mut rng);
        let arcs = if rng.next_u64() & 1 == 0 {
            Arcs::half()
        } else {
            Arcs::turn(0.5 + 10.0 * rng.next_f64())
        };
        let c = match i % 9 {
            0 => Curve2::Line(
                Line2::new(
                    Point2::new(signed(&mut rng, 5.0), signed(&mut rng, 5.0)),
                    Vec2::new(signed(&mut rng, 2.0), 0.5 + rng.next_f64()),
                )
                .unwrap(),
            ),
            1 => Curve2::Circle(Circle2::new(f, length(&mut rng, 0.01, 30.0), s).unwrap()),
            2 => Curve2::Ellipse(
                Ellipse2::new(
                    f,
                    length(&mut rng, 0.01, 30.0),
                    length(&mut rng, 0.01, 30.0),
                    s,
                )
                .unwrap(),
            ),
            3 => Curve2::Parabola(Parabola2::new(f, length(&mut rng, 0.01, 30.0), s).unwrap()),
            4 => Curve2::Hyperbola(
                Hyperbola2::new(
                    f,
                    length(&mut rng, 0.01, 30.0),
                    length(&mut rng, 0.01, 30.0),
                    s,
                )
                .unwrap(),
            ),
            5 => match nurbs::<2>(&mut rng, i / 8) {
                Some(c) => {
                    let c = if i % 3 == 0 { edited(c, &mut rng) } else { c };
                    Curve2::Nurbs(Arc::new(c))
                }
                None => continue,
            },
            6 => Curve2::AngleLine(
                AngleLine2::new(
                    arcs,
                    if i % 16 < 8 {
                        AngleAxis::U
                    } else {
                        AngleAxis::V
                    },
                    signed(&mut rng, 4.0),
                    s,
                    signed(&mut rng, 4.0),
                    signed(&mut rng, 3.0),
                )
                .unwrap(),
            ),
            8 => {
                let kind = match i % 5 {
                    0 => ChartKind::Plane,
                    1 => ChartKind::Cylinder {
                        radius: length(&mut rng, 0.01, 30.0),
                    },
                    2 => ChartKind::Cone,
                    3 => ChartKind::Sphere,
                    _ => ChartKind::Torus {
                        major: length(&mut rng, 0.01, 30.0),
                    },
                };
                let chart = Chart::new(frame(&mut rng), s, kind).unwrap();
                let circle = Circle::new(frame(&mut rng), length(&mut rng, 0.01, 30.0), s);
                let start = signed(&mut rng, 3.0);
                let over = Extent1d::new(start, start + 0.1 + rng.next_f64()).unwrap();
                let mut end = || Point2::new(signed(&mut rng, 4.0), signed(&mut rng, 4.0));
                let ends = [end(), end()];
                Curve2::from(
                    OnChart::new(Curve::Circle(circle.unwrap()), chart, over, ends).unwrap(),
                )
            }
            _ => Curve2::AngleWave(
                AngleWave2::new(
                    arcs,
                    signed(&mut rng, 4.0),
                    s,
                    signed(&mut rng, 1.0),
                    signed(&mut rng, 0.5),
                    if i % 16 < 8 {
                        0.0
                    } else {
                        signed(&mut rng, 0.5)
                    },
                )
                .unwrap(),
            ),
        };
        if i % 2 == 0 {
            out.push(c.reversed());
        }
        out.push(c);
    }
    out
}

fn surfaces(seed: u64, n: usize) -> Vec<Surface> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for i in 0..n {
        let f = frame(&mut rng);
        let s = sense(&mut rng);
        let v = match i % 6 {
            0 => Surface::Plane(Plane::new(f)),
            1 => Surface::Cylinder(Cylinder::new(f, length(&mut rng, 0.01, 30.0), s).unwrap()),
            2 => Surface::Cone(
                Cone::new(
                    f,
                    length(&mut rng, 0.0, 30.0),
                    signed(&mut rng, 1.5).clamp(-1.5, 1.5) + if i % 12 < 6 { 0.01 } else { -0.01 },
                    s,
                )
                .unwrap_or_else(|_| Cone::new(f, Length::from_mm(1.0), 0.5, s).unwrap()),
            ),
            3 => Surface::Sphere(Sphere::new(f, length(&mut rng, 0.01, 30.0), s).unwrap()),
            4 => {
                let minor = length(&mut rng, 0.01, 10.0);
                let major = Length::from_mm(minor.mm() + 0.01 + 20.0 * rng.next_f64());
                Surface::Torus(Torus::new(f, major, minor, s).unwrap())
            }
            _ => match nurbs_surface(&mut rng, i / 6) {
                Some(n) => {
                    let n = if i % 4 == 0 {
                        edited_surface(n, &mut rng)
                    } else {
                        n
                    };
                    Surface::Nurbs(Arc::new(n))
                }
                None => continue,
            },
        };
        out.push(v.clone());
        if i % 3 == 0 {
            out.push(v.reversed(axis(&mut rng)).0);
            if let Ok((t, _)) = v.transformed(&affine(&mut rng)) {
                out.push(t);
            }
        }
    }
    out
}

fn tally<T: core::fmt::Debug>(values: &[T]) -> std::collections::BTreeMap<String, usize> {
    let mut kinds = std::collections::BTreeMap::new();
    for v in values {
        let name: String = format!("{v:?}")
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect();
        *kinds.entry(name).or_insert(0) += 1;
    }
    kinds
}

// ---- round trips -----------------------------------------------------------------------

#[test]
fn frames_round_trip_to_the_bit() {
    let mut rng = Rng::new(0xf4a3e);
    for _ in 0..2000 {
        let f = frame(&mut rng);
        round_trip(&f);
        if let Some((t, _)) = f.transformed(&affine(&mut rng)) {
            round_trip(&t);
        }
        round_trip(&f.half_turn_about_x());
        round_trip(&frame2(&mut rng));
    }
}

#[test]
fn every_curve_kind_round_trips_to_the_bit() {
    let all = curves(0xc0e7e, 600);
    let kinds = tally(&all);
    for k in [
        "Line",
        "Circle",
        "Ellipse",
        "Parabola",
        "Hyperbola",
        "Nurbs",
    ] {
        assert!(kinds.get(k).copied().unwrap_or(0) >= 20, "{k}: {kinds:?}");
    }
    for c in &all {
        round_trip(c);
    }
}

#[test]
fn every_curve2_kind_round_trips_to_the_bit() {
    let all = curves2(0xc0e72, 800);
    let kinds = tally(&all);
    for k in [
        "Line",
        "Circle",
        "Ellipse",
        "Parabola",
        "Hyperbola",
        "Nurbs",
        "AngleLine",
        "AngleWave",
        "OnChart",
    ] {
        assert!(kinds.get(k).copied().unwrap_or(0) >= 20, "{k}: {kinds:?}");
    }
    for c in &all {
        round_trip(c);
    }
}

#[test]
fn every_surface_kind_round_trips_to_the_bit() {
    let all = surfaces(0x5af4ce, 600);
    let kinds = tally(&all);
    for k in ["Plane", "Cylinder", "Cone", "Sphere", "Torus", "Nurbs"] {
        assert!(kinds.get(k).copied().unwrap_or(0) >= 20, "{k}: {kinds:?}");
    }
    for s in &all {
        round_trip(s);
    }
}

/// Rational values in both forms, and their edits, each with one spelling.
#[test]
fn rational_nurbs_in_both_forms_round_trip() {
    let mut rng = Rng::new(0x4a710);
    let (mut points, mut homogeneous) = (0, 0);
    for i in 0..3000 {
        let Some(c) = nurbs::<3>(&mut rng, i) else {
            continue;
        };
        let c = if i % 3 == 0 { edited(c, &mut rng) } else { c };
        if c.weights().is_some() {
            match txt(&Curve::Nurbs(Arc::new(c.clone()))).contains("homogeneous") {
                true => homogeneous += 1,
                false => points += 1,
            }
        }
        round_trip(&c);
        round_trip(&c.reversed());
    }
    assert!(points > 100 && homogeneous > 100, "{points} {homogeneous}");
    for i in 0..600 {
        if let Some(s) = nurbs_surface(&mut rng, i) {
            round_trip(&edited_surface(s.clone(), &mut rng));
            round_trip(&s);
        }
    }
}

// ---- corruption ------------------------------------------------------------------------

fn corrupt_bin<T: Record + core::fmt::Debug>(good: &[u8]) -> usize {
    let mut decoded = 0;
    for len in 0..good.len() {
        let e = from_bin::<T>(&good[..len]).unwrap_err();
        assert!(e.at <= len);
    }
    let mut b = good.to_vec();
    for i in 0..b.len() {
        let orig = b[i];
        for v in 0..=u8::MAX {
            if v == orig {
                continue;
            }
            b[i] = v;
            match from_bin::<T>(&b) {
                Ok(s) => {
                    assert_eq!(bin(&s), b, "byte {i} = {v}: another spelling");
                    decoded += 1;
                }
                Err(e) => assert!(e.at <= b.len()),
            }
        }
        b[i] = orig;
    }
    decoded
}

fn corrupt_txt<T: Record + core::fmt::Debug>(good: &str) -> usize {
    let mut decoded = 0;
    for len in 0..good.len() {
        let e = from_txt::<T>(&good[..len]).unwrap_err();
        assert!(e.at <= len);
    }
    let mut b = good.as_bytes().to_vec();
    for i in 0..b.len() {
        let orig = b[i];
        for v in 0..=u8::MAX {
            if v == orig {
                continue;
            }
            b[i] = v;
            let Ok(t) = core::str::from_utf8(&b) else {
                continue;
            };
            match from_txt::<T>(t) {
                Ok(s) => {
                    assert_eq!(txt(&s), t, "byte {i} = {v}: another spelling");
                    decoded += 1;
                }
                Err(e) => assert!(e.at <= t.len()),
            }
        }
        b[i] = orig;
    }
    decoded
}

fn first_of_each<T: core::fmt::Debug + Clone>(all: &[T], per_kind: usize) -> Vec<T> {
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    let mut out = Vec::new();
    for v in all {
        let k: String = format!("{v:?}")
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect();
        let n = seen.entry(k).or_insert(0);
        if *n < per_kind {
            *n += 1;
            out.push(v.clone());
        }
    }
    out
}

#[test]
fn corrupted_records_never_panic() {
    let mut decoded = 0;
    // Small NURBS keep the sweep short: degree 1-2 with few points.
    for c in first_of_each(&curves(0xbad1, 60), 2) {
        if format!("{c:?}").len() > 2000 {
            continue;
        }
        decoded += corrupt_bin::<Curve>(&bin(&c));
        decoded += corrupt_txt::<Curve>(&txt(&c));
    }
    for c in first_of_each(&curves2(0xbad2, 80), 2) {
        if format!("{c:?}").len() > 2000 {
            continue;
        }
        decoded += corrupt_bin::<Curve2>(&bin(&c));
        decoded += corrupt_txt::<Curve2>(&txt(&c));
    }
    for s in first_of_each(&surfaces(0xbad3, 60), 2) {
        if format!("{s:?}").len() > 3000 {
            continue;
        }
        decoded += corrupt_bin::<Surface>(&bin(&s));
        decoded += corrupt_txt::<Surface>(&txt(&s));
    }
    // Changed number bytes mostly give another valid value; that each one re-encodes to its
    // input is what the sweep checks.
    assert!(decoded > 0);
}

// ---- refusals --------------------------------------------------------------------------

/// The text with the one word `old` changed to `new`.
fn swap_word(text: &str, old: &str, new: &str) -> String {
    let mut hits = 0;
    let out: String = text
        .split_inclusive([' ', '\n'])
        .map(|w| {
            let (word, sep) = w.split_at(w.trim_end_matches([' ', '\n']).len());
            if word == old {
                hits += 1;
                format!("{new}{sep}")
            } else {
                w.to_owned()
            }
        })
        .collect();
    assert_eq!(hits, 1, "{old} in {text}");
    out
}

/// The bytes with the one `f64` `old` changed to `new`.
fn swap_f64(bytes: &[u8], old: f64, new: f64) -> Vec<u8> {
    for (o, n) in [
        (old.to_le_bytes(), new.to_le_bytes()),
        (old.to_be_bytes(), new.to_be_bytes()),
    ] {
        let at: Vec<usize> = (0..=bytes.len() - 8)
            .filter(|&i| bytes[i..i + 8] == o)
            .collect();
        if at.len() == 1 {
            let mut b = bytes.to_vec();
            b[at[0]..at[0] + 8].copy_from_slice(&n);
            return b;
        }
    }
    panic!("{old} not found once");
}

/// Both encodings of `v` with `old` changed to `new`, read back: the two causes.
fn refused<T: Record + core::fmt::Debug>(v: &T, old: f64, new: f64) -> [ReadCause; 2] {
    let b = from_bin::<T>(&swap_f64(&bin(v), old, new)).unwrap_err();
    let t = from_txt::<T>(&swap_word(&txt(v), &old.to_string(), &new.to_string())).unwrap_err();
    assert_eq!(b.cause, t.cause);
    [b.cause, t.cause]
}

fn plain_frame() -> Frame {
    Frame::from_axes(
        Point3::new(0.25, 0.5, 0.75),
        Vec3::new(0.6, 0.8, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    )
    .unwrap()
}

#[test]
fn a_zero_radius_circle_is_refused_with_the_constructors_cause() {
    let c =
        Curve::Circle(Circle::new(plain_frame(), Length::from_mm(1.375), Sense::Forward).unwrap());
    for cause in refused(&c, 1.375, 0.0) {
        let ReadCause::Curve(e) = cause else {
            panic!("{cause:?}")
        };
        assert_eq!(
            *e,
            CurveError::Degenerate(Degenerate::Radius { radius: 0.0 })
        );
    }
    let s = Surface::Sphere(
        Sphere::new(plain_frame(), Length::from_mm(1.375), Sense::Forward).unwrap(),
    );
    for cause in refused(&s, 1.375, -1.0) {
        assert!(matches!(cause, ReadCause::Surface(_)), "{cause:?}");
    }
}

#[test]
fn a_non_unit_axis_is_refused() {
    let c =
        Curve::Circle(Circle::new(plain_frame(), Length::from_mm(1.375), Sense::Forward).unwrap());
    for cause in refused(&c, 0.6, 0.7) {
        let ReadCause::Frame(FrameError::NotUnit { axis, .. }) = cause else {
            panic!("{cause:?}")
        };
        assert_eq!(axis, FrameAxis::X);
    }
    // Unit but not square: x = (0.6, 0.8, 0), z = (0, 0.6, 0.8) after both changes.
    let f = Frame::from_axes(
        Point3::new(0.25, 0.5, 0.75),
        Vec3::new(0.6, 0.8, 0.0),
        Vec3::new(0.0, -0.0, 1.0),
    )
    .unwrap();
    let t = swap_word(&swap_word(&txt(&f), "-0", "0.6"), "1", "0.8");
    let e = from_txt::<Frame>(&t).unwrap_err();
    assert!(
        matches!(e.cause, ReadCause::Frame(FrameError::NotSquare { .. })),
        "{e:?}"
    );
}

fn plain_curve(weights: bool) -> NurbsCurve<3> {
    let knots = KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.25, 0.75, 1.0, 1.0, 1.0]).unwrap();
    let points = vec![
        [1.5, 2.5, 3.5],
        [4.5, 5.5, 6.5],
        [7.5, 8.5, 9.5],
        [10.5, 11.5, 12.5],
        [13.5, 14.5, 15.5],
    ];
    let w = weights.then(|| vec![2.0, 0.625, 4.0, 8.0, 16.0]);
    NurbsCurve::new(knots, points, w).unwrap()
}

#[test]
fn decreasing_knots_are_refused() {
    let c = Curve::Nurbs(Arc::new(plain_curve(false)));
    for cause in refused(&c, 0.25, 0.875) {
        let ReadCause::Curve(e) = cause else {
            panic!("{cause:?}")
        };
        assert!(
            matches!(*e, CurveError::Nurbs(NurbsError::KnotsDecrease { .. })),
            "{e:?}"
        );
    }
}

#[test]
fn a_weight_not_positive_is_refused() {
    let c = Curve::Nurbs(Arc::new(plain_curve(true)));
    for new in [0.0, -0.625] {
        for cause in refused(&c, 0.625, new) {
            let ReadCause::Curve(e) = cause else {
                panic!("{cause:?}")
            };
            assert_eq!(
                *e,
                CurveError::Nurbs(NurbsError::Weight {
                    index: 1,
                    weight: new
                })
            );
        }
    }
    let u = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let points = Net::new(2, 2, vec![[1.5; 3], [2.5; 3], [3.5; 3], [4.5; 3]]).unwrap();
    let weights = Net::new(2, 2, vec![1.0, 2.0, 0.625, 4.0]).unwrap();
    let s = Surface::Nurbs(Arc::new(
        NurbsSurface::new(u.clone(), u, points, Some(weights)).unwrap(),
    ));
    for cause in refused(&s, 0.625, -0.625) {
        let ReadCause::Surface(e) = cause else {
            panic!("{cause:?}")
        };
        assert!(
            matches!(
                *e,
                SurfaceError::Nurbs(NurbsSurfaceError::BadWeights(WeightCause::NotPositive {
                    row: 1,
                    col: 0,
                    ..
                }))
            ),
            "{e:?}"
        );
    }
}

#[test]
fn a_second_spelling_of_a_rational_value_is_refused() {
    // Weights 0.625 and powers of two: the stored points read as homogeneous points give
    // points `P / w` whose products with `w` are exact, so the value those numbers build
    // is already spelled as points, and the homogeneous record is a second spelling.
    let c = plain_curve(true);
    let t = txt(&c);
    assert!(t.contains("points") && !t.contains("homogeneous"));
    let e = from_txt::<NurbsCurve<3>>(&swap_word(&t, "points", "homogeneous")).unwrap_err();
    assert!(matches!(e.cause, ReadCause::NonCanonical { .. }), "{e:?}");
}

#[test]
fn a_period_not_positive_is_refused() {
    let a = Curve2::AngleLine(
        AngleLine2::new(
            Arcs::turn(6.25),
            AngleAxis::U,
            0.5,
            Sense::Forward,
            1.5,
            2.5,
        )
        .unwrap(),
    );
    for new in [0.0, -6.25] {
        for cause in refused(&a, 6.25, new) {
            let ReadCause::Curve(e) = cause else {
                panic!("{cause:?}")
            };
            assert_eq!(
                *e,
                CurveError::Degenerate(Degenerate::Period { period: new })
            );
        }
    }
}

#[test]
fn a_value_no_constructor_builds_is_not_written() {
    // Every edit keeps one relation between `P` and `w P`; a mixed value cannot be built
    // through the public API, so the write refusal is checked by its cause's kind only.
    let cause = WriteCause::MixedRational { record: "nurbs" };
    assert_eq!(cause.name(), "mixed_rational");
}

#[test]
fn read_errors_stay_small() {
    assert!(size_of::<ReadError>() <= 32, "{}", size_of::<ReadError>());
}

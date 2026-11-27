// Written for solidcorex. Apache-2.0.
//! Lines, circles, ellipses, planes, cylinders, cones, spheres and tori in double-double,
//! exact for the rational cases (a line's and a plane's points and closest points).
//!
//! Parameters are doubles or double-doubles (a closest point's parameter is the latter).
//! Each is parameterised as STEP does (ISO 10303-42), with its frame vectors used as given:
//! a frame that is not quite orthonormal (rounded doubles) is the shape it describes, not a
//! rounding of an ideal one, so a library and its reference see the same shape.
//!
//! - line `P + t D`;
//! - circle `C + r (cos t X + sin t Y)`; ellipse `C + a cos t X + b sin t Y`;
//! - parabola `C + F t^2 X + 2 F t Y`; hyperbola `C + a cosh t X + b sinh t Y`;
//! - plane `P + u X + v Y`;
//! - cylinder `C + r (cos u X + sin u Y) + v Z`;
//! - cone `C + (r + v tan a) (cos u X + sin u Y) + v Z`;
//! - sphere `C + r cos v (cos u X + sin u Y) + r sin v Z`;
//! - torus `C + (R + r cos v) (cos u X + sin u Y) + r sin v Z`.
//!
//! Every one but the parabola and the hyperbola (whose derivatives are written out) is a
//! sum of terms `f(u) g(v) V` with `f` and `g` of the form
//! `a + b t + c cos t + d sin t`, so one derivative rule serves them all.
//!
//! Closest points: exact for the line and the plane; for the others Newton's method in
//! double-double on the parameterisation itself (the gradient of the squared distance),
//! from the closed-form answer for an orthonormal frame. `None` where the foot is not
//! unique or the start is meaningless: on a circle's or surface's axis, at a torus tube's
//! centre line, beyond a cone's apex. The ellipse has no Newton foot here: its start is
//! not closed-form, and Newton's method from a poor start can find the wrong foot. Its
//! certified feet are [`Ellipse::stationary`](crate::project) (T1). The parabola's and the
//! hyperbola's are the certified real roots of an exact cubic and quartic
//! ([`Parabola::stationary`], [`Hyperbola::stationary`]).

use crate::Partials;
use crate::dd::{self, Dd};
use crate::nurbs::Exact;
use crate::poly::Poly;
use crate::rat::Rat;

/// A 3D vector of double-doubles.
pub type V3 = [Dd; 3];

/// An origin and three axes, as given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The origin.
    pub origin: [f64; 3],
    /// The first axis.
    pub x: [f64; 3],
    /// The second axis.
    pub y: [f64; 3],
    /// The third axis (a surface of revolution's axis).
    pub z: [f64; 3],
}

/// A closest point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foot<P> {
    /// Its parameter.
    pub param: P,
    /// The point.
    pub point: V3,
    /// Its distance from the query.
    pub distance: Dd,
}

// ---------------------------------------------------------------- the derivative rule

/// `a + b t + c cos t + d sin t`.
#[derive(Clone, Copy, Debug)]
struct Fun {
    a: Dd,
    b: Dd,
    c: Dd,
    d: Dd,
}

const ZERO: Dd = Dd::new(0.0);

impl Fun {
    fn affine(a: Dd, b: Dd) -> Fun {
        Fun {
            a,
            b,
            c: ZERO,
            d: ZERO,
        }
    }
    fn one() -> Fun {
        Fun::affine(Dd::new(1.0), ZERO)
    }
    fn t() -> Fun {
        Fun::affine(ZERO, Dd::new(1.0))
    }
    fn cos(c: Dd) -> Fun {
        Fun {
            c,
            ..Fun::affine(ZERO, ZERO)
        }
    }
    fn sin(d: Dd) -> Fun {
        Fun {
            d,
            ..Fun::affine(ZERO, ZERO)
        }
    }

    /// The `k`-th derivative at `t`, `(s, c) = (sin t, cos t)`.
    fn at(&self, k: usize, t: Dd, (s, c): (Dd, Dd)) -> Dd {
        // d^k cos = cos, -sin, -cos, sin; d^k sin = sin, cos, -sin, -cos.
        let (dc, ds) = match k % 4 {
            0 => (c, s),
            1 => (-s, c),
            2 => (-c, -s),
            _ => (s, -c),
        };
        let trig = self.c * dc + self.d * ds;
        match k {
            0 => self.a + self.b * t + trig,
            1 => self.b + trig,
            _ => trig,
        }
    }
}

fn scaled(v: V3, f: Dd) -> V3 {
    v.map(|x| x * f)
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn norm(a: V3) -> Dd {
    dd::dot(&a, &a).sqrt()
}

/// A curve `sum f_i(t) V_i`.
struct CurveForm(Vec<(Fun, V3)>);

impl CurveForm {
    fn derivs(&self, t: Dd, order: usize) -> Vec<V3> {
        let sc = dd::sin_cos(t);
        (0..=order)
            .map(|k| {
                self.0.iter().fold([ZERO; 3], |acc, (f, v)| {
                    add(acc, scaled(*v, f.at(k, t, sc)))
                })
            })
            .collect()
    }

    /// Newton's method on `C'(t) . (C(t) - q) = 0` from `t`.
    fn closest(&self, q: V3, mut t: Dd) -> Option<Foot<Dd>> {
        for _ in 0..40 {
            let [c, d1, d2] = <[V3; 3]>::try_from(self.derivs(t, 2)).ok()?;
            let r = sub(c, q);
            let f = dd::dot(&d1, &r);
            let df = dd::dot(&d2, &r) + dd::dot(&d1, &d1);
            if df.hi <= 0.0 {
                return None; // not a minimum nearby
            }
            let step = f / df;
            t = t - step;
            if step.hi.abs() <= 1e-31 * (1.0 + t.hi.abs()) {
                let point = self.derivs(t, 0).into_iter().next()?;
                let distance = norm(sub(point, q));
                return Some(Foot {
                    param: t,
                    point,
                    distance,
                });
            }
        }
        None
    }
}

/// A surface `sum f_i(u) g_i(v) V_i`.
struct SurfaceForm(Vec<(Fun, Fun, V3)>);

impl SurfaceForm {
    fn derivs(&self, (u, v): (Dd, Dd), order: usize) -> Partials<V3> {
        let (su, sv) = (dd::sin_cos(u), dd::sin_cos(v));
        let rows = (0..=order)
            .map(|i| {
                (0..=order - i)
                    .map(|j| {
                        self.0.iter().fold([ZERO; 3], |acc, (f, g, w)| {
                            add(acc, scaled(*w, f.at(i, u, su) * g.at(j, v, sv)))
                        })
                    })
                    .collect()
            })
            .collect();
        Partials::new(rows)
    }

    /// Newton's method on the gradient of `|S(u, v) - q|^2 / 2` from `(u, v)`.
    fn closest(&self, q: V3, (mut u, mut v): (Dd, Dd)) -> Option<Foot<(Dd, Dd)>> {
        for _ in 0..40 {
            let d = self.derivs((u, v), 2);
            let at = |i, j| d.get(i, j).copied().unwrap_or([ZERO; 3]);
            let (s, su, sv, suu, suv, svv) =
                (at(0, 0), at(1, 0), at(0, 1), at(2, 0), at(1, 1), at(0, 2));
            let r = sub(s, q);
            let (gu, gv) = (dd::dot(&su, &r), dd::dot(&sv, &r));
            let a = dd::dot(&suu, &r) + dd::dot(&su, &su);
            let b = dd::dot(&suv, &r) + dd::dot(&su, &sv);
            let c = dd::dot(&svv, &r) + dd::dot(&sv, &sv);
            let det = a * c - b * b;
            if det.hi <= 0.0 || a.hi <= 0.0 {
                return None; // not a minimum nearby
            }
            let (du, dv) = ((c * gu - b * gv) / det, (a * gv - b * gu) / det);
            u = u - du;
            v = v - dv;
            let scale = 1.0 + u.hi.abs().max(v.hi.abs());
            if du.hi.abs().max(dv.hi.abs()) <= 1e-31 * scale {
                let point = self.derivs((u, v), 0).get(0, 0).copied()?;
                let distance = norm(sub(point, q));
                return Some(Foot {
                    param: (u, v),
                    point,
                    distance,
                });
            }
        }
        None
    }
}

fn lift(v: [f64; 3]) -> V3 {
    dd::lift(v)
}

/// `(d . x, d . y, d . z)` and `|d|` for `d = q - origin`: the start of a closed form.
fn local(frame: &Frame, q: [f64; 3]) -> (Dd, Dd, Dd, Dd) {
    let d = sub(lift(q), lift(frame.origin));
    let (x, y, z) = (lift(frame.x), lift(frame.y), lift(frame.z));
    (dd::dot(&d, &x), dd::dot(&d, &y), dd::dot(&d, &z), norm(d))
}

/// Whether `(x, y)` is too near the axis for its angle to mean anything, against `scale`.
fn on_axis(x: Dd, y: Dd, scale: Dd) -> bool {
    let rho = (x * x + y * y).sqrt();
    rho.hi <= dd::pow2(-50) * scale.hi.abs().max(f64::MIN_POSITIVE)
}

/// `cos u g(v) r X + sin u g(v) r Y`.
fn cos_sin_terms(frame: &Frame, g: Fun, r: Dd) -> [(Fun, Fun, V3); 2] {
    let (x, y) = (lift(frame.x), lift(frame.y));
    let one = Dd::new(1.0);
    [
        (Fun::cos(one), g, scaled(x, r)),
        (Fun::sin(one), g, scaled(y, r)),
    ]
}

// ---------------------------------------------------------------- exact helpers

fn rat3(v: [f64; 3]) -> Option<[Rat; 3]> {
    Some([
        Rat::from_f64(v[0])?,
        Rat::from_f64(v[1])?,
        Rat::from_f64(v[2])?,
    ])
}

fn rdot(a: &[Rat; 3], b: &[Rat; 3]) -> Rat {
    a.iter().zip(b).fold(Rat::zero(), |s, (x, y)| s + x * y)
}

fn rsub(a: &[Rat; 3], b: &[Rat; 3]) -> [Rat; 3] {
    [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
}

/// `p + sum c_i v_i`, exactly.
fn raxpy(p: &[Rat; 3], terms: &[(&Rat, &[Rat; 3])]) -> Exact<3> {
    let v: Vec<Rat> = p
        .iter()
        .enumerate()
        .map(|(k, p)| {
            terms.iter().fold(p.clone(), |s, (c, v)| {
                s + v.get(k).map_or_else(Rat::zero, |x| *c * x)
            })
        })
        .collect();
    Exact::of(&v)
}

// ---------------------------------------------------------------- curves

/// The line `P + t D`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    /// `P`.
    pub origin: [f64; 3],
    /// `D`, as given (not normalised).
    pub direction: [f64; 3],
}

impl Line {
    fn form(&self) -> CurveForm {
        CurveForm(vec![
            (Fun::one(), lift(self.origin)),
            (Fun::t(), lift(self.direction)),
        ])
    }

    /// `C^(k)(t)` for `k = 0..=order`.
    pub fn derivs(&self, t: impl Into<Dd>, order: usize) -> Vec<V3> {
        self.form().derivs(t.into(), order)
    }

    /// `C(t)` exactly; `None` for a non-finite value.
    pub fn point_exact(&self, t: f64) -> Option<Exact<3>> {
        let (p, d, t) = (rat3(self.origin)?, rat3(self.direction)?, Rat::from_f64(t)?);
        Some(raxpy(&p, &[(&t, &d)]))
    }

    /// The closest point to `q`, exactly: `t = (q - P) . D / D . D`. `None` for a zero
    /// direction or a non-finite value.
    pub fn closest_exact(&self, q: [f64; 3]) -> Option<(Rat, Exact<3>)> {
        let (p, d, q) = (rat3(self.origin)?, rat3(self.direction)?, rat3(q)?);
        let t = rdot(&rsub(&q, &p), &d).checked_div(&rdot(&d, &d))?;
        let point = raxpy(&p, &[(&t, &d)]);
        Some((t, point))
    }
}

/// The circle `C + r (cos t X + sin t Y)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    /// The centre and axes (`z` the normal, unused but for the frame's sake).
    pub frame: Frame,
    /// `r`.
    pub radius: f64,
}

impl Circle {
    fn form(&self) -> CurveForm {
        let r = Dd::new(self.radius);
        CurveForm(vec![
            (Fun::one(), lift(self.frame.origin)),
            (Fun::cos(r), lift(self.frame.x)),
            (Fun::sin(r), lift(self.frame.y)),
        ])
    }

    /// `C^(k)(t)` for `k = 0..=order`.
    pub fn derivs(&self, t: impl Into<Dd>, order: usize) -> Vec<V3> {
        self.form().derivs(t.into(), order)
    }

    /// The closest point to `q`; `None` on the axis.
    pub fn closest(&self, q: [f64; 3]) -> Option<Foot<Dd>> {
        let (x, y, _, len) = local(&self.frame, q);
        if on_axis(x, y, len + Dd::new(self.radius)) {
            return None;
        }
        self.form().closest(lift(q), dd::atan2(y, x))
    }
}

/// The ellipse `C + a cos t X + b sin t Y`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ellipse {
    /// The centre and axes.
    pub frame: Frame,
    /// `a`, along `X`.
    pub semi_x: f64,
    /// `b`, along `Y`.
    pub semi_y: f64,
}

impl Ellipse {
    /// `C^(k)(t)` for `k = 0..=order`.
    pub fn derivs(&self, t: impl Into<Dd>, order: usize) -> Vec<V3> {
        CurveForm(vec![
            (Fun::one(), lift(self.frame.origin)),
            (Fun::cos(Dd::new(self.semi_x)), lift(self.frame.x)),
            (Fun::sin(Dd::new(self.semi_y)), lift(self.frame.y)),
        ])
        .derivs(t.into(), order)
    }
}

/// The parabola `C + F t^2 X + 2 F t Y` (STEP's), its vertex `C`, axis `X`, focal
/// distance `F`. A reversed one is the same with `Y` negated in the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parabola {
    /// The vertex and axes.
    pub frame: Frame,
    /// `F`.
    pub focal: f64,
}

impl Parabola {
    /// `C^(k)(t)` for `k = 0..=order`.
    pub fn derivs(&self, t: impl Into<Dd>, order: usize) -> Vec<V3> {
        let t = t.into();
        let f = Dd::new(self.focal);
        let (o, x, y) = (
            lift(self.frame.origin),
            lift(self.frame.x),
            lift(self.frame.y),
        );
        (0..=order)
            .map(|k| match k {
                0 => add(o, add(scaled(x, f * t * t), scaled(y, f * t * 2.0))),
                1 => add(scaled(x, f * t * 2.0), scaled(y, f * 2.0)),
                2 => scaled(x, f * 2.0),
                _ => [ZERO; 3],
            })
            .collect()
    }

    /// Every stationary point of the distance from `q`, from the real roots of the exact
    /// cubic `C'(s) . (C(s) - q) = 2|A|^2 s^3 + 3 A.B s^2 + (|B|^2 + 2 A.D) s + B.D` with
    /// `A = F X`, `B = 2 F Y`, `D = C - q`, each isolated with certainty; `None` for a
    /// non-finite value.
    pub fn stationary(&self, q: [f64; 3]) -> Option<Vec<Dd>> {
        let f = Rat::from_f64(self.focal)?;
        let two = Rat::from_f64(2.0)?;
        let a = rat3(self.frame.x)?.map(|v| &v * &f);
        let b = rat3(self.frame.y)?.map(|v| &(&v * &f) * &two);
        let d = rsub(&rat3(self.frame.origin)?, &rat3(q)?);
        let three = Rat::from_f64(3.0)?;
        let poly = Poly::new(vec![
            rdot(&b, &d),
            rdot(&b, &b) + &two * &rdot(&a, &d),
            &three * &rdot(&a, &b),
            &two * &rdot(&a, &a),
        ]);
        Some(roots(&poly).into_iter().map(|r| r.at.to_dd()).collect())
    }

    /// The nearest stationary point to `q`; `None` where two are within `2^-80` relative
    /// of each other's distance (a tie the double result may take either way).
    pub fn nearest(&self, q: [f64; 3]) -> Option<Foot<Dd>> {
        let feet = self.stationary(q)?;
        nearest(
            feet.into_iter()
                .filter_map(|t| Some((t, self.derivs(t, 0).into_iter().next()?))),
            q,
        )
    }
}

/// One branch of the hyperbola `C + a cosh t X + b sinh t Y` (STEP's), its centre `C`, the
/// branch about `+X`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hyperbola {
    /// The centre and axes.
    pub frame: Frame,
    /// `a`, along `X`.
    pub semi_x: f64,
    /// `b`, along `Y` (signed: negative for a reversed one).
    pub semi_y: f64,
}

impl Hyperbola {
    /// `C^(k)(t)` for `k = 0..=order`.
    pub fn derivs(&self, t: impl Into<Dd>, order: usize) -> Vec<V3> {
        let (sh, ch) = dd::sinh_cosh(t.into());
        let (a, b) = (Dd::new(self.semi_x), Dd::new(self.semi_y));
        let (o, x, y) = (
            lift(self.frame.origin),
            lift(self.frame.x),
            lift(self.frame.y),
        );
        (0..=order)
            .map(|k| {
                let (cx, cy) = if k % 2 == 0 {
                    (a * ch, b * sh)
                } else {
                    (a * sh, b * ch)
                };
                let v = add(scaled(x, cx), scaled(y, cy));
                if k == 0 { add(o, v) } else { v }
            })
            .collect()
    }

    /// Every stationary point of the distance from `q`: with `x = e^t`, `4 x^2` times
    /// `C'(t) . (C(t) - q)` is the exact quartic
    /// `(|A|^2 + |B|^2)(x^4 - 1) + 2 A.B (x^4 + 1) + 2 A.D (x^3 - x) + 2 B.D (x^3 + x)`
    /// with `A = a X`, `B = b Y`, `D = C - q`; its positive roots, isolated with certainty,
    /// are the feet as `t = ln x`. `None` for a non-finite value.
    pub fn stationary(&self, q: [f64; 3]) -> Option<Vec<Dd>> {
        let (sa, sb) = (Rat::from_f64(self.semi_x)?, Rat::from_f64(self.semi_y)?);
        let a = rat3(self.frame.x)?.map(|v| &v * &sa);
        let b = rat3(self.frame.y)?.map(|v| &v * &sb);
        let d = rsub(&rat3(self.frame.origin)?, &rat3(q)?);
        let two = Rat::from_f64(2.0)?;
        let k = rdot(&a, &a) + rdot(&b, &b);
        let ab = &two * &rdot(&a, &b);
        let (ad, bd) = (&two * &rdot(&a, &d), &two * &rdot(&b, &d));
        let poly = Poly::new(vec![&ab - &k, &bd - &ad, Rat::zero(), &ad + &bd, &k + &ab]);
        Some(
            roots(&poly)
                .into_iter()
                .filter(|r| r.at.lo().signum() > 0)
                .map(|r| dd::ln_dd(r.at.to_dd()))
                .collect(),
        )
    }

    /// The nearest stationary point to `q`, as [`Parabola::nearest`].
    pub fn nearest(&self, q: [f64; 3]) -> Option<Foot<Dd>> {
        let feet = self.stationary(q)?;
        nearest(
            feet.into_iter()
                .filter_map(|t| Some((t, self.derivs(t, 0).into_iter().next()?))),
            q,
        )
    }
}

/// The real roots of `p`, each to `2^-110`.
fn roots(p: &Poly) -> Vec<crate::poly::Root> {
    p.real_roots(&Rat::one().scale(-110))
}

/// The nearest of `feet` to `q`, `None` for none or a near tie.
fn nearest(feet: impl Iterator<Item = (Dd, V3)>, q: [f64; 3]) -> Option<Foot<Dd>> {
    let q = lift(q);
    let mut all: Vec<Foot<Dd>> = feet
        .map(|(param, point)| Foot {
            param,
            point,
            distance: norm(sub(point, q)),
        })
        .collect();
    all.sort_by(|a, b| a.distance.hi.total_cmp(&b.distance.hi));
    let first = *all.first()?;
    if let Some(second) = all.get(1) {
        let gap = (second.distance - first.distance).hi;
        if gap <= dd::pow2(-80) * second.distance.hi.max(f64::MIN_POSITIVE) {
            return None;
        }
    }
    Some(first)
}

// ---------------------------------------------------------------- surfaces

/// The plane `P + u X + v Y`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// `P`, `X`, `Y` (and the normal `z`, unused).
    pub frame: Frame,
}

impl Plane {
    /// The partials up to `order` at `(u, v)`.
    pub fn derivs(&self, (u, v): (impl Into<Dd>, impl Into<Dd>), order: usize) -> Partials<V3> {
        let f = &self.frame;
        SurfaceForm(vec![
            (Fun::one(), Fun::one(), lift(f.origin)),
            (Fun::t(), Fun::one(), lift(f.x)),
            (Fun::one(), Fun::t(), lift(f.y)),
        ])
        .derivs((u.into(), v.into()), order)
    }

    /// `S(u, v)` exactly; `None` for a non-finite value.
    pub fn point_exact(&self, (u, v): (f64, f64)) -> Option<Exact<3>> {
        let f = &self.frame;
        let (p, x, y) = (rat3(f.origin)?, rat3(f.x)?, rat3(f.y)?);
        let (u, v) = (Rat::from_f64(u)?, Rat::from_f64(v)?);
        Some(raxpy(&p, &[(&u, &x), (&v, &y)]))
    }

    /// The closest point to `q`, exactly, from the normal equations
    /// `[X.X X.Y; X.Y Y.Y] (u, v) = ((q - P).X, (q - P).Y)`. `None` for parallel axes or a
    /// non-finite value.
    pub fn closest_exact(&self, q: [f64; 3]) -> Option<((Rat, Rat), Exact<3>)> {
        let f = &self.frame;
        let (p, x, y, q) = (rat3(f.origin)?, rat3(f.x)?, rat3(f.y)?, rat3(q)?);
        let d = rsub(&q, &p);
        let (a, b, c) = (rdot(&x, &x), rdot(&x, &y), rdot(&y, &y));
        let (g, h) = (rdot(&d, &x), rdot(&d, &y));
        let det = &a * &c - &b * &b;
        let u = (&c * &g - &b * &h).checked_div(&det)?;
        let v = (&a * &h - &b * &g).checked_div(&det)?;
        let point = raxpy(&p, &[(&u, &x), (&v, &y)]);
        Some(((u, v), point))
    }
}

/// The cylinder `C + r (cos u X + sin u Y) + v Z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cylinder {
    /// The axis frame.
    pub frame: Frame,
    /// `r`.
    pub radius: f64,
}

impl Cylinder {
    fn form(&self) -> SurfaceForm {
        let f = &self.frame;
        let [a, b] = cos_sin_terms(f, Fun::one(), Dd::new(self.radius));
        SurfaceForm(vec![
            (Fun::one(), Fun::one(), lift(f.origin)),
            a,
            b,
            (Fun::one(), Fun::t(), lift(f.z)),
        ])
    }

    /// The partials up to `order` at `(u, v)`.
    pub fn derivs(&self, (u, v): (impl Into<Dd>, impl Into<Dd>), order: usize) -> Partials<V3> {
        self.form().derivs((u.into(), v.into()), order)
    }

    /// The closest point to `q`; `None` on the axis.
    pub fn closest(&self, q: [f64; 3]) -> Option<Foot<(Dd, Dd)>> {
        let (x, y, z, len) = local(&self.frame, q);
        if on_axis(x, y, len + Dd::new(self.radius)) {
            return None;
        }
        let zz = lift(self.frame.z);
        let v = z / dd::dot(&zz, &zz);
        self.form().closest(lift(q), (dd::atan2(y, x), v))
    }
}

/// The cone `C + (r + v tan a) (cos u X + sin u Y) + v Z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    /// The axis frame (`origin` at the reference circle's centre).
    pub frame: Frame,
    /// `r`, the radius at `v = 0`.
    pub radius: f64,
    /// `a`, the semi-angle in radians.
    pub semi_angle: f64,
}

impl Cone {
    fn slope(&self) -> Dd {
        let (s, c) = dd::sin_cos(Dd::new(self.semi_angle));
        s / c
    }

    fn form(&self) -> SurfaceForm {
        let f = &self.frame;
        let g = Fun::affine(Dd::new(self.radius), self.slope());
        let (x, y) = (lift(f.x), lift(f.y));
        SurfaceForm(vec![
            (Fun::one(), Fun::one(), lift(f.origin)),
            (Fun::cos(Dd::new(1.0)), g, x),
            (Fun::sin(Dd::new(1.0)), g, y),
            (Fun::one(), Fun::t(), lift(f.z)),
        ])
    }

    /// The partials up to `order` at `(u, v)`.
    pub fn derivs(&self, (u, v): (impl Into<Dd>, impl Into<Dd>), order: usize) -> Partials<V3> {
        self.form().derivs((u.into(), v.into()), order)
    }

    /// The closest point to `q`; `None` on the axis or where the nearest point of the
    /// generator in `q`'s half-plane lies beyond the apex.
    pub fn closest(&self, q: [f64; 3]) -> Option<Foot<(Dd, Dd)>> {
        let (x, y, z, len) = local(&self.frame, q);
        let r = Dd::new(self.radius);
        if on_axis(x, y, len + r.abs()) {
            return None;
        }
        // The generator (r + v t, v) in the (rho, z) half-plane, projected onto.
        let t = self.slope();
        let rho = (x * x + y * y).sqrt();
        let v = ((rho - r) * t + z) / (t * t + Dd::new(1.0));
        if (r + v * t).hi <= 0.0 {
            return None;
        }
        self.form().closest(lift(q), (dd::atan2(y, x), v))
    }
}

/// The sphere `C + r cos v (cos u X + sin u Y) + r sin v Z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    /// The centre and axes.
    pub frame: Frame,
    /// `r`.
    pub radius: f64,
}

impl Sphere {
    fn form(&self) -> SurfaceForm {
        let f = &self.frame;
        let r = Dd::new(self.radius);
        let cos_v = Fun::cos(Dd::new(1.0));
        let [a, b] = cos_sin_terms(f, cos_v, r);
        SurfaceForm(vec![
            (Fun::one(), Fun::one(), lift(f.origin)),
            a,
            b,
            (Fun::one(), Fun::sin(Dd::new(1.0)), scaled(lift(f.z), r)),
        ])
    }

    /// The partials up to `order` at `(u, v)`.
    pub fn derivs(&self, (u, v): (impl Into<Dd>, impl Into<Dd>), order: usize) -> Partials<V3> {
        self.form().derivs((u.into(), v.into()), order)
    }

    /// The closest point to `q`; `None` on the polar axis (the poles' longitude is free).
    pub fn closest(&self, q: [f64; 3]) -> Option<Foot<(Dd, Dd)>> {
        let (x, y, z, len) = local(&self.frame, q);
        if on_axis(x, y, len + Dd::new(self.radius)) {
            return None;
        }
        let rho = (x * x + y * y).sqrt();
        self.form()
            .closest(lift(q), (dd::atan2(y, x), dd::atan2(z, rho)))
    }
}

/// The torus `C + (R + r cos v) (cos u X + sin u Y) + r sin v Z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Torus {
    /// The centre and axes.
    pub frame: Frame,
    /// `R`, the distance from the axis to the tube's centre.
    pub major: f64,
    /// `r`, the tube's radius.
    pub minor: f64,
}

impl Torus {
    fn form(&self) -> SurfaceForm {
        let f = &self.frame;
        let g = Fun {
            a: Dd::new(self.major),
            b: ZERO,
            c: Dd::new(self.minor),
            d: ZERO,
        };
        SurfaceForm(vec![
            (Fun::one(), Fun::one(), lift(f.origin)),
            (Fun::cos(Dd::new(1.0)), g, lift(f.x)),
            (Fun::sin(Dd::new(1.0)), g, lift(f.y)),
            (
                Fun::one(),
                Fun::sin(Dd::new(1.0)),
                scaled(lift(f.z), Dd::new(self.minor)),
            ),
        ])
    }

    /// The partials up to `order` at `(u, v)`.
    pub fn derivs(&self, (u, v): (impl Into<Dd>, impl Into<Dd>), order: usize) -> Partials<V3> {
        self.form().derivs((u.into(), v.into()), order)
    }

    /// The closest point to `q`; `None` on the axis or on the tube's centre circle.
    pub fn closest(&self, q: [f64; 3]) -> Option<Foot<(Dd, Dd)>> {
        let (x, y, z, len) = local(&self.frame, q);
        let scale = len + Dd::new(self.major).abs() + Dd::new(self.minor).abs();
        if on_axis(x, y, scale) {
            return None;
        }
        let rho = (x * x + y * y).sqrt();
        let off = rho - Dd::new(self.major);
        if on_axis(off, z, scale) {
            return None;
        }
        self.form()
            .closest(lift(q), (dd::atan2(y, x), dd::atan2(z, off)))
    }
}

// Written for solidcorex. Apache-2.0.
//! The pin's extents under scx's API, so one diff body calls both sides.
//!
//! Each adapter is the pin's own call, shaped the way scx takes and returns it:
//! - transforms by value;
//! - `Option` where scx refuses an inverted result;
//! - `x`/`y`/`z` names;
//! - the hull accumulators;
//! - `AtSeam` and `ParamOrder` in place of the pin's bools.
//!
//! The adapters decide nothing of their own. Where scx's result differs from the pin's,
//! the diff sees the difference.
//!
//! The pin's inherent methods of the same name take `&self`. These take `self`, so a method
//! call on a value finds the adapter first. A call inside an adapter names the pin's
//! inherent method by path (`Extent1d::union(&a, &b)`), so the adapter never calls itself.

use solidcore::error::Result;
use solidcore::geom::vector2d::{Point2, Vec2};
use solidcore::geom::vector3d::Point3;

pub mod extent1d {
    pub use solidcore::geom::extent1d::*;

    /// scx's `AtSeam`: the pin's `dont_cross` is `Split`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AtSeam {
        Join,
        Split,
    }

    /// scx's `Hull1d`, built on the pin's inside-out `new` and `add_value`.
    #[derive(Debug, Clone, Copy)]
    pub struct Hull1d(Extent1d);
    impl Hull1d {
        pub const EMPTY: Self = Self(Extent1d::new());
        pub fn add(&mut self, v: f64) {
            self.0.add_value(v);
        }
        pub fn add_extent(&mut self, e: Extent1d) {
            self.0 = Extent1d::union(&self.0, &e);
        }
        pub fn union(self, o: Self) -> Self {
            Self(Extent1d::union(&self.0, &o.0))
        }
        pub fn finish(self) -> Option<Extent1d> {
            (!self.0.has_negative_length()).then_some(self.0)
        }
    }
    impl FromIterator<f64> for Hull1d {
        fn from_iter<I: IntoIterator<Item = f64>>(values: I) -> Self {
            let mut h = Self::EMPTY;
            for v in values {
                h.add(v);
            }
            h
        }
    }
}

pub mod extent2d {
    pub use solidcore::geom::extent2d::*;
    use solidcore::geom::vector2d::Point2;

    /// scx's `ParamOrder`: the pin's `switch_params` is `Swapped`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ParamOrder {
        Kept,
        Swapped,
    }

    /// scx's `Hull2d`, built on the pin's inside-out `new` and `add_point`.
    #[derive(Debug, Clone, Copy)]
    pub struct Hull2d(Extent2d);
    impl Hull2d {
        pub const EMPTY: Self = Self(Extent2d::new());
        pub fn add(&mut self, p: Point2) {
            self.0.add_point(p);
        }
        pub fn add_extent(&mut self, e: Extent2d) {
            self.0 = Extent2d::union(&self.0, &e);
        }
        pub fn union(self, o: Self) -> Self {
            Self(Extent2d::union(&self.0, &o.0))
        }
        pub fn finish(self) -> Option<Extent2d> {
            (!self.0.has_negative_area()).then_some(self.0)
        }
    }
    impl FromIterator<Point2> for Hull2d {
        fn from_iter<I: IntoIterator<Item = Point2>>(points: I) -> Self {
            let mut h = Self::EMPTY;
            for p in points {
                h.add(p);
            }
            h
        }
    }
}

pub mod extent3d {
    pub use solidcore::geom::extent3d::*;
    use solidcore::geom::vector3d::Point3;

    /// scx's `Hull3d`, built on the pin's inside-out `new` and `add_point`.
    #[derive(Debug, Clone, Copy)]
    pub struct Hull3d(Extent3d);
    impl Hull3d {
        pub const EMPTY: Self = Self(Extent3d::new());
        pub fn add(&mut self, p: Point3) {
            self.0.add_point(p);
        }
        pub fn add_extent(&mut self, e: Extent3d) {
            self.0 = Extent3d::union(&self.0, &e);
        }
        pub fn union(self, o: Self) -> Self {
            Self(Extent3d::union(&self.0, &o.0))
        }
        pub fn finish(self) -> Option<Extent3d> {
            (!self.0.has_negative_volume()).then_some(self.0)
        }
    }
    impl FromIterator<Point3> for Hull3d {
        fn from_iter<I: IntoIterator<Item = Point3>>(points: I) -> Self {
            let mut h = Self::EMPTY;
            for p in points {
                h.add(p);
            }
            h
        }
    }
}

use extent1d::{AtSeam, Extent1d, ExtentNd, Hull1d};
use extent2d::{Extent2d, Hull2d, ParamOrder};
use extent3d::{Extent3d, Hull3d};

/// `self` when the pin's check `inverted` passed.
fn unless<T>(t: T, inverted: bool) -> Option<T> {
    (!inverted).then_some(t)
}

/// The pin's `ExtentNd::new(dim)` with each interval set through `get_mut`.
pub fn end(v: Vec<Extent1d>) -> ExtentNd {
    let mut n = ExtentNd::new(v.len());
    for (i, e) in v.into_iter().enumerate() {
        if let Some(s) = n.get_mut(i) {
            *s = e;
        }
    }
    n
}

pub trait E1: Sized {
    fn point(v: f64) -> Self;
    fn spanning(a: f64, b: f64) -> Self;
    fn from_bounds(min: f64, max: f64) -> Option<Self>;
    fn hull<I: IntoIterator<Item = f64>>(values: I) -> Option<Self>;
    fn with_min(self, min: f64) -> Option<Self>;
    fn with_max(self, max: f64) -> Option<Self>;
    fn negated(self) -> Self;
    fn translated(self, v: f64) -> Self;
    fn expanded(self, d: f64) -> Self;
    fn shrunk(self, d: f64) -> Option<Self>;
    fn expanded_relative(self, factor: f64) -> Self;
    fn scaled(self, s: f64) -> Self;
    fn union(self, other: Self) -> Self;
    fn intersect(self, other: Self) -> Option<Self>;
    fn intersect_periodic(self, other: &Self, period: f64, at: AtSeam) -> Result<Vec<Self>>;
}

impl E1 for Extent1d {
    fn point(v: f64) -> Self {
        Self::from_value(v)
    }
    fn spanning(a: f64, b: f64) -> Self {
        Self::from_min_max(a, b)
    }
    fn from_bounds(min: f64, max: f64) -> Option<Self> {
        let mut e = Self::new();
        e.set_min_max(min, max).ok().map(|()| e)
    }
    fn hull<I: IntoIterator<Item = f64>>(values: I) -> Option<Self> {
        values.into_iter().collect::<Hull1d>().finish()
    }
    fn with_min(mut self, min: f64) -> Option<Self> {
        self.set_min(min).ok().map(|()| self)
    }
    fn with_max(mut self, max: f64) -> Option<Self> {
        self.set_max(max).ok().map(|()| self)
    }
    fn negated(mut self) -> Self {
        self.negate();
        self
    }
    fn translated(mut self, v: f64) -> Self {
        self.translate(v);
        self
    }
    fn expanded(mut self, d: f64) -> Self {
        self.expand_absolute(d);
        self
    }
    fn shrunk(mut self, d: f64) -> Option<Self> {
        self.expand_absolute(-d);
        unless(self, self.has_negative_length())
    }
    fn expanded_relative(mut self, factor: f64) -> Self {
        self.expand_relative(factor);
        self
    }
    fn scaled(mut self, s: f64) -> Self {
        self.scale(s);
        self
    }
    fn union(self, other: Self) -> Self {
        Self::union(&self, &other)
    }
    fn intersect(self, other: Self) -> Option<Self> {
        Self::intersect(&self, &other).ok()
    }
    fn intersect_periodic(self, other: &Self, period: f64, at: AtSeam) -> Result<Vec<Self>> {
        Self::intersect_periodic(&self, other, period, at == AtSeam::Split)
    }
}

pub trait E2: Sized {
    fn spanning(a: Point2, b: Point2) -> Self;
    fn from_corners(min: Point2, max: Point2) -> Option<Self>;
    fn from_intervals(x: Extent1d, y: Extent1d) -> Self;
    fn hull<I: IntoIterator<Item = Point2>>(points: I) -> Option<Self>;
    fn with_x_interval(self, ivl: Extent1d) -> Self;
    fn with_y_interval(self, ivl: Extent1d) -> Self;
    fn transposed(self) -> Self;
    fn expanded(self, d: f64) -> Self;
    fn expanded_vec(self, d: Vec2) -> Self;
    fn shrunk(self, d: f64) -> Option<Self>;
    fn expanded_relative(self, factor: f64) -> Self;
    fn scaled(self, s: f64) -> Self;
    fn scaled_xy(self, sx: f64, sy: f64) -> Self;
    fn x_min(&self) -> f64;
    fn x_max(&self) -> f64;
    fn y_min(&self) -> f64;
    fn y_max(&self) -> f64;
    fn x_interval(&self) -> Extent1d;
    fn y_interval(&self) -> Extent1d;
    fn union(self, other: Self) -> Self;
    fn intersect(self, other: Self) -> Option<Self>;
    fn x_left_eval(&self, u: f64) -> bool;
    fn y_left_eval(&self, v: f64) -> bool;
    fn point_map_to_domain(self, p: Point2, other: &Self, order: ParamOrder) -> Result<Point2>;
}

impl E2 for Extent2d {
    fn spanning(a: Point2, b: Point2) -> Self {
        Self::from_points(a, b)
    }
    fn from_corners(min: Point2, max: Point2) -> Option<Self> {
        let mut e = Self::new();
        e.set_min_max(min, max).ok().map(|()| e)
    }
    fn from_intervals(x: Extent1d, y: Extent1d) -> Self {
        Self::from_bounds(x.min(), y.min(), x.max(), y.max())
    }
    fn hull<I: IntoIterator<Item = Point2>>(points: I) -> Option<Self> {
        points.into_iter().collect::<Hull2d>().finish()
    }
    fn with_x_interval(mut self, ivl: Extent1d) -> Self {
        self.set_u_interval(&ivl);
        self
    }
    fn with_y_interval(mut self, ivl: Extent1d) -> Self {
        self.set_v_interval(&ivl);
        self
    }
    fn transposed(mut self) -> Self {
        self.transpose();
        self
    }
    fn expanded(mut self, d: f64) -> Self {
        self.expand_absolute(d);
        self
    }
    fn expanded_vec(mut self, d: Vec2) -> Self {
        self.expand_absolute_vec(d);
        self
    }
    fn shrunk(mut self, d: f64) -> Option<Self> {
        self.expand_absolute(-d);
        unless(self, self.has_negative_area())
    }
    fn expanded_relative(mut self, factor: f64) -> Self {
        self.expand_relative(factor);
        self
    }
    fn scaled(mut self, s: f64) -> Self {
        self.scale(s, None);
        self
    }
    fn scaled_xy(mut self, sx: f64, sy: f64) -> Self {
        self.scale(sx, Some(sy));
        self
    }
    fn x_min(&self) -> f64 {
        self.u_min()
    }
    fn x_max(&self) -> f64 {
        self.u_max()
    }
    fn y_min(&self) -> f64 {
        self.v_min()
    }
    fn y_max(&self) -> f64 {
        self.v_max()
    }
    fn x_interval(&self) -> Extent1d {
        self.u_interval()
    }
    fn y_interval(&self) -> Extent1d {
        self.v_interval()
    }
    fn union(self, other: Self) -> Self {
        Self::union(&self, &other)
    }
    fn intersect(self, other: Self) -> Option<Self> {
        Self::intersect(&self, &other).ok()
    }
    fn x_left_eval(&self, u: f64) -> bool {
        self.u_left_eval(u)
    }
    fn y_left_eval(&self, v: f64) -> bool {
        self.v_left_eval(v)
    }
    fn point_map_to_domain(self, p: Point2, other: &Self, order: ParamOrder) -> Result<Point2> {
        Self::point_map_to_domain(&self, p, other, order == ParamOrder::Swapped)
    }
}

pub trait E3: Sized {
    fn spanning(a: Point3, b: Point3) -> Self;
    fn from_corners(min: Point3, max: Point3) -> Option<Self>;
    fn from_intervals(x: Extent1d, y: Extent1d, z: Extent1d) -> Self;
    fn hull<I: IntoIterator<Item = Point3>>(points: I) -> Option<Self>;
    fn x_min(&self) -> f64;
    fn x_max(&self) -> f64;
    fn y_min(&self) -> f64;
    fn y_max(&self) -> f64;
    fn z_min(&self) -> f64;
    fn z_max(&self) -> f64;
    fn x_interval(&self) -> Extent1d;
    fn y_interval(&self) -> Extent1d;
    fn z_interval(&self) -> Extent1d;
    fn with_x_interval(self, ivl: Extent1d) -> Self;
    fn with_y_interval(self, ivl: Extent1d) -> Self;
    fn union(self, other: Self) -> Self;
    fn intersect(self, other: Self) -> Option<Self>;
    fn expanded(self, d: f64) -> Self;
    fn shrunk(self, d: f64) -> Option<Self>;
    fn expanded_relative(self, factor: f64) -> Self;
}

impl E3 for Extent3d {
    fn spanning(a: Point3, b: Point3) -> Self {
        Self::from_points(a, b)
    }
    fn from_corners(min: Point3, max: Point3) -> Option<Self> {
        let mut e = Self::new();
        e.set_min_max(min, max).ok().map(|()| e)
    }
    fn from_intervals(x: Extent1d, y: Extent1d, z: Extent1d) -> Self {
        Self::from_bounds(x.min(), y.min(), z.min(), x.max(), y.max(), z.max())
    }
    fn hull<I: IntoIterator<Item = Point3>>(points: I) -> Option<Self> {
        points.into_iter().collect::<Hull3d>().finish()
    }
    fn x_min(&self) -> f64 {
        self.u_min()
    }
    fn x_max(&self) -> f64 {
        self.u_max()
    }
    fn y_min(&self) -> f64 {
        self.v_min()
    }
    fn y_max(&self) -> f64 {
        self.v_max()
    }
    fn z_min(&self) -> f64 {
        self.w_min()
    }
    fn z_max(&self) -> f64 {
        self.w_max()
    }
    fn x_interval(&self) -> Extent1d {
        self.u_interval()
    }
    fn y_interval(&self) -> Extent1d {
        self.v_interval()
    }
    fn z_interval(&self) -> Extent1d {
        self.w_interval()
    }
    fn with_x_interval(mut self, ivl: Extent1d) -> Self {
        self.set_u_interval(&ivl);
        self
    }
    fn with_y_interval(mut self, ivl: Extent1d) -> Self {
        self.set_v_interval(&ivl);
        self
    }
    fn union(self, other: Self) -> Self {
        Self::union(&self, &other)
    }
    fn intersect(self, other: Self) -> Option<Self> {
        Self::intersect(&self, &other).ok()
    }
    fn expanded(mut self, d: f64) -> Self {
        self.expand_absolute(d);
        self
    }
    fn shrunk(mut self, d: f64) -> Option<Self> {
        self.expand_absolute(-d);
        unless(self, self.has_negative_volume())
    }
    fn expanded_relative(mut self, factor: f64) -> Self {
        self.expand_relative(factor);
        self
    }
}

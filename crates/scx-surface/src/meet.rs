// Written for solidcorex. Apache-2.0.
//! Whether two tolerant points are one point (research 74 §30, SU8; the pin's
//! `query/vertex_meet`, not in SMLib): each is a ball, its point with its accuracy as the
//! radius (a tolerant vertex), and the two are one when the balls touch. A file may write one
//! corner as two vertices at one point; a reader joins them by this test.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `points_meet(a, a_tol, b, b_tol) -> Option<f64>`: `None` for apart, and also for a point or tolerance not finite or a tolerance negative | [`points_meet`]`-> Result<`[`Meet`]`, `[`MeetError`]`>`: apart is [`Meet::Apart`]; a point not finite is [`MeetError::NotFinite`]; a tolerance is an [`Accuracy`], which cannot be negative or not finite |
//! | the test is `a.distance(b) <= a_tol + b_tol` on rounded numbers | the distance is the enclosure of `gap::point_point_gap` (SU6) against the rounded-out sum: [`Meet::One`] when it is at most the sum however it rounds, [`Meet::Apart`] when it is above it however it rounds, else [`Meet::Undecided`] (R2) |
//! | the joined tolerance is `max(a_tol, b_tol + distance)` rounded to nearest | the same, with the distance's upper bound, rounded up, so the joined ball holds `b`'s |
//!
//! The one case the pin decides and this does not is a tie to the last bit: balls that touch
//! exactly (a 3-4-5 triangle against tolerances 2 and 3), or miss by an ulp. The enclosure
//! of a distance is a few ulps wide, so those come back [`Meet::Undecided`] with the
//! bracket, for the caller to take as it takes any bracket that straddles its allowance.

use core::fmt;

use scx_num::eft::two_sum;
use scx_num::error::ErrorKind;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;

use crate::gap::{Bracket, point_point_gap};

/// Whether two tolerant points are one.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Meet {
    /// The balls touch. The first point's ball with this accuracy holds the second's: the
    /// least such radius, `max(a, b + distance)`, rounded up.
    One {
        /// The accuracy the first point takes as the one point.
        accuracy: Accuracy,
    },
    /// The balls are apart, the distance certainly above the sum of the accuracies.
    Apart,
    /// The distance is within rounding of the sum of the accuracies.
    Undecided {
        /// The certified distance.
        distance: Bracket,
    },
}

/// Why two points have no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeetError {
    /// A coordinate is not finite, or the distance or the joined accuracy overflows.
    NotFinite,
}

impl MeetError {
    /// The cause's kind (research 62 §10).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Refused
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "meet_not_finite"
    }
}

impl fmt::Display for MeetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a point, their distance or the joined accuracy is not finite")
    }
}

impl std::error::Error for MeetError {}

/// `a + b` rounded down: the double at or below the exact sum.
fn sum_down(a: f64, b: f64) -> f64 {
    let (s, e) = two_sum(a, b);
    if e < 0.0 { s.next_down() } else { s }
}

/// `a + b` rounded up: the double at or above the exact sum.
fn sum_up(a: f64, b: f64) -> f64 {
    let (s, e) = two_sum(a, b);
    if e > 0.0 { s.next_up() } else { s }
}

/// Whether the tolerant point `a` (accuracy `a_acc`) and `b` (accuracy `b_acc`) are one: their
/// distance is no more than the sum of their accuracies.
///
/// # Errors
///
/// [`MeetError::NotFinite`] for a coordinate that is not finite.
pub fn points_meet(
    a: Point3,
    a_acc: Accuracy,
    b: Point3,
    b_acc: Accuracy,
) -> Result<Meet, MeetError> {
    let distance = point_point_gap(a, b).map_err(|_| MeetError::NotFinite)?;
    let (a_mm, b_mm) = (a_acc.length().mm(), b_acc.length().mm());
    if distance.upper() <= sum_down(a_mm, b_mm) {
        let joined = a_mm.max(sum_up(b_mm, distance.upper()));
        let accuracy = Accuracy::new(Length::from_mm(joined)).ok_or(MeetError::NotFinite)?;
        Ok(Meet::One { accuracy })
    } else if distance.lower() > sum_up(a_mm, b_mm) {
        Ok(Meet::Apart)
    } else {
        Ok(Meet::Undecided { distance })
    }
}

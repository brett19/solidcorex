// Written for solidcorex. Apache-2.0.
//! [`Closed`]: a closed-form answer, or the word that the general algorithm must answer
//! instead (research 63 D6).

/// A closed-form answer, or [`Closed::General`]: the case has no closed form here and the
/// general algorithm answers it. `General` is not an error and not "nothing": a solved
/// answer (an empty one included) is final, and `General` carries no partial answer.
///
/// `#[non_exhaustive]` (63 D6): a `Partial` answer (part solved, the rest left to the
/// general algorithm) may join in step 5, so a match outside this crate keeps a wildcard
/// arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Closed<T> {
    /// The answer, in closed form.
    Solved(T),
    /// No closed form here: the general algorithm answers.
    General,
}

impl<T> Closed<T> {
    /// The solved answer mapped by `f`; `General` stays `General`.
    #[must_use]
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Closed<U> {
        match self {
            Self::Solved(t) => Closed::Solved(f(t)),
            Self::General => Closed::General,
        }
    }

    /// The solved answer, `None` for `General`.
    #[must_use]
    pub fn solved(self) -> Option<T> {
        match self {
            Self::Solved(t) => Some(t),
            Self::General => None,
        }
    }

    /// The solved answer by reference, `None` for `General`.
    #[must_use]
    pub const fn as_solved(&self) -> Option<&T> {
        match self {
            Self::Solved(t) => Some(t),
            Self::General => None,
        }
    }

    /// Whether the general algorithm must answer.
    #[must_use]
    pub const fn is_general(&self) -> bool {
        matches!(self, Self::General)
    }
}

#[cfg(test)]
mod tests {
    use super::Closed;

    #[test]
    fn map_and_solved() {
        let s: Closed<i32> = Closed::Solved(2);
        assert_eq!(s.map(|x| x * 3), Closed::Solved(6));
        assert_eq!(s.solved(), Some(2));
        assert_eq!(s.as_solved(), Some(&2));
        assert!(!s.is_general());
        let g: Closed<i32> = Closed::General;
        assert_eq!(g.map(|x| x * 3), Closed::General);
        assert_eq!(g.solved(), None);
        assert!(g.is_general());
    }
}

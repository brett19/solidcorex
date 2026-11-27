// Written for solidcorex. Apache-2.0.
//! The errors of building a draft (research 62): each has a `kind()` and a stable `name()`.
//! The errors of finishing one are in [`finish`](crate::finish).

use core::fmt;

use scx_num::error::ErrorKind;

use crate::id::{Entity, EntityKind};

/// A table has given every `u32` id: the body cannot take another entity of this kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableFull {
    /// The kind whose ids ran out.
    pub kind: EntityKind,
}

impl TableFull {
    /// [`ErrorKind::Exhausted`]: a count ran out.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Exhausted
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "table_full"
    }
}

impl fmt::Display for TableFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the {} table has no id left", self.kind.name())
    }
}

impl core::error::Error for TableFull {}

/// The draft holds no live entity with this id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Absent {
    /// The id asked for.
    pub entity: Entity,
}

impl Absent {
    /// [`ErrorKind::Refused`]: the caller names an entity the draft does not hold.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Refused
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "absent"
    }
}

impl fmt::Display for Absent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no live {} {}", self.entity.kind().name(), self.entity)
    }
}

impl core::error::Error for Absent {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::EdgeId;

    #[test]
    fn kinds_and_names() {
        let full = TableFull {
            kind: EntityKind::Face,
        };
        assert_eq!(
            (full.kind(), full.name()),
            (ErrorKind::Exhausted, "table_full")
        );
        assert_eq!(full.to_string(), "the face table has no id left");
        let absent = Absent {
            entity: EdgeId::from_raw(4).into(),
        };
        assert_eq!(
            (absent.kind(), absent.name()),
            (ErrorKind::Refused, "absent")
        );
        assert_eq!(absent.to_string(), "no live edge e4");
    }
}

// Written for solidcorex. Apache-2.0.
//! Entity ids (01 §2.2): typed `u32` newtypes, dense within one body's line of descent and
//! never reused. A deleted entity leaves its slot empty; [`Brep::compact`](crate::Brep::compact)
//! renumbers, and says how in an [`IdMap`].
//!
//! An id says which entity in this body. It is not a name: names across edits come from
//! `Changes` (BR3) and `scx-fates`. There is no body id: a body is a value.

use core::fmt;

macro_rules! id {
    ($(#[$doc:meta])* $name:ident, $kind:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// The id numbered `raw`. Any number is an id; a lookup answers `None` when the
            /// body has no live entity there.
            #[must_use]
            pub const fn from_raw(raw: u32) -> Self {
                Self(raw)
            }

            /// The id's number.
            #[must_use]
            pub const fn raw(self) -> u32 {
                self.0
            }

            /// The id's slot: its number as an index, for dense side tables and bitsets.
            #[must_use]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }

        impl From<$name> for Entity {
            fn from(id: $name) -> Self {
                Self::$kind(id)
            }
        }

        impl crate::table::Id for $name {
            const KIND: EntityKind = EntityKind::$kind;

            fn from_slot(slot: u32) -> Self {
                Self(slot)
            }

            fn slot(self) -> usize {
                self.index()
            }
        }
    };
}

id!(
    /// A region: a volume. [`RegionId::OUTSIDE`] is the infinite region outside everything.
    RegionId,
    Region,
    "r"
);
id!(
    /// A shell: a connected set of face sides, wire edges and lone vertices bounding one region.
    ShellId,
    Shell,
    "s"
);
id!(
    /// A face.
    FaceId,
    Face,
    "f"
);
id!(
    /// An edge: its geometry part and its adjacency part share the id.
    EdgeId,
    Edge,
    "e"
);
id!(
    /// A vertex.
    VertexId,
    Vertex,
    "v"
);

impl RegionId {
    /// The infinite region outside everything: slot 0 of every body (01 §2.1).
    pub const OUTSIDE: Self = Self(0);
}

/// The kind of an entity, without its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    /// A region.
    Region,
    /// A shell.
    Shell,
    /// A face.
    Face,
    /// An edge.
    Edge,
    /// A vertex.
    Vertex,
}

impl EntityKind {
    /// The kind's stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::Shell => "shell",
            Self::Face => "face",
            Self::Edge => "edge",
            Self::Vertex => "vertex",
        }
    }
}

/// An entity of any kind, by id: what an error names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Entity {
    /// A region.
    Region(RegionId),
    /// A shell.
    Shell(ShellId),
    /// A face.
    Face(FaceId),
    /// An edge.
    Edge(EdgeId),
    /// A vertex.
    Vertex(VertexId),
}

impl Entity {
    /// The entity's kind.
    #[must_use]
    pub const fn kind(self) -> EntityKind {
        match self {
            Self::Region(_) => EntityKind::Region,
            Self::Shell(_) => EntityKind::Shell,
            Self::Face(_) => EntityKind::Face,
            Self::Edge(_) => EntityKind::Edge,
            Self::Vertex(_) => EntityKind::Vertex,
        }
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Region(id) => id.fmt(f),
            Self::Shell(id) => id.fmt(f),
            Self::Face(id) => id.fmt(f),
            Self::Edge(id) => id.fmt(f),
            Self::Vertex(id) => id.fmt(f),
        }
    }
}

/// How [`Brep::compact`](crate::Brep::compact) renumbered: each old id's new id, `None` for
/// an old id that named no live entity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdMap {
    pub(crate) regions: Vec<Option<u32>>,
    pub(crate) shells: Vec<Option<u32>>,
    pub(crate) faces: Vec<Option<u32>>,
    pub(crate) edges: Vec<Option<u32>>,
    pub(crate) vertices: Vec<Option<u32>>,
}

fn renumbered(map: &[Option<u32>], old: usize) -> Option<u32> {
    map.get(old).copied().flatten()
}

impl IdMap {
    /// The new id of region `old`.
    #[must_use]
    pub fn region(&self, old: RegionId) -> Option<RegionId> {
        renumbered(&self.regions, old.index()).map(RegionId)
    }

    /// The new id of shell `old`.
    #[must_use]
    pub fn shell(&self, old: ShellId) -> Option<ShellId> {
        renumbered(&self.shells, old.index()).map(ShellId)
    }

    /// The new id of face `old`.
    #[must_use]
    pub fn face(&self, old: FaceId) -> Option<FaceId> {
        renumbered(&self.faces, old.index()).map(FaceId)
    }

    /// The new id of edge `old`.
    #[must_use]
    pub fn edge(&self, old: EdgeId) -> Option<EdgeId> {
        renumbered(&self.edges, old.index()).map(EdgeId)
    }

    /// The new id of vertex `old`.
    #[must_use]
    pub fn vertex(&self, old: VertexId) -> Option<VertexId> {
        renumbered(&self.vertices, old.index()).map(VertexId)
    }

    /// Whether every old id kept its number.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        [
            &self.regions,
            &self.shells,
            &self.faces,
            &self.edges,
            &self.vertices,
        ]
        .iter()
        .all(|map| {
            map.iter()
                .enumerate()
                .all(|(old, new)| new.is_some_and(|n| n as usize == old))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_print_order_and_index() {
        assert_eq!(FaceId::from_raw(3).to_string(), "f3");
        assert_eq!(Entity::from(EdgeId::from_raw(7)).to_string(), "e7");
        assert!(VertexId::from_raw(1) < VertexId::from_raw(2));
        assert_eq!(ShellId::from_raw(9).index(), 9);
        assert_eq!(RegionId::OUTSIDE.raw(), 0);
        assert_eq!(Entity::from(RegionId::OUTSIDE).kind().name(), "region");
    }

    #[test]
    fn id_map_answers_none_past_its_end() {
        let map = IdMap {
            faces: vec![Some(0), None, Some(1)],
            ..IdMap::default()
        };
        assert_eq!(map.face(FaceId(2)), Some(FaceId(1)));
        assert_eq!(map.face(FaceId(1)), None);
        assert_eq!(map.face(FaceId(5)), None);
        assert!(!map.is_identity());
    }
}

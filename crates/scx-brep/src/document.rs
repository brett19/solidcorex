// Written for solidcorex. Apache-2.0.
//! The document a body is finished into (01 §6.2): it fixes the resolution every finished
//! body is stamped with.
//!
//! 01 also gives it the units and a size class, and the constructor
//! `Document::new(units, size_class)` that derives the resolution from them; neither type
//! exists yet, so this holds the resolution alone and is `#[non_exhaustive]` for them.

use scx_num::units::Resolution;

/// The settings bodies are finished under.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct Document {
    resolution: Resolution,
}

impl Document {
    /// A document whose bodies are stamped with `resolution`.
    #[must_use]
    pub const fn with_resolution(resolution: Resolution) -> Self {
        Self { resolution }
    }

    /// The resolution.
    #[must_use]
    pub const fn resolution(&self) -> Resolution {
        self.resolution
    }
}

// Written for solidcorex. Apache-2.0.
//! Enum tags: one `u8` on the wire, one word in text, mapped both ways by exhaustive matches
//! (research 63 §7.2). An unknown byte or word is refused; there is no `Unknown` variant.

/// An enum written as its tag: a byte in binary, a word in text.
///
/// The two maps must agree: `from_code(t.code()) == Some(t)` and
/// `from_name(t.name()) == Some(t)` for every variant, and no two variants share a code or
/// a name. The crate's own tags are built by the `tags!` macro, whose matches the compiler
/// checks for both (a repeated code or name is an unreachable pattern).
pub trait Tag: Copy + Sized {
    /// The enum's name, for errors: `"slot"`, `"bool"`.
    const RECORD: &'static str;
    /// The variant's byte.
    fn code(self) -> u8;
    /// The variant with byte `code`, if any.
    fn from_code(code: u8) -> Option<Self>;
    /// The variant's word: lowercase ASCII letters, digits and `_`.
    fn name(self) -> &'static str;
    /// The variant with word `name`, if any.
    fn from_name(name: &str) -> Option<Self>;
}

/// Defines an enum with its [`Tag`]: each variant's byte and word, both maps exhaustive.
macro_rules! tags {
    (
        $(#[$meta:meta])*
        $vis:vis enum $ty:ident: $record:literal {
            $( $(#[$vmeta:meta])* $variant:ident = $code:literal, $word:literal; )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $ty {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $crate::Tag for $ty {
            const RECORD: &'static str = $record;

            fn code(self) -> u8 {
                match self {
                    $( Self::$variant => $code, )+
                }
            }

            fn from_code(code: u8) -> Option<Self> {
                match code {
                    $( $code => Some(Self::$variant), )+
                    _ => None,
                }
            }

            fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $word, )+
                }
            }

            fn from_name(name: &str) -> Option<Self> {
                match name {
                    $( $word => Some(Self::$variant), )+
                    _ => None,
                }
            }
        }
    };
}
pub(crate) use tags;

/// `false` is byte 0, word `false`; `true` is byte 1, word `true`. Any other byte is
/// [`BadTag`](crate::ReadCause::BadTag): a flag is never "nonzero" (63 §7.1, `manifold: i32`).
impl Tag for bool {
    const RECORD: &'static str = "bool";

    fn code(self) -> u8 {
        u8::from(self)
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        if self { "true" } else { "false" }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "false" => Some(false),
            "true" => Some(true),
            _ => None,
        }
    }
}

tags! {
    /// One entry of a table on the wire (01 §2.2, 63 §7.2 `Slot { Live(rec), Gap(u32) }`):
    /// a live record, or a run of deleted slots as a count. Never in memory: a decoded
    /// table is `Vec<Option<R>>`.
    pub(crate) enum SlotKind: "slot" {
        /// A record follows.
        Live = 0, "live";
        /// A `u32` count of deleted slots follows: at least 1, never after another run.
        Gap = 1, "gap";
    }
}

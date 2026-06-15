//! The scalar/leaf primitive types of the BAML type system.

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PrimitiveType {
    Int,
    Bigint,
    Float,
    String,
    Bool,
    Null,
    Uint8Array,
    Image,
    Audio,
    Video,
    Pdf,
}

impl PrimitiveType {
    /// Map primitives with builtin companion classes to their class path in the `baml` package.
    ///
    /// Media primitives (`image`, `audio`, `video`, `pdf`) have corresponding
    /// classes in `baml_builtins2/baml_std/baml/ns_media/media.baml`, and
    /// `uint8array` has its class in `baml_builtins2/baml_std/baml/uint8array.baml`.
    pub fn builtin_class_path(&self) -> &'static [&'static str] {
        match self {
            Self::Int => &["Int"],
            Self::Bigint => &["Bigint"],
            Self::Float => &["Float"],
            Self::Bool => &["Bool"],
            Self::Null => &["Null"],
            Self::String => &["String"],
            Self::Uint8Array => &["Uint8Array"],
            Self::Image => &["media", "Image"],
            Self::Audio => &["media", "Audio"],
            Self::Video => &["media", "Video"],
            Self::Pdf => &["media", "Pdf"],
        }
    }

    pub fn from_literal(lit: &baml_base::Literal) -> Self {
        match lit {
            baml_base::Literal::Int(_) => Self::Int,
            baml_base::Literal::Bigint(_) => Self::Bigint,
            baml_base::Literal::Float(_) => Self::Float,
            baml_base::Literal::String(_) => Self::String,
            baml_base::Literal::Bool(_) => Self::Bool,
        }
    }

    /// The lowercase primitive/keyword spelling for this type (`string`, `int`,
    /// `image`, …). Single source of truth — the [`fmt::Display`] impl delegates
    /// here.
    pub fn alias(&self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Bigint => "bigint",
            Self::Float => "float",
            Self::String => "string",
            Self::Bool => "bool",
            Self::Null => "null",
            Self::Uint8Array => "uint8array",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Pdf => "pdf",
        }
    }

    /// Inverse of [`builtin_class_path`](Self::builtin_class_path): map a class
    /// path (relative to the `baml` package, e.g. `["media", "Image"]`) back to
    /// the primitive it is the companion class for.
    pub fn from_builtin_class_path(path: &[&str]) -> Option<Self> {
        const ALL: [PrimitiveType; 11] = [
            PrimitiveType::Int,
            PrimitiveType::Bigint,
            PrimitiveType::Float,
            PrimitiveType::Bool,
            PrimitiveType::Null,
            PrimitiveType::String,
            PrimitiveType::Uint8Array,
            PrimitiveType::Image,
            PrimitiveType::Audio,
            PrimitiveType::Video,
            PrimitiveType::Pdf,
        ];
        ALL.into_iter().find(|p| p.builtin_class_path() == path)
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.alias())
    }
}

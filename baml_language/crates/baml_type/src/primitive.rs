//! The scalar/leaf primitive types of the BAML type system.

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
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
    pub const ALL: [PrimitiveType; 11] = [
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

    /// Resolve a lowercase source spelling to its semantic primitive.
    pub fn from_alias(alias: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|primitive| primitive.alias() == alias)
    }

    /// Inverse of [`builtin_class_path`](Self::builtin_class_path): map a class
    /// path (relative to the `baml` package, e.g. `["media", "Image"]`) back to
    /// the primitive it is the companion class for.
    pub fn from_builtin_class_path(path: &[&str]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|primitive| primitive.builtin_class_path() == path)
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.alias())
    }
}

/// A built-in source-level type name.
///
/// Primitive values have companion classes in the `baml` package. `json` is a
/// stdlib type alias, while `void`, `never`, and `unknown` are compiler
/// intrinsics with no addressable definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinTypeName {
    Primitive(PrimitiveType),
    Json,
    Void,
    Never,
    Unknown,
}

impl BuiltinTypeName {
    /// Every builtin source-level type name — the enumeration counterpart of
    /// [`from_alias`](Self::from_alias), so what completion offers is what
    /// the resolver accepts.
    pub fn all() -> impl Iterator<Item = Self> {
        PrimitiveType::ALL.into_iter().map(Self::Primitive).chain([
            Self::Json,
            Self::Void,
            Self::Never,
            Self::Unknown,
        ])
    }

    pub fn from_alias(alias: &str) -> Option<Self> {
        if let Some(primitive) = PrimitiveType::from_alias(alias) {
            return Some(Self::Primitive(primitive));
        }
        Some(match alias {
            "json" => Self::Json,
            "void" => Self::Void,
            "never" => Self::Never,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }

    pub fn alias(self) -> &'static str {
        match self {
            Self::Primitive(primitive) => primitive.alias(),
            Self::Json => "json",
            Self::Void => "void",
            Self::Never => "never",
            Self::Unknown => "unknown",
        }
    }

    /// The path of this type's definition relative to the `baml` package.
    ///
    /// Compiler intrinsics deliberately return `None`: their documentation is
    /// supplied by the language-topic registry instead.
    pub fn builtin_definition_path(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Primitive(primitive) => Some(primitive.builtin_class_path()),
            Self::Json => Some(&["json", "json"]),
            Self::Void | Self::Never | Self::Unknown => None,
        }
    }

    pub fn from_builtin_definition_path(path: &[&str]) -> Option<Self> {
        if path == ["json", "json"] {
            return Some(Self::Json);
        }
        PrimitiveType::from_builtin_class_path(path).map(Self::Primitive)
    }
}

impl fmt::Display for BuiltinTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.alias())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_aliases_roundtrip() {
        for primitive in PrimitiveType::ALL {
            assert_eq!(
                PrimitiveType::from_alias(primitive.alias()),
                Some(primitive)
            );
        }
        assert_eq!(PrimitiveType::from_alias("void"), None);
    }

    #[test]
    fn builtin_type_names_distinguish_definitions_from_intrinsics() {
        assert_eq!(
            BuiltinTypeName::from_alias("string")
                .and_then(BuiltinTypeName::builtin_definition_path),
            Some(&["String"][..])
        );
        assert_eq!(
            BuiltinTypeName::from_alias("json").and_then(BuiltinTypeName::builtin_definition_path),
            Some(&["json", "json"][..])
        );
        assert_eq!(
            BuiltinTypeName::from_alias("never").and_then(BuiltinTypeName::builtin_definition_path),
            None
        );
        assert_eq!(
            BuiltinTypeName::from_builtin_definition_path(&["json", "json"]),
            Some(BuiltinTypeName::Json)
        );
    }
}

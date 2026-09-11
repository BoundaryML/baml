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
        crate::compiler_aliases::by_target(crate::compiler_aliases::AliasTarget::Primitive(*self))
            .definition
            .expect("primitives have member declarations")
            .path
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
        crate::compiler_aliases::by_target(crate::compiler_aliases::AliasTarget::Primitive(*self))
            .display_name()
    }

    /// Resolve a lowercase source spelling to its semantic primitive.
    pub fn from_alias(alias: &str) -> Option<Self> {
        match crate::compiler_aliases::by_spelling(alias)?.target {
            crate::compiler_aliases::AliasTarget::Primitive(primitive) => Some(primitive),
            _ => None,
        }
    }

    /// Inverse of [`builtin_class_path`](Self::builtin_class_path): map a class
    /// path (relative to the `baml` package, e.g. `["media", "Image"]`) back to
    /// the primitive it is the companion class for.
    pub fn from_builtin_class_path(path: &[&str]) -> Option<Self> {
        match crate::compiler_aliases::by_definition_path(baml_base::LangPackage::Baml, path)?
            .target
        {
            crate::compiler_aliases::AliasTarget::Primitive(primitive) => Some(primitive),
            _ => None,
        }
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
        Self::from_target(crate::compiler_aliases::by_spelling(alias)?.target)
    }

    pub fn alias(self) -> &'static str {
        crate::compiler_aliases::by_target(self.target()).display_name()
    }

    /// The path of this type's definition relative to the `baml` package.
    ///
    /// Compiler intrinsics deliberately return `None`: their documentation is
    /// supplied by the language-topic registry instead.
    pub fn builtin_definition_path(self) -> Option<&'static [&'static str]> {
        crate::compiler_aliases::by_target(self.target())
            .definition
            .map(|definition| definition.path)
    }

    fn target(self) -> crate::compiler_aliases::AliasTarget {
        use crate::compiler_aliases::AliasTarget;
        match self {
            Self::Primitive(primitive) => AliasTarget::Primitive(primitive),
            Self::Json => AliasTarget::Json,
            Self::Void => AliasTarget::Void,
            Self::Never => AliasTarget::Never,
            Self::Unknown => AliasTarget::Unknown,
        }
    }

    fn from_target(target: crate::compiler_aliases::AliasTarget) -> Option<Self> {
        use crate::compiler_aliases::AliasTarget;
        Some(match target {
            AliasTarget::Primitive(primitive) => Self::Primitive(primitive),
            AliasTarget::Json => Self::Json,
            AliasTarget::Void => Self::Void,
            AliasTarget::Never => Self::Never,
            AliasTarget::Unknown => Self::Unknown,
            _ => return None,
        })
    }

    pub fn from_builtin_definition_path(path: &[&str]) -> Option<Self> {
        Self::from_target(
            crate::compiler_aliases::by_definition_path(baml_base::LangPackage::Baml, path)?.target,
        )
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

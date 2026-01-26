/*
 * EvaluationContext is used to evaluate a function call with context-specific information.
 *
 * For example, client_registry and type_builder
 */

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::type_definition::BamlTypeId;

// a type that has checks and asserts (and possibly more) attached.
// These are all user-defined types. Maybe rename to UserType
pub type TypeReference = TypeReferenceWithMetadata<TypeMetadata>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Default, TS)]
#[ts(export)]
pub struct TypeMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    checks: Vec<CheckedType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    asserts: Vec<AssertedType>,
}

impl TypeMetadata {
    pub fn new(checks: Vec<CheckedType>, asserts: Vec<AssertedType>) -> Self {
        Self { checks, asserts }
    }

    pub fn merge(&mut self, other: TypeMetadata) {
        self.checks.extend(other.checks);
        self.asserts.extend(other.asserts);
    }
}

impl std::fmt::Display for TypeMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for check in &self.checks {
            write!(f, "@check({})", check.name)?;
        }
        for assert in &self.asserts {
            write!(
                f,
                "@assert({})",
                assert.name.as_deref().unwrap_or("unnamed")
            )?;
        }
        Ok(())
    }
}

/// FieldType represents the type of either a class field or a function arg.
/// THIS IS ONLY FOR NON_STREAMING TYPES.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, TS)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TypeReferenceWithMetadata<Metadata> {
    // Unknown type
    Unknown, // Not supported

    // Primitive types
    String(Metadata),
    Int(Metadata),
    Float(Metadata),
    Bool(Metadata),
    Media(MediaTypeDefinition, Metadata),
    Literal(LiteralTypeDefinition, Metadata),

    // User-defined types
    Class {
        type_id: Arc<BamlTypeId>,
        metadata: Metadata,
    },
    Enum {
        type_id: Arc<BamlTypeId>,
        metadata: Metadata,
    },
    RecursiveTypeAlias {
        type_id: Arc<BamlTypeId>,
        metadata: Metadata,
    },

    // Container types
    List(Box<Self>, Metadata),
    Map {
        key: Box<Self>,
        value: Box<Self>,
        metadata: Metadata,
    },
    Union {
        union_type: UnionType<Metadata>,
        metadata: Metadata,
    },
    Tuple {
        items: Vec<Self>,
        metadata: Metadata,
    },
}

impl<T: std::fmt::Display> std::fmt::Display for TypeReferenceWithMetadata<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeReferenceWithMetadata::Unknown => write!(f, "unknown"),
            TypeReferenceWithMetadata::String(meta) => write!(f, "string {meta}"),
            TypeReferenceWithMetadata::Int(meta) => write!(f, "int {meta}"),
            TypeReferenceWithMetadata::Float(meta) => write!(f, "float {meta}"),
            TypeReferenceWithMetadata::Bool(meta) => write!(f, "bool {meta}"),
            TypeReferenceWithMetadata::Media(media_type_definition, meta) => {
                write!(f, "{media_type_definition} {meta}")
            }
            TypeReferenceWithMetadata::Literal(literal_type_definition, _) => {
                write!(f, "{literal_type_definition}")
            }
            TypeReferenceWithMetadata::Class { type_id, metadata }
            | TypeReferenceWithMetadata::Enum { type_id, metadata }
            | TypeReferenceWithMetadata::RecursiveTypeAlias { type_id, metadata } => {
                write!(f, "{} {}", type_id.0, metadata)
            }
            TypeReferenceWithMetadata::List(type_reference_with_metadata, metadata) => {
                write!(f, "{type_reference_with_metadata}[] {metadata}")
            }
            TypeReferenceWithMetadata::Map {
                key,
                value,
                metadata,
            } => write!(f, "map<{key}, {value}> {metadata}"),
            TypeReferenceWithMetadata::Union {
                union_type,
                metadata,
            } => write!(
                f,
                "({}) {}",
                union_type
                    .types
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(" | "),
                metadata
            ),
            TypeReferenceWithMetadata::Tuple { items, metadata } => {
                write!(
                    f,
                    "({}) {}",
                    items
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    metadata
                )
            }
        }
    }
}

impl std::fmt::Display for MediaTypeDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaTypeDefinition::Image => write!(f, "image"),
            MediaTypeDefinition::Audio => write!(f, "audio"),
            MediaTypeDefinition::Pdf => write!(f, "pdf"),
            MediaTypeDefinition::Video => write!(f, "video"),
        }
    }
}

impl std::fmt::Display for LiteralTypeDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiteralTypeDefinition::String(s) => write!(f, "'{s}'"),
            LiteralTypeDefinition::Int(i) => write!(f, "{i}"),
            LiteralTypeDefinition::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Hash, TS)]
pub struct UnionType<Metadata> {
    pub types: Vec<TypeReferenceWithMetadata<Metadata>>,
    pub is_nullable: bool,
}

type CheckedType = NarrowingType<String>;
type AssertedType = NarrowingType<Option<String>>;

impl<Metadata: Default> TypeReferenceWithMetadata<Metadata> {
    pub fn string() -> Self {
        Self::String(Metadata::default())
    }

    pub fn int() -> Self {
        Self::Int(Metadata::default())
    }

    pub fn float() -> Self {
        Self::Float(Metadata::default())
    }

    pub fn bool() -> Self {
        Self::Bool(Metadata::default())
    }

    pub fn media(media_type: MediaTypeDefinition) -> Self {
        Self::Media(media_type, Metadata::default())
    }

    pub fn literal(literal_type: LiteralTypeDefinition) -> Self {
        Self::Literal(literal_type, Metadata::default())
    }

    pub fn list(item: TypeReferenceWithMetadata<Metadata>) -> Self {
        Self::List(Box::new(item), Metadata::default())
    }

    pub fn map(
        key: TypeReferenceWithMetadata<Metadata>,
        value: TypeReferenceWithMetadata<Metadata>,
    ) -> Self {
        Self::Map {
            key: Box::new(key),
            value: Box::new(value),
            metadata: Metadata::default(),
        }
    }

    pub fn union(mut one_of: Vec<TypeReferenceWithMetadata<Metadata>>, is_nullable: bool) -> Self {
        if one_of.is_empty() {
            return Self::Unknown;
        }
        if one_of.len() == 1 && !is_nullable {
            return one_of.pop().unwrap();
        }
        Self::Union {
            union_type: UnionType {
                types: one_of,
                is_nullable,
            },
            metadata: Metadata::default(),
        }
    }

    pub fn tuple(items: Vec<TypeReferenceWithMetadata<Metadata>>) -> Self {
        Self::Tuple {
            items,
            metadata: Metadata::default(),
        }
    }

    pub fn class(name: Arc<BamlTypeId>) -> Self {
        Self::Class {
            type_id: name,
            metadata: Metadata::default(),
        }
    }

    pub fn enum_type(name: Arc<BamlTypeId>) -> Self {
        Self::Enum {
            type_id: name,
            metadata: Metadata::default(),
        }
    }

    pub fn recursive_type_alias(name: Arc<BamlTypeId>) -> Self {
        Self::RecursiveTypeAlias {
            type_id: name,
            metadata: Metadata::default(),
        }
    }

    pub fn metadata(&self) -> Option<&Metadata> {
        Some(match self {
            Self::Unknown => return None,
            Self::String(metadata) => metadata,
            Self::Int(metadata) => metadata,
            Self::Float(metadata) => metadata,
            Self::Bool(metadata) => metadata,
            Self::Media(_, metadata) => metadata,
            Self::Literal(_, metadata) => metadata,
            Self::List(_, metadata) => metadata,
            Self::Map { metadata, .. } => metadata,
            Self::Union { metadata, .. } => metadata,
            Self::Tuple { metadata, .. } => metadata,
            Self::Class { metadata, .. } => metadata,
            Self::Enum { metadata, .. } => metadata,
            Self::RecursiveTypeAlias { metadata, .. } => metadata,
        })
    }

    pub fn metadata_mut(&mut self) -> Option<&mut Metadata> {
        Some(match self {
            Self::Unknown => return None,
            Self::String(metadata) => metadata,
            Self::Int(metadata) => metadata,
            Self::Float(metadata) => metadata,
            Self::Bool(metadata) => metadata,
            Self::Media(_, metadata) => metadata,
            Self::Literal(_, metadata) => metadata,
            Self::List(_, metadata) => metadata,
            Self::Map { metadata, .. } => metadata,
            Self::Union { metadata, .. } => metadata,
            Self::Tuple { metadata, .. } => metadata,
            Self::Class { metadata, .. } => metadata,
            Self::Enum { metadata, .. } => metadata,
            Self::RecursiveTypeAlias { metadata, .. } => metadata,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
pub struct NarrowingType<T> {
    pub name: T,
    pub expressions: Expression,
}

#[derive(Debug, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MediaTypeDefinition {
    Image,
    Audio,
    Pdf,
    Video,
}

/// Subset of [`crate::BamlValue`] allowed for literal type definitions.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialOrd, Ord, PartialEq, Eq, Hash, TS)]
#[ts(export)]
pub enum LiteralTypeDefinition {
    String(String),
    Int(i64),
    Bool(bool),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Hash, TS)]
#[serde(tag = "expression_type", content = "data", rename_all = "snake_case")]
pub enum Expression {
    Jinja(String),
}

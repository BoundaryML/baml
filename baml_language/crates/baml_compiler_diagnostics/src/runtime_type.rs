//! Shared diagnostics for runtime type construction, reflection, and rendering.
//!
//! Static checking and runtime validation deliberately call the same typed
//! constructors here. The constructor selects both the diagnostic id and its
//! complete message, so runtime code cannot reuse a compiler code with a
//! divergent hand-built string.

use std::fmt;

use crate::{Diagnostic, DiagnosticId};

/// The declaration kind whose members collide after serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializedKeyContainer {
    Class,
    Enum,
}

/// The kind of repeated runtime-type member reported by E0012.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateMemberKind {
    Field,
    Variant,
}

/// The declaration position containing an invalid runtime-supplied name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidIdentifierKind {
    Class,
    Enum,
    Field,
    EnumVariant,
    ExportedType,
}

impl fmt::Display for DuplicateMemberKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Field => f.write_str("field"),
            Self::Variant => f.write_str("variant"),
        }
    }
}

impl fmt::Display for SerializedKeyContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Class => f.write_str("class"),
            Self::Enum => f.write_str("enum"),
        }
    }
}

impl fmt::Display for InvalidIdentifierKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Class => f.write_str("class"),
            Self::Enum => f.write_str("enum"),
            Self::Field => f.write_str("field"),
            Self::EnumVariant => f.write_str("enum variant"),
            Self::ExportedType => f.write_str("exported type"),
        }
    }
}

/// Check a runtime-supplied name with the compiler lexer's identifier rules.
pub fn is_baml_identifier(value: &str) -> bool {
    baml_compiler_lexer::is_baml_identifier(value)
}

/// E0010 — a runtime-supplied declaration name is not a BAML identifier.
pub fn invalid_identifier(kind: InvalidIdentifierKind, name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::InvalidSyntax,
        format!("invalid {kind} name `{name}`"),
    )
}

/// E0001 — the compiler's type-mismatch headline.
pub fn mismatched_types() -> Diagnostic {
    Diagnostic::error(DiagnosticId::TypeMismatch, "mismatched types")
}

/// E0002 — a value-shaped generic argument omitted the required marker.
pub fn computed_generic_argument_requires_unreflect(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::UnknownType,
        format!("computed type argument `{name}` must be written as `unreflect({name})`"),
    )
}

/// E0012 — a runtime type contains the same member more than once.
pub fn duplicate_member(kind: DuplicateMemberKind, container: &str, member: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::DuplicateField,
        format!("duplicate {kind} `{container}.{member}`"),
    )
}

/// E0149 — two members serialize to the same key.
pub fn duplicate_serialized_key(key: &str, container: SerializedKeyContainer) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::DuplicateFieldAlias,
        format!("duplicate serialized key `{key}` in {container}"),
    )
}

/// E0160 — runtime-only empty-union construction failure.
pub fn runtime_empty_union() -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::RuntimeEmptyUnion,
        "a runtime union must contain at least one member",
    )
}

/// E0161 — an open interface reached an LLM schema render.
pub fn open_interface_at_render(field: &str, open_type: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::OpenInterfaceAtRender,
        format!(
            "field `{field}` has open interface type `{open_type}`, which cannot be rendered as an LLM output schema"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_own_code_and_complete_message() {
        let cases = [
            (mismatched_types(), "E0001", "mismatched types"),
            (
                computed_generic_argument_requires_unreflect("runtime_t"),
                "E0002",
                "computed type argument `runtime_t` must be written as `unreflect(runtime_t)`",
            ),
            (
                duplicate_member(DuplicateMemberKind::Field, "Collision", "wire"),
                "E0012",
                "duplicate field `Collision.wire`",
            ),
            (
                duplicate_serialized_key("wire", SerializedKeyContainer::Class),
                "E0149",
                "duplicate serialized key `wire` in class",
            ),
            (
                invalid_identifier(InvalidIdentifierKind::Class, "type"),
                "E0010",
                "invalid class name `type`",
            ),
            (
                invalid_identifier(InvalidIdentifierKind::EnumVariant, "Choice.function"),
                "E0010",
                "invalid enum variant name `Choice.function`",
            ),
        ];

        for (diagnostic, code, message) in cases {
            assert_eq!(diagnostic.code(), code);
            assert_eq!(diagnostic.message, message);
        }
    }
}

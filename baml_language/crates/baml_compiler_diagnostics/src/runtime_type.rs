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

/// E0001 — a reflection class operation received a non-instance value.
pub fn expected_class_instance(callee: &str, got: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::TypeMismatch,
        format!("{callee} expected a class instance, got {got}"),
    )
}

/// E0002 — a value-shaped generic argument omitted the required marker.
pub fn computed_generic_argument_requires_unreflect(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::UnknownType,
        format!("computed type argument `{name}` must be written as `unreflect({name})`"),
    )
}

/// E0010 — an indirect call cannot carry a deferred runtime argument check.
pub fn runtime_type_argument_on_indirect_call() -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::InvalidSyntax,
        "runtime-checked arguments are not supported on indirect calls",
    )
}

/// E0001 — sealed reflection-kind values come only from an existing `type`.
pub fn cannot_construct_reflection_kind(class_name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::TypeMismatch,
        format!(
            "reflection kind `{class_name}` cannot be constructed; obtain it from a type value"
        ),
    )
}

/// E0158 — the mounted callable has no location-free bytecode link contract.
pub fn mounted_package_call_unsupported(path: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::MountedPackageCallUnsupported,
        format!(
            "cannot call mounted callable `{path}`: this callable kind has no loc-free bytecode link contract"
        ),
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
                expected_class_instance("reflect.class.get_field", "int"),
                "E0001",
                "reflect.class.get_field expected a class instance, got int",
            ),
            (
                computed_generic_argument_requires_unreflect("runtime_t"),
                "E0002",
                "computed type argument `runtime_t` must be written as `unreflect(runtime_t)`",
            ),
            (
                runtime_type_argument_on_indirect_call(),
                "E0010",
                "runtime-checked arguments are not supported on indirect calls",
            ),
            (
                duplicate_member(DuplicateMemberKind::Field, "Collision", "wire"),
                "E0012",
                "duplicate field `Collision.wire`",
            ),
            (
                cannot_construct_reflection_kind("baml.reflect.class.Type"),
                "E0001",
                "reflection kind `baml.reflect.class.Type` cannot be constructed; obtain it from a type value",
            ),
            (
                mounted_package_call_unsupported("dep.tool"),
                "E0158",
                "cannot call mounted callable `dep.tool`: this callable kind has no loc-free bytecode link contract",
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
            (
                runtime_empty_union(),
                "E0160",
                "a runtime union must contain at least one member",
            ),
            (
                open_interface_at_render("payload", "user.Open"),
                "E0161",
                "field `payload` has open interface type `user.Open`, which cannot be rendered as an LLM output schema",
            ),
        ];

        for (diagnostic, code, message) in cases {
            assert_eq!(diagnostic.code(), code);
            assert_eq!(diagnostic.message, message);
        }
    }
}

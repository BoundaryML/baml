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

/// E0165 — reflection cannot construct a complete generic frame.
///
/// Package extraction supplies a package-qualified display name; dynamic
/// `call_any` has no package context and supplies the callable's bare declared
/// name. The difference is intentional and keeps both diagnostics actionable.
///
/// A by-name lookup has nowhere to put type arguments, so it is still refused;
/// the message names the descriptor route that does accept them.
pub fn unspecialized_reflected_generic(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::UnspecializedReflectedGeneric,
        format!(
            "generic function `{name}` cannot be extracted by name through reflection: look it up \
             in `Package.functions()` and `specialize` it first"
        ),
    )
}

/// E0165 — a reflected generic callable was invoked without specialization.
///
/// The sibling above covers *extraction*: a callable whose signature still
/// mentions its own type parameters cannot even be handed out. This one covers
/// the callables that get past that edge — a companion whose signature is free
/// of `T` but whose body still materializes it. Invoking one would fail inside
/// the body as an internal error, so reflection refuses it up front.
pub fn unspecialized_reflected_generic_call(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::UnspecializedReflectedGeneric,
        format!(
            "generic function `{name}` cannot be invoked through reflection until it is \
             specialized: its body needs type arguments — look it up in `Package.functions()` \
             and `specialize` it first"
        ),
    )
}

/// E0165 — a signature-shaped read of a descriptor that has no signature yet.
///
/// A generic function whose declared surface mentions its own type parameters
/// has no realized function type at all, so `params`/`return_type` have nothing
/// to decompose. The two siblings above refuse to hand out or invoke such a
/// callable; this one refuses to read its shape.
pub fn unspecialized_reflected_generic_signature(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::UnspecializedReflectedGeneric,
        format!(
            "generic function `{name}` has no signature until it is specialized: its parameter \
             and return types still mention its own type parameters"
        ),
    )
}

/// E0169 — `specialize` was given the wrong number of type arguments.
pub fn specialize_arity_mismatch(name: &str, expected: usize, supplied: usize) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::ReflectSpecializationFailed,
        format!(
            "cannot specialize generic function `{name}`: it declares {}, but {} {} supplied",
            count_of(expected, "type parameter"),
            count_of(supplied, "type argument"),
            if supplied == 1 { "was" } else { "were" },
        ),
    )
}

/// E0169 — a supplied type argument fails one of its parameter's bounds.
pub fn specialize_bound_violation(
    name: &str,
    parameter: &str,
    bound: &str,
    supplied: &str,
) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::ReflectSpecializationFailed,
        format!(
            "cannot specialize generic function `{name}`: type argument `{supplied}` does not \
             satisfy the bound `{parameter} extends {bound}`"
        ),
    )
}

/// E0169 — `specialize` was called on a callable that declares no type
/// parameters. `is_generic` is the guard.
pub fn specialize_non_generic(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::ReflectSpecializationFailed,
        format!("function `{name}` is not generic; there is nothing to specialize"),
    )
}

/// E0169 — `specialize` was called on a descriptor whose type parameters are
/// already bound. Distinct from the sibling above: the callable *is* generic,
/// it just has nothing left to bind, and specialization is not incremental.
pub fn specialize_already_specialized(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::ReflectSpecializationFailed,
        format!("generic function `{name}` is already specialized; every type parameter is bound"),
    )
}

/// E0169 — a fully supplied frame still did not reconstruct a signature.
///
/// Nothing known produces this: every slot is bound, so every template
/// realizes. It exists so the impossible case is a catchable diagnostic rather
/// than a descriptor that silently reports `unknown` and denies being generic.
pub fn specialize_signature_unreconstructible(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::ReflectSpecializationFailed,
        format!(
            "cannot specialize generic function `{name}`: its signature does not reconstruct              even with every type argument supplied"
        ),
    )
}

/// E0169 — `specialize` was called on a function type that reflection did not
/// hand out, so there is no callable behind it to specialize.
pub fn specialize_without_descriptor() -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::ReflectSpecializationFailed,
        "this function type is not a reflected function descriptor: only the entries of \
         `Package.functions()` carry the callable `specialize` needs"
            .to_string(),
    )
}

/// `3 type parameters` / `1 type parameter`, so a diagnostic never reads
/// "declares 1 type parameters".
fn count_of(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// E0002 — a value-shaped generic argument omitted the required marker.
pub fn computed_generic_argument_requires_unreflect(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::UnknownType,
        format!("computed type argument `{name}` must be written as `unreflect({name})`"),
    )
}

/// What a [`runtime_type_must_be_named`] report could recover of the code the
/// user actually wrote. Both halves are optional: a reporting site that cannot
/// print one cleanly leaves it out and the suggestion degrades to the generic
/// spelling rather than inventing source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeTypeNameRewrite {
    /// The carrier expression written inside `unreflect(...)`.
    pub carrier: Option<String>,
    /// The whole written expression with the inline slot replaced by `Out`.
    pub named: Option<String>,
}

/// The name an E0168 suggestion introduces for the runtime type.
pub const RUNTIME_TYPE_NAME: &str = "Out";

/// A suggestion longer than this stops being a suggestion and starts being a
/// wall of text; the `type` line alone still says what to do.
const RUNTIME_TYPE_REWRITE_BUDGET: usize = 120;

impl RuntimeTypeNameRewrite {
    /// Derive both halves from the source the author actually wrote.
    /// `expression` is the whole written expression and `slot` is the byte
    /// range of its `unreflect(...)` argument. Nothing is produced unless the
    /// slot really reads as that marker — a caller whose spans did not line up
    /// gets the generic suggestion, never a rewrite of source we misread — and
    /// a half that would not print cleanly (empty, multi-line, or past the
    /// budget) is dropped on its own.
    pub fn from_source(expression: &str, slot: std::ops::Range<usize>) -> Self {
        let Some(carrier) = expression
            .get(slot.clone())
            .map(str::trim)
            .and_then(|slot| slot.strip_prefix("unreflect"))
            .map(str::trim_start)
            .and_then(|rest| rest.strip_prefix('('))
            .and_then(|rest| rest.strip_suffix(')'))
            .map(str::trim)
        else {
            return Self::default();
        };
        let named = format!(
            "{}{RUNTIME_TYPE_NAME}{}",
            &expression[..slot.start],
            &expression[slot.end..]
        );
        Self {
            carrier: printable(carrier).then(|| carrier.to_owned()),
            named: printable(&named).then_some(named),
        }
    }
}

fn printable(text: &str) -> bool {
    !text.is_empty() && !text.contains('\n') && text.len() <= RUNTIME_TYPE_REWRITE_BUDGET
}

/// E0168 — the note that explains why an inline `unreflect(...)` is too
/// short-lived for the value this expression produces.
pub const RUNTIME_TYPE_MUST_BE_NAMED_NOTE: &str = concat!(
    "a type created at runtime only lasts for one call when written inline with ",
    "`unreflect(...)`, but the value this expression creates would still need it afterwards",
);

/// E0168 — an inline `unreflect(value)` type argument would escape its call.
pub fn runtime_type_must_be_named() -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::RuntimeTypeMustBeNamed,
        "this runtime type must be given a name before it can be used here",
    )
}

/// E0168 — the suggested rewrite, written with the user's own carrier
/// expression whenever the reporting site could print it.
pub fn runtime_type_must_be_named_help(rewrite: &RuntimeTypeNameRewrite) -> String {
    let carrier = rewrite.carrier.as_deref().unwrap_or("...");
    let mut help = format!(
        "name the type first, then use the name:\n    type {RUNTIME_TYPE_NAME} = unreflect({carrier});"
    );
    if let Some(named) = &rewrite.named {
        help.push_str("\n    ");
        help.push_str(named);
    }
    help
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

/// E0166 — builtin companion carriers stand in for a builtin type; they hold
/// no fields and are never instantiated. `carries_methods` is false for the
/// empty-bodied companions (`baml.Bool`, `baml.Null`), which would otherwise
/// be described as carrying methods they do not have.
pub fn cannot_construct_builtin_companion(
    class_name: &str,
    builtin: &str,
    origin: &str,
    carries_methods: bool,
) -> Diagnostic {
    let role = if carries_methods {
        format!("it only carries the methods of `{builtin}`")
    } else {
        format!("it is only the companion of `{builtin}`")
    };
    Diagnostic::error(
        DiagnosticId::CannotConstructBuiltinCompanion,
        format!(
            "companion class `{class_name}` cannot be constructed; {role}, whose values come from {origin}"
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

/// E0164 — a non-data type reached an LLM schema render.
pub fn non_data_type_at_render(ty: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::NonDataTypeAtRender,
        format!("non-data type `{ty}` cannot be rendered as an LLM output schema"),
    )
}

/// E0164 — a class field contains a non-data type at the LLM schema boundary.
pub fn non_data_field_at_render(field: &str, ty: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticId::NonDataTypeAtRender,
        format!(
            "field `{field}` has non-data type `{ty}`, which cannot be rendered as an LLM output schema"
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
                unspecialized_reflected_generic("root.Extract"),
                "E0165",
                "generic function `root.Extract` cannot be extracted by name through reflection: look it up in `Package.functions()` and `specialize` it first",
            ),
            (
                unspecialized_reflected_generic_call("GenericList$render_prompt"),
                "E0165",
                "generic function `GenericList$render_prompt` cannot be invoked through reflection until it is specialized: its body needs type arguments — look it up in `Package.functions()` and `specialize` it first",
            ),
            (
                unspecialized_reflected_generic_signature("root.Extract"),
                "E0165",
                "generic function `root.Extract` has no signature until it is specialized: its parameter and return types still mention its own type parameters",
            ),
            (
                specialize_arity_mismatch("root.Extract", 1, 2),
                "E0169",
                "cannot specialize generic function `root.Extract`: it declares 1 type parameter, but 2 type arguments were supplied",
            ),
            (
                specialize_arity_mismatch("root.Pair", 2, 1),
                "E0169",
                "cannot specialize generic function `root.Pair`: it declares 2 type parameters, but 1 type argument was supplied",
            ),
            (
                specialize_bound_violation("root.Extract", "T", "baml.AnyClass", "int"),
                "E0169",
                "cannot specialize generic function `root.Extract`: type argument `int` does not satisfy the bound `T extends baml.AnyClass`",
            ),
            (
                specialize_non_generic("root.Present"),
                "E0169",
                "function `root.Present` is not generic; there is nothing to specialize",
            ),
            (
                specialize_already_specialized("root.Extract"),
                "E0169",
                "generic function `root.Extract` is already specialized; every type parameter is bound",
            ),
            (
                specialize_signature_unreconstructible("root.Extract"),
                "E0169",
                "cannot specialize generic function `root.Extract`: its signature does not reconstruct even with every type argument supplied",
            ),
            (
                specialize_without_descriptor(),
                "E0169",
                "this function type is not a reflected function descriptor: only the entries of `Package.functions()` carry the callable `specialize` needs",
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
                cannot_construct_builtin_companion("baml.Int", "int", "literals", true),
                "E0166",
                "companion class `baml.Int` cannot be constructed; it only carries the methods of `int`, whose values come from literals",
            ),
            (
                cannot_construct_builtin_companion("baml.Bool", "bool", "literals", false),
                "E0166",
                "companion class `baml.Bool` cannot be constructed; it is only the companion of `bool`, whose values come from literals",
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
            (
                non_data_type_at_render("never"),
                "E0164",
                "non-data type `never` cannot be rendered as an LLM output schema",
            ),
            (
                non_data_field_at_render("Envelope.payload", "unknown"),
                "E0164",
                "field `Envelope.payload` has non-data type `unknown`, which cannot be rendered as an LLM output schema",
            ),
            (
                runtime_type_must_be_named(),
                "E0168",
                "this runtime type must be given a name before it can be used here",
            ),
        ];

        for (diagnostic, code, message) in cases {
            assert_eq!(diagnostic.code(), code);
            assert_eq!(diagnostic.message, message);
        }
    }

    #[test]
    fn runtime_type_help_uses_the_authors_own_spelling_and_degrades_cleanly() {
        assert_eq!(
            runtime_type_must_be_named_help(&RuntimeTypeNameRewrite {
                carrier: Some("t".to_owned()),
                named: Some(r#"Holder<Out> { label: "h" }"#.to_owned()),
            }),
            "name the type first, then use the name:\n    \
             type Out = unreflect(t);\n    \
             Holder<Out> { label: \"h\" }"
        );
        assert_eq!(
            runtime_type_must_be_named_help(&RuntimeTypeNameRewrite::default()),
            "name the type first, then use the name:\n    type Out = unreflect(...);"
        );
    }

    #[test]
    fn runtime_type_rewrite_reads_back_the_written_source() {
        let written = r#"Holder<unreflect(t)> { label: "h" }"#;
        let slot = written.find("unreflect").expect("slot")..written.find("> {").expect("end");
        assert_eq!(
            RuntimeTypeNameRewrite::from_source(written, slot),
            RuntimeTypeNameRewrite {
                carrier: Some("t".to_owned()),
                named: Some(r#"Holder<Out> { label: "h" }"#.to_owned()),
            }
        );
    }

    #[test]
    fn runtime_type_rewrite_drops_halves_it_cannot_print() {
        // A multi-line literal keeps the carrier but not the whole rewrite.
        let written = "Holder<unreflect(t)> {\n  label: \"h\",\n}";
        let slot = written.find("unreflect").expect("slot")..written.find("> {").expect("end");
        assert_eq!(
            RuntimeTypeNameRewrite::from_source(written, slot),
            RuntimeTypeNameRewrite {
                carrier: Some("t".to_owned()),
                named: None,
            }
        );
        // A carrier spanning lines is not spelled back at the user, but the
        // rewrite that replaces it still is — naming the slot is what removes
        // the newline.
        let written = "f<unreflect(pick(\n  a))>()";
        let slot = written.find("unreflect").expect("slot")..written.find(")>(").expect("end") + 1;
        assert_eq!(
            RuntimeTypeNameRewrite::from_source(written, slot),
            RuntimeTypeNameRewrite {
                carrier: None,
                named: Some("f<Out>()".to_owned()),
            }
        );
        // An out-of-range slot degrades instead of panicking.
        assert_eq!(
            RuntimeTypeNameRewrite::from_source("f<unreflect(t)>()", 4..900),
            RuntimeTypeNameRewrite::default()
        );
    }
}

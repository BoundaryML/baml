//! Map a resolved entity to a semantic token type + modifiers.
//!
//! BAML's analog of rust-analyzer's `highlight_def`: classification is driven by
//! what a name *resolves to*, never by syntactic context — and the type system is
//! not consulted here, only resolution facts. A reference is classified the same
//! way as its definition; the caller adds `ModifierSet::DECLARATION` at
//! definition sites.

use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    semantic_index::DefinitionSite,
};
use baml_compiler2_hir_ty::infer::MemberResolution;
use baml_compiler2_ppir::resolve::ResolvedName;

use super::{ModifierSet, SemanticTokenType};

/// Primitive type names — the built-in scalar / collection types.
const PRIMITIVE_TYPES: &[&str] = &[
    "int",
    "bigint",
    "float",
    "string",
    "bool",
    "bytes",
    "uint8array",
    "null",
    "void",
    "image",
    "audio",
    "video",
    "pdf",
    "json",
    "map",
    "unknown",
    "never",
];

/// Classify a name iff it is a primitive type (`string`, `int`, ...): a
/// `defaultLibrary` `Type`. Used for both type-position names and value-position
/// path roots (`string.from(...)`), so a primitive is highlighted identically
/// wherever it appears.
pub(super) fn classify_primitive(name: &str) -> Option<(SemanticTokenType, ModifierSet)> {
    PRIMITIVE_TYPES
        .contains(&name)
        .then_some((SemanticTokenType::Type, ModifierSet::DEFAULT_LIBRARY))
}

/// A namespace classification, flagged `defaultLibrary` when it belongs to a
/// builtin / dependency package (`baml`, ...) rather than the file's own
/// package. Shared by value-position (path roots/tails) and type-position
/// (`type_run`) so a namespace is highlighted identically wherever it appears.
pub(super) fn namespace_class(is_builtin: bool) -> (SemanticTokenType, ModifierSet) {
    let modifiers = if is_builtin {
        ModifierSet::DEFAULT_LIBRARY
    } else {
        ModifierSet::empty()
    };
    (SemanticTokenType::Namespace, modifiers)
}

/// The base token type for a definition kind.
///
/// Mirrors `baml_cli::paint::kind_style` so terminal `describe` highlighting and
/// LSP semantic tokens agree on the palette.
pub(super) fn token_type_for_kind(kind: DefinitionKind) -> SemanticTokenType {
    use DefinitionKind as K;
    use SemanticTokenType as T;
    match kind {
        K::Class => T::Class,
        K::Enum => T::Enum,
        K::Interface => T::Interface,
        K::TypeAlias | K::AssociatedType => T::Type,
        K::Function | K::TemplateString => T::Function,
        K::Method => T::Method,
        K::Client | K::Test | K::RetryPolicy => T::Struct,
        K::Field => T::Property,
        K::Variant => T::EnumMember,
        K::Parameter => T::Parameter,
        K::Let | K::Binding => T::Variable,
    }
}

/// Classify a resolved bare name / path root.
///
/// Returns `None` for unresolved names so the walker can fall back to a neutral
/// classification rather than guessing.
pub(super) fn classify_resolved(
    resolved: &ResolvedName<'_>,
) -> Option<(SemanticTokenType, ModifierSet)> {
    match resolved {
        ResolvedName::Local {
            definition_site, ..
        } => {
            let token_type = match definition_site {
                // Function parameters and catch bindings highlight as parameters.
                Some(DefinitionSite::Parameter(_) | DefinitionSite::CatchBinding(_)) => {
                    SemanticTokenType::Parameter
                }
                // `let` bindings and pattern bindings are both plain variables.
                _ => SemanticTokenType::Variable,
            };
            Some((token_type, ModifierSet::empty()))
        }
        ResolvedName::Item(def) => Some((token_type_for_definition(*def), ModifierSet::empty())),
        ResolvedName::Builtin(def) => Some((
            token_type_for_definition(*def),
            ModifierSet::DEFAULT_LIBRARY,
        )),
        ResolvedName::Unknown => None,
    }
}

/// The base token type for a resolved top-level definition.
pub(super) fn token_type_for_definition(def: Definition<'_>) -> SemanticTokenType {
    token_type_for_kind(def.kind())
}

/// Classify a member access / path segment resolution.
pub(super) fn classify_member(res: &MemberResolution<'_>) -> (SemanticTokenType, ModifierSet) {
    use MemberResolution as M;
    use SemanticTokenType as T;
    let token_type = match res {
        M::Field { .. } | M::InterfaceVirtualField { .. } => T::Property,
        M::Variant { .. } => T::EnumMember,
        M::Free { .. } => T::Function,
        M::BoundMethod { .. }
        | M::UnboundMethod { .. }
        | M::InterfaceConcreteMethod { .. }
        | M::InterfaceVirtualMethod { .. } => T::Method,
    };
    (token_type, ModifierSet::empty())
}

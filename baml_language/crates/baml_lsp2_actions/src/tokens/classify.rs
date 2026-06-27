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
use baml_compiler2_tir::{inference::MemberResolution, resolve::ResolvedName};

use super::{ModifierSet, SemanticTokenType};

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
                Some(DefinitionSite::Parameter(_)) => SemanticTokenType::Parameter,
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
        M::Field { .. } | M::InterfaceField { .. } => T::Property,
        M::Variant { .. } => T::EnumMember,
        M::Free { .. } => T::Function,
        M::BoundMethod { .. }
        | M::UnboundMethod { .. }
        | M::InterfaceDefaultMethod { .. }
        | M::InterfaceMethod { .. } => T::Method,
    };
    (token_type, ModifierSet::empty())
}

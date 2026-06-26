//! Precompiled per-file `FileSemanticIndex` cache for the frozen stdlib.
//!
//! Building a file's `FileSemanticIndex` runs the lexer, parser, AST lowering,
//! and the semantic-index builder. The stdlib is frozen per compiler version, so
//! that output is deterministic. This module lets a consumer embed it (built once
//! by `baml_builtins2_prebuilt`) and install it, so `file_semantic_index` skips
//! all of that for `<builtin>/` files and just rehydrates the cached index.
//!
//! The index is stored in a borsh-friendly plain form ([`PrecompiledFile`]): all
//! arena indices are `u32`, and the two `'db`-bound fields are reconstructed on
//! load against the runtime database -- `scope_ids` by re-interning each
//! `FileScopeId`, and `symbol_contributions` by rebuilding each `Definition`
//! loc-handle via `XxxLoc::new(db, file, id)`. The result is identical to the
//! from-source `FileSemanticIndex`.
//!
//! The cache is optional: when unset (tests, LSP), `file_semantic_index` builds
//! from source as before, so behavior is unchanged.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use baml_base::{Name, SourceFile};
use baml_compiler2_ast::{EnvVarRef, ExprId};
use la_arena::{Idx, RawIdx};
use text_size::TextRange;

use crate::{
    Db,
    contributions::{Contribution, Definition, DefinitionKind, FileSymbolContributions},
    item_tree::{ItemTree, ItemTreeSourceMap},
    loc::{
        ClassLoc, ClientLoc, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc, RetryPolicyLoc,
        TemplateStringLoc, TestLoc, TypeAliasLoc,
    },
    scope::{FileScopeId, Scope, ScopeId},
    semantic_index::{FileSemanticIndex, PathResolution, ScopeBindings},
};

fn expr_id_to_u32(id: ExprId) -> u32 {
    id.into_raw().into_u32()
}

fn expr_id_from_u32(raw: u32) -> ExprId {
    Idx::from_raw(RawIdx::from_u32(raw))
}

/// A namespace-level `Definition`, flattened to `(kind, packed LocalItemId)`.
/// `symbol_contributions` only ever holds the ten namespace-level definition
/// kinds, so reconstruction covers exactly those.
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct PlainContribution {
    name: Name,
    span_start: u32,
    span_end: u32,
    kind: DefinitionKind,
    id: u32,
}

fn definition_to_plain(db: &dyn Db, def: Definition<'_>) -> (DefinitionKind, u32) {
    let id = match def {
        Definition::Class(l) => l.id(db).as_u32(),
        Definition::Enum(l) => l.id(db).as_u32(),
        Definition::Interface(l) => l.id(db).as_u32(),
        Definition::TypeAlias(l) => l.id(db).as_u32(),
        Definition::Function(l) => l.id(db).as_u32(),
        Definition::TemplateString(l) => l.id(db).as_u32(),
        Definition::Client(l) => l.id(db).as_u32(),
        Definition::Test(l) => l.id(db).as_u32(),
        Definition::RetryPolicy(l) => l.id(db).as_u32(),
        Definition::Let(l) => l.id(db).as_u32(),
    };
    (def.kind(), id)
}

fn plain_to_definition(
    db: &dyn Db,
    file: SourceFile,
    kind: DefinitionKind,
    id: u32,
) -> Definition<'_> {
    use crate::ids::LocalItemId;
    match kind {
        DefinitionKind::Class => {
            Definition::Class(ClassLoc::new(db, file, LocalItemId::from_u32(id)))
        }
        DefinitionKind::Enum => Definition::Enum(EnumLoc::new(db, file, LocalItemId::from_u32(id))),
        DefinitionKind::Interface => {
            Definition::Interface(InterfaceLoc::new(db, file, LocalItemId::from_u32(id)))
        }
        DefinitionKind::TypeAlias => {
            Definition::TypeAlias(TypeAliasLoc::new(db, file, LocalItemId::from_u32(id)))
        }
        DefinitionKind::Function => {
            Definition::Function(FunctionLoc::new(db, file, LocalItemId::from_u32(id)))
        }
        DefinitionKind::TemplateString => {
            Definition::TemplateString(TemplateStringLoc::new(db, file, LocalItemId::from_u32(id)))
        }
        DefinitionKind::Client => {
            Definition::Client(ClientLoc::new(db, file, LocalItemId::from_u32(id)))
        }
        DefinitionKind::Test => Definition::Test(TestLoc::new(db, file, LocalItemId::from_u32(id))),
        DefinitionKind::RetryPolicy => {
            Definition::RetryPolicy(RetryPolicyLoc::new(db, file, LocalItemId::from_u32(id)))
        }
        DefinitionKind::Let => Definition::Let(LetLoc::new(db, file, LocalItemId::from_u32(id))),
        other => panic!("symbol_contributions held a non-namespace definition kind: {other:?}"),
    }
}

/// A borsh-friendly snapshot of one file's `FileSemanticIndex`, with arena
/// indices flattened to `u32` and the `'db` fields dropped (reconstructed on
/// load). `item_tree_source_map` is omitted (rehydrated empty): its name-span
/// side-tables only refine diagnostics, never consulted for the frozen stdlib.
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PrecompiledFile {
    scopes: Vec<Scope>,
    expr_scopes: Vec<(u32, FileScopeId)>,
    scope_bindings: Vec<ScopeBindings>,
    item_tree: ItemTree,
    contributions_types: Vec<PlainContribution>,
    contributions_values: Vec<PlainContribution>,
    path_resolutions: Vec<(u32, PathResolution)>,
    env_var_refs: Vec<EnvVarRef>,
}

impl PrecompiledFile {
    /// Snapshot a freshly-built `FileSemanticIndex` (build-time only).
    pub fn from_index(db: &dyn Db, index: &FileSemanticIndex<'_>) -> Self {
        let conv = |list: &[(Name, Contribution<'_>)]| -> Vec<PlainContribution> {
            list.iter()
                .map(|(name, c)| {
                    let (kind, id) = definition_to_plain(db, c.definition);
                    PlainContribution {
                        name: name.clone(),
                        span_start: c.name_span.start().into(),
                        span_end: c.name_span.end().into(),
                        kind,
                        id,
                    }
                })
                .collect()
        };
        Self {
            scopes: index.scopes.clone(),
            expr_scopes: index
                .expr_scopes
                .iter()
                .map(|(e, s)| (expr_id_to_u32(*e), *s))
                .collect(),
            scope_bindings: index.scope_bindings.clone(),
            item_tree: (*index.item_tree).clone(),
            contributions_types: conv(&index.symbol_contributions.types),
            contributions_values: conv(&index.symbol_contributions.values),
            path_resolutions: index
                .path_resolutions
                .iter()
                .map(|(e, r)| (expr_id_to_u32(*e), r.clone()))
                .collect(),
            env_var_refs: index.env_var_refs.clone(),
        }
    }

    /// Rebuild a `FileSemanticIndex` against the runtime `db`/`file`,
    /// reconstructing the two `'db`-bound fields. Identical to from-source.
    pub fn rehydrate<'db>(&self, db: &'db dyn Db, file: SourceFile) -> FileSemanticIndex<'db> {
        #[allow(clippy::cast_possible_truncation)]
        let scope_ids: Vec<ScopeId<'db>> = (0..self.scopes.len())
            .map(|i| ScopeId::new(db, file, FileScopeId::new(i as u32)))
            .collect();
        let rebuild = |list: &[PlainContribution]| -> Vec<(Name, Contribution<'db>)> {
            list.iter()
                .map(|pc| {
                    (
                        pc.name.clone(),
                        Contribution {
                            name_span: TextRange::new(pc.span_start.into(), pc.span_end.into()),
                            definition: plain_to_definition(db, file, pc.kind, pc.id),
                        },
                    )
                })
                .collect()
        };
        FileSemanticIndex {
            scopes: self.scopes.clone(),
            expr_scopes: self
                .expr_scopes
                .iter()
                .map(|(u, s)| (expr_id_from_u32(*u), *s))
                .collect(),
            scope_bindings: self.scope_bindings.clone(),
            scope_ids,
            item_tree: Arc::new(self.item_tree.clone()),
            item_tree_source_map: Arc::new(ItemTreeSourceMap::default()),
            symbol_contributions: Arc::new(FileSymbolContributions {
                types: rebuild(&self.contributions_types),
                values: rebuild(&self.contributions_values),
            }),
            extra: None,
            path_resolutions: self
                .path_resolutions
                .iter()
                .map(|(u, r)| (expr_id_from_u32(*u), r.clone()))
                .collect(),
            env_var_refs: self.env_var_refs.clone(),
        }
    }
}

static PRECOMPILED_BUILTINS: OnceLock<HashMap<String, PrecompiledFile>> = OnceLock::new();

/// Install the precompiled stdlib semantic-index cache (first writer wins).
/// Keyed by builtin virtual path (e.g. `<builtin>/baml/string.baml`).
pub fn set_precompiled_builtins(map: HashMap<String, PrecompiledFile>) {
    let _ = PRECOMPILED_BUILTINS.set(map);
}

/// Precompiled index for a builtin file by virtual path, if installed.
pub fn precompiled_builtin(path: &str) -> Option<&'static PrecompiledFile> {
    PRECOMPILED_BUILTINS.get()?.get(path)
}

//! Pre-Processed Intermediate Representation (PPIR).
//!
//! Sits between the parser and HIR. Responsible for:
//! 1. Stream annotation capture from CST (type-level via `PpirTy::from_ast`,
//!    field-level via `build_ppir_fields`)
//! 2. Cross-file name classification (`PpirNames`)
//! 3. Stream type expansion (`stream_expand` on `PpirTy`)
//! 4. `@sap.*` attribute synthesis (`sap_missing`, `sap_in_progress_never`)
//!
//! PPIR does **not** depend on HIR — it defines its own types and reads the CST
//! directly. HIR depends on PPIR, calls its tracked functions, and converts
//! PPIR output types into HIR types.

use baml_base::{Name, SourceFile};
use baml_compiler_parser::syntax_tree;
use baml_compiler_syntax::SyntaxKind;
use baml_workspace::Project;
use rowan::ast::AstNode as _;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;

mod expand;
pub mod normalize;
pub mod simplify;
mod ty;

pub use expand::{
    Class, Field, PpirExpandedClass, PpirExpandedField, PpirExpandedTypeAlias, PpirSapMissing,
    TypeAlias, default_sap_missing, default_starts_as, stream_expand,
};
pub use normalize::{
    NormalizedStreamClass, NormalizedStreamField, StartsAs, StartsAsLiteral,
    default_starts_as_semantic, infer_typeof_s, normalize_class_fields,
    parse_starts_as_value,
};
pub use ty::{PpirField, PpirTy, PpirTypeAttrs};

//
// ──────────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for PPIR queries.
///
/// Extends `baml_workspace::Db` — NOT `baml_compiler_hir::Db`.
/// PPIR sits below HIR in the dependency chain.
#[salsa::db]
pub trait Db: baml_workspace::Db {}

//
// ───────────────────────────────────────────────────── TRACKED STRUCTS ─────
//

/// Cross-file name classification: sets of class, enum, and type alias
/// names across all files in the project.
#[salsa::tracked]
pub struct PpirNames<'db> {
    #[tracked]
    #[returns(ref)]
    pub class_names: FxHashSet<Name>,
    #[tracked]
    #[returns(ref)]
    pub enum_names: FxHashSet<Name>,
    #[tracked]
    #[returns(ref)]
    pub type_alias_names: FxHashSet<Name>,
}

/// Per-file result of PPIR expansion.
/// Contains expanded data for classes and type aliases.
/// Phase 3 consumes this to synthesize `stream_*` classes and type aliases.
#[salsa::tracked]
pub struct PpirExpandedItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub classes: Vec<PpirExpandedClass>,
    #[tracked]
    #[returns(ref)]
    pub type_aliases: Vec<PpirExpandedTypeAlias>,
}

/// Per-file result of PPIR stream expansion and normalization.
/// Bridge struct: produces the old output format for HIR consumption.
/// Will be replaced when Phase 3 takes over synthesis.
#[salsa::tracked]
pub struct PpirStreamItems<'db> {
    /// Generated `stream_*` classes.
    #[tracked]
    #[returns(ref)]
    pub classes: Vec<Class>,
    /// Generated `stream_*` type aliases.
    #[tracked]
    #[returns(ref)]
    pub type_aliases: Vec<TypeAlias>,
    /// Normalized per-field streaming annotations for each user class.
    #[tracked]
    #[returns(ref)]
    pub normalized: Vec<NormalizedStreamClass>,
}

//
// ────────────────────────────────────────────────────────── SALSA QUERIES ─────
//

/// Collect name sets across all files by walking the CST directly.
///
/// Reads from `syntax_tree(file)` — does NOT depend on HIR.
/// With Salsa early cutoff, if you edit a function body, the CST changes
/// but the name sets don't, so `ppir_names` returns the same result and
/// no downstream queries are invalidated.
#[salsa::tracked]
pub fn ppir_names(db: &dyn Db, project: Project) -> PpirNames<'_> {
    let mut class_names = FxHashSet::default();
    let mut enum_names = FxHashSet::default();
    let mut type_alias_names = FxHashSet::default();

    for file in project.files(db) {
        // Skip builtin files — they define internal types, not user-defined classes/enums/aliases.
        if file
            .path(db)
            .to_str()
            .is_some_and(|p| p.starts_with("<builtin>/"))
        {
            continue;
        }
        let cst = syntax_tree(db, *file);
        for child in cst.children() {
            match child.kind() {
                SyntaxKind::CLASS_DEF => {
                    if let Some(class_def) =
                        baml_compiler_syntax::ast::ClassDef::cast(child.clone())
                    {
                        if let Some(name_tok) = class_def.name() {
                            class_names.insert(SmolStr::new(name_tok.text()));
                        }
                    }
                }
                SyntaxKind::ENUM_DEF => {
                    if let Some(enum_def) = baml_compiler_syntax::ast::EnumDef::cast(child.clone())
                    {
                        if let Some(name_tok) = enum_def.name() {
                            enum_names.insert(SmolStr::new(name_tok.text()));
                        }
                    }
                }
                SyntaxKind::TYPE_ALIAS_DEF => {
                    if let Some(alias_def) =
                        baml_compiler_syntax::ast::TypeAliasDef::cast(child.clone())
                    {
                        if let Some(name_tok) = alias_def.name() {
                            type_alias_names.insert(SmolStr::new(name_tok.text()));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    PpirNames::new(db, class_names, enum_names, type_alias_names)
}

/// Compute expanded stream data for a single file.
///
/// For each class: runs `stream_expand` per field to compute `stream_type`,
/// synthesizes `@sap.*` attributes (`sap_in_progress_never`, `sap_missing`).
/// For each type alias: runs `stream_expand` on the alias body.
///
/// Does NOT synthesize `stream_*` definitions — that is Phase 3's responsibility.
#[salsa::tracked]
pub fn ppir_expanded_items(db: &dyn Db, file: SourceFile) -> PpirExpandedItems<'_> {
    let file_path = file.path(db);
    if file_path
        .to_str()
        .is_some_and(|p| p.starts_with("<builtin>/"))
    {
        return PpirExpandedItems::new(db, Vec::new(), Vec::new());
    }

    let cst = syntax_tree(db, file);
    let project = db.project();
    let names = ppir_names(db, project);

    let mut expanded_classes = Vec::new();
    let mut expanded_aliases = Vec::new();
    let mut seen_class_names = FxHashSet::default();
    let mut seen_alias_names = FxHashSet::default();

    for child in cst.children() {
        match child.kind() {
            SyntaxKind::CLASS_DEF => {
                let Some(class_def) = baml_compiler_syntax::ast::ClassDef::cast(child.clone())
                else {
                    continue;
                };
                let Some(name_tok) = class_def.name() else {
                    continue;
                };
                let class_name: Name = SmolStr::new(name_tok.text());
                if class_name.starts_with("stream_") {
                    continue;
                }
                if !seen_class_names.insert(class_name.clone()) {
                    continue;
                }

                let is_dynamic = class_def
                    .block_attributes()
                    .any(|a| a.full_name().as_deref() == Some("dynamic"));

                // Build PPIR fields from CST (type-level attrs captured by PpirTy::from_ast)
                let ppir_fields = expand::build_ppir_fields(&class_def);

                // Expand each field
                let expanded_fields: Vec<PpirExpandedField> = ppir_fields
                    .iter()
                    .map(|pf| expand::expand_field(pf, &names, db))
                    .collect();

                expanded_classes.push(PpirExpandedClass {
                    name: class_name,
                    fields: expanded_fields,
                    is_dynamic,
                });
            }

            SyntaxKind::TYPE_ALIAS_DEF => {
                let Some(alias_def) = baml_compiler_syntax::ast::TypeAliasDef::cast(child.clone())
                else {
                    continue;
                };
                let Some(name_tok) = alias_def.name() else {
                    continue;
                };
                let alias_name: Name = SmolStr::new(name_tok.text());
                if alias_name.starts_with("stream_") {
                    continue;
                }
                if !seen_alias_names.insert(alias_name.clone()) {
                    continue;
                }

                let ty = alias_def
                    .ty()
                    .map(|te| PpirTy::from_ast(&te))
                    .unwrap_or(PpirTy::Unknown {
                        attrs: PpirTypeAttrs::default(),
                    });

                let expanded_body = expand::stream_expand(&ty, &names, db);

                expanded_aliases.push(PpirExpandedTypeAlias {
                    name: alias_name,
                    expanded_body,
                });
            }

            _ => {}
        }
    }

    PpirExpandedItems::new(db, expanded_classes, expanded_aliases)
}

/// Bridge query: synthesize `stream_*` items from expansion data.
///
/// Calls `ppir_expanded_items` and converts the expansion data into the old
/// output format (Class, TypeAlias, NormalizedStreamClass) for HIR consumption.
/// Will be replaced when Phase 3 takes over synthesis.
#[salsa::tracked]
pub fn ppir_stream_items(db: &dyn Db, file: SourceFile) -> PpirStreamItems<'_> {
    let expanded = ppir_expanded_items(db, file);
    let expanded_classes = expanded.classes(db);
    let expanded_aliases = expanded.type_aliases(db);

    // Synthesize bridge classes
    let stream_classes: Vec<Class> = expanded_classes
        .iter()
        .map(expand::synthesize_bridge_class)
        .collect();

    // Synthesize bridge type aliases
    let stream_aliases: Vec<TypeAlias> = expanded_aliases
        .iter()
        .map(expand::synthesize_bridge_type_alias)
        .collect();

    // Compute normalized streaming annotations
    let normalized_classes: Vec<NormalizedStreamClass> = expanded_classes
        .iter()
        .map(|ec| NormalizedStreamClass {
            name: ec.name.clone(),
            fields: normalize::normalize_expanded_fields(&ec.fields),
        })
        .collect();

    PpirStreamItems::new(db, stream_classes, stream_aliases, normalized_classes)
}

#[cfg(test)]
mod tests;

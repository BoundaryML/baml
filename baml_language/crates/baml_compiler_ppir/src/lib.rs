//! Pre-Processed Intermediate Representation (PPIR).
//!
//! Sits between the parser and HIR. Responsible for:
//! 1. Stream annotation capture from CST
//! 2. Cross-file name classification
//! 3. Stream type expansion (generating `stream_*` classes and type aliases)
//! 4. Attribute desugaring (@stream.done / @`stream.not_null` → canonical forms)
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

mod cst_extract;
mod expand;
mod ty;

pub use expand::{Class, Field, TypeAlias, default_starts_as, desugar_stream_attrs};
pub use ty::{ClassifiedField, PpirTypeRef, Ty};

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

/// Per-file result of PPIR stream expansion.
#[salsa::tracked]
pub struct PpirStreamItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub classes: Vec<Class>,
    #[tracked]
    #[returns(ref)]
    pub type_aliases: Vec<TypeAlias>,
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

/// Synthesize `stream_*` items for a single file.
///
/// Reads from CST only (via `syntax_tree`) + `ppir_names` for classification.
/// Does NOT depend on HIR.
///
/// Returns `PpirStreamItems` containing `Class` and `TypeAlias`
/// values. HIR converts these to `hir::Class` and `hir::TypeAlias` in
/// `file_item_tree`.
#[salsa::tracked]
pub fn ppir_stream_items(db: &dyn Db, file: SourceFile) -> PpirStreamItems<'_> {
    // Skip builtin files — they define internal types that don't need stream_* variants.
    let file_path = file.path(db);
    if file_path
        .to_str()
        .is_some_and(|p| p.starts_with("<builtin>/"))
    {
        return PpirStreamItems::new(db, Vec::new(), Vec::new());
    }

    let cst = syntax_tree(db, file);
    let project = db.project();
    let names = ppir_names(db, project);

    // Extract stream annotations from CST
    let stream_attrs_by_class = cst_extract::extract_stream_attrs_from_cst(&cst);

    let mut stream_classes = Vec::new();
    let mut stream_aliases = Vec::new();

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

                // Check if class is @@dynamic
                let is_dynamic = class_def
                    .block_attributes()
                    .any(|a| a.full_name().as_deref() == Some("dynamic"));

                let cst_attrs = stream_attrs_by_class.get(class_name.as_str());

                // Build PPIR fields from CST
                let ppir_fields = expand::build_ppir_fields(&class_def, cst_attrs, &names, db);

                // Expand to stream_* class
                let stream_class =
                    expand::expand_stream_class(&class_name, is_dynamic, &ppir_fields);
                stream_classes.push(stream_class);
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

                // Parse the alias's type expression
                let type_ref = alias_def
                    .ty()
                    .map(|te| PpirTypeRef::from_ast(&te))
                    .unwrap_or(PpirTypeRef::Unknown);

                // Expand to stream_* alias
                let stream_alias =
                    expand::expand_stream_type_alias(&alias_name, &type_ref, &names, db);
                stream_aliases.push(stream_alias);
            }

            _ => {}
        }
    }

    PpirStreamItems::new(db, stream_classes, stream_aliases)
}

#[cfg(test)]
mod tests;

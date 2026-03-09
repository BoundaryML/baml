//! Pre-Processed Intermediate Representation (PPIR).
//!
//! Sits between the parser and HIR. Responsible for:
//! 1. Stream annotation capture from CST (type-level via `PpirTy::from_ast`,
//!    field-level via `build_ppir_fields`)
//! 2. Cross-file name classification (`PpirNames`)
//! 3. Stream type expansion (`stream_expand` on `PpirTy`)
//! 4. `@sap.*` attribute synthesis (`sap_starts_as`, `sap_in_progress_never`)
//!
//! PPIR does **not** depend on HIR — it defines its own types and reads the CST
//! directly. HIR depends on PPIR, calls its tracked functions, and converts
//! PPIR output types into HIR types.

use std::path::PathBuf;

use baml_base::{FileId, Name, SourceFile};
use baml_compiler_parser::syntax_tree;
use baml_compiler_syntax::SyntaxKind;
use baml_workspace::Project;
use rowan::ast::AstNode as _;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

mod desugar;
pub mod expand_cst;
pub mod normalize;
mod stream_attrs;
mod ty;
mod validate;

pub use desugar::{
    PpirDesugaredClass, PpirDesugaredField, PpirDesugaredTypeAlias, PpirStreamStartsAs,
    default_sap_starts_as, extract_starts_as_text, stream_expand,
};
pub use normalize::{
    StartsAs, StartsAsLiteral, default_starts_as_semantic, infer_typeof_s, parse_starts_as_value,
};
pub use stream_attrs::{
    StreamAttrArgRule, StreamAttrKind, StreamAttrTarget, parse_stream_attr, valid_stream_attr_names,
};
pub use ty::{PpirField, PpirTy, PpirTypeAttrs};
pub use validate::ppir_stream_diagnostics;

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
///
/// `class_names` and `enum_names` are maps from name → list of `@@stream.*`
/// block attribute names on that type (empty vec if none). This carries
/// block attrs co-located with the name for cross-type lookups.
#[salsa::tracked]
pub struct PpirNames<'db> {
    /// Map from class name → list of @@stream.* block attribute names.
    /// E.g. "Education" → ["stream.not_null"], "Resume" → []
    #[tracked]
    #[returns(ref)]
    pub class_names: FxHashMap<Name, Vec<Name>>,
    /// Map from enum name → list of @@stream.* block attribute names.
    #[tracked]
    #[returns(ref)]
    pub enum_names: FxHashMap<Name, Vec<Name>>,
    #[tracked]
    #[returns(ref)]
    pub type_alias_names: FxHashSet<Name>,
}

/// Per-file result of PPIR desugaring.
/// Contains desugared data for classes and type aliases.
/// Used by expand_cst to clone-and-transform original CST nodes.
#[salsa::tracked]
pub struct PpirDesugaredItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub classes: Vec<PpirDesugaredClass>,
    #[tracked]
    #[returns(ref)]
    pub type_aliases: Vec<PpirDesugaredTypeAlias>,
}

/// Per-file CST expansion result.
///
/// Contains a synthesized SOURCE_FILE GreenNode with all stream_* CLASS_DEF
/// and TYPE_ALIAS_DEF children. The FileId is computed deterministically
/// via `FileId::stream_expansion()`.
#[salsa::tracked]
pub struct PpirExpansionCst<'db> {
    /// The synthesized SOURCE_FILE GreenNode.
    /// None if the origin file has no stream_* expansions.
    #[tracked]
    #[returns(ref)]
    pub green: Option<baml_compiler_syntax::GreenNode>,

    /// The source text of the synthesized file (for diagnostic rendering).
    #[tracked]
    #[returns(ref)]
    pub text: String,

    /// The display path for diagnostics (e.g. `<generated:stream/resume.baml>`).
    #[tracked]
    #[returns(ref)]
    pub display_path: String,

    /// Synthetic SourceFile for interning items under a separate FileId.
    /// None if the origin file has no stream_* expansions.
    #[tracked]
    pub source_file: Option<SourceFile>,
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
    /// Collect `@@stream.*` block attribute names from any definition's block attrs.
    fn stream_block_attrs(
        block_attrs: impl Iterator<Item = baml_compiler_syntax::ast::BlockAttribute>,
    ) -> Vec<Name> {
        block_attrs
            .filter_map(|a| {
                let name = a.full_name()?;
                name.starts_with("stream.")
                    .then(|| SmolStr::from(name.as_str()))
            })
            .collect()
    }

    let mut class_names: FxHashMap<Name, Vec<Name>> = FxHashMap::default();
    let mut enum_names: FxHashMap<Name, Vec<Name>> = FxHashMap::default();
    let mut type_alias_names: FxHashSet<Name> = FxHashSet::default();

    for file in project.files(db) {
        if file.is_virtual(db) {
            continue;
        }
        let cst = syntax_tree(db, *file);
        for child in cst.children() {
            match child.kind() {
                SyntaxKind::CLASS_DEF => {
                    if let Some(def) = baml_compiler_syntax::ast::ClassDef::cast(child) {
                        if let Some(tok) = def.name() {
                            let name = SmolStr::new(tok.text());
                            class_names
                                .entry(name)
                                .or_insert_with(|| stream_block_attrs(def.block_attributes()));
                        }
                    }
                }
                SyntaxKind::ENUM_DEF => {
                    if let Some(def) = baml_compiler_syntax::ast::EnumDef::cast(child) {
                        if let Some(tok) = def.name() {
                            let name = SmolStr::new(tok.text());
                            enum_names
                                .entry(name)
                                .or_insert_with(|| stream_block_attrs(def.block_attributes()));
                        }
                    }
                }
                SyntaxKind::TYPE_ALIAS_DEF => {
                    if let Some(def) = baml_compiler_syntax::ast::TypeAliasDef::cast(child) {
                        if let Some(tok) = def.name() {
                            type_alias_names.insert(SmolStr::new(tok.text()));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    PpirNames::new(db, class_names, enum_names, type_alias_names)
}

/// Compute desugared stream data for a single file.
///
/// For each class: runs `stream_expand` per field to compute `stream_type`,
/// synthesizes `@sap.*` attributes (`sap_in_progress_never`, `sap_starts_as`).
/// For each type alias: runs `stream_expand` on the alias body.
///
/// Does NOT synthesize `stream_*` definitions — that happens in `expand_cst`
/// via clone-and-transform of the original CST.
#[salsa::tracked]
pub fn ppir_desugared_items(db: &dyn Db, file: SourceFile) -> PpirDesugaredItems<'_> {
    /// Return `Some(name)` for a CST node name that is not a `stream_*` generated
    /// name and has not yet been seen (dedup guard). Returns `None` to skip.
    fn accept_name(
        seen: &mut FxHashSet<Name>,
        tok: Option<baml_compiler_syntax::SyntaxToken>,
    ) -> Option<Name> {
        let name = SmolStr::new(tok?.text());
        if name.starts_with("stream_") || !seen.insert(name.clone()) {
            return None;
        }
        Some(name)
    }

    if file.is_virtual(db) {
        return PpirDesugaredItems::new(db, Vec::new(), Vec::new());
    }

    let cst = syntax_tree(db, file);
    let project = db.project();
    let names = ppir_names(db, project);

    let mut desugared_classes: Vec<PpirDesugaredClass> = Vec::new();
    let mut desugared_aliases: Vec<PpirDesugaredTypeAlias> = Vec::new();
    let mut seen_class_names: FxHashSet<Name> = FxHashSet::default();
    let mut seen_alias_names: FxHashSet<Name> = FxHashSet::default();

    for child in cst.children() {
        match child.kind() {
            SyntaxKind::CLASS_DEF => {
                let Some(def) = baml_compiler_syntax::ast::ClassDef::cast(child) else {
                    continue;
                };
                let Some(name) = accept_name(&mut seen_class_names, def.name()) else {
                    continue;
                };
                let fields = desugar::build_ppir_fields(&def)
                    .iter()
                    .map(|pf| desugar::desugar_field(pf, names, db))
                    .collect();
                desugared_classes.push(PpirDesugaredClass { name, fields });
            }

            SyntaxKind::TYPE_ALIAS_DEF => {
                let Some(def) = baml_compiler_syntax::ast::TypeAliasDef::cast(child) else {
                    continue;
                };
                let Some(name) = accept_name(&mut seen_alias_names, def.name()) else {
                    continue;
                };
                let ty = def
                    .ty()
                    .map(|te| PpirTy::from_ast(&te))
                    .unwrap_or(PpirTy::Unknown {
                        attrs: PpirTypeAttrs::default(),
                    });
                let expanded_body = desugar::stream_expand(&ty, names, db);
                desugared_aliases.push(PpirDesugaredTypeAlias {
                    name,
                    expanded_body,
                });
            }

            _ => {}
        }
    }

    PpirDesugaredItems::new(db, desugared_classes, desugared_aliases)
}

/// Compute the CST expansion for a single file.
///
/// Performs stream type desugaring, then clones-and-transforms original CST nodes
/// via `expand_cst` to produce synthesized `stream_*` definitions. Non-stream
/// attributes (alias, description, skip, dynamic, etc.) pass through automatically.
#[salsa::tracked]
pub fn ppir_expansion_cst(db: &dyn Db, file: SourceFile) -> PpirExpansionCst<'_> {
    let desugared = ppir_desugared_items(db, file);
    let desugared_classes = desugared.classes(db);
    let desugared_aliases = desugared.type_aliases(db);

    let original_cst = syntax_tree(db, file);
    let green =
        expand_cst::build_stream_source_file(&original_cst, desugared_classes, desugared_aliases);

    match green {
        None => PpirExpansionCst::new(db, None, String::new(), String::new(), None),
        Some(green) => {
            let text = baml_compiler_syntax::SyntaxNode::new_root(green.clone())
                .text()
                .to_string();
            let file_name = file
                .path(db)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let display_path =
                format!("{}stream/{file_name}>", SourceFile::generated_path_prefix());
            let file_id = FileId::stream_expansion(file.file_id(db));
            let synth_file =
                SourceFile::new(db, text.clone(), PathBuf::from(&display_path), file_id);
            PpirExpansionCst::new(db, Some(green), text, display_path, Some(synth_file))
        }
    }
}

#[cfg(test)]
mod tests;

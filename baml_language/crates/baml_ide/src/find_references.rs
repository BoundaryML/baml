//! Find all references for BAML files.
//!
//! This module provides LSP-agnostic find-references functionality.
//! Given a cursor position, it finds all references to the symbol under the cursor.

use std::path::PathBuf;

use baml_db::{
    FileId, Span,
    baml_compiler_hir::{ExprBody, ExprId, FunctionLoc},
    baml_compiler_tir::ResolvedValue,
};
use baml_project::ProjectDatabase;
use rowan::ast::AstNode;
use text_size::TextSize;

/// A reference location in source code.
#[derive(Debug, Clone)]
pub struct Reference {
    /// The file containing the reference.
    pub file_path: PathBuf,
    /// The span of the reference.
    pub span: Span,
    /// Whether this is the definition (not just a reference).
    pub is_definition: bool,
}

impl Reference {
    /// Create a new reference.
    pub fn new(file_path: PathBuf, span: Span, is_definition: bool) -> Self {
        Self {
            file_path,
            span,
            is_definition,
        }
    }
}

/// Find all references to the symbol at the given position.
///
/// Returns an empty vector if:
/// - No symbol is found at the position
/// - The symbol cannot be resolved
///
/// The returned references include the definition itself (marked with `is_definition: true`).
pub fn find_all_references(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Vec<Reference> {
    // First, find what symbol is at the cursor position
    let target_resolution = find_symbol_at_position(db, file_id, position);
    let Some(target) = target_resolution else {
        return Vec::new();
    };

    // Search all functions in all files for references to this symbol
    find_references_to_symbol(db, &target)
}

/// Find the symbol at the given cursor position.
fn find_symbol_at_position(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Option<ResolvedValue> {
    // Get the source file
    let source_files = db.get_source_files();
    let source_file = source_files.iter().find(|f| f.file_id(db) == file_id)?;
    let text = source_file.text(db);

    // Find the word at the cursor position
    let word_range = crate::goto_definition::find_word_at_offset(text, position)?;
    let word = &text[word_range.start().into()..word_range.end().into()];

    // First, check if we're on a top-level definition (class, enum, function)
    if let Some(resolution) = find_definition_at_position(db, *source_file, position, word) {
        return Some(resolution);
    }

    // Try to find expression at position within a function
    if let Some(function_loc) = find_function_at_position(db, file_id, position) {
        // Get the function body
        let body = baml_db::baml_compiler_hir::function_body(db, function_loc);
        let baml_db::baml_compiler_hir::FunctionBody::Expr(expr_body, source_map) = &*body else {
            return None;
        };

        // Find the expression at this position
        if let Some(expr_id) = find_expr_at_position(expr_body, source_map, position) {
            // Get the type inference results
            let inference_result =
                baml_db::baml_compiler_tir::function_type_inference(db, function_loc);

            // Look up the resolution for this expression
            if let Some(resolution) = inference_result.expr_resolutions.get(&expr_id) {
                return Some(resolution.clone());
            }
        }

        // Check if we're on a local variable definition (let statement)
        // or a parameter definition
        if let Some(resolution) =
            find_local_definition_at_position(db, function_loc, position, word)
        {
            return Some(resolution);
        }
    }

    None
}

/// Find a top-level definition (class, enum, function) at the given position.
fn find_definition_at_position(
    db: &ProjectDatabase,
    source_file: baml_db::SourceFile,
    position: TextSize,
    word: &str,
) -> Option<ResolvedValue> {
    use baml_db::baml_compiler_hir::QualifiedName;
    use rowan::ast::AstNode;

    // Get the syntax tree
    let tree = baml_db::baml_compiler_parser::syntax_tree(db, source_file);
    let ast_file = baml_db::baml_compiler_syntax::ast::SourceFile::cast(tree)?;

    // Check each top-level item
    for item in ast_file.items() {
        let item_range = item.syntax().text_range();
        if !item_range.contains(position) {
            continue;
        }

        match item {
            baml_db::baml_compiler_syntax::ast::Item::Class(class_node) => {
                if let Some(name_token) = class_node.name() {
                    if name_token.text_range().contains(position) && name_token.text() == word {
                        return Some(ResolvedValue::Class(QualifiedName::local(
                            baml_db::Name::new(word),
                        )));
                    }
                }
            }
            baml_db::baml_compiler_syntax::ast::Item::Enum(enum_node) => {
                if let Some(name_token) = enum_node.name() {
                    if name_token.text_range().contains(position) && name_token.text() == word {
                        return Some(ResolvedValue::Enum(QualifiedName::local(
                            baml_db::Name::new(word),
                        )));
                    }
                }
            }
            baml_db::baml_compiler_syntax::ast::Item::Function(func_node) => {
                if let Some(name_token) = func_node.name() {
                    if name_token.text_range().contains(position) && name_token.text() == word {
                        return Some(ResolvedValue::Function(QualifiedName::local(
                            baml_db::Name::new(word),
                        )));
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Find a local variable or parameter definition at the given position.
fn find_local_definition_at_position(
    db: &ProjectDatabase,
    function_loc: FunctionLoc<'_>,
    position: TextSize,
    word: &str,
) -> Option<ResolvedValue> {
    use baml_db::baml_compiler_tir::DefinitionSite;

    // Check parameters first
    let signature = baml_db::baml_compiler_hir::function_signature(db, function_loc);
    let sig_source_map =
        baml_db::baml_compiler_hir::function_signature_source_map(db, function_loc);
    for (index, param) in signature.params.iter().enumerate() {
        if param.name.as_str() == word {
            if let Some(param_span) = sig_source_map.param_span(index) {
                if param_span.contains(position) {
                    return Some(ResolvedValue::Local {
                        name: baml_db::Name::new(word),
                        definition_site: Some(DefinitionSite::Parameter(index)),
                    });
                }
            }
        }
    }

    // Check let statements in the function body
    let body = baml_db::baml_compiler_hir::function_body(db, function_loc);
    if let baml_db::baml_compiler_hir::FunctionBody::Expr(expr_body, source_map) = &*body {
        for (stmt_id, stmt) in expr_body.stmts.iter() {
            if let baml_db::baml_compiler_hir::Stmt::Let { pattern, .. } = stmt {
                // Get the pattern name
                let pat = &expr_body.patterns[*pattern];
                if let baml_db::baml_compiler_hir::Pattern::Binding(name) = pat {
                    if name.as_str() == word {
                        // Check if position is within this statement's span
                        if let Some(span) = source_map.stmt_span(stmt_id) {
                            if span.range.contains(position) {
                                return Some(ResolvedValue::Local {
                                    name: name.clone(),
                                    definition_site: Some(DefinitionSite::Statement(stmt_id)),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Find the function containing the given position (reusing logic from `goto_definition`).
fn find_function_at_position(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Option<FunctionLoc<'_>> {
    // Get the source file
    let source_files = db.get_source_files();
    let source_file = source_files.iter().find(|f| f.file_id(db) == file_id)?;

    // Get all items in the file
    let file_items = baml_db::baml_compiler_hir::file_items(db, *source_file);

    // Get the syntax tree to check text ranges
    let tree = baml_db::baml_compiler_parser::syntax_tree(db, *source_file);
    let ast_file = baml_db::baml_compiler_syntax::ast::SourceFile::cast(tree)?;

    // Iterate through items to find functions
    for item_id in file_items.items(db) {
        if let baml_db::baml_compiler_hir::ItemId::Function(func_loc) = item_id {
            let item_tree = baml_db::baml_compiler_hir::file_item_tree(db, *source_file);
            let func = &item_tree[func_loc.id(db)];
            let func_name = &func.name;

            // Find the function node in the AST
            for item in ast_file.items() {
                if let baml_db::baml_compiler_syntax::ast::Item::Function(func_node) = item {
                    if let Some(name) = func_node.name() {
                        if name.text() == func_name {
                            let range = func_node.syntax().text_range();
                            if range.contains(position) {
                                return Some(*func_loc);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Find the expression at the given position.
fn find_expr_at_position(
    body: &ExprBody,
    source_map: &baml_db::baml_compiler_hir::HirSourceMap,
    position: TextSize,
) -> Option<ExprId> {
    // Find the smallest expression that contains this position
    let mut candidates: Vec<(ExprId, text_size::TextRange)> = Vec::new();
    for (expr_id, _expr) in body.exprs.iter() {
        if let Some(span) = source_map.expr_span(expr_id) {
            if span.range.contains(position) {
                candidates.push((expr_id, span.range));
            }
        }
    }

    // Sort by range length (smallest first) and return the smallest
    candidates.sort_by_key(|(_, range)| range.len());
    candidates.first().map(|(expr_id, _)| *expr_id)
}

/// Find all references to a specific symbol.
fn find_references_to_symbol(db: &ProjectDatabase, target: &ResolvedValue) -> Vec<Reference> {
    let mut references = Vec::new();

    // Get all files in the project
    let source_files = db.get_source_files();

    for source_file in &source_files {
        let file_id = source_file.file_id(db);
        let file_path = match db.file_id_to_path(file_id) {
            Some(path) => path.clone(),
            None => continue,
        };

        // Get all items in the file
        let file_items = baml_db::baml_compiler_hir::file_items(db, *source_file);

        // Check each function in the file
        for item_id in file_items.items(db) {
            if let baml_db::baml_compiler_hir::ItemId::Function(func_loc) = item_id {
                // Get the function body
                let body = baml_db::baml_compiler_hir::function_body(db, *func_loc);
                let baml_db::baml_compiler_hir::FunctionBody::Expr(_expr_body, source_map) = &*body
                else {
                    continue;
                };

                // Get the type inference results
                let inference_result =
                    baml_db::baml_compiler_tir::function_type_inference(db, *func_loc);

                // Search for matching resolutions
                for (expr_id, resolution) in &inference_result.expr_resolutions {
                    if is_same_resolution(target, resolution) {
                        // Get the span for this expression
                        if let Some(span) = source_map.expr_span(*expr_id) {
                            references.push(Reference::new(
                                file_path.clone(),
                                span,
                                false, // Not tracking definitions separately for now
                            ));
                        }
                    }
                }
            }
        }
    }

    references
}

/// Check if two resolved values refer to the same entity.
/// For enums, also matches enum variants that belong to the same enum.
/// For classes, also matches field accesses on that class.
fn is_same_resolution(a: &ResolvedValue, b: &ResolvedValue) -> bool {
    match (a, b) {
        (
            ResolvedValue::Local {
                name: n1,
                definition_site: d1,
            },
            ResolvedValue::Local {
                name: n2,
                definition_site: d2,
            },
        ) => n1 == n2 && d1 == d2,

        (ResolvedValue::Function(f1), ResolvedValue::Function(f2)) => f1 == f2,
        (ResolvedValue::Class(c1), ResolvedValue::Class(c2)) => c1 == c2,
        (ResolvedValue::Enum(e1), ResolvedValue::Enum(e2)) => e1 == e2,
        (ResolvedValue::TypeAlias(t1), ResolvedValue::TypeAlias(t2)) => t1 == t2,

        (
            ResolvedValue::EnumVariant {
                enum_fqn: e1,
                variant: v1,
            },
            ResolvedValue::EnumVariant {
                enum_fqn: e2,
                variant: v2,
            },
        ) => e1 == e2 && v1 == v2,

        // When searching for an enum, also match enum variants using that enum
        (
            ResolvedValue::Enum(enum_fqn),
            ResolvedValue::EnumVariant {
                enum_fqn: variant_enum,
                ..
            },
        )
        | (
            ResolvedValue::EnumVariant {
                enum_fqn: variant_enum,
                ..
            },
            ResolvedValue::Enum(enum_fqn),
        ) => enum_fqn == variant_enum,

        // When searching for a class, also match object instantiations
        (ResolvedValue::Class(c1), ResolvedValue::Field { class_fqn: c2, .. })
        | (ResolvedValue::Field { class_fqn: c2, .. }, ResolvedValue::Class(c1)) => c1 == c2,

        (
            ResolvedValue::Field {
                class_fqn: c1,
                field: f1,
            },
            ResolvedValue::Field {
                class_fqn: c2,
                field: f2,
            },
        ) => c1 == c2 && f1 == f2,

        (ResolvedValue::BuiltinFunction(p1), ResolvedValue::BuiltinFunction(p2)) => p1 == p2,

        _ => false,
    }
}

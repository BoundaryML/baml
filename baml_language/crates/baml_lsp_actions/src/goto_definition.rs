//! Go to definition for BAML files.
//!
//! This module provides LSP-agnostic goto-definition types.
//! Given a cursor position, it finds the definition of the symbol under the cursor.

use std::{path::PathBuf, sync::Arc};

use baml_db::{
    FileId, Span,
    baml_compiler_hir::{Expr, ExprBody, ExprId, FunctionLoc, QualifiedName},
    baml_compiler_tir::{DefinitionSite, ResolvedValue},
};
use baml_project::ProjectDatabase;
use rowan::ast::AstNode;
use text_size::{TextRange, TextSize};

/// A navigation target representing a definition location.
#[derive(Debug, Clone)]
pub struct NavigationTarget {
    /// The name of the symbol.
    pub name: String,
    /// The file containing the definition.
    pub file_path: PathBuf,
    /// The span of the definition.
    pub span: Span,
}

impl NavigationTarget {
    /// Create a new navigation target.
    pub fn new(name: impl Into<String>, file_path: PathBuf, span: Span) -> Self {
        Self {
            name: name.into(),
            file_path,
            span,
        }
    }
}

/// Find the word (identifier) at the given offset.
pub fn find_word_at_offset(text: &str, offset: TextSize) -> Option<TextRange> {
    let offset: usize = offset.into();
    if offset > text.len() {
        return None;
    }

    let bytes = text.as_bytes();

    // Find start of word
    let mut start = offset;
    while start > 0 && is_identifier_char(bytes[start - 1]) {
        start -= 1;
    }

    // Find end of word
    let mut end = offset;
    while end < bytes.len() && is_identifier_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    #[allow(clippy::cast_possible_truncation)]
    Some(TextRange::new(
        TextSize::new(start as u32),
        TextSize::new(end as u32),
    ))
}

/// Check if a byte is a valid identifier character.
fn is_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Go to the definition of the symbol at the given position.
///
/// Returns `None` if:
/// - No symbol is found at the position
/// - The symbol cannot be resolved
/// - The definition location cannot be determined
pub fn goto_definition(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Option<NavigationTarget> {
    // Get the source file
    let source_files = db.get_source_files();
    let source_file = source_files.iter().find(|f| f.file_id(db) == file_id)?;
    let text = source_file.text(db);

    // Find the word at the cursor position
    let word_range = find_word_at_offset(text, position)?;
    let word = &text[word_range.start().into()..word_range.end().into()];
    tracing::debug!(word, "goto_definition");

    // Get the function containing this position
    let Some(function_loc) = find_function_at_position(db, file_id, position) else {
        // Outside a function body, try resolving the word as a type/value name directly.
        // This usually happens when this position came from an inlay hint and was already resolved.
        let fqn = QualifiedName::local(word.into());
        return lookup_symbol_definition(db, &fqn);
    };

    // Get the function body
    let body = baml_db::baml_compiler_hir::function_body(db, function_loc);

    // Find the expression at this position
    let (expr_body, source_map) = match &*body {
        baml_db::baml_compiler_hir::FunctionBody::Expr(expr_body, source_map) => {
            (expr_body, source_map)
        }
        _other => return None, // Can't find expressions in missing or error bodies
    };

    let expr_id = find_expr_at_position(expr_body, source_map, position);

    // If no expression found at position, fall through to the type name lookup fallback
    let Some(expr_id) = expr_id else {
        let fqn = QualifiedName::local(word.into());
        return lookup_symbol_definition(db, &fqn);
    };

    let expr = &expr_body.exprs[expr_id];

    // Special case: if cursor is on a field name in an Object constructor,
    // navigate to the field definition in the class
    if let Expr::Object {
        type_name: Some(class_name),
        fields,
        ..
    } = &expr_body.exprs[expr_id]
    {
        // Check if the word at cursor matches any field name in the object
        if fields.iter().any(|(name, _)| name.as_str() == word) {
            // Look up the class to get its location
            let project = db.get_project()?;
            let symbol_table = baml_db::baml_compiler_hir::symbol_table(db, project);
            let class_fqn = QualifiedName::local(class_name.clone());
            if let Some(baml_db::baml_compiler_hir::Definition::Class(class_loc)) =
                symbol_table.lookup_type(db, &class_fqn)
            {
                if let Some(span) =
                    baml_db::baml_compiler_hir::class_field_name_span(db, class_loc, word)
                {
                    let file_path = db.file_id_to_path(span.file_id)?;
                    return Some(NavigationTarget::new(
                        word.to_string(),
                        file_path.clone(),
                        span,
                    ));
                }
            }
        }
    }

    // Special case: if cursor is on the first segment of a Path (the receiver in `s.field`),
    // resolve the first segment as a local variable instead of the whole path
    if let Expr::Path(segments) = expr {
        if segments.len() > 1 && segments.first().map(smol_str::SmolStr::as_str) == Some(word) {
            // First, check if it's a function parameter
            let signature = baml_db::baml_compiler_hir::function_signature(db, function_loc);
            let sig_source_map =
                baml_db::baml_compiler_hir::function_signature_source_map(db, function_loc);
            if let Some((index, param)) = signature
                .params
                .iter()
                .enumerate()
                .find(|(_, p)| p.name == word)
            {
                if let Some(param_span) = sig_source_map.param_span(index) {
                    let span = Span::new(file_id, param_span);
                    let file_path = db.file_id_to_path(file_id)?;
                    return Some(NavigationTarget::new(
                        param.name.clone(),
                        file_path.clone(),
                        span,
                    ));
                }
            }

            // If not a parameter, look for a local variable resolution
            let inference_result = get_function_inference(db, function_loc);
            for resolution in inference_result.expr_resolutions.values() {
                if let ResolvedValue::Local {
                    name,
                    definition_site: _,
                } = resolution
                {
                    if name == word {
                        if let Some(target) = resolution_to_navigation_target(
                            db,
                            resolution,
                            source_map,
                            file_id,
                            function_loc,
                        ) {
                            return Some(target);
                        }
                    }
                }
            }
        }
    }

    // Get the type inference results for the function
    let inference_result = get_function_inference(db, function_loc);

    // Look up the resolution for this expression
    let resolution = inference_result.expr_resolutions.get(&expr_id);

    // If we have a resolution, try to navigate to it
    if let Some(resolution) = resolution {
        if let Some(target) =
            resolution_to_navigation_target(db, resolution, source_map, file_id, function_loc)
        {
            return Some(target);
        }
    }

    // Fallback: try looking up the word as a type name
    // This handles cases like type annotations in match patterns (e.g., `f: Failure`)
    // where the cursor is on a type name that isn't part of an expression
    let fqn = QualifiedName::local(word.into());
    lookup_symbol_definition(db, &fqn)
}

/// Find the function containing the given position.
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
    let ast_file = baml_db::baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

    // Iterate through items to find functions
    for item_id in file_items.items(db) {
        if let baml_db::baml_compiler_hir::ItemId::Function(func_loc) = item_id {
            // Get the function from the item tree
            let item_tree = baml_db::baml_compiler_hir::file_item_tree(db, *source_file);
            let func = &item_tree[func_loc.id(db)];
            let func_name = &func.name;

            // Find the function node in the AST
            for item in ast_file.items() {
                match item {
                    baml_db::baml_compiler_syntax::ast::Item::Function(func_node) => {
                        if let Some(name) = func_node.name() {
                            if name.text() == func_name {
                                // Check if position is within this function's range
                                let range = func_node.syntax().text_range();
                                if range.contains(position) {
                                    return Some(*func_loc);
                                }
                            }
                        }
                    }
                    baml_db::baml_compiler_syntax::ast::Item::Class(class_node) => {
                        // Check methods in classes
                        // Method func_name is qualified as "ClassName.methodName"
                        // AST method name is just "methodName"
                        let class_name = class_node
                            .name()
                            .map(|n| n.text().to_string())
                            .unwrap_or_else(|| "UnnamedClass".to_string());
                        for method in class_node.methods() {
                            if let Some(name) = method.name() {
                                // Compare against qualified name (ClassName.methodName)
                                let qualified_method_name =
                                    QualifiedName::local_method_from_str(&class_name, name.text());
                                if qualified_method_name.as_str() == func_name.as_str() {
                                    let range = method.syntax().text_range();
                                    if range.contains(position) {
                                        return Some(*func_loc);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

/// Get the type inference results for a function.
fn get_function_inference(
    db: &ProjectDatabase,
    function_loc: FunctionLoc,
) -> Arc<baml_db::baml_compiler_tir::InferenceResult> {
    // Query the TIR for the function's type inference results
    baml_db::baml_compiler_tir::function_type_inference(db, function_loc)
}

/// Find the expression at the given position.
fn find_expr_at_position(
    body: &ExprBody,
    source_map: &baml_db::baml_compiler_hir::HirSourceMap,
    position: TextSize,
) -> Option<ExprId> {
    // Find ALL expressions that contain this position, then select the smallest
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

/// Convert a resolution to a navigation target.
fn resolution_to_navigation_target(
    db: &ProjectDatabase,
    resolution: &ResolvedValue,
    source_map: &baml_db::baml_compiler_hir::HirSourceMap,
    file_id: FileId,
    function_loc: FunctionLoc,
) -> Option<NavigationTarget> {
    match resolution {
        ResolvedValue::Local {
            name,
            definition_site,
        } => {
            // Navigate to the local variable's definition
            match definition_site {
                Some(DefinitionSite::Statement(stmt_id)) => {
                    // Get the span from the source map's statement spans
                    let span = source_map.stmt_span(*stmt_id)?;
                    let file_path = db.file_id_to_path(file_id)?.clone();
                    Some(NavigationTarget::new(name.clone(), file_path, span))
                }
                Some(DefinitionSite::Parameter(_index)) => {
                    // Get the function signature to find the parameter span
                    // Note: We use the name from the resolution because param_types doesn't
                    // preserve order, so the index may not be accurate
                    let signature =
                        baml_db::baml_compiler_hir::function_signature(db, function_loc);
                    let sig_source_map =
                        baml_db::baml_compiler_hir::function_signature_source_map(db, function_loc);
                    let (param_idx, param) = signature
                        .params
                        .iter()
                        .enumerate()
                        .find(|(_, p)| p.name == *name)?;
                    let param_span = sig_source_map.param_span(param_idx)?;

                    // Create a span using the file_id and text range
                    let span = Span::new(file_id, param_span);
                    let file_path = db.file_id_to_path(file_id)?.clone();

                    Some(NavigationTarget::new(param.name.clone(), file_path, span))
                }
                None => None,
            }
        }
        ResolvedValue::Function(fqn) => lookup_symbol_definition(db, fqn),
        ResolvedValue::Class(fqn) => lookup_symbol_definition(db, fqn),
        ResolvedValue::Enum(fqn) => lookup_symbol_definition(db, fqn),
        ResolvedValue::TypeAlias(fqn) => lookup_symbol_definition(db, fqn),
        ResolvedValue::EnumVariant {
            enum_fqn,
            variant: _,
        } => {
            // TODO: Look up the specific enum variant
            // This requires the symbol table to track variant spans
            lookup_symbol_definition(db, enum_fqn)
        }
        ResolvedValue::Field { class_fqn, field } => {
            // Look up the class in the symbol table to get the ClassLoc
            let project = db.get_project()?;
            let symbol_table = baml_db::baml_compiler_hir::symbol_table(db, project);
            let definition = symbol_table.lookup_type(db, class_fqn)?;

            if let baml_db::baml_compiler_hir::Definition::Class(class_loc) = definition {
                // Get the field's span within the class
                if let Some(span) =
                    baml_db::baml_compiler_hir::class_field_name_span(db, class_loc, field)
                {
                    let file_path = db.file_id_to_path(span.file_id)?;
                    return Some(NavigationTarget::new(
                        field.clone(),
                        file_path.clone(),
                        span,
                    ));
                }
            }

            // Field not found - it might be a method (desugared to a top-level function)
            // Methods are registered with qualified names: "ClassName.methodName"
            let method_fqn = QualifiedName::local_method(&class_fqn.name, field);
            if let Some(target) = lookup_symbol_definition(db, &method_fqn) {
                return Some(target);
            }

            // Fallback to class definition if neither field nor method found
            lookup_symbol_definition(db, class_fqn)
        }
        ResolvedValue::BuiltinFunction(_) => {
            // Builtins don't have source definitions
            None
        }
        _ => None,
    }
}

/// Look up a symbol's definition in the symbol table.
pub(crate) fn lookup_symbol_definition(
    db: &ProjectDatabase,
    fqn: &QualifiedName,
) -> Option<NavigationTarget> {
    // Get the symbol table
    let project = db.get_project()?;
    let symbol_table = baml_db::baml_compiler_hir::symbol_table(db, project);

    // Look up the symbol in both type and value namespaces
    let definition = symbol_table
        .lookup_type(db, fqn)
        .or_else(|| symbol_table.lookup_value(db, fqn))?;

    // Get the span using the cached query
    let span = baml_db::baml_compiler_hir::definition_name_span(db, definition);
    let file_path = db.file_id_to_path(span.file_id)?;

    Some(NavigationTarget::new(
        fqn.name.to_string(),
        file_path.clone(),
        span,
    ))
}

#[cfg(test)]
mod tests {
    use baml_project::ProjectDatabase;

    use super::*;

    /// Create a test database with the given BAML source code.
    fn setup_test_db(source: &str) -> (ProjectDatabase, FileId) {
        let mut db = ProjectDatabase::new();

        // Create a temporary directory for the test
        let temp_dir = std::env::temp_dir().join(format!("baml_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Set the project root
        db.set_project_root(&temp_dir);

        // Add the test file
        let file_path = temp_dir.join("test.baml");
        db.add_file(&file_path, source);
        let file_id = db.path_to_file_id(&file_path).unwrap();

        // Clean up temp dir on drop would be nice but not critical for tests

        (db, file_id)
    }

    #[test]
    fn test_goto_definition_match_scrutinee() {
        let source = r#"enum SentimentResponse {
    Happy { data string }
    Sad { reason string }
}

function Foo(r SentimentResponse, s string) -> string {
    match (r) {
        Happy => s.data
        Sad(f) => f.reason
    }
}"#;

        let (db, file_id) = setup_test_db(source);

        // Find the position of 'r' in 'match (r)'
        let match_pos = source.find("match (r)").unwrap();
        let r_pos = match_pos + "match (".len();
        #[allow(clippy::cast_possible_truncation)]
        let position = TextSize::from(r_pos as u32);

        // Try to go to definition
        let result = goto_definition(&db, file_id, position);

        // Should find the parameter 'r' definition
        assert!(
            result.is_some(),
            "Should find definition for 'r' in match scrutinee"
        );

        if let Some(nav_target) = result {
            assert_eq!(
                nav_target.name, "r",
                "Expected to find parameter 'r' but found '{}'",
                nav_target.name
            );
            // The parameter span should contain "r SentimentResponse"
            // The exact span range depends on how the parser handles it
            assert!(
                nav_target.span.range.start() < TextSize::from(100),
                "Parameter should be in the function signature"
            );
            assert!(
                nav_target.span.range.end() > TextSize::from(93),
                "Parameter span should include the parameter name"
            );
        }
    }

    #[test]
    fn test_find_word_at_offset() {
        let text = "class Foo { name string }";

        // At 'F' in Foo
        let word = find_word_at_offset(text, TextSize::new(6));
        assert!(word.is_some());
        let range = word.unwrap();
        assert_eq!(&text[range.start().into()..range.end().into()], "Foo");

        // At 'n' in name
        let word = find_word_at_offset(text, TextSize::new(12));
        assert!(word.is_some());
        let range = word.unwrap();
        assert_eq!(&text[range.start().into()..range.end().into()], "name");

        // At space after "class" - finds "class" because cursor is at word boundary
        let word = find_word_at_offset(text, TextSize::new(5));
        assert!(word.is_some());
        let range = word.unwrap();
        assert_eq!(&text[range.start().into()..range.end().into()], "class");

        // At opening brace (pure punctuation with no adjacent identifier)
        // "{ " at offset 10 - byte 10 is '{', byte 9 is ' '
        // This should return None since we're not adjacent to an identifier
        let word = find_word_at_offset(text, TextSize::new(10));
        assert!(word.is_none());
    }

    #[test]
    fn test_is_identifier_char() {
        assert!(is_identifier_char(b'a'));
        assert!(is_identifier_char(b'Z'));
        assert!(is_identifier_char(b'0'));
        assert!(is_identifier_char(b'_'));
        assert!(!is_identifier_char(b' '));
        assert!(!is_identifier_char(b'{'));
    }
}

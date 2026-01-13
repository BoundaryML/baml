//! Go to definition for BAML files.
//!
//! This module provides LSP-agnostic goto-definition types.
//! Given a cursor position, it finds the definition of the symbol under the cursor.

use std::{path::PathBuf, sync::Arc};

use baml_db::{
    Span, FileId,
    baml_compiler_hir::{ExprId, FunctionLoc, FullyQualifiedName, ExprBody},
    baml_compiler_tir::{ResolvedValue, DefinitionSite},
};
use baml_project::ProjectDatabase;
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
    let _word_range = find_word_at_offset(&text, position)?;
    // let word = &text[word_range.start().into()..word_range.end().into()];

    // Get the function containing this position
    let function_loc = find_function_at_position(db, file_id, position)?;

    // Get the function body
    let body = baml_db::baml_compiler_hir::function_body(db, function_loc);

    // Find the expression at this position
    let expr_body = match &*body {
        baml_db::baml_compiler_hir::FunctionBody::Expr(expr_body) => expr_body,
        _ => return None, // Can't find expressions in missing or error bodies
    };
    let expr_id = find_expr_at_position(expr_body, position)?;

    // Get the type inference results for the function
    let inference_result = get_function_inference(db, function_loc)?;

    // Look up the resolution for this expression
    let resolution = inference_result.expr_resolutions.get(&expr_id)?;

    // Convert the resolution to a navigation target
    resolution_to_navigation_target(db, resolution, expr_body, file_id, function_loc)
}

/// Find the function containing the given position.
fn find_function_at_position(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Option<FunctionLoc> {
    // Get the source file
    let source_files = db.get_source_files();
    let source_file = source_files.iter().find(|f| f.file_id(db) == file_id)?;

    // Get all items in the file
    let file_items = baml_db::baml_compiler_hir::file_items(db, *source_file);

    // Get the syntax tree to check text ranges
    let tree = baml_db::baml_compiler_parser::syntax_tree(db, *source_file);
    use rowan::ast::AstNode;
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
                        for method in class_node.methods() {
                            if let Some(name) = method.name() {
                                if name.text() == func_name {
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
) -> Option<Arc<baml_db::baml_compiler_tir::InferenceResult>> {
    // Query the TIR for the function's type inference results
    Some(baml_db::baml_compiler_tir::function_type_inference(db, function_loc))
}

/// Find the expression at the given position.
fn find_expr_at_position(
    body: &ExprBody,
    position: TextSize,
) -> Option<ExprId> {
    // Find the smallest (most specific) expression that contains this position
    let mut smallest_expr: Option<(ExprId, text_size::TextRange)> = None;

    for (expr_id, span) in &body.expr_spans {
        if span.range.contains(position) {
            match smallest_expr {
                None => smallest_expr = Some((*expr_id, span.range)),
                Some((_, current_range)) => {
                    // If this span is smaller, use it (more specific)
                    if span.range.len() < current_range.len() {
                        smallest_expr = Some((*expr_id, span.range));
                    }
                }
            }
        }
    }

    smallest_expr.map(|(expr_id, _)| expr_id)
}

/// Convert a resolution to a navigation target.
fn resolution_to_navigation_target(
    db: &ProjectDatabase,
    resolution: &ResolvedValue,
    body: &ExprBody,
    file_id: FileId,
    function_loc: FunctionLoc,
) -> Option<NavigationTarget> {
    match resolution {
        ResolvedValue::Local { name, definition_site } => {
            // Navigate to the local variable's definition
            match definition_site {
                Some(DefinitionSite::Statement(stmt_id)) => {
                    // Get the span from the function body's statement spans
                    let span = body.get_stmt_span(*stmt_id)?;
                    let file_path = db.file_id_to_path(file_id)?.to_path_buf();
                    Some(NavigationTarget::new(
                        name.clone(),
                        file_path,
                        span,
                    ))
                }
                Some(DefinitionSite::Parameter(index)) => {
                    // Get the function signature to find the parameter span
                    let signature = baml_db::baml_compiler_hir::function_signature(db, function_loc);
                    let param = signature.params.get(*index)?;
                    let param_span = param.span?;

                    // Create a span using the file_id and text range
                    let span = Span::new(file_id, param_span);
                    let file_path = db.file_id_to_path(file_id)?.to_path_buf();

                    Some(NavigationTarget::new(
                        param.name.clone(),
                        file_path,
                        span,
                    ))
                }
                None => None,
            }
        }
        ResolvedValue::Function(fqn) => {
            // Look up the function in the symbol table
            lookup_symbol_definition(db, fqn)
        }
        ResolvedValue::Class(fqn) => {
            // Look up the class in the symbol table
            lookup_symbol_definition(db, fqn)
        }
        ResolvedValue::Enum(fqn) => {
            // Look up the enum in the symbol table
            lookup_symbol_definition(db, fqn)
        }
        ResolvedValue::TypeAlias(fqn) => {
            // Look up the type alias in the symbol table
            lookup_symbol_definition(db, fqn)
        }
        ResolvedValue::EnumVariant { enum_fqn, variant: _ } => {
            // TODO: Look up the specific enum variant
            // This requires the symbol table to track variant spans
            lookup_symbol_definition(db, enum_fqn)
        }
        ResolvedValue::Field { class_fqn, field: _ } => {
            // TODO: Look up the specific field in the class
            // This requires the symbol table to track field spans
            lookup_symbol_definition(db, class_fqn)
        }
        ResolvedValue::BuiltinFunction { path: _ } => {
            // Builtins don't have source definitions
            None
        }
        _ => None,
    }
}

/// Look up a symbol's definition in the symbol table.
fn lookup_symbol_definition(
    db: &ProjectDatabase,
    fqn: &FullyQualifiedName,
) -> Option<NavigationTarget> {
    // Get the symbol table
    let project = db.get_project()?;
    let symbol_table = baml_db::baml_compiler_hir::symbol_table(db, project);

    // Look up the symbol in both type and value namespaces
    let definition = symbol_table
        .lookup_type(db, fqn)
        .or_else(|| symbol_table.lookup_value(db, fqn))?;

    // Get the location from the definition
    use baml_db::baml_compiler_hir::Definition;
    let (file_id, span) = match definition {
        Definition::Class(loc) => {
            let file = loc.file(db);
            let file_id = file.file_id(db);
            // TODO: Get the actual span from the class definition
            let span = Span::new(file_id, TextRange::default());
            (file_id, span)
        }
        Definition::Enum(loc) => {
            let file = loc.file(db);
            let file_id = file.file_id(db);
            // TODO: Get the actual span from the enum definition
            let span = Span::new(file_id, TextRange::default());
            (file_id, span)
        }
        Definition::Function(loc) => {
            let file = loc.file(db);
            let file_id = file.file_id(db);
            // TODO: Get the actual span from the function definition
            let span = Span::new(file_id, TextRange::default());
            (file_id, span)
        }
        Definition::TypeAlias(loc) => {
            let file = loc.file(db);
            let file_id = file.file_id(db);
            // TODO: Get the actual span from the type alias definition
            let span = Span::new(file_id, TextRange::default());
            (file_id, span)
        }
        Definition::Client(loc) => {
            let file = loc.file(db);
            let file_id = file.file_id(db);
            // TODO: Get the actual span from the client definition
            let span = Span::new(file_id, TextRange::default());
            (file_id, span)
        }
        Definition::Generator(loc) => {
            let file = loc.file(db);
            let file_id = file.file_id(db);
            // TODO: Get the actual span from the generator definition
            let span = Span::new(file_id, TextRange::default());
            (file_id, span)
        }
        Definition::Test(loc) => {
            let file = loc.file(db);
            let file_id = file.file_id(db);
            // TODO: Get the actual span from the test definition
            let span = Span::new(file_id, TextRange::default());
            (file_id, span)
        }
    };

    // Get the file path from the file ID
    let file_path = db.file_id_to_path(file_id)?;

    Some(NavigationTarget::new(
        fqn.name.to_string(),
        file_path.clone(),
        span,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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

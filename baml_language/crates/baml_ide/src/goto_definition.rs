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
    tracing::debug!("=== baml_ide::goto_definition called ===");
    tracing::debug!("  Position: {:?}", position);

    // Get the source file
    let source_files = db.get_source_files();
    let source_file = source_files.iter().find(|f| f.file_id(db) == file_id)?;
    let text = source_file.text(db);
    tracing::debug!("  Found source file, text length: {}", text.len());

    // Find the word at the cursor position
    let word_range = find_word_at_offset(&text, position)?;
    let word = &text[word_range.start().into()..word_range.end().into()];
    tracing::debug!("  Word at position: '{}' (range: {:?})", word, word_range);

    // Get the function containing this position
    tracing::debug!("  Looking for function at position...");
    let function_loc = find_function_at_position(db, file_id, position)?;
    tracing::debug!("  Found function containing position");

    // Get the function body
    let body = baml_db::baml_compiler_hir::function_body(db, function_loc);
    tracing::debug!("  Got function body");

    // Find the expression at this position
    let expr_body = match &*body {
        baml_db::baml_compiler_hir::FunctionBody::Expr(expr_body) => {
            tracing::debug!("  Function body is Expr type");
            expr_body
        },
        other => {
            tracing::debug!("  Function body is not Expr type: {:?}", other);
            return None; // Can't find expressions in missing or error bodies
        }
    };

    tracing::debug!("  Looking for expression at position...");
    let expr_id = find_expr_at_position(expr_body, position)?;
    tracing::debug!("  Found expression at position: {:?}", expr_id);

    // Debug: let's see what this expression actually is
    let expr = &expr_body.exprs[expr_id];
    tracing::debug!("  Expression content: {:?}", expr);

    // Get the type inference results for the function
    tracing::debug!("  Getting type inference results...");
    let inference_result = get_function_inference(db, function_loc)?;
    tracing::debug!("  Got inference results, {} resolutions", inference_result.expr_resolutions.len());

    // Look up the resolution for this expression
    let resolution = inference_result.expr_resolutions.get(&expr_id)?;
    tracing::debug!("  Found resolution for expression: {:?}", resolution);

    // Convert the resolution to a navigation target
    tracing::debug!("  Converting resolution to navigation target...");
    resolution_to_navigation_target(db, resolution, expr_body, file_id, function_loc)
}

/// Find the function containing the given position.
fn find_function_at_position(
    db: &ProjectDatabase,
    file_id: FileId,
    position: TextSize,
) -> Option<FunctionLoc> {
    tracing::debug!("    find_function_at_position: position {:?}", position);

    // Get the source file
    let source_files = db.get_source_files();
    let source_file = source_files.iter().find(|f| f.file_id(db) == file_id)?;
    tracing::debug!("    Got source file");

    // Get all items in the file
    let file_items = baml_db::baml_compiler_hir::file_items(db, *source_file);
    tracing::debug!("    Got {} items in file", file_items.items(db).len());

    // Get the syntax tree to check text ranges
    let tree = baml_db::baml_compiler_parser::syntax_tree(db, *source_file);
    use rowan::ast::AstNode;
    let ast_file = baml_db::baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

    // Iterate through items to find functions
    let mut function_count = 0;
    for item_id in file_items.items(db) {
        if let baml_db::baml_compiler_hir::ItemId::Function(func_loc) = item_id {
            function_count += 1;
            // Get the function from the item tree
            let item_tree = baml_db::baml_compiler_hir::file_item_tree(db, *source_file);
            let func = &item_tree[func_loc.id(db)];
            let func_name = &func.name;
            tracing::debug!("    Checking function #{}: {}", function_count, func_name);

            // Find the function node in the AST
            for item in ast_file.items() {
                match item {
                    baml_db::baml_compiler_syntax::ast::Item::Function(func_node) => {
                        if let Some(name) = func_node.name() {
                            if name.text() == func_name {
                                // Check if position is within this function's range
                                let range = func_node.syntax().text_range();
                                tracing::debug!("      Function {} range: {:?}, contains position {:?}: {}",
                                    func_name, range, position, range.contains(position));
                                if range.contains(position) {
                                    tracing::debug!("      Found matching function!");
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

    tracing::debug!("    find_expr_at_position: looking for position {:?}", position);
    tracing::debug!("      Total expressions in body: {}", body.expr_spans.len());

    // First, log ALL expressions and their ranges for debugging
    tracing::debug!("      All expressions in body:");
    tracing::debug!("      Total expressions in arena: {}", body.exprs.len());
    tracing::debug!("      Total expressions with spans: {}", body.expr_spans.len());

    // Check if there are expressions without spans
    for (idx, expr) in body.exprs.iter() {
        if !body.expr_spans.contains_key(&idx) {
            tracing::debug!("        WARNING: Expression {:?} has no span! Content: {:?}", idx, expr);
        }
    }

    let mut expr_list: Vec<_> = body.expr_spans.iter().collect();
    expr_list.sort_by_key(|(_, span)| span.range.start());
    for (expr_id, span) in &expr_list {
        let expr = &body.exprs[**expr_id];
        tracing::debug!("        {:?}: range {:?} (len {:?}) - {:?}", expr_id, span.range, span.range.len(), expr);
    }

    // Find ALL expressions that contain this position, then select the smallest
    let mut candidates: Vec<(ExprId, text_size::TextRange)> = Vec::new();
    for (expr_id, span) in &body.expr_spans {
        if span.range.contains(position) {
            candidates.push((*expr_id, span.range));
        }
    }

    // Sort by range length (smallest first)
    candidates.sort_by_key(|(_, range)| range.len());

    // Log the candidates for debugging
    if !candidates.is_empty() {
        tracing::debug!("      Found {} expressions containing position:", candidates.len());
        for (expr_id, range) in &candidates {
            let expr = &body.exprs[*expr_id];
            tracing::debug!("        {:?}: range {:?} (len {:?}) - {:?}", expr_id, range, range.len(), expr);
        }
        tracing::debug!("      Selecting smallest: {:?}", candidates[0].0);
    }

    smallest_expr = candidates.first().copied();

    match smallest_expr {
        Some((expr_id, range)) => {
            tracing::debug!("      Selected smallest expression: {:?} with range {:?}", expr_id, range);
            Some(expr_id)
        }
        None => {
            tracing::debug!("      No expression found at position");
            None
        }
    }
}

/// Convert a resolution to a navigation target.
fn resolution_to_navigation_target(
    db: &ProjectDatabase,
    resolution: &ResolvedValue,
    body: &ExprBody,
    file_id: FileId,
    function_loc: FunctionLoc,
) -> Option<NavigationTarget> {
    tracing::debug!("    resolution_to_navigation_target");
    tracing::debug!("      Resolution type: {:?}", resolution);

    match resolution {
        ResolvedValue::Local { name, definition_site } => {
            tracing::debug!("      Local variable: {}", name);
            // Navigate to the local variable's definition
            match definition_site {
                Some(DefinitionSite::Statement(stmt_id)) => {
                    tracing::debug!("        Definition site: Statement {:?}", stmt_id);
                    // Get the span from the function body's statement spans
                    let span = body.get_stmt_span(*stmt_id)?;
                    tracing::debug!("        Got statement span: {:?}", span);
                    let file_path = db.file_id_to_path(file_id)?.to_path_buf();
                    tracing::debug!("        Target file path: {:?}", file_path);
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
            tracing::debug!("      Function reference: {}", fqn.name);
            // Look up the function in the symbol table
            lookup_symbol_definition(db, fqn)
        }
        ResolvedValue::Class(fqn) => {
            tracing::debug!("      Class reference: {}", fqn.name);
            // Look up the class in the symbol table
            lookup_symbol_definition(db, fqn)
        }
        ResolvedValue::Enum(fqn) => {
            tracing::debug!("      Enum reference: {}", fqn.name);
            // Look up the enum in the symbol table
            lookup_symbol_definition(db, fqn)
        }
        ResolvedValue::TypeAlias(fqn) => {
            tracing::debug!("      Type alias reference: {}", fqn.name);
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
    tracing::debug!("      lookup_symbol_definition: {}", fqn.name);

    // Get the symbol table
    let project = db.get_project()?;
    let symbol_table = baml_db::baml_compiler_hir::symbol_table(db, project);
    tracing::debug!("        Got symbol table");

    // Look up the symbol in both type and value namespaces
    let definition = symbol_table
        .lookup_type(db, fqn)
        .or_else(|| symbol_table.lookup_value(db, fqn));

    let definition = match definition {
        Some(def) => {
            tracing::debug!("        Found definition in symbol table");
            def
        }
        None => {
            tracing::debug!("        Symbol not found in symbol table");
            return None;
        }
    };

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
    use baml_project::ProjectDatabase;

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
        let position = TextSize::from(r_pos as u32);

        // Try to go to definition
        let result = goto_definition(&db, file_id, position);

        // Should find the parameter 'r' definition
        assert!(result.is_some(), "Should find definition for 'r' in match scrutinee");

        if let Some(nav_target) = result {
            assert_eq!(nav_target.name, "r", "Expected to find parameter 'r' but found '{}'", nav_target.name);
            // The parameter span should contain "r SentimentResponse"
            // The exact span range depends on how the parser handles it
            assert!(nav_target.span.range.start() < TextSize::from(100),
                "Parameter should be in the function signature");
            assert!(nav_target.span.range.end() > TextSize::from(93),
                "Parameter span should include the parameter name");
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

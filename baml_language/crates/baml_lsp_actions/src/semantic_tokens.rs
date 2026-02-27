//! Semantic tokens for BAML files.

use baml_db::{
    Name, QualifiedName, SourceFile, baml_compiler_hir, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxNodeExt, SyntaxToken},
    baml_compiler_tir::{self, DefinitionSite, ResolvedValue},
};
use baml_project::ProjectDatabase;
use rowan::NodeOrToken;
use text_size::TextRange;

/// The semantic token type for a BAML file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenType {
    Namespace,
    Type,
    Class,
    Enum,
    Interface,
    Struct,
    TypeParameter,
    Parameter,
    Variable,
    Property,
    EnumMember,
    Event,
    Function,
    Method,
    Macro,
    Keyword,
    Modifier,
    Comment,
    String,
    Number,
    Regexp,
    Operator,
    Decorator,
}

/// Token type legend order
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::Namespace,
    SemanticTokenType::Type,
    SemanticTokenType::Class,
    SemanticTokenType::Enum,
    SemanticTokenType::Interface,
    SemanticTokenType::Struct,
    SemanticTokenType::TypeParameter,
    SemanticTokenType::Parameter,
    SemanticTokenType::Variable,
    SemanticTokenType::Property,
    SemanticTokenType::EnumMember,
    SemanticTokenType::Event,
    SemanticTokenType::Function,
    SemanticTokenType::Method,
    SemanticTokenType::Macro,
    SemanticTokenType::Keyword,
    SemanticTokenType::Modifier,
    SemanticTokenType::Comment,
    SemanticTokenType::String,
    SemanticTokenType::Number,
    SemanticTokenType::Regexp,
    SemanticTokenType::Operator,
    SemanticTokenType::Decorator,
];

impl SemanticTokenType {
    /// Get the index of the token type in the legend.
    #[allow(clippy::cast_possible_truncation)]
    pub fn legend_index(self) -> u32 {
        TOKEN_TYPES.iter().position(|t| *t == self).unwrap_or(0) as u32
    }

    /// Get the string representation of the token type.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Type => "type",
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Struct => "struct",
            Self::TypeParameter => "typeParameter",
            Self::Parameter => "parameter",
            Self::Variable => "variable",
            Self::Property => "property",
            Self::EnumMember => "enumMember",
            Self::Event => "event",
            Self::Function => "function",
            Self::Method => "method",
            Self::Macro => "macro",
            Self::Keyword => "keyword",
            Self::Modifier => "modifier",
            Self::Comment => "comment",
            Self::String => "string",
            Self::Number => "number",
            Self::Regexp => "regexp",
            Self::Operator => "operator",
            Self::Decorator => "decorator",
        }
    }
}

/// A semantic token for a BAML file.
#[derive(Debug, Clone)]
pub struct SemanticToken {
    pub range: TextRange,
    pub token_type: SemanticTokenType,
}

/// Emit a semantic token for a single leaf token.
fn emit_token(token: &SyntaxToken, token_type: SemanticTokenType, out: &mut Vec<SemanticToken>) {
    if !token.kind().is_whitespace() {
        out.push(SemanticToken {
            range: token.text_range(),
            token_type,
        });
    }
}

/// Emit a token type for all non-trivia leaf tokens under a node.
fn emit_node(node: &SyntaxNode, token_type: SemanticTokenType, out: &mut Vec<SemanticToken>) {
    for child in node.descendants_with_tokens() {
        if let NodeOrToken::Token(t) = child {
            emit_token(&t, token_type, out);
        }
    }
}

/// Emit semantic tokens for a single file.
pub fn semantic_tokens(db: &ProjectDatabase, file: SourceFile) -> Vec<SemanticToken> {
    let root = baml_compiler_parser::syntax_tree(db, file);
    let mut out = Vec::new();
    visit_node(db, file, &root, &mut out);
    out
}

/// Dispatch a single node to its visitor.
fn visit_node(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    match node.kind() {
        ref n if n.is_comment() => emit_node(node, SemanticTokenType::Comment, out), // Handle header comments which are actually nodes
        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
            emit_node(node, SemanticTokenType::String, out);
        }
        SyntaxKind::ATTRIBUTE => visit_attribute(db, file, node, out),
        SyntaxKind::BLOCK_ATTRIBUTE => visit_attribute(db, file, node, out),
        SyntaxKind::TYPE_ALIAS_DEF => visit_type_alias_def(db, file, node, out),
        SyntaxKind::ENUM_DEF => visit_word_as(db, file, node, SemanticTokenType::Enum, out),
        SyntaxKind::ENUM_VARIANT => {
            visit_word_as(db, file, node, SemanticTokenType::EnumMember, out);
        }
        SyntaxKind::CLASS_DEF => visit_word_as(db, file, node, SemanticTokenType::Class, out),
        SyntaxKind::FIELD => visit_word_as(db, file, node, SemanticTokenType::Property, out),
        SyntaxKind::FUNCTION_DEF => visit_function_def(db, file, node, out),
        SyntaxKind::PARAMETER => visit_word_as(db, file, node, SemanticTokenType::Parameter, out),
        SyntaxKind::TYPE_EXPR => visit_type_expr(db, file, node, out),
        SyntaxKind::LET_STMT => {
            visit_first_word_as(db, file, node, SemanticTokenType::Variable, out);
        }
        SyntaxKind::OBJECT_LITERAL => visit_object_literal(db, file, node, out),
        SyntaxKind::OBJECT_FIELD => {
            visit_first_word_as(db, file, node, SemanticTokenType::Property, out);
        }
        SyntaxKind::CLIENT_TYPE => visit_word_as(db, file, node, SemanticTokenType::Type, out),
        SyntaxKind::CONFIG_ITEM => visit_word_as(db, file, node, SemanticTokenType::Property, out),
        SyntaxKind::CLIENT_DEF | SyntaxKind::GENERATOR_DEF | SyntaxKind::RETRY_POLICY_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Struct, out);
        } // Put these as struct so they're in theory different from classes
        SyntaxKind::TEST_DEF => visit_word_as(db, file, node, SemanticTokenType::Struct, out), // TODO: semantic tokens for test def functions
        SyntaxKind::TEMPLATE_STRING_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Function, out); // Sorta like a function?
        }
        SyntaxKind::PROMPT_FIELD => visit_word_as(db, file, node, SemanticTokenType::Property, out),
        SyntaxKind::CLIENT_FIELD => visit_client_field(db, file, node, out),
        _ => visit_children(db, file, node, out),
    }
}

/// Classify a leaf token into a semantic token type.
fn visit_token(token: &SyntaxToken, out: &mut Vec<SemanticToken>) {
    let kind = token.kind();
    if kind.is_whitespace() {
        return;
    }
    let token_type = if kind.is_keyword() {
        SemanticTokenType::Keyword
    } else if kind.is_operator() {
        SemanticTokenType::Operator
    } else if kind.is_comment() {
        SemanticTokenType::Comment
    } else if matches!(
        kind,
        SyntaxKind::INTEGER_LITERAL | SyntaxKind::FLOAT_LITERAL
    ) {
        SemanticTokenType::Number
    } else {
        return;
    };
    out.push(SemanticToken {
        range: token.text_range(),
        token_type,
    });
}

/// Walk all children of a node, dispatching child nodes via `visit_node`
/// and classifying leaf tokens via `visit_token`.
fn visit_children(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => visit_token(&t, out),
        }
    }
}

/// Visit a node where all WORD tokens should be classified as `word_type`.
fn visit_word_as(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    word_type: SemanticTokenType,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, word_type, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a node where the first WORD token should be classified as `word_type`.
fn visit_first_word_as(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    word_type: SemanticTokenType,
    out: &mut Vec<SemanticToken>,
) {
    let mut found_word = false;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => {
                if !found_word && t.kind() == SyntaxKind::WORD {
                    found_word = true;
                    emit_token(&t, word_type, out);
                } else {
                    visit_token(&t, out);
                }
            }
        }
    }
}

/// Visit a `CLIENT_FIELD` node, classifying the client name as a property.
fn visit_client_field(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::KW_CLIENT => emit_token(&t, SemanticTokenType::Property, out), // Make this match all the other fields
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit an attribute or a block attribute node.
fn visit_attribute(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::AT_AT | SyntaxKind::AT | SyntaxKind::WORD => {
                    emit_token(&t, SemanticTokenType::Decorator, out);
                }
                ref k if k.is_keyword() => emit_token(&t, SemanticTokenType::Decorator, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a `TYPE_ALIAS_DEF` node, classifying "type" as a keyword, and the type name as a type.
fn visit_type_alias_def(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    let mut found_keyword = false; // "type" is not actually a keyword, it's a WORD token.
    let mut found_name = false;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => {
                if !found_keyword && t.kind() == SyntaxKind::WORD {
                    found_keyword = true;
                    emit_token(&t, SemanticTokenType::Keyword, out);
                } else if !found_name && t.kind() == SyntaxKind::WORD {
                    found_name = true;
                    emit_token(&t, SemanticTokenType::Type, out);
                } else {
                    visit_token(&t, out);
                }
            }
        }
    }
}

/// Resolve the semantic token type for a type name by looking it up in the symbol table.
fn resolve_type_name(db: &ProjectDatabase, name: &str) -> SemanticTokenType {
    if matches!(
        name,
        "int" | "float" | "string" | "bool" | "map" | "unknown"
    ) {
        return SemanticTokenType::Type;
    }

    let Some(project) = db.project() else {
        return SemanticTokenType::Namespace;
    };
    let symbol_table = baml_compiler_hir::symbol_table(db, project);
    let fqn = QualifiedName::local(Name::new(name));
    match symbol_table.lookup_type(db, &fqn) {
        Some(baml_compiler_hir::Definition::Class(_)) => SemanticTokenType::Class,
        Some(baml_compiler_hir::Definition::Enum(_)) => SemanticTokenType::Enum,
        Some(baml_compiler_hir::Definition::TypeAlias(_)) => SemanticTokenType::Type,
        _ => SemanticTokenType::Namespace,
    }
}

/// Visit a `TYPE_EXPR` node, resolving type names to their definitions.
fn visit_type_expr(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, resolve_type_name(db, t.text()), out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit an `OBJECT_LITERAL` node, resolving type names to their definitions.
fn visit_object_literal(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, resolve_type_name(db, t.text()), out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a `FUNCTION_DEF` node. Walk the header (name, params, return type) via CST,
/// then for the expression body, switch to HIR/TIR-based resolution.
fn visit_function_def(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    // Walk children: handle the header via CST, but intercept EXPR_FUNCTION_BODY
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => {
                if n.kind() == SyntaxKind::EXPR_FUNCTION_BODY {
                    if let Some(func_loc) = find_function_loc(db, file, node) {
                        emit_expr_body_tokens(db, file, func_loc, &n, out);
                    } else {
                        visit_children(db, file, &n, out);
                    }
                } else {
                    visit_node(db, file, &n, out);
                }
            }
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => emit_token(&t, SemanticTokenType::Function, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Find the `FunctionLoc` for a `FUNCTION_DEF` CST node by matching its name
/// against the `ItemTree`. Handles both top-level functions and class methods.
fn find_function_loc<'db>(
    db: &'db ProjectDatabase,
    file: SourceFile,
    func_def_node: &SyntaxNode,
) -> Option<baml_compiler_hir::FunctionId<'db>> {
    // Get the function name from the CST node.
    let func_name_token = func_def_node.first_child_token_of_kind(SyntaxKind::WORD)?;
    let func_name = func_name_token.text();

    // Check if this is a method inside a CLASS_DEF by walking up.
    let qualified_name = if let Some(parent) = func_def_node.parent() {
        if parent.kind() == SyntaxKind::CLASS_DEF {
            // Find the class name (first WORD in the CLASS_DEF).
            let class_name_token = parent.first_child_token_of_kind(SyntaxKind::WORD)?;
            let class_name = class_name_token.text();
            QualifiedName::local_method_from_str(class_name, func_name).to_string()
        } else {
            func_name.to_string()
        }
    } else {
        func_name.to_string()
    };

    let file_items = baml_compiler_hir::file_items(db, file);
    let item_tree = baml_compiler_hir::file_item_tree(db, file);
    for item in file_items.items(db) {
        if let baml_compiler_hir::ItemId::Function(loc) = item {
            let func = &item_tree[loc.id(db)];
            if func.name.as_str() == qualified_name {
                return Some(*loc);
            }
        }
    }
    None
}

/// Map a `ResolvedValue` to a semantic token type.
fn resolved_value_to_token_type(resolved: &ResolvedValue) -> SemanticTokenType {
    match resolved {
        ResolvedValue::Local {
            definition_site, ..
        } => match definition_site {
            Some(DefinitionSite::Statement(_)) => SemanticTokenType::Variable,
            Some(DefinitionSite::Parameter(_)) => SemanticTokenType::Parameter,
            None => SemanticTokenType::Variable,
        },
        ResolvedValue::Function(_) => SemanticTokenType::Function,
        ResolvedValue::BuiltinFunction(_) => SemanticTokenType::Function,
        ResolvedValue::Class(_) => SemanticTokenType::Class,
        ResolvedValue::Enum(_) => SemanticTokenType::Enum,
        ResolvedValue::TypeAlias(_) => SemanticTokenType::Type,
        ResolvedValue::EnumVariant { .. } => SemanticTokenType::EnumMember,
        ResolvedValue::Field { .. } => SemanticTokenType::Property,
        ResolvedValue::ModuleItem { .. } => SemanticTokenType::Variable,
        ResolvedValue::TypeMethod { .. } => SemanticTokenType::Method,
        ResolvedValue::Unknown => SemanticTokenType::Variable,
    }
}

/// Emit semantic tokens for an expression function body using HIR/TIR resolution.
///
/// Instead of walking the CST blindly, we query the HIR `ExprBody` and TIR `InferenceResult`
/// to get resolution info for every expression, then emit tokens using the source map spans.
/// We still fall back to the CST walk for keywords, operators, comments, and literals
/// that don't have corresponding HIR expressions.
fn emit_expr_body_tokens(
    db: &ProjectDatabase,
    file: SourceFile,
    func_loc: baml_compiler_hir::FunctionId<'_>,
    body_node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    // Get the HIR body and TIR inference result
    let body = baml_compiler_hir::function_body(db, func_loc);
    let baml_compiler_hir::FunctionBody::Expr(expr_body, source_map) = &*body else {
        // If we can't get the HIR body, just visit the children normally.
        visit_children(db, file, body_node, out);
        return;
    };

    let inference = baml_compiler_tir::function_type_inference(db, func_loc);

    // Build a set of ranges covered by HIR-resolved expressions.
    let mut resolved_ranges: Vec<TextRange> = Vec::new();
    let mut body_tokens: Vec<SemanticToken> = Vec::new();
    let file_text = file.text(db);
    for (expr_id, expr) in expr_body.exprs.iter() {
        let Some(span) = source_map.expr_span(expr_id) else {
            continue;
        };

        match expr {
            // All references to functions, variables, etc. are path expressions.
            baml_compiler_hir::Expr::Path(segments) => {
                let seg_resolutions = inference.path_segment_resolutions.get(&expr_id);
                let whole_resolution = inference.expr_resolutions.get(&expr_id);
                // emit_path_segment_tokens returns the ranges it actually emitted.
                // For compiler-generated synthetic paths (e.g. for-in desugaring),
                // the segment names won't appear in the source text, so nothing
                // is emitted and the CST fallback isn't blocked.
                let emitted = emit_path_segment_tokens(
                    segments,
                    seg_resolutions,
                    whole_resolution,
                    span.range,
                    file_text,
                    &mut body_tokens,
                );
                resolved_ranges.extend(emitted);
            }
            baml_compiler_hir::Expr::FieldAccess { field, .. } => {
                // The field name is at the end of the FieldAccess span (e.g. "obj.field").
                // Use rfind to avoid matching a substring in the base expression.
                let span_start: usize = span.range.start().into();
                let span_end: usize = span.range.end().into();
                let span_text = &file_text[span_start..span_end];
                let field_str = field.as_str();
                if let Some(offset) = span_text.rfind(field_str) {
                    let field_start = span_start + offset;
                    let field_end = field_start + field_str.len();
                    let range = TextRange::new(
                        field_start.try_into().unwrap(),
                        field_end.try_into().unwrap(),
                    );

                    if let Some(resolved) = inference.expr_resolutions.get(&expr_id) {
                        let token_type = resolved_value_to_token_type(resolved);
                        body_tokens.push(SemanticToken { range, token_type });
                        resolved_ranges.push(range);
                    }
                }
            }
            _ => {}
        }
    }

    // Fallback: run the normal CST visitor for everything not covered by HIR.
    let mut fallback_tokens: Vec<SemanticToken> = Vec::new();
    visit_children(db, file, body_node, &mut fallback_tokens);
    for tok in fallback_tokens {
        // Skip tokens that overlap with HIR-resolved ranges.
        if resolved_ranges.iter().any(|r| r.contains_range(tok.range)) {
            continue;
        }
        body_tokens.push(tok);
    }

    // Sort by position so delta-encoding in the LSP layer doesn't overflow.
    body_tokens.sort_by_key(|t| t.range.start());
    out.extend(body_tokens);
}

/// Emit one semantic token per segment of a path expression (e.g. `Status.Active`).
///
/// Returns the ranges of tokens that were actually emitted, so the caller can
/// add them to `resolved_ranges` to prevent CST fallback from re-emitting them.
///
/// `seg_resolutions` comes from `InferenceResult::path_segment_resolutions` and has
/// one entry per segment for multi-segment paths. For single-segment paths it may be
/// absent, in which case we fall back to `whole_resolution`.
fn emit_path_segment_tokens(
    segments: &[Name],
    seg_resolutions: Option<&Vec<ResolvedValue>>,
    whole_resolution: Option<&ResolvedValue>,
    path_range: TextRange,
    file_text: &str,
    out: &mut Vec<SemanticToken>,
) -> Vec<TextRange> {
    let path_start: usize = path_range.start().into();
    let path_end: usize = path_range.end().into();
    let path_text = &file_text[path_start..path_end];

    let mut emitted_ranges = Vec::new();

    // Walk through the path text finding each segment's position.
    let mut cursor = 0usize;
    for (i, seg_name) in segments.iter().enumerate() {
        let name_str = seg_name.as_str();
        // Find this segment name in the remaining path text.
        let Some(offset_in_path) = path_text[cursor..].find(name_str) else {
            continue;
        };
        let seg_start = path_start + cursor + offset_in_path;
        let seg_end = seg_start + name_str.len();
        let range = TextRange::new(seg_start.try_into().unwrap(), seg_end.try_into().unwrap());
        cursor += offset_in_path + name_str.len();

        // Resolve: prefer per-segment, then whole-expression, then default.
        let token_type = if let Some(resolutions) = seg_resolutions {
            resolutions
                .get(i)
                .map(resolved_value_to_token_type)
                .unwrap_or(SemanticTokenType::Variable)
        } else if let Some(resolved) = whole_resolution {
            // Until we have namespaces, we just mark it as the whole resolution, TODO: fix this when we have namespaces
            resolved_value_to_token_type(resolved)
        } else {
            continue;
        };

        out.push(SemanticToken { range, token_type });
        emitted_ranges.push(range);
    }

    emitted_ranges
}

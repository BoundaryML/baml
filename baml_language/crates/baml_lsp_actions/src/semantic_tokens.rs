//! Semantic tokens for BAML files.

use std::collections::HashMap;

use baml_db::{
    Name, QualifiedName, SourceFile, Span, baml_compiler_hir, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
    baml_compiler_tir::{self, DefinitionSite, ResolvedValue},
};
use baml_project::ProjectDatabase;
use rowan::NodeOrToken;
use text_size::TextRange;

use crate::utils::find_function_at_position;

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
        TOKEN_TYPES
            .iter()
            .position(|t| *t == self)
            .expect("SemanticTokenType missing in legend") as u32 // This should never happen if you made the legend correctly
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

/// Emit semantic tokens for a single file. Always returns semantic tokens in document order.
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
        // String literals are nodes, emit the whole thing with all its children as string.
        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
            emit_node(node, SemanticTokenType::String, out);
        }
        SyntaxKind::ATTRIBUTE | SyntaxKind::BLOCK_ATTRIBUTE => visit_attribute(db, file, node, out),
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
        // Highlight top-level let statements for now...
        SyntaxKind::LET_STMT => {
            visit_first_word_as(db, file, node, SemanticTokenType::Variable, out);
        }
        SyntaxKind::CLIENT_TYPE => visit_word_as(db, file, node, SemanticTokenType::Type, out),
        SyntaxKind::CONFIG_ITEM => visit_config_item(db, file, node, out),
        // Put these as struct so they're in theory different from classes
        SyntaxKind::CLIENT_DEF | SyntaxKind::GENERATOR_DEF | SyntaxKind::RETRY_POLICY_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Struct, out);
        }
        // TODO: semantic tokens for test def functions
        SyntaxKind::TEST_DEF => visit_word_as(db, file, node, SemanticTokenType::Struct, out),
        // I guess this is sorta like a function?
        SyntaxKind::TEMPLATE_STRING_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Function, out);
        }
        SyntaxKind::PROMPT_FIELD => visit_word_as(db, file, node, SemanticTokenType::Property, out),
        SyntaxKind::CLIENT_FIELD => visit_client_field(db, file, node, out),
        _ => visit_children(db, file, node, out),
    }
}

/// Classify a leaf token into a semantic token type.
fn visit_token(token: &SyntaxToken, out: &mut Vec<SemanticToken>) {
    out.push(SemanticToken {
        range: token.text_range(),
        token_type: match token.kind() {
            ref kind if kind.is_whitespace() => return,
            ref kind if kind.is_keyword() => SemanticTokenType::Keyword,
            ref kind if kind.is_operator() => SemanticTokenType::Operator,
            ref kind if kind.is_comment() => SemanticTokenType::Comment,
            SyntaxKind::INTEGER_LITERAL | SyntaxKind::FLOAT_LITERAL => SemanticTokenType::Number,
            _ => return,
        },
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

/// Visit a `CONFIG_ITEM` node, classifying the key as a property.
fn visit_config_item(
    db: &ProjectDatabase,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                // Handle keywords like `retry_policy` as properties.
                ref k if k.is_keyword() => emit_token(&t, SemanticTokenType::Property, out),
                SyntaxKind::WORD => emit_token(&t, SemanticTokenType::Property, out),
                _ => visit_token(&t, out),
            },
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
        "int" | "float" | "string" | "bool" | "map" | "unknown" | "never"
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
                // Capture the type name.
                // This will need adjustment when namespaces are brought into the mix.
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
    // Walk children: handle the signature via CST, but intercept EXPR_FUNCTION_BODY
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => {
                if n.kind() == SyntaxKind::EXPR_FUNCTION_BODY {
                    if let Some(func_loc) =
                        find_function_at_position(db, file, node.text_range().start())
                    {
                        if let Some(visitor) = ExprBodyVisitor::new(db, file, func_loc) {
                            visitor.visit_children(&n, out);
                        }
                    }
                } else {
                    visit_node(db, file, &n, out);
                }
            }
            NodeOrToken::Token(t) => match t.kind() {
                // Capture the function name.
                SyntaxKind::WORD => emit_token(&t, SemanticTokenType::Function, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Map a `ResolvedValue` to a semantic token type.
/// Returns `None` for unknown/unresolved values so they don't get highlighted.
fn resolved_value_to_token_type(resolved: &ResolvedValue) -> Option<SemanticTokenType> {
    match resolved {
        ResolvedValue::Local {
            definition_site, ..
        } => Some(match definition_site {
            Some(DefinitionSite::Statement(_)) => SemanticTokenType::Variable,
            Some(DefinitionSite::Parameter(_)) => SemanticTokenType::Parameter,
            Some(DefinitionSite::Pattern(_)) => SemanticTokenType::Variable,
            None => return None,
        }),
        ResolvedValue::Function(_) => Some(SemanticTokenType::Function),
        ResolvedValue::BuiltinFunction(_) => Some(SemanticTokenType::Function),
        ResolvedValue::Class(_) => Some(SemanticTokenType::Class),
        ResolvedValue::Enum(_) => Some(SemanticTokenType::Enum),
        ResolvedValue::TypeAlias(_) => Some(SemanticTokenType::Type),
        ResolvedValue::EnumVariant { .. } => Some(SemanticTokenType::EnumMember),
        ResolvedValue::Field { .. } => Some(SemanticTokenType::Property),
        ResolvedValue::ModuleItem { .. } => Some(SemanticTokenType::Variable),
        ResolvedValue::TypeMethod { .. } => Some(SemanticTokenType::Method),
        ResolvedValue::Unknown => None,
    }
}

/// Build a lookup map from `TextRange` to `SemanticTokenType` using HIR/TIR resolution.
///
/// This pre-computes the semantic classification for every token that has HIR resolution,
/// keyed by the exact source range. The map is then consulted during a single CST walk
/// so tokens are emitted in document order without needing a sort.
/// Build a `TextRange` covering `name_len` bytes starting at `base + offset`.
fn text_range_at(base: usize, offset: usize, name_len: usize) -> TextRange {
    let start = base + offset;
    TextRange::new(
        start.try_into().unwrap(),
        (start + name_len).try_into().unwrap(),
    )
}

/// Extract the text slice for a span, returning `(start, slice)`.
/// Returns `None` if the span extends past the end of `file_text`.
fn span_text<'a>(span: &Span, file_text: &'a str) -> Option<(usize, &'a str)> {
    let start: usize = span.range.start().into();
    let end: usize = span.range.end().into();
    if end > file_text.len() {
        return None;
    }
    Some((start, &file_text[start..end]))
}

fn build_resolution_map(
    db: &ProjectDatabase,
    expr_body: &baml_compiler_hir::ExprBody,
    source_map: &baml_compiler_hir::HirSourceMap,
    inference: &baml_compiler_tir::InferenceResult,
    file_text: &str,
) -> HashMap<TextRange, SemanticTokenType> {
    let mut map = HashMap::new();

    // Class field map for validating object field names.
    let class_fields = db
        .project()
        .map(|p| baml_compiler_tir::class_field_types(db, p));

    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            baml_compiler_hir::Expr::Path(_) => {
                let Some(seg_spans) = source_map.path_segment_spans(expr_id) else {
                    continue;
                };
                let seg_resolutions = inference.path_segment_resolutions.get(&expr_id);
                let seg_types = inference.path_segment_types.get(&expr_id);
                let whole_resolution = inference.expr_resolutions.get(&expr_id);

                for (i, range) in seg_spans.iter().enumerate() {
                    // Skip segments whose inferred type is Unknown/Error (e.g. non-existent fields).
                    if let Some(types) = seg_types {
                        if let Some(ty) = types.get(i) {
                            if ty.is_unknown() || ty.is_error() {
                                continue;
                            }
                        }
                    }

                    let token_type = if let Some(resolutions) = seg_resolutions {
                        resolutions.get(i).and_then(resolved_value_to_token_type)
                    } else if let Some(resolved) = whole_resolution {
                        resolved_value_to_token_type(resolved)
                    } else {
                        continue;
                    };

                    if let Some(token_type) = token_type {
                        map.insert(*range, token_type);
                    }
                }
            }
            baml_compiler_hir::Expr::FieldAccess { .. } => {
                if let Some(range) = source_map.field_access_field_span(expr_id) {
                    if let Some(token_type) = inference
                        .expr_resolutions
                        .get(&expr_id)
                        .and_then(resolved_value_to_token_type)
                    {
                        map.insert(range, token_type);
                    }
                }
            }
            baml_compiler_hir::Expr::Object {
                type_name, fields, ..
            } => {
                // Highlight the type name (e.g. `Point` in `Point { x: 1, y: 2 }`)
                // Type name still uses text scanning since it's not stored per-segment.
                if let Some(name) = type_name {
                    let Some(span) = source_map.expr_span(expr_id) else {
                        continue;
                    };
                    let Some((span_start, text)) = span_text(&span, file_text) else {
                        continue;
                    };
                    let name_str = name.as_str();
                    if let Some(offset) = text.find(name_str) {
                        let range = text_range_at(span_start, offset, name_str.len());
                        let token_type = inference
                            .expr_resolutions
                            .get(&expr_id)
                            .and_then(resolved_value_to_token_type)
                            .unwrap_or(SemanticTokenType::Class);
                        map.insert(range, token_type);
                    }
                }

                // Highlight field names as properties, but only if the field
                // actually exists on the resolved class.
                let known_fields = class_fields.as_ref().and_then(|cft| {
                    let classes = cft.classes(db);
                    match inference.expr_resolutions.get(&expr_id)? {
                        ResolvedValue::Class(qn) => classes.get(&qn.display_name()).cloned(),
                        _ => None,
                    }
                });
                if let Some(field_spans) = source_map.object_field_name_spans(expr_id) {
                    for (i, (field_name, _)) in fields.iter().enumerate() {
                        let Some(range) = field_spans.get(i) else {
                            continue;
                        };
                        if known_fields
                            .as_ref()
                            .is_none_or(|f| f.contains_key(field_name))
                        {
                            map.insert(*range, SemanticTokenType::Property);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Walk patterns for match/catch bindings (e.g. `d` in `d: Color | never => d`)
    for (pat_id, pattern) in expr_body.patterns.iter() {
        let name = match pattern {
            baml_compiler_hir::Pattern::Binding(name) => name,
            baml_compiler_hir::Pattern::TypedBinding { name, .. } => name,
            _ => continue,
        };
        let name_str = name.as_str();
        if name_str == "_" {
            continue;
        }
        let Some(span) = source_map.pattern_span(pat_id) else {
            continue;
        };
        let Some((pat_start, pat_text)) = span_text(&span, file_text) else {
            continue;
        };
        if let Some(offset) = pat_text.find(name_str) {
            let range = text_range_at(pat_start, offset, name_str.len());
            map.insert(range, SemanticTokenType::Variable);
        }
    }

    map
}

/// Visitor for expression function bodies.
///
/// Pre-builds a `TextRange` -> `SemanticTokenType` resolution map from HIR/TIR,
/// then walks the CST in document order. Leaf tokens are checked against the
/// map first; if there is no entry the normal syntactic classifier is used.
struct ExprBodyVisitor<'a> {
    db: &'a ProjectDatabase,
    file: SourceFile,
    resolution_map: HashMap<TextRange, SemanticTokenType>,
}

impl<'a> ExprBodyVisitor<'a> {
    fn new(
        db: &'a ProjectDatabase,
        file: SourceFile,
        func_loc: baml_compiler_hir::FunctionId<'_>,
    ) -> Option<Self> {
        let body = baml_compiler_hir::function_body(db, func_loc);
        let baml_compiler_hir::FunctionBody::Expr(expr_body, source_map) = &*body else {
            return None;
        };
        let inference = baml_compiler_tir::function_type_inference(db, func_loc);
        let file_text = file.text(db);
        let resolution_map = build_resolution_map(db, expr_body, source_map, &inference, file_text);
        Some(Self {
            db,
            file,
            resolution_map,
        })
    }

    /// Dispatch a single node. Mirrors `visit_node` for the subset of node
    /// kinds that can appear inside expression bodies.
    fn visit_node(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        match node.kind() {
            ref n if n.is_comment() => emit_node(node, SemanticTokenType::Comment, out),
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                emit_node(node, SemanticTokenType::String, out);
            }
            SyntaxKind::TYPE_EXPR => visit_type_expr(self.db, self.file, node, out),
            SyntaxKind::LET_STMT => {
                self.visit_first_word_as(node, SemanticTokenType::Variable, out);
            }
            SyntaxKind::OBJECT_FIELD => {
                // Field names are handled by the resolution map (only valid
                // fields on the resolved class get highlighted as Property).
                self.visit_children(node, out);
            }
            SyntaxKind::OBJECT_LITERAL => {
                self.visit_children(node, out);
            }
            _ => self.visit_children(node, out),
        }
    }

    /// Classify a leaf token. Resolution map wins; otherwise fall back to the
    /// default token classifier.
    fn visit_token(&self, token: &SyntaxToken, out: &mut Vec<SemanticToken>) {
        if let Some(&token_type) = self.resolution_map.get(&token.text_range()) {
            emit_token(token, token_type, out);
        } else {
            visit_token(token, out);
        }
    }

    /// Walk all children, dispatching nodes and tokens.
    fn visit_children(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => self.visit_node(&n, out),
                NodeOrToken::Token(t) => self.visit_token(&t, out),
            }
        }
    }

    /// First WORD gets `word_type`, everything else dispatched normally.
    fn visit_first_word_as(
        &self,
        node: &SyntaxNode,
        word_type: SemanticTokenType,
        out: &mut Vec<SemanticToken>,
    ) {
        let mut found_word = false;
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => self.visit_node(&n, out),
                NodeOrToken::Token(t) => {
                    if !found_word && t.kind() == SyntaxKind::WORD {
                        found_word = true;
                        emit_token(&t, word_type, out);
                    } else {
                        self.visit_token(&t, out);
                    }
                }
            }
        }
    }
}

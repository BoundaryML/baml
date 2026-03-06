//! Semantic tokens for BAML files (compiler2 / lsp2_actions version).
//!
//! Provides `semantic_tokens(db, file) -> Vec<SemanticToken>` using a hybrid
//! CST + compiler2 approach:
//!
//! - **Structural tokens** (keywords, comments, strings, numbers, operators)
//!   come from a single CST walk with syntactic classification.
//!
//! - **Expression bodies** use compiler2's type-aware classification.
//!   `infer_scope_types(db, scope_id)` gives `ExprId → Ty`, and
//!   `function_body_source_map` gives `ExprId → TextRange`. We pre-build a
//!   `TextRange → SemanticTokenType` map from these two sources and consult it
//!   during the CST walk so tokens are emitted in document order without
//!   sorting.
//!
//! - **Type expressions** in annotations use CST node kinds (already works
//!   for structural classification; name resolution upgrades keyword vs
//!   class vs enum).

use std::collections::HashMap;

use baml_base::SourceFile;
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use baml_compiler2_ast::{Expr, ExprBody, Pattern};
use baml_compiler2_hir::{
    body::FunctionBody, contributions::Definition, loc::FunctionLoc, scope::ScopeKind,
};
use baml_compiler2_tir::ty::Ty;
use rowan::NodeOrToken;
use text_size::TextRange;

use crate::Db;

// ── SemanticTokenType ─────────────────────────────────────────────────────────

/// The semantic token type for a BAML file.
///
/// Copied from `baml_lsp_actions::semantic_tokens::SemanticTokenType` — the
/// same enum values, same `TOKEN_TYPES` legend ordering. The v2 crate owns
/// this type so there is no dependency on the v1 compiler.
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

/// Token type legend — order determines the LSP legend index.
///
/// The order MUST match what is advertised in `server_capabilities()`.
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
    /// Get the index of this token type in the `TOKEN_TYPES` legend.
    ///
    /// The index is the `token_type` field in the LSP `SemanticToken` struct.
    #[allow(clippy::cast_possible_truncation)]
    pub fn legend_index(self) -> u32 {
        TOKEN_TYPES
            .iter()
            .position(|t| *t == self)
            .expect("SemanticTokenType missing in legend") as u32
    }

    /// String representation matching the LSP semantic token type names.
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

// ── SemanticToken ─────────────────────────────────────────────────────────────

/// A classified token ready for LSP encoding.
#[derive(Debug, Clone)]
pub struct SemanticToken {
    pub range: TextRange,
    pub token_type: SemanticTokenType,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Compute semantic tokens for a file.
///
/// Always returns tokens in document order (required by the LSP
/// `textDocument/semanticTokens/full` contract).
///
/// Regular function (not a Salsa query). Internally calls Salsa-cached queries
/// (`infer_scope_types`, `function_body`, `function_body_source_map`,
/// `file_semantic_index`, `syntax_tree`).
pub fn semantic_tokens(db: &dyn Db, file: SourceFile) -> Vec<SemanticToken> {
    let root = baml_compiler_parser::syntax_tree(db, file);
    let mut out = Vec::new();
    visit_node(db, file, &root, &mut out);
    out
}

// ── CST walker ───────────────────────────────────────────────────────────────

/// Emit a token for a single non-whitespace leaf token.
fn emit_token(token: &SyntaxToken, token_type: SemanticTokenType, out: &mut Vec<SemanticToken>) {
    if !token.kind().is_whitespace() {
        out.push(SemanticToken {
            range: token.text_range(),
            token_type,
        });
    }
}

/// Emit `token_type` for every non-trivia leaf token under `node`.
fn emit_node(node: &SyntaxNode, token_type: SemanticTokenType, out: &mut Vec<SemanticToken>) {
    for child in node.descendants_with_tokens() {
        if let NodeOrToken::Token(t) = child {
            emit_token(&t, token_type, out);
        }
    }
}

/// Dispatch a node to its visitor.
fn visit_node(db: &dyn Db, file: SourceFile, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
    match node.kind() {
        // Comment nodes (headers are nodes, not just tokens)
        ref n if n.is_comment() => emit_node(node, SemanticTokenType::Comment, out),
        // String literals — emit the whole thing as String
        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
            emit_node(node, SemanticTokenType::String, out);
        }
        SyntaxKind::ATTRIBUTE | SyntaxKind::BLOCK_ATTRIBUTE => {
            visit_attribute(db, file, node, out);
        }
        SyntaxKind::TYPE_ALIAS_DEF => visit_type_alias_def(db, file, node, out),
        SyntaxKind::ENUM_DEF => visit_word_as(db, file, node, SemanticTokenType::Enum, out),
        SyntaxKind::ENUM_VARIANT => {
            visit_word_as(db, file, node, SemanticTokenType::EnumMember, out);
        }
        SyntaxKind::CLASS_DEF => visit_word_as(db, file, node, SemanticTokenType::Class, out),
        SyntaxKind::FIELD => visit_word_as(db, file, node, SemanticTokenType::Property, out),
        SyntaxKind::FUNCTION_DEF => visit_function_def(db, file, node, out),
        SyntaxKind::PARAMETER => {
            visit_word_as(db, file, node, SemanticTokenType::Parameter, out);
        }
        SyntaxKind::TYPE_EXPR => visit_type_expr(db, file, node, out),
        SyntaxKind::LET_STMT => {
            visit_first_word_as(db, file, node, SemanticTokenType::Variable, out);
        }
        SyntaxKind::CLIENT_TYPE => {
            visit_word_as(db, file, node, SemanticTokenType::Type, out);
        }
        SyntaxKind::CONFIG_ITEM => visit_config_item(db, file, node, out),
        SyntaxKind::CLIENT_DEF | SyntaxKind::GENERATOR_DEF | SyntaxKind::RETRY_POLICY_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Struct, out);
        }
        SyntaxKind::TEST_DEF => visit_word_as(db, file, node, SemanticTokenType::Struct, out),
        SyntaxKind::TEMPLATE_STRING_DEF => {
            visit_word_as(db, file, node, SemanticTokenType::Function, out);
        }
        SyntaxKind::PROMPT_FIELD => {
            visit_word_as(db, file, node, SemanticTokenType::Property, out);
        }
        SyntaxKind::CLIENT_FIELD => visit_client_field(db, file, node, out),
        _ => visit_children(db, file, node, out),
    }
}

/// Classify a leaf token syntactically.
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

/// Walk all children, dispatching nodes and tokens.
fn visit_children(db: &dyn Db, file: SourceFile, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => visit_token(&t, out),
        }
    }
}

/// Visit a node where all WORD tokens are classified as `word_type`.
fn visit_word_as(
    db: &dyn Db,
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

/// Visit a node where the first WORD token is classified as `word_type`.
fn visit_first_word_as(
    db: &dyn Db,
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

/// Visit a `CONFIG_ITEM` node — key as Property.
fn visit_config_item(
    db: &dyn Db,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                ref k if k.is_keyword() => emit_token(&t, SemanticTokenType::Property, out),
                SyntaxKind::WORD => emit_token(&t, SemanticTokenType::Property, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a `CLIENT_FIELD` node — `client` keyword as Property.
fn visit_client_field(
    db: &dyn Db,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::KW_CLIENT => emit_token(&t, SemanticTokenType::Property, out),
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit an `ATTRIBUTE` or `BLOCK_ATTRIBUTE` node — all content as Decorator.
fn visit_attribute(db: &dyn Db, file: SourceFile, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
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

/// Visit a `TYPE_ALIAS_DEF` — "type" word as Keyword, name as Type.
fn visit_type_alias_def(
    db: &dyn Db,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    let mut found_keyword = false; // "type" is a WORD in the grammar
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

/// Resolve a type name to a `SemanticTokenType` using compiler2.
///
/// Checks the package items for classes, enums, and type aliases.
/// Falls back to `Type` for primitive keywords.
fn resolve_type_name(db: &dyn Db, file: SourceFile, name: &str) -> SemanticTokenType {
    if matches!(
        name,
        "int"
            | "float"
            | "string"
            | "bool"
            | "map"
            | "unknown"
            | "never"
            | "image"
            | "audio"
            | "video"
            | "pdf"
            | "null"
    ) {
        return SemanticTokenType::Type;
    }

    // Look up in package items.
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
    let name_obj = baml_base::Name::new(name);

    match pkg_items.lookup_type(&[name_obj]) {
        Some(Definition::Class(_)) => SemanticTokenType::Class,
        Some(Definition::Enum(_)) => SemanticTokenType::Enum,
        Some(Definition::TypeAlias(_)) => SemanticTokenType::Type,
        _ => SemanticTokenType::Namespace,
    }
}

/// Visit a `TYPE_EXPR` node — resolve WORD tokens to their definition kind.
fn visit_type_expr(db: &dyn Db, file: SourceFile, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => visit_node(db, file, &n, out),
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::WORD => {
                    emit_token(&t, resolve_type_name(db, file, t.text()), out);
                }
                _ => visit_token(&t, out),
            },
        }
    }
}

/// Visit a `FUNCTION_DEF` node.
///
/// Handles the header (name, params, return type) via CST. For expression
/// bodies (`EXPR_FUNCTION_BODY`) switches to `ExprBodyVisitor` which uses
/// compiler2 type information for richer classification.
fn visit_function_def(
    db: &dyn Db,
    file: SourceFile,
    node: &SyntaxNode,
    out: &mut Vec<SemanticToken>,
) {
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => {
                if n.kind() == SyntaxKind::EXPR_FUNCTION_BODY {
                    // Try to build the type-aware visitor for the expression body.
                    if let Some(visitor) =
                        ExprBodyVisitor::for_function_at(db, file, node.text_range().start())
                    {
                        visitor.visit_children(&n, out);
                    } else {
                        // Fall back to pure-CST walk if compiler2 data is unavailable.
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

// ── ExprBodyVisitor ───────────────────────────────────────────────────────────

/// Visitor for expression function bodies.
///
/// Pre-builds a `TextRange → SemanticTokenType` resolution map from compiler2's
/// `infer_scope_types` and `function_body_source_map`, then walks the CST in
/// document order. Leaf tokens are checked against the map first; if there is
/// no entry the normal syntactic classifier is used.
struct ExprBodyVisitor<'db> {
    db: &'db dyn Db,
    file: SourceFile,
    resolution_map: HashMap<TextRange, SemanticTokenType>,
}

impl<'db> ExprBodyVisitor<'db> {
    /// Try to build an `ExprBodyVisitor` for the function whose `FUNCTION_DEF`
    /// node starts at `node_start`.
    ///
    /// 1. Find the function in the item tree by matching its span start.
    /// 2. Load `function_body` — only proceeds if it is `FunctionBody::Expr`.
    /// 3. Load `function_body_source_map` — provides `ExprId → TextRange`.
    /// 4. Load `infer_scope_types` for the function scope — provides
    ///    `ExprId → Ty` and `PatId → Ty`.
    /// 5. Pre-build a `TextRange → SemanticTokenType` map.
    fn for_function_at(
        db: &'db dyn Db,
        file: SourceFile,
        node_start: text_size::TextSize,
    ) -> Option<Self> {
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);

        // Find the function whose span starts at node_start (the FUNCTION_DEF node).
        let (func_local_id, _func_data) = item_tree
            .functions
            .iter()
            .find(|(_, f)| f.span.start() == node_start)?;

        let func_loc = FunctionLoc::new(db, file, *func_local_id);

        // Only expression-body functions benefit from type-aware classification.
        let body = baml_compiler2_hir::body::function_body(db, func_loc);
        let FunctionBody::Expr(expr_body) = body.as_ref() else {
            return None;
        };

        // Source map: ExprId → TextRange.
        let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;

        // Find the function scope for infer_scope_types.
        let index = baml_compiler2_hir::file_semantic_index(db, file);
        let func_scope_file_id = index
            .scopes
            .iter()
            .enumerate()
            .find(|(_, s)| s.kind == ScopeKind::Function && s.range.start() == node_start)
            .map(|(i, _)| {
                #[allow(clippy::cast_possible_truncation)]
                baml_compiler2_hir::scope::FileScopeId::new(i as u32)
            })?;

        let func_scope_id = index.scope_ids[func_scope_file_id.index() as usize];
        let inference = baml_compiler2_tir::inference::infer_scope_types(db, func_scope_id);

        let file_text = file.text(db);
        let resolution_map = build_resolution_map(expr_body, &source_map, inference, file_text);

        Some(Self {
            db,
            file,
            resolution_map,
        })
    }

    /// Dispatch a single node inside an expression body.
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
            SyntaxKind::OBJECT_FIELD | SyntaxKind::OBJECT_LITERAL => {
                self.visit_children(node, out);
            }
            _ => self.visit_children(node, out),
        }
    }

    /// Classify a leaf token. Resolution map wins over syntactic defaults.
    fn visit_token(&self, token: &SyntaxToken, out: &mut Vec<SemanticToken>) {
        if let Some(&token_type) = self.resolution_map.get(&token.text_range()) {
            emit_token(token, token_type, out);
        } else {
            visit_token(token, out);
        }
    }

    /// Walk all children.
    fn visit_children(&self, node: &SyntaxNode, out: &mut Vec<SemanticToken>) {
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => self.visit_node(&n, out),
                NodeOrToken::Token(t) => self.visit_token(&t, out),
            }
        }
    }

    /// First WORD as `word_type`, rest dispatched normally.
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

// ── Resolution map builder ────────────────────────────────────────────────────

/// Map a `Ty` to a `SemanticTokenType` for expression-level tokens.
///
/// Returns `None` for unknown/error types so they don't get classified.
fn ty_to_token_type(ty: &Ty) -> Option<SemanticTokenType> {
    match ty {
        Ty::Class(_) => Some(SemanticTokenType::Class),
        Ty::Enum(_) => Some(SemanticTokenType::Enum),
        Ty::EnumVariant(_, _) => Some(SemanticTokenType::EnumMember),
        Ty::TypeAlias(_) => Some(SemanticTokenType::Type),
        Ty::Function { .. } => Some(SemanticTokenType::Function),
        // Primitives, lists, maps, unions etc. — don't highlight specially
        _ => None,
    }
}

/// Build a `TextRange → SemanticTokenType` map from compiler2 type information.
///
/// Iterates all expressions in `expr_body`. For each expression whose type
/// we can classify and whose source range we know, inserts an entry. The
/// `ExprBodyVisitor` then consults this map during its CST walk.
///
/// Key mappings:
///
/// - `Expr::Path(names)` — single-segment path; `expr_span` is exactly the
///   identifier's range. Ty from `ScopeInference::expression_type`.
///
/// - `Expr::FieldAccess { field, .. }` — `expr_span` covers `base.field`.
///   We extract just the field name by text-scanning the tail of the span.
///
/// - `Expr::Object { type_name, fields, .. }` — the constructor name is
///   inside the `expr_span`; extracted via text search. Field names are
///   emitted as Property (only if the object expression resolves to a class
///   and the field name appears in the span).
///
/// - `Pattern::Binding` / `Pattern::TypedBinding` — bound variable names are
///   classified as Variable.
fn build_resolution_map(
    expr_body: &ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &baml_compiler2_tir::inference::ScopeInference<'_>,
    file_text: &str,
) -> HashMap<TextRange, SemanticTokenType> {
    let mut map: HashMap<TextRange, SemanticTokenType> = HashMap::new();

    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            Expr::Path(names) if !names.is_empty() => {
                // Single-segment path after AST lowering; multi-segment paths
                // were desugared to FieldAccess chains. The expr_span is the
                // identifier's own range.
                let span = source_map.expr_span(expr_id);
                if span.is_empty() {
                    continue;
                }
                // Only classify if we have a type for this expression.
                if let Some(ty) = inference.expression_type(expr_id) {
                    if let Some(token_type) = ty_to_token_type(ty) {
                        map.insert(span, token_type);
                    }
                }
            }

            Expr::FieldAccess { field, .. } => {
                // expr_span covers `base.field` — extract only the field name.
                let span = source_map.expr_span(expr_id);
                if span.is_empty() {
                    continue;
                }
                let start: usize = span.start().into();
                let end: usize = span.end().into();
                if end > file_text.len() {
                    continue;
                }
                let text = &file_text[start..end];
                let field_str = field.as_str();
                // The field name is at the end of the span (after the last dot).
                if let Some(offset) = text.rfind(field_str) {
                    // Verify the character before is a dot (avoids substring false matches).
                    let dot_pos = if offset > 0 { offset - 1 } else { 0 };
                    if offset > 0 && text.as_bytes().get(dot_pos) != Some(&b'.') {
                        continue;
                    }
                    let field_start = start + offset;
                    let field_end = field_start + field_str.len();
                    if field_start < field_end && field_end <= file_text.len() {
                        let field_range = TextRange::new(
                            field_start.try_into().unwrap_or_default(),
                            field_end.try_into().unwrap_or_default(),
                        );
                        map.insert(field_range, SemanticTokenType::Property);
                    }
                }
            }

            Expr::Object {
                type_name, fields, ..
            } => {
                let span = source_map.expr_span(expr_id);
                if span.is_empty() {
                    continue;
                }
                let span_start: usize = span.start().into();
                let span_end: usize = span.end().into();
                if span_end > file_text.len() {
                    continue;
                }
                let span_text = &file_text[span_start..span_end];

                // Classify the constructor type name.
                if let Some(name) = type_name {
                    let name_str = name.as_str();
                    if let Some(offset) = span_text.find(name_str) {
                        let name_start = span_start + offset;
                        let name_end = name_start + name_str.len();
                        let name_range = TextRange::new(
                            name_start.try_into().unwrap_or_default(),
                            name_end.try_into().unwrap_or_default(),
                        );
                        // Use the expression type if available.
                        let token_type = inference
                            .expression_type(expr_id)
                            .and_then(ty_to_token_type)
                            .unwrap_or(SemanticTokenType::Class);
                        map.insert(name_range, token_type);
                    }
                }

                // Classify field names as Property.
                for (field_name, _field_expr) in fields {
                    let field_str = field_name.as_str();
                    if let Some(offset) = span_text.find(field_str) {
                        let field_start = span_start + offset;
                        let field_end = field_start + field_str.len();
                        let field_range = TextRange::new(
                            field_start.try_into().unwrap_or_default(),
                            field_end.try_into().unwrap_or_default(),
                        );
                        map.insert(field_range, SemanticTokenType::Property);
                    }
                }
            }

            _ => {}
        }
    }

    // Classify pattern binding names as Variable.
    for (pat_id, pattern) in expr_body.patterns.iter() {
        let name_str = match pattern {
            Pattern::Binding(name) => name.as_str(),
            Pattern::TypedBinding { name, .. } => name.as_str(),
            _ => continue,
        };
        if name_str == "_" {
            continue;
        }

        let span = source_map.pattern_span(pat_id);
        if span.is_empty() {
            continue;
        }
        let span_start: usize = span.start().into();
        let span_end: usize = span.end().into();
        if span_end > file_text.len() {
            continue;
        }
        let pat_text = &file_text[span_start..span_end];
        if let Some(offset) = pat_text.find(name_str) {
            let name_start = span_start + offset;
            let name_end = name_start + name_str.len();
            let name_range = TextRange::new(
                name_start.try_into().unwrap_or_default(),
                name_end.try_into().unwrap_or_default(),
            );
            map.insert(name_range, SemanticTokenType::Variable);
        }
    }

    map
}

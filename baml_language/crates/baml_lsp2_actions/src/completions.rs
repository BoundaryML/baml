//! `completions_at` — context-aware code completions at a cursor position.
//!
//! This is a regular function (not a Salsa query). It detects the completion
//! context from the CST parent node of the token at `offset`, then gathers
//! relevant completion items:
//!
//! ## Context detection
//!
//! - **Type position** (token inside a `TYPE_EXPR` node): suggest classes,
//!   enums, type aliases, and builtin primitives.
//!
//! - **Field access** (token immediately after `.` in a `PATH_EXPR` or
//!   `FIELD_ACCESS_EXPR`): resolve the base expression type, then suggest its
//!   fields, methods, or enum variants.
//!
//! - **Value position** (token inside an expression in an
//!   `EXPR_FUNCTION_BODY`): suggest local variables in scope, builtin package
//!   roots, then all package-level functions and template strings.
//!
//! - **Top-level** (token at the source file root): suggest declaration
//!   keywords (`class`, `function`, `enum`, …).
//!
//! ## Candidate sources
//!
//! - `scope_bindings_query(scope_id)` — local variables in the current and
//!   ancestor scopes.
//! - `package_items(pkg_id)` — all top-level definitions across the package.
//! - `package_items(builtin_pkg_id)` — builtin definitions from the `baml`
//!   and `env` packages.
//! - `resolve_class_fields(class_loc)` — fields for field-access completions.
//! - `enum_data(enum_loc)` — variants for field-access on enums.

use std::collections::HashSet;

use baml_base::{Name, SourceFile, attr::TyAttr};
use baml_compiler_syntax::{SyntaxKind, SyntaxNode};
use baml_compiler2_hir::{
    contributions::Definition,
    loc::FunctionLoc,
    package::{PackageId, package_items},
    scope::ScopeKind,
    semantic_index::ScopeBindings,
    signature::function_signature,
};
use baml_type::{MediaKind, PrimitiveType, Ty};
use rowan::{NodeOrToken, ast::AstNode};
use text_size::TextSize;

use crate::{Db, utils};

/// Format a function signature as `(param1: type1, param2: type2) -> return_type`.
fn format_function_signature(db: &dyn Db, func_loc: FunctionLoc<'_>) -> String {
    let sig = function_signature(db, func_loc);
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|param| {
            let optional = if param.has_default { "?" } else { "" };
            format!(
                "{}{}: {}",
                param.name.as_str(),
                optional,
                utils::display_type_expr(&param.ty)
            )
        })
        .collect();
    let ret = sig
        .return_type
        .as_ref()
        .map(utils::display_type_expr)
        .unwrap_or_else(|| "null".to_string());
    format!("({}) -> {}", params.join(", "), ret)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinMethodMode {
    Instance,
    Static,
    All,
}

/// Format a builtin class method signature for completion details.
///
/// Instance completions hide the synthetic `self` parameter because the receiver
/// is already present in source (`img.base64()`, not `img.base64(img)`).
fn format_builtin_method_signature(
    db: &dyn Db,
    func_loc: FunctionLoc<'_>,
    mode: BuiltinMethodMode,
) -> String {
    let sig = function_signature(db, func_loc);
    let params: Vec<String> = sig
        .params
        .iter()
        .enumerate()
        .filter_map(|(idx, param)| {
            if mode == BuiltinMethodMode::Instance && idx == 0 && param.name.as_str() == "self" {
                None
            } else {
                Some(format!(
                    "{}: {}",
                    param.name.as_str(),
                    utils::display_type_expr(&param.ty)
                ))
            }
        })
        .collect();
    let ret = sig
        .return_type
        .as_ref()
        .map(utils::display_type_expr)
        .unwrap_or_else(|| "null".to_string());
    format!("({}) -> {}", params.join(", "), ret)
}

// ── CompletionKind ────────────────────────────────────────────────────────────

/// The semantic kind of a completion item.
///
/// Maps to LSP `CompletionItemKind` in the request handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    /// A top-level declaration keyword (`class`, `function`, `enum`, …).
    Keyword,
    /// A user-defined function.
    Function,
    /// A user-defined class.
    Class,
    /// A user-defined enum.
    Enum,
    /// An enum variant (produced in field-access context on an enum type).
    EnumVariant,
    /// A class field (produced in field-access context on a class type).
    Field,
    /// A local variable (let binding or parameter).
    Variable,
    /// A primitive type keyword (`int`, `float`, `string`, …).
    Primitive,
    /// A type alias.
    TypeAlias,
    /// A template string.
    TemplateString,
    /// A client definition.
    Client,
    /// A generator definition.
    Generator,
    /// A test definition.
    Test,
    /// A retry policy definition.
    RetryPolicy,
    /// A class method.
    Method,
    /// A namespace (module) containing other definitions.
    Module,
    /// A named function parameter in a call argument list.
    Parameter,
}

/// How an editor should interpret [`Completion::insert_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionInsertTextFormat {
    /// Insert the text literally.
    #[default]
    PlainText,
    /// Interpret LSP snippet tabstops such as `$0` and `${1:Name}`.
    Snippet,
}

// ── Completion ────────────────────────────────────────────────────────────────

/// A single completion item returned by `completions_at`.
///
/// The LSP layer (`request.rs`) converts this to `lsp_types::CompletionItem`.
#[derive(Debug, Clone)]
pub struct Completion {
    /// The text displayed in the completion list.
    pub label: String,
    /// Semantic kind for icon and sorting.
    pub kind: CompletionKind,
    /// Optional detail string (e.g., type signature).
    pub detail: Option<String>,
    /// Text inserted on acceptance (defaults to `label` if `None`).
    pub insert_text: Option<String>,
    /// Whether `insert_text` is literal text or an LSP snippet.
    pub insert_text_format: CompletionInsertTextFormat,
    /// Sort key (lower sorts first).
    pub sort_text: Option<String>,
}

impl Completion {
    fn new(label: impl Into<String>, kind: CompletionKind) -> Self {
        let label = label.into();
        Self {
            label,
            kind,
            detail: None,
            insert_text: None,
            insert_text_format: CompletionInsertTextFormat::PlainText,
            sort_text: None,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.sort_text = Some(sort.into());
        self
    }

    fn with_insert_text(mut self, insert_text: impl Into<String>) -> Self {
        self.insert_text = Some(insert_text.into());
        self
    }

    fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.insert_text = Some(snippet.into());
        self.insert_text_format = CompletionInsertTextFormat::Snippet;
        self
    }
}

// ── CompletionContext ─────────────────────────────────────────────────────────

/// The detected completion context.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CompletionContext {
    /// Cursor is in a type expression (e.g., after `:` in a field or parameter).
    TypePosition,
    /// Cursor is after a `.` — member/method/variant access on a base expression.
    MemberAccess,
    /// Cursor is in a value expression inside a function body.
    ValuePosition,
    /// Cursor is inside a function call argument list.
    CallArguments,
    /// Cursor is at the top level (not inside any item body).
    TopLevel,
    /// Context cannot be determined (e.g., cursor in a comment or string).
    Unknown,
}

// ── completions_at ────────────────────────────────────────────────────────────

/// Compute context-aware completions at `offset` in `file`.
///
/// Regular function (not cached). The expensive work is internally
/// Salsa-cached (`file_semantic_index`, `package_items`, etc.).
///
/// Returns an empty `Vec` if no completions are applicable.
pub fn completions_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Vec<Completion> {
    let Some(token) = utils::find_token_at_offset(db, file, offset) else {
        return completions_at_empty_file(db, file);
    };

    let context = detect_context(&token, offset);

    match context {
        CompletionContext::TypePosition => completions_for_type_position(db, file, offset),
        CompletionContext::MemberAccess => completions_for_field_access(db, file, &token, offset),
        CompletionContext::ValuePosition => completions_for_value_position(db, file, offset),
        CompletionContext::CallArguments => {
            completions_for_call_arguments(db, file, &token, offset)
        }
        CompletionContext::TopLevel => completions_for_top_level(),
        CompletionContext::Unknown => Vec::new(),
    }
}

// ── Context detection ─────────────────────────────────────────────────────────

/// Detect what kind of completion context the cursor is in.
///
/// We walk the token's ancestor nodes looking for context-indicating patterns:
///
/// 1. If any ancestor is `TYPE_EXPR` → type position.
/// 2. If the preceding non-trivia sibling token is `.` → field access.
/// 3. If inside `EXPR_FUNCTION_BODY` → value position.
/// 4. If ancestor is `SOURCE_FILE` with no enclosing item → top level.
fn detect_context(
    token: &baml_compiler_syntax::SyntaxToken,
    _offset: TextSize,
) -> CompletionContext {
    // Check for field access: immediately after a DOT token.
    // Walk prev_sibling_or_token to find the token just before the cursor's token.
    if is_field_access_position(token) {
        return CompletionContext::MemberAccess;
    }

    if find_call_args_completion_ancestor(token).is_some() {
        return CompletionContext::CallArguments;
    }

    // Walk ancestors to detect the structural context.
    let mut node = token.parent();
    while let Some(current) = node {
        let kind = current.kind();

        match kind {
            // Inside a TYPE_EXPR node → type position. For PARAMETER /
            // FIELD, only treat as type position if we're specifically in
            // the type-annotation part (not the name part).
            SyntaxKind::TYPE_EXPR
            | SyntaxKind::UNION_TYPE
            | SyntaxKind::OPTIONAL_TYPE
            | SyntaxKind::ARRAY_TYPE
            | SyntaxKind::MAP_TYPE
            | SyntaxKind::FUNCTION_TYPE
            | SyntaxKind::PARAMETER
            | SyntaxKind::FIELD
                if is_in_type_annotation(&current) =>
            {
                return CompletionContext::TypePosition;
            }

            // Inside an expression function body → value position.
            SyntaxKind::EXPR_FUNCTION_BODY
            | SyntaxKind::EXPR
            | SyntaxKind::BINARY_EXPR
            | SyntaxKind::UNARY_EXPR
            | SyntaxKind::CALL_EXPR
            | SyntaxKind::PATH_EXPR
            | SyntaxKind::PAREN_EXPR
            | SyntaxKind::BLOCK_EXPR
            | SyntaxKind::IF_EXPR
            | SyntaxKind::IF_LET_EXPR
            | SyntaxKind::FOR_EXPR
            | SyntaxKind::LET_STMT
            | SyntaxKind::RETURN_STMT => {
                return CompletionContext::ValuePosition;
            }

            // At the source file root → top level.
            SyntaxKind::SOURCE_FILE => {
                return CompletionContext::TopLevel;
            }

            _ => {}
        }

        node = current.parent();
    }

    CompletionContext::Unknown
}

/// Returns `true` if `token` is a `WORD` immediately preceded by a `.` token,
/// or if `token` IS a `.` token (cursor right after the dot with nothing typed),
/// indicating a field access completion context.
fn is_field_access_position(token: &baml_compiler_syntax::SyntaxToken) -> bool {
    // If the token itself is a DOT, this is a field access position —
    // the cursor is right after the dot with no partial segment typed yet.
    if token.kind() == SyntaxKind::DOT {
        if let Some(parent) = token.parent() {
            let has_base_before = parent.children_with_tokens().any(|c| match c {
                NodeOrToken::Token(t) => {
                    t.kind() == SyntaxKind::WORD
                        && t.text_range().end() <= token.text_range().start()
                }
                NodeOrToken::Node(n) => n.text_range().end() <= token.text_range().start(),
            });
            return has_base_before;
        }
        return false;
    }

    // Check previous sibling tokens in the parent node.
    let Some(parent) = token.parent() else {
        return false;
    };

    // Walk siblings before our token.
    let mut found_our_token = false;
    let mut prev_meaningful: Option<SyntaxKind> = None;

    // Collect all children of parent in order, find our token and look at what precedes it.
    for child in parent.children_with_tokens() {
        match &child {
            NodeOrToken::Token(t) => {
                if t == token {
                    found_our_token = true;
                    break;
                }
                if !t.kind().is_trivia() {
                    prev_meaningful = Some(t.kind());
                }
            }
            NodeOrToken::Node(_) => {
                if !found_our_token {
                    prev_meaningful = None; // Reset — node precedes our token
                }
            }
        }
    }

    if found_our_token {
        if let Some(SyntaxKind::DOT) = prev_meaningful {
            return true;
        }
    }

    // Also check parent's kind: PATH_EXPR with multiple segments indicates field access.
    if parent.kind() == SyntaxKind::PATH_EXPR {
        // In a PATH_EXPR like `foo.bar`, `bar` is a field access on `foo`.
        // Count WORD tokens — if more than one, we're in multi-segment path.
        let words: Vec<_> = parent
            .children_with_tokens()
            .filter_map(|c| {
                if let NodeOrToken::Token(t) = c {
                    if t.kind() == SyntaxKind::WORD {
                        Some(t)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Check if any DOT appears before our token in this path.
        let has_dot_before = parent.children_with_tokens().any(|c| {
            if let NodeOrToken::Token(t) = &c {
                t.kind() == SyntaxKind::DOT && t.text_range().end() <= token.text_range().start()
            } else {
                false
            }
        });

        if words.len() > 1 && has_dot_before {
            return true;
        }
    }

    false
}

/// Returns `true` if `node` or any ancestor is a `TYPE_EXPR`.
fn is_in_type_annotation(node: &SyntaxNode) -> bool {
    let mut current: Option<SyntaxNode> = Some(node.clone());
    while let Some(n) = current {
        let k = n.kind();
        if k == SyntaxKind::TYPE_EXPR {
            return true;
        }
        // Stop searching upward if we hit an expression context.
        if matches!(
            k,
            SyntaxKind::EXPR_FUNCTION_BODY
                | SyntaxKind::EXPR
                | SyntaxKind::PATH_EXPR
                | SyntaxKind::CALL_EXPR
        ) {
            return false;
        }
        current = n.parent();
    }
    false
}

// ── Call-argument completions ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CallParamCompletion {
    name: String,
    ty: String,
    optional: bool,
}

fn find_call_args_completion_ancestor(
    token: &baml_compiler_syntax::SyntaxToken,
) -> Option<SyntaxNode> {
    let parent = token.parent()?;
    match parent.kind() {
        SyntaxKind::CALL_ARGS => Some(parent),
        SyntaxKind::CALL_ARG => parent
            .parent()
            .filter(|node| node.kind() == SyntaxKind::CALL_ARGS),
        _ => None,
    }
}

fn completions_for_call_arguments(
    db: &dyn Db,
    file: SourceFile,
    token: &baml_compiler_syntax::SyntaxToken,
    offset: TextSize,
) -> Vec<Completion> {
    let Some(args_node) = find_call_args_completion_ancestor(token) else {
        return Vec::new();
    };
    let Some(call_node) = args_node.parent().filter(|node| {
        matches!(
            node.kind(),
            SyntaxKind::CALL_EXPR | SyntaxKind::OPTIONAL_CALL_EXPR
        )
    }) else {
        return Vec::new();
    };

    let provided = provided_call_args(&args_node, offset);
    let Some(params) = call_params_for_call_node(db, file, offset, &call_node, &args_node) else {
        return completions_for_value_position(db, file, offset);
    };

    let mut items = Vec::new();
    for (idx, param) in params.into_iter().enumerate() {
        if idx < provided.positional_count || provided.named.contains(&param.name) {
            continue;
        }
        let detail = if param.optional {
            format!("{} (optional)", param.ty)
        } else {
            param.ty.clone()
        };
        let sort_group = usize::from(param.optional);
        items.push(
            Completion::new(param.name.as_str(), CompletionKind::Parameter)
                .with_detail(detail)
                .with_insert_text(format!("{} = ", param.name))
                .with_sort(format!("{sort_group}_{}", param.name)),
        );
    }

    items
}

struct ProvidedCallArgs {
    named: HashSet<String>,
    positional_count: usize,
}

fn provided_call_args(args_node: &SyntaxNode, offset: TextSize) -> ProvidedCallArgs {
    let mut named = HashSet::new();
    let mut positional_count = 0;

    for node in args_node
        .children()
        .filter(|node| node.kind() == SyntaxKind::CALL_ARG)
    {
        let Some(arg) = baml_compiler_syntax::ast::CallArg::cast(node.clone()) else {
            continue;
        };
        if let Some(label) = arg.label() {
            named.insert(label.text().to_string());
        } else if node.text_range().end() <= offset {
            positional_count += 1;
        }
    }

    ProvidedCallArgs {
        named,
        positional_count,
    }
}

fn call_params_for_call_node(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    call_node: &SyntaxNode,
    args_node: &SyntaxNode,
) -> Option<Vec<CallParamCompletion>> {
    let callee_name = callee_name_token(call_node, args_node)?;
    let name = Name::new(callee_name.text());
    let method_like = callee_has_dot_before_args(call_node, args_node);

    match baml_compiler2_ppir::resolve::resolve_name_at(
        db,
        file,
        callee_name.text_range().start(),
        &name,
    ) {
        baml_compiler2_ppir::resolve::ResolvedName::Item(Definition::Function(func_loc))
        | baml_compiler2_ppir::resolve::ResolvedName::Builtin(Definition::Function(func_loc)) => {
            Some(function_params_for_completion(db, func_loc, method_like))
        }
        baml_compiler2_ppir::resolve::ResolvedName::Local {
            definition_site: Some(site),
            ..
        } => local_variable_ty(db, file, offset, site)
            .and_then(|ty| params_from_function_ty(db, file, &ty)),
        _ => None,
    }
}

fn callee_name_token(
    call_node: &SyntaxNode,
    args_node: &SyntaxNode,
) -> Option<baml_compiler_syntax::SyntaxToken> {
    let args_start = args_node.text_range().start();
    call_node
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|token| token.text_range().end() <= args_start)
        .filter(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT))
        .last()
}

fn callee_has_dot_before_args(call_node: &SyntaxNode, args_node: &SyntaxNode) -> bool {
    let args_start = args_node.text_range().start();
    call_node
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .any(|token| token.kind() == SyntaxKind::DOT && token.text_range().end() <= args_start)
}

fn function_params_for_completion(
    db: &dyn Db,
    func_loc: FunctionLoc<'_>,
    method_like: bool,
) -> Vec<CallParamCompletion> {
    let file = func_loc.file(db);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let iface = baml_compiler2_hir_ty::package_interface::package_interface(db, pkg_id);
    let sig = function_signature(db, func_loc);

    let mut params: Vec<CallParamCompletion> =
        if let Some(exported) = iface.lookup_function(&pkg_info.namespace_path, &sig.name) {
            exported
                .params
                .iter()
                .filter_map(|param| {
                    param.name.as_ref().map(|name| CallParamCompletion {
                        name: name.as_str().to_string(),
                        ty: utils::display_ty_for_file(db, file, &param.ty),
                        optional: param.is_optional(),
                    })
                })
                .collect()
        } else {
            sig.params
                .iter()
                .map(|param| CallParamCompletion {
                    name: param.name.as_str().to_string(),
                    ty: utils::display_type_expr(&param.ty),
                    optional: param.has_default,
                })
                .collect()
        };

    if method_like && params.first().is_some_and(|param| param.name == "self") {
        params.remove(0);
    }

    params
}

fn params_from_function_ty(
    db: &dyn Db,
    file: SourceFile,
    ty: &Ty,
) -> Option<Vec<CallParamCompletion>> {
    let Ty::Function { params, .. } = ty else {
        return None;
    };
    Some(
        params
            .iter()
            .filter_map(|param| {
                param.name.as_ref().map(|name| CallParamCompletion {
                    name: name.as_str().to_string(),
                    ty: utils::display_ty_for_file(db, file, &param.ty),
                    optional: param.is_optional(),
                })
            })
            .collect(),
    )
}

// ── Type-position completions ─────────────────────────────────────────────────

/// Completions for a type annotation position.
///
/// Suggests: builtin primitives + all user-defined types (classes, enums,
/// type aliases) from `package_items` (user + builtin packages).
fn completions_for_type_position(
    db: &dyn Db,
    file: SourceFile,
    _offset: TextSize,
) -> Vec<Completion> {
    let mut items: Vec<Completion> = Vec::new();

    // ── Builtin primitives ────────────────────────────────────────────────────
    for prim in &[
        "int",
        "float",
        "string",
        "bool",
        "null",
        "uint8array",
        "image",
        "audio",
        "video",
        "pdf",
    ] {
        items
            .push(Completion::new(*prim, CompletionKind::Primitive).with_sort(format!("0_{prim}")));
    }

    // ── User package types ────────────────────────────────────────────────────
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let pkg = package_items(db, pkg_id);

    for ns_items in pkg.namespaces.values() {
        for (name, def) in &ns_items.types {
            let (kind, detail) = match def {
                Definition::Class(_) => (CompletionKind::Class, "class"),
                Definition::Enum(_) => (CompletionKind::Enum, "enum"),
                Definition::TypeAlias(_) => (CompletionKind::TypeAlias, "type alias"),
                _ => continue,
            };
            items.push(
                Completion::new(name.as_str(), kind)
                    .with_detail(detail)
                    .with_sort(format!("1_{}", name.as_str())),
            );
        }
    }

    // ── Builtin package types (baml, env) ─────────────────────────────────────
    for builtin_pkg in &["baml", "env"] {
        let builtin_id = PackageId::new(db, Name::new(builtin_pkg));
        let builtin = package_items(db, builtin_id);
        for ns_items in builtin.namespaces.values() {
            for (name, def) in &ns_items.types {
                let (kind, detail) = match def {
                    Definition::Class(_) => (CompletionKind::Class, "builtin class"),
                    Definition::Enum(_) => (CompletionKind::Enum, "builtin enum"),
                    Definition::TypeAlias(_) => (CompletionKind::TypeAlias, "builtin type"),
                    _ => continue,
                };
                items.push(
                    Completion::new(name.as_str(), kind)
                        .with_detail(format!("{builtin_pkg}.{}", name.as_str()))
                        .with_sort(format!("2_{}", name.as_str())),
                );
                let _ = detail; // suppress unused warning
            }
        }
    }

    items
}

// ── Field-access completions ──────────────────────────────────────────────────
/// Completions after a `.` — fields, methods, or enum variants.
///
/// When the token is a DOT (cursor right after `.`), finds the base WORD
/// before the DOT in the CST and resolves it. When the token is a WORD
/// after a DOT, uses the existing `find_base_for_field_access` logic.
fn completions_for_field_access(
    db: &dyn Db,
    file: SourceFile,
    token: &baml_compiler_syntax::SyntaxToken,
    offset: TextSize,
) -> Vec<Completion> {
    // Collect all path segments before the cursor dot.
    let segments = if token.kind() == SyntaxKind::DOT {
        find_path_segments_before_dot(token)
    } else {
        // WORD after a dot — collect segments before the preceding dot.
        find_path_segments_for_word_after_dot(token)
    };

    if segments.is_empty() {
        return Vec::new();
    }

    // Resolve root segment type.
    let root = Name::new(&segments[0]);
    let resolved = baml_compiler2_ppir::resolve::resolve_name_at(db, file, offset, &root);

    let mut ty = match resolved {
        baml_compiler2_ppir::resolve::ResolvedName::Item(def)
        | baml_compiler2_ppir::resolve::ResolvedName::Builtin(def) => definition_to_ty(db, def),
        baml_compiler2_ppir::resolve::ResolvedName::Local {
            definition_site: Some(site),
            ..
        } => local_variable_ty(db, file, offset, site),
        _ => None,
    };

    // If the root didn't resolve to a type, check if it's a package namespace.
    // This handles cases like `baml.` where `baml` is a package name, not a value.
    if ty.is_none() {
        if let Some(completions) = completions_for_package_path(db, file, &segments) {
            return completions;
        }

        if segments.len() == 1 {
            if let Some(class_path) = builtin_static_class_path_for_root(&segments[0]) {
                return completions_for_builtin_class_methods(
                    db,
                    class_path,
                    BuiltinMethodMode::Static,
                );
            }
        }
    }

    // Chain through intermediate segments to get the type at the last segment.
    for seg in &segments[1..] {
        ty = ty.and_then(|t| resolve_field_type(db, &t, seg));
    }

    ty.map(|t| completions_for_ty_members(db, file, &t))
        .unwrap_or_default()
}

/// Completions for package namespace paths like `baml.`, `log.`, etc.
///
/// When the first segment is a known package name, we provide completions for
/// items within that package's namespace. This handles cases where the path
/// doesn't resolve to a type (e.g., `baml` is a package, not a value).
fn completions_for_package_path(
    db: &dyn Db,
    file: SourceFile,
    segments: &[String],
) -> Option<Vec<Completion>> {
    if segments.is_empty() {
        return None;
    }

    // Get the file's package context to access dependency packages.
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let own_pkg_id = PackageId::new(db, pkg_info.package);
    let res_ctx =
        baml_compiler2_hir_ty::package_interface::package_resolution_context(db, own_pkg_id);

    // Check if the first segment is a known package name. The BEP-066 keyword
    // shorthands (`reflect.` ≡ `baml.reflect.`, `type.` ≡ `baml.type.`)
    // complete as the `baml` namespaces they alias; a real package of that
    // name wins.
    let first_segment = Name::new(&segments[0]);
    let (pkg_items, namespace_path): (_, Vec<Name>) =
        match res_ctx.items_for_package(db, &first_segment) {
            Some(items) => (items, segments[1..].iter().map(Name::new).collect()),
            None if matches!(segments[0].as_str(), "reflect" | "type") => {
                let baml_items = res_ctx.items_for_package(db, &Name::new("baml"))?;
                (baml_items, segments.iter().map(Name::new).collect())
            }
            None => return None,
        };

    // The namespace path within the package.
    // For `baml.` we have segments=["baml"], so namespace_path=[].
    // For `baml.events.` we have segments=["baml", "events"], so namespace_path=["events"].

    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. Add items from the exact namespace (if it exists).
    if let Some(ns_items) = pkg_items.namespaces.get(&namespace_path) {
        for (name, def) in &ns_items.values {
            let (kind, detail): (CompletionKind, String) = match def {
                Definition::Function(func_loc) => (
                    CompletionKind::Function,
                    format_function_signature(db, *func_loc),
                ),
                Definition::Client(_) => (CompletionKind::Client, "client".to_string()),
                Definition::RetryPolicy(_) => {
                    (CompletionKind::RetryPolicy, "retry_policy".to_string())
                }
                _ => continue,
            };
            if seen.insert(name.as_str().to_string()) {
                items.push(
                    Completion::new(name.as_str(), kind)
                        .with_detail(detail)
                        .with_sort(format!("0_{}", name.as_str())),
                );
            }
        }
        for (name, def) in &ns_items.types {
            let (kind, detail) = match def {
                Definition::Class(_) => (CompletionKind::Class, "class"),
                Definition::Enum(_) => (CompletionKind::Enum, "enum"),
                Definition::TypeAlias(_) => (CompletionKind::TypeAlias, "type"),
                _ => continue,
            };
            if seen.insert(name.as_str().to_string()) {
                items.push(
                    Completion::new(name.as_str(), kind)
                        .with_detail(detail)
                        .with_sort(format!("1_{}", name.as_str())),
                );
            }
        }
    }

    // 2. Add child namespace names (sub-namespaces that extend our path).
    for ns_path in pkg_items.namespaces.keys() {
        // Check if this namespace is a child of our current path.
        if ns_path.len() > namespace_path.len() {
            let is_child = ns_path[..namespace_path.len()]
                .iter()
                .zip(namespace_path.iter())
                .all(|(a, b)| a == b);

            if is_child {
                // The next segment after our namespace_path is a child namespace.
                let child_name = &ns_path[namespace_path.len()];
                if seen.insert(child_name.as_str().to_string()) {
                    items.push(
                        Completion::new(child_name.as_str(), CompletionKind::Module)
                            .with_detail("namespace")
                            .with_sort(format!("0_{}", child_name.as_str())),
                    );
                }
            }
        }
    }

    if let Some(class_completions) = completions_for_package_class_path(db, pkg_items, segments) {
        for completion in class_completions {
            if seen.insert(completion.label.clone()) {
                items.push(completion);
            }
        }
    }

    if items.is_empty() { None } else { Some(items) }
}

fn completions_for_package_class_path(
    db: &dyn Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    segments: &[String],
) -> Option<Vec<Completion>> {
    if segments.len() < 2 {
        return None;
    }

    let path: Vec<Name> = segments[1..].iter().map(Name::new).collect();
    let (class_name, namespace) = path.split_last()?;
    let Some(Definition::Class(class_loc)) = pkg_items.lookup_type(namespace, class_name) else {
        return None;
    };

    Some(completions_for_class_methods(
        db,
        class_loc,
        BuiltinMethodMode::All,
    ))
}

fn builtin_static_class_path_for_root(root: &str) -> Option<&'static [&'static str]> {
    match root {
        "image" => Some(&["media", "Image"]),
        "audio" => Some(&["media", "Audio"]),
        "video" => Some(&["media", "Video"]),
        "pdf" => Some(&["media", "Pdf"]),
        "string" => Some(&["String"]),
        _ => None,
    }
}

fn builtin_instance_class_path_for_primitive(ty: &Ty) -> Option<&'static [&'static str]> {
    match ty {
        Ty::String { .. } | Ty::Literal(baml_base::Literal::String(_), _, _) => {
            Some(PrimitiveType::String.builtin_class_path())
        }
        Ty::Uint8Array { .. } => Some(PrimitiveType::Uint8Array.builtin_class_path()),
        Ty::Media(MediaKind::Image, _) => Some(PrimitiveType::Image.builtin_class_path()),
        Ty::Media(MediaKind::Audio, _) => Some(PrimitiveType::Audio.builtin_class_path()),
        Ty::Media(MediaKind::Video, _) => Some(PrimitiveType::Video.builtin_class_path()),
        Ty::Media(MediaKind::Pdf, _) => Some(PrimitiveType::Pdf.builtin_class_path()),
        _ => None,
    }
}

fn method_has_self_param(db: &dyn Db, func_loc: FunctionLoc<'_>) -> bool {
    function_signature(db, func_loc)
        .params
        .first()
        .is_some_and(|param| param.name.as_str() == "self")
}

fn completions_for_builtin_class_methods(
    db: &dyn Db,
    class_path: &[&str],
    mode: BuiltinMethodMode,
) -> Vec<Completion> {
    if class_path.is_empty() {
        return Vec::new();
    }

    let builtin_id = PackageId::new(db, Name::new("baml"));
    let builtin = package_items(db, builtin_id);
    let path: Vec<Name> = class_path.iter().map(Name::new).collect();
    let Some((class_name, namespace)) = path.split_last() else {
        return Vec::new();
    };
    let Some(Definition::Class(class_loc)) = builtin.lookup_type(namespace, class_name) else {
        return Vec::new();
    };

    completions_for_class_methods(db, class_loc, mode)
}

fn completions_for_class_methods(
    db: &dyn Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'_>,
    mode: BuiltinMethodMode,
) -> Vec<Completion> {
    let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
    let mut items = Vec::new();

    for &func_loc in &class_data.methods {
        let has_self = method_has_self_param(db, func_loc);
        if mode != BuiltinMethodMode::All
            && !matches!(
                (mode, has_self),
                (BuiltinMethodMode::Instance, true) | (BuiltinMethodMode::Static, false)
            )
        {
            continue;
        }

        let method = baml_compiler2_ppir::item_data::function_data(db, func_loc);
        items.push(
            Completion::new(method.name.as_str(), CompletionKind::Method)
                .with_detail(format_builtin_method_signature(db, func_loc, mode))
                .with_sort(format!("1_{}", method.name.as_str())),
        );
    }

    items
}

/// Resolve the type of a field/member on a given type.
///
/// For a `Ty::Class`, looks up resolved class fields and returns the field's type.
fn resolve_field_type(db: &dyn Db, ty: &Ty, field_name: &str) -> Option<Ty> {
    match ty {
        Ty::Class(qn, type_args, _) => {
            let pkg_info_name = qn.package().as_str();
            let pkg_id = PackageId::new(db, Name::new(pkg_info_name));
            let pkg = package_items(db, pkg_id);

            let class_def = pkg.lookup_type(qn.namespace(), qn.name());
            let Definition::Class(class_loc) = class_def? else {
                return None;
            };

            let class_generic_params =
                baml_compiler2_hir_ty::lower::class_generic_frame(db, class_loc);
            let bindings = baml_type::unify::bind_type_vars(&class_generic_params, type_args);
            let resolved = baml_compiler2_hir_ty::lower::resolve_class_fields(db, class_loc);
            for (name, field_ty, _) in resolved {
                if name.as_str() == field_name {
                    return Some(baml_type::unify::substitute_ty(field_ty, &bindings));
                }
            }
            None
        }
        _ => None,
    }
}

/// Collect all WORD tokens in the parent node up to (not including) the given DOT token.
///
/// For `o.inner.` this returns `["o", "inner"]`.
fn find_path_segments_before_dot(dot_token: &baml_compiler_syntax::SyntaxToken) -> Vec<String> {
    let Some(parent) = dot_token.parent() else {
        return Vec::new();
    };
    let mut segments = Vec::new();
    for child in parent.children_with_tokens() {
        match &child {
            NodeOrToken::Token(t) => {
                if t == dot_token {
                    break;
                }
                if t.kind() == SyntaxKind::WORD {
                    segments.push(t.text().to_string());
                }
            }
            NodeOrToken::Node(n) => {
                if n.text_range().end() <= dot_token.text_range().start() {
                    for d in n.descendants_with_tokens() {
                        if let NodeOrToken::Token(t) = d {
                            if t.kind() == SyntaxKind::WORD {
                                segments.push(t.text().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    segments
}

/// Collect all WORD segments before the dot that precedes the current WORD token.
///
/// For `o.inner.na` (cursor at `na`), this returns `["o", "inner"]`.
fn find_path_segments_for_word_after_dot(token: &baml_compiler_syntax::SyntaxToken) -> Vec<String> {
    let Some(parent) = token.parent() else {
        return Vec::new();
    };
    // Find the DOT preceding this WORD token.
    let mut dot_pos: Option<text_size::TextSize> = None;
    for child in parent.children_with_tokens() {
        if let NodeOrToken::Token(t) = &child {
            if t.kind() == SyntaxKind::DOT && t.text_range().end() <= token.text_range().start() {
                dot_pos = Some(t.text_range().start());
            }
        }
    }
    let Some(dot_pos) = dot_pos else {
        return Vec::new();
    };
    // Collect all WORD tokens before the dot.
    let mut segments = Vec::new();
    for child in parent.children_with_tokens() {
        match &child {
            NodeOrToken::Token(t) => {
                if t.text_range().start() >= dot_pos {
                    break;
                }
                if t.kind() == SyntaxKind::WORD {
                    segments.push(t.text().to_string());
                }
            }
            NodeOrToken::Node(n) => {
                if n.text_range().end() <= dot_pos {
                    for d in n.descendants_with_tokens() {
                        if let NodeOrToken::Token(t) = d {
                            if t.kind() == SyntaxKind::WORD {
                                segments.push(t.text().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    segments
}

/// Returns completions for the members of `ty`.
fn completions_for_ty_members(db: &dyn Db, file: SourceFile, ty: &Ty) -> Vec<Completion> {
    match ty {
        Ty::Class(qn, type_args, _) => {
            // Find the class definition and return its fields and methods.
            let pkg_info_name = qn.package().as_str();
            let pkg_id = PackageId::new(db, Name::new(pkg_info_name));
            let pkg = package_items(db, pkg_id);

            let class_def = pkg.lookup_type(qn.namespace(), qn.name());
            let Some(Definition::Class(class_loc)) = class_def else {
                return Vec::new();
            };

            let mut items = Vec::new();

            let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            let class_generic_params =
                baml_compiler2_hir_ty::lower::class_generic_frame(db, class_loc);
            let bindings = baml_type::unify::bind_type_vars(&class_generic_params, type_args);

            // Fields from resolved class fields, specialized for the receiver's
            // concrete type arguments.
            let resolved = baml_compiler2_hir_ty::lower::resolve_class_fields(db, class_loc);
            for (field_name, field_ty, _field_attrs) in resolved {
                let field_ty = baml_type::unify::substitute_ty(field_ty, &bindings);
                items.push(
                    Completion::new(field_name.as_str(), CompletionKind::Field)
                        .with_detail(utils::display_ty_for_file(db, file, &field_ty))
                        .with_sort(format!("0_{}", field_name.as_str())),
                );
            }

            // Methods from the class's firewall data.
            for &func_loc in &class_data.methods {
                let method = baml_compiler2_ppir::item_data::function_data(db, func_loc);
                items.push(
                    Completion::new(method.name.as_str(), CompletionKind::Method)
                        .with_detail("method")
                        .with_sort(format!("1_{}", method.name.as_str())),
                );
            }

            items
        }

        Ty::Enum(qn, _) => {
            // Find the enum and return its variants.
            let pkg_id = PackageId::new(db, Name::new(qn.package().as_str()));
            let pkg = package_items(db, pkg_id);

            let enum_def = pkg.lookup_type(qn.namespace(), qn.name());
            let Some(Definition::Enum(enum_loc)) = enum_def else {
                return Vec::new();
            };

            let enum_data = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);

            enum_data
                .variants
                .iter()
                .map(|v| {
                    Completion::new(v.name.as_str(), CompletionKind::EnumVariant)
                        .with_sort(format!("0_{}", v.name.as_str()))
                })
                .collect()
        }

        Ty::List(..) | Ty::EvolvingList(..) => {
            completions_for_builtin_class_methods(db, &["Array"], BuiltinMethodMode::Instance)
        }

        Ty::Map { .. } | Ty::EvolvingMap(..) => {
            completions_for_builtin_class_methods(db, &["Map"], BuiltinMethodMode::Instance)
        }

        Ty::String { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(_, _)
        | Ty::Literal(baml_base::Literal::String(_), _, _) => {
            builtin_instance_class_path_for_primitive(ty)
                .map(|class_path| {
                    completions_for_builtin_class_methods(
                        db,
                        class_path,
                        BuiltinMethodMode::Instance,
                    )
                })
                .unwrap_or_default()
        }

        _ => Vec::new(),
    }
}

/// Convert a `Definition` to its representative `Ty`.
///
/// Used by field-access completions to determine what fields/variants are
/// available on an item reference (e.g., `MyEnum.` → enum variants).
fn definition_to_ty(db: &dyn Db, def: Definition<'_>) -> Option<Ty> {
    match def {
        Definition::Class(class_loc) => {
            let class = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, class_loc.file(db));
            Some(Ty::Class(
                baml_type::QualifiedTypeName::new(
                    pkg_info.package,
                    pkg_info.namespace_path,
                    class.name.clone(),
                ),
                vec![],
                TyAttr::default(),
            ))
        }
        Definition::Enum(enum_loc) => {
            let enum_data = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, enum_loc.file(db));
            Some(Ty::Enum(
                baml_type::QualifiedTypeName::new(
                    pkg_info.package,
                    pkg_info.namespace_path,
                    enum_data.name.clone(),
                ),
                TyAttr::default(),
            ))
        }
        _ => None,
    }
}

/// Look up the type of a local variable (let binding or parameter) at a scope position.
///
/// For `Statement` bindings (let), searches all scopes in the file for the
/// binding type — this handles variables inside lambdas (test bodies, closures)
/// where the enclosing function is a synthesized `$init_test` wrapper.
fn local_variable_ty(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    site: baml_compiler2_hir::semantic_index::DefinitionSite,
) -> Option<Ty> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);

    match site {
        baml_compiler2_hir::semantic_index::DefinitionSite::Parameter(param_idx) => {
            // Get declared type from function or lambda signature.
            let scope_id = index.scope_at_offset(at_offset, None);
            let ancestors = index.ancestor_scopes(scope_id);

            // Find the nearest Function or Lambda scope.
            let enclosing_scope = ancestors.iter().find(|ancestor_id| {
                matches!(
                    index.scopes[ancestor_id.index() as usize].kind,
                    ScopeKind::Function | ScopeKind::Lambda
                )
            })?;
            let enclosing_scope_data = &index.scopes[enclosing_scope.index() as usize];

            match enclosing_scope_data.kind {
                ScopeKind::Function => {
                    // Function parameter — look up from function signature.
                    let func_scope_range = enclosing_scope_data.range;
                    let func_loc =
                        crate::utils::function_at_scope_range(db, file, func_scope_range)?;
                    let sig = baml_compiler2_hir_ty::lower::function_signature(db, func_loc);
                    sig.params.get(param_idx).map(|param| param.ty.to_plain())
                }
                ScopeKind::Lambda => {
                    // Lambda parameter — use TIR inference for the lambda scope
                    // to get the inferred param type (handles both annotated and
                    // unannotated params like those in `.map((item) -> { ... })`).
                    let lambda_scope_id = index.scope_ids[enclosing_scope.index() as usize];
                    let body = baml_compiler2_hir_ty::ide::scope_body(db, lambda_scope_id)?;
                    let lambda_expr = body.scope_expr?;
                    let inference = baml_compiler2_hir_ty::infer::infer_body(db, body.owner);
                    match inference.type_of_expr.get(&lambda_expr)?.to_plain() {
                        baml_type::Ty::Function { params, .. } => {
                            params.get(param_idx).map(|param| param.ty.clone())
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        baml_compiler2_hir::semantic_index::DefinitionSite::Statement(_) => {
            // Handles variables declared inside lambdas (test bodies,
            // closures) as well as directly in the function body — both index
            // the same arena.
            find_binding_ty_for_local(db, file, at_offset, site)
        }
        baml_compiler2_hir::semantic_index::DefinitionSite::PatternBinding(_)
        | baml_compiler2_hir::semantic_index::DefinitionSite::CatchBinding(_) => {
            find_binding_ty_for_local(db, file, at_offset, site)
        }
    }
}

/// Find the binding type for a local variable by locating the correct `ExprBody`
/// (which may be a nested lambda body for test/testset code) and looking up the
/// binding type from TIR inference.
///
/// For `Statement` bindings, the `StmtId` indexes the enclosing function's
/// `ExprBody`. Lambda bodies are lowered into that same arena, so the statement
/// resolves against it however deeply the cursor sits inside nested lambdas.
fn find_binding_ty_for_local(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    site: baml_compiler2_hir::semantic_index::DefinitionSite,
) -> Option<Ty> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);

    let pat_id = match site {
        baml_compiler2_hir::semantic_index::DefinitionSite::Statement(stmt_id) => {
            // Walk out to the enclosing Function scope, which owns the arena
            // every statement in it — lambda bodies included — lives in.
            let scope_id = index.scope_at_offset(at_offset, None);
            let ancestors = index.ancestor_scopes(scope_id);

            let mut func_range: Option<text_size::TextRange> = None;
            for ancestor_id in &ancestors {
                let s = &index.scopes[ancestor_id.index() as usize];
                if s.kind == ScopeKind::Function {
                    func_range = Some(s.range);
                    break;
                }
            }

            // 2. Start from the outermost Function scope → find its body.
            let func_range = func_range?;
            let func_loc = crate::utils::function_at_scope_range(db, file, func_range)?;
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let baml_compiler2_hir::body::FunctionBody::Expr(ref top_body) = *body else {
                return None;
            };

            extract_pat_from_stmt(top_body, stmt_id)
        }
        baml_compiler2_hir::semantic_index::DefinitionSite::PatternBinding(pat_id)
        | baml_compiler2_hir::semantic_index::DefinitionSite::CatchBinding(pat_id) => Some(pat_id),
        baml_compiler2_hir::semantic_index::DefinitionSite::Parameter(_) => None,
    };

    let pat_id = pat_id?;

    // Search ancestor scopes (innermost first) for the binding type.
    // We must NOT search all scopes because `PatId` is an arena index that
    // can collide across different `ExprBody` arenas (e.g., `PatId(0)` in
    // the test body vs `PatId(0)` in the testset body).
    let cursor_scope = index.scope_at_offset(at_offset, None);
    for ancestor_id in index.ancestor_scopes(cursor_scope) {
        let scope_id = index.scope_ids[ancestor_id.index() as usize];
        let Some(inference) = baml_compiler2_hir_ty::ide::infer_for_scope(db, scope_id) else {
            continue;
        };
        if let Some(ty) = inference.type_of_pat.get(&pat_id) {
            return Some(ty.to_plain());
        }
    }
    None
}

/// Extract `PatId` from a `StmtId` in a specific `ExprBody`.
fn extract_pat_from_stmt(
    expr_body: &baml_compiler2_ast::ExprBody,
    stmt_id: baml_compiler2_ast::StmtId,
) -> Option<baml_compiler2_ast::PatId> {
    let stmt = &expr_body.stmts[stmt_id];
    match stmt {
        baml_compiler2_ast::Stmt::Let { pattern, .. }
        | baml_compiler2_ast::Stmt::For {
            binding: pattern, ..
        } => Some(*pattern),
        _ => None,
    }
}

// ── Value-position completions ────────────────────────────────────────────────

/// Completions for a value expression position (inside a function body).
///
/// Suggests: local variables in scope (innermost first), then all package-level
/// functions and template strings.
fn completions_for_value_position(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
) -> Vec<Completion> {
    let mut items: Vec<Completion> = Vec::new();
    let mut sort_prefix = 0usize;

    // ── Locals (innermost scope first) ───────────────────────────────────────
    {
        let index = baml_compiler2_hir::file_semantic_index(db, file);
        let scope_id = index.scope_at_offset(offset, None);

        let mut emitted_locals: HashSet<Name> = HashSet::new();
        for ancestor_id in index.ancestor_scopes(scope_id) {
            let bindings: &ScopeBindings = &index.scope_bindings[ancestor_id.index() as usize];

            // Let bindings (reverse source order so most-recent is first).
            for binding in bindings.bindings.iter().rev() {
                // Only show bindings that are visible at the cursor position.
                if index.binding_visible_at(binding, offset)
                    && emitted_locals.insert(binding.name.clone())
                {
                    items.push(
                        Completion::new(binding.name.as_str(), CompletionKind::Variable)
                            .with_sort(format!("{:03}_{}", sort_prefix, binding.name.as_str())),
                    );
                    sort_prefix += 1;
                }
            }

            // Parameters.
            for (name, _idx) in &bindings.params {
                if !emitted_locals.insert(name.clone()) {
                    continue;
                }
                items.push(
                    Completion::new(name.as_str(), CompletionKind::Variable)
                        .with_detail("parameter")
                        .with_sort(format!("{:03}_{}", sort_prefix, name.as_str())),
                );
                sort_prefix += 1;
            }
        }
    }

    // ── Accessible package roots (`baml`, `log`, etc.) ────────────────────────
    for package_name in crate::listing::non_user_package_names(db) {
        items.push(
            Completion::new(package_name.as_str(), CompletionKind::Module)
                .with_detail("package")
                .with_sort(format!("{:03}_{}", sort_prefix + 500, package_name)),
        );
    }
    // The BEP-066 keyword shorthand `reflect` (≡ `baml.reflect`) completes like
    // a package root even though it is a namespace of `baml`. (`type` is not
    // offered bare: as an expression head it only carries `of`/`of_value`, and
    // a bare `type` completion would collide with the primitive type name.)
    items.push(
        Completion::new("reflect", CompletionKind::Module)
            .with_detail("baml.reflect")
            .with_sort(format!("{:03}_reflect", sort_prefix + 500)),
    );

    // ── Package-level values (functions, template strings, clients) ───────────
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let pkg = package_items(db, pkg_id);

    let local_sort_base = sort_prefix + 1000;

    for ns_items in pkg.namespaces.values() {
        for (name, def) in &ns_items.values {
            let (kind, detail): (CompletionKind, String) = match def {
                Definition::Function(func_loc) => (
                    CompletionKind::Function,
                    format_function_signature(db, *func_loc),
                ),
                Definition::Client(_) => (CompletionKind::Client, "client".to_string()),
                Definition::RetryPolicy(_) => {
                    (CompletionKind::RetryPolicy, "retry_policy".to_string())
                }
                Definition::Let(loc) => {
                    match baml_compiler2_ppir::item_data::let_data(db, *loc).origin {
                        baml_compiler2_ast::ast::LetOrigin::Client => {
                            (CompletionKind::Client, "client".to_string())
                        }
                        baml_compiler2_ast::ast::LetOrigin::RetryPolicy => {
                            (CompletionKind::RetryPolicy, "retry_policy".to_string())
                        }
                        baml_compiler2_ast::ast::LetOrigin::Source => continue,
                    }
                }
                _ => continue,
            };
            items.push(
                Completion::new(name.as_str(), kind)
                    .with_detail(detail)
                    .with_sort(format!("{:03}_{}", local_sort_base, name.as_str())),
            );
        }
    }

    // ── Package-level types (for value contexts where types are used) ─────────
    for ns_items in pkg.namespaces.values() {
        for (name, def) in &ns_items.types {
            let (kind, detail) = match def {
                Definition::Class(_) => (CompletionKind::Class, "class"),
                Definition::Enum(_) => (CompletionKind::Enum, "enum"),
                Definition::TypeAlias(_) => (CompletionKind::TypeAlias, "type"),
                _ => continue,
            };
            items.push(
                Completion::new(name.as_str(), kind)
                    .with_detail(detail)
                    .with_sort(format!("{:03}_{}", local_sort_base + 1, name.as_str())),
            );
        }
    }

    items
}

// ── Top-level completions ─────────────────────────────────────────────────────

/// Completions at the top level of a BAML file.
///
/// Suggests declaration keywords that can start a new top-level item.
fn completions_for_top_level() -> Vec<Completion> {
    vec![
        Completion::new("class", CompletionKind::Keyword)
            .with_detail("class declaration")
            .with_snippet("class ${1:Name} {\n  ${2:field} ${3:string}\n  $0\n}")
            .with_sort("00_class"),
        Completion::new("enum", CompletionKind::Keyword)
            .with_detail("enum declaration")
            .with_snippet("enum ${1:Name} {\n  ${2:Value}\n  $0\n}")
            .with_sort("01_enum"),
        Completion::new("function", CompletionKind::Keyword)
            .with_detail("function declaration")
            .with_snippet("function ${1:Name}(${2}) -> ${3:string} {\n  $0\n}")
            .with_sort("02_function"),
        Completion::new("client", CompletionKind::Keyword)
            .with_detail("LLM client declaration")
            .with_snippet(
                "client<llm> ${1:Name} {\n  provider ${2:openai}\n  options {\n    model ${3:gpt-4o}\n  }\n  $0\n}",
            )
            .with_sort("03_client"),
        Completion::new("test", CompletionKind::Keyword)
            .with_detail("test case declaration")
            .with_snippet("test \"${1:test name}\" {\n  $0\n}")
            .with_sort("05_test"),
        Completion::new("retry_policy", CompletionKind::Keyword)
            .with_detail("retry policy declaration")
            .with_snippet("retry_policy ${1:Name} {\n  max_retries ${2:3}\n  $0\n}")
            .with_sort("06_retry_policy"),
        Completion::new("type", CompletionKind::Keyword)
            .with_detail("type alias declaration")
            .with_snippet("type ${1:Name} = ${2:string}$0")
            .with_sort("08_type"),
        Completion::new("interface", CompletionKind::Keyword)
            .with_detail("interface declaration")
            .with_snippet("interface ${1:Name} {\n  $0\n}")
            .with_sort("09_interface"),
        Completion::new("implements", CompletionKind::Keyword)
            .with_detail("out-of-body interface implementation")
            .with_snippet("implements ${1:Interface} for ${2:Class} {\n  $0\n}")
            .with_sort("10_implements"),
    ]
}

// ── Empty file fallback ───────────────────────────────────────────────────────

/// Completions when the file is empty or the cursor is at a position where
/// `find_token_at_offset` returns `None`.
fn completions_at_empty_file(_db: &dyn Db, _file: SourceFile) -> Vec<Completion> {
    completions_for_top_level()
}

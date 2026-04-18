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
//!   `EXPR_FUNCTION_BODY`): suggest local variables in scope, then all
//!   package-level functions and template strings.
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
//! - `file_item_tree(file)[enum_loc.id]` — variants for field-access on enums.

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
use baml_compiler2_tir::ty::Ty;
use rowan::NodeOrToken;
use text_size::TextSize;

use crate::{Db, utils};

/// Format a function signature as `(param1: type1, param2: type2) -> return_type`.
fn format_function_signature(db: &dyn Db, func_loc: FunctionLoc<'_>) -> String {
    let sig = function_signature(db, func_loc);
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|(name, te)| format!("{}: {}", name.as_str(), utils::display_type_expr(te)))
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

    // Walk ancestors to detect the structural context.
    let mut node = token.parent();
    while let Some(current) = node {
        let kind = current.kind();

        match kind {
            // Inside a TYPE_EXPR node → type position.
            SyntaxKind::TYPE_EXPR
            | SyntaxKind::UNION_TYPE
            | SyntaxKind::OPTIONAL_TYPE
            | SyntaxKind::ARRAY_TYPE
            | SyntaxKind::MAP_TYPE
            | SyntaxKind::FUNCTION_TYPE
            | SyntaxKind::PARAMETER
            | SyntaxKind::FIELD => {
                // Only treat as type position if we're in the type annotation part,
                // not the name part. Check if any ancestor is specifically TYPE_EXPR.
                if is_in_type_annotation(&current) {
                    return CompletionContext::TypePosition;
                }
            }

            // Inside an expression function body → value position.
            SyntaxKind::EXPR_FUNCTION_BODY
            | SyntaxKind::EXPR
            | SyntaxKind::BINARY_EXPR
            | SyntaxKind::UNARY_EXPR
            | SyntaxKind::CALL_EXPR
            | SyntaxKind::CALL_ARGS
            | SyntaxKind::PATH_EXPR
            | SyntaxKind::PAREN_EXPR
            | SyntaxKind::BLOCK_EXPR
            | SyntaxKind::IF_EXPR
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
        "int", "float", "string", "bool", "null", "image", "audio", "video", "pdf",
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
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(db, file, offset, &root);

    let mut ty = match resolved {
        baml_compiler2_tir::resolve::ResolvedName::Item(def)
        | baml_compiler2_tir::resolve::ResolvedName::Builtin(def) => definition_to_ty(db, def),
        baml_compiler2_tir::resolve::ResolvedName::Local {
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
    }

    // Chain through intermediate segments to get the type at the last segment.
    for seg in &segments[1..] {
        ty = ty.and_then(|t| resolve_field_type(db, &t, seg));
    }

    ty.map(|t| completions_for_ty_members(db, &t))
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
    let res_ctx = baml_compiler2_tir::package_interface::package_resolution_context(db, own_pkg_id);

    // Check if the first segment is a known package name.
    let first_segment = Name::new(&segments[0]);
    let pkg_items = res_ctx.items_for_package(db, &first_segment)?;

    // The namespace path within the package.
    // For `baml.` we have segments=["baml"], so namespace_path=[].
    // For `baml.events.` we have segments=["baml", "events"], so namespace_path=["events"].
    let namespace_path: Vec<Name> = segments[1..].iter().map(Name::new).collect();

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
                Definition::TemplateString(_) => (
                    CompletionKind::TemplateString,
                    "template_string".to_string(),
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

    if items.is_empty() { None } else { Some(items) }
}

/// Resolve the type of a field/member on a given type.
///
/// For a `Ty::Class`, looks up resolved class fields and returns the field's type.
fn resolve_field_type(db: &dyn Db, ty: &Ty, field_name: &str) -> Option<Ty> {
    match ty {
        Ty::Class(qn, _, _) => {
            let pkg_info_name = qn.package().as_str();
            let pkg_id = PackageId::new(db, Name::new(pkg_info_name));
            let pkg = package_items(db, pkg_id);

            let class_def = pkg.lookup_type(qn.namespace(), qn.name());
            let Definition::Class(class_loc) = class_def? else {
                return None;
            };

            let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
            for (name, field_ty, _) in &resolved.fields {
                if name.as_str() == field_name {
                    return Some(field_ty.clone());
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
fn completions_for_ty_members(db: &dyn Db, ty: &Ty) -> Vec<Completion> {
    match ty {
        Ty::Class(qn, _, _) => {
            // Find the class definition and return its fields and methods.
            let pkg_info_name = qn.package().as_str();
            let pkg_id = PackageId::new(db, Name::new(pkg_info_name));
            let pkg = package_items(db, pkg_id);

            let class_def = pkg.lookup_type(qn.namespace(), qn.name());
            let Some(Definition::Class(class_loc)) = class_def else {
                return Vec::new();
            };

            let mut items = Vec::new();

            // Fields from resolved class fields.
            let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
            for (field_name, field_ty, _field_attrs) in &resolved.fields {
                items.push(
                    Completion::new(field_name.as_str(), CompletionKind::Field)
                        .with_detail(utils::display_ty(field_ty))
                        .with_sort(format!("0_{}", field_name.as_str())),
                );
            }

            // Methods from item tree.
            let item_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
            let class_data = &item_tree[class_loc.id(db)];
            for method_id in &class_data.methods {
                let method = &item_tree[*method_id];
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

            let item_tree = baml_compiler2_hir::file_item_tree(db, enum_loc.file(db));
            let enum_data = &item_tree[enum_loc.id(db)];

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
            // Built-in list methods.
            builtin_list_completions()
        }

        Ty::Map(..) | Ty::EvolvingMap(..) => {
            // Built-in map methods.
            builtin_map_completions()
        }

        Ty::Primitive(baml_compiler2_tir::ty::PrimitiveType::String, _) => {
            // Built-in string methods.
            builtin_string_completions()
        }

        _ => Vec::new(),
    }
}

/// Built-in methods for list types.
fn builtin_list_completions() -> Vec<Completion> {
    vec![
        Completion::new("length", CompletionKind::Method).with_detail("int"),
        Completion::new("map", CompletionKind::Method).with_detail("(f: (T) -> U) -> U[]"),
        Completion::new("filter", CompletionKind::Method).with_detail("(f: (T) -> bool) -> T[]"),
        Completion::new("reduce", CompletionKind::Method)
            .with_detail("(f: (U, T) -> U, init: U) -> U"),
        Completion::new("find", CompletionKind::Method).with_detail("(f: (T) -> bool) -> T?"),
        Completion::new("any", CompletionKind::Method).with_detail("(f: (T) -> bool) -> bool"),
        Completion::new("all", CompletionKind::Method).with_detail("(f: (T) -> bool) -> bool"),
    ]
}

/// Built-in methods for map types.
fn builtin_map_completions() -> Vec<Completion> {
    vec![
        Completion::new("keys", CompletionKind::Method).with_detail("K[]"),
        Completion::new("values", CompletionKind::Method).with_detail("V[]"),
        Completion::new("entries", CompletionKind::Method).with_detail("{ key: K, value: V }[]"),
    ]
}

/// Built-in methods for string types.
fn builtin_string_completions() -> Vec<Completion> {
    vec![
        Completion::new("length", CompletionKind::Method).with_detail("int"),
        Completion::new("upper", CompletionKind::Method).with_detail("string"),
        Completion::new("lower", CompletionKind::Method).with_detail("string"),
        Completion::new("trim", CompletionKind::Method).with_detail("string"),
        Completion::new("split", CompletionKind::Method).with_detail("(sep: string) -> string[]"),
        Completion::new("contains", CompletionKind::Method).with_detail("(sub: string) -> bool"),
        Completion::new("starts_with", CompletionKind::Method)
            .with_detail("(prefix: string) -> bool"),
        Completion::new("ends_with", CompletionKind::Method)
            .with_detail("(suffix: string) -> bool"),
    ]
}

/// Convert a `Definition` to its representative `Ty`.
///
/// Used by field-access completions to determine what fields/variants are
/// available on an item reference (e.g., `MyEnum.` → enum variants).
fn definition_to_ty(db: &dyn Db, def: Definition<'_>) -> Option<Ty> {
    match def {
        Definition::Class(class_loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
            let class = &item_tree[class_loc.id(db)];
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, class_loc.file(db));
            Some(Ty::Class(
                baml_compiler2_tir::ty::QualifiedTypeName::new(
                    pkg_info.package,
                    pkg_info.namespace_path,
                    class.name.clone(),
                ),
                vec![],
                TyAttr::default(),
            ))
        }
        Definition::Enum(enum_loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, enum_loc.file(db));
            let enum_data = &item_tree[enum_loc.id(db)];
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, enum_loc.file(db));
            Some(Ty::Enum(
                baml_compiler2_tir::ty::QualifiedTypeName::new(
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
            let item_tree = baml_compiler2_hir::file_item_tree(db, file);
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
                    let (func_local_id, _) = item_tree
                        .functions
                        .iter()
                        .find(|(_, f)| f.span == func_scope_range)?;
                    let func_loc =
                        baml_compiler2_hir::loc::FunctionLoc::new(db, file, *func_local_id);
                    let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
                    sig.params.get(param_idx).map(|(_, te)| {
                        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                        let pkg_id = PackageId::new(db, pkg_info.package);
                        let pkg = package_items(db, pkg_id);
                        let mut diags = Vec::new();
                        baml_compiler2_tir::lower_type_expr::lower_type_expr(
                            db,
                            te,
                            pkg,
                            &[],
                            &mut diags,
                        )
                    })
                }
                ScopeKind::Lambda => {
                    // Lambda parameter — use TIR inference for the lambda scope
                    // to get the inferred param type (handles both annotated and
                    // unannotated params like those in `.map((item) -> { ... })`).
                    let lambda_scope_id = index.scope_ids[enclosing_scope.index() as usize];
                    let inference =
                        baml_compiler2_tir::inference::infer_scope_types(db, lambda_scope_id);
                    inference.param_type(param_idx).cloned()
                }
                _ => None,
            }
        }
        baml_compiler2_hir::semantic_index::DefinitionSite::Statement(_) => {
            // Search all scopes for the binding type. This handles variables
            // inside lambdas (test bodies, closures) where the variable's
            // StmtId is in a nested ExprBody, not the outer function's body.
            find_binding_ty_for_local(db, file, at_offset, site)
        }
        baml_compiler2_hir::semantic_index::DefinitionSite::PatternBinding(_) => {
            find_binding_ty_for_local(db, file, at_offset, site)
        }
    }
}

/// Find the binding type for a local variable by locating the correct `ExprBody`
/// (which may be a nested lambda body for test/testset code) and looking up the
/// binding type from TIR inference.
///
/// For `Statement` bindings, we walk the scope tree to build a nesting path from
/// the cursor's innermost Lambda scope up to the enclosing Function scope, then
/// descend through the `ExprBody` tree using each body's source map to match
/// lambda expression spans against scope ranges. This ensures we find the correct
/// `ExprBody` even for deeply nested testset/test lambdas, where `func_def.span`
/// may not match the scope range set by the HIR builder.
fn find_binding_ty_for_local(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    site: baml_compiler2_hir::semantic_index::DefinitionSite,
) -> Option<Ty> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);

    let pat_id = match site {
        baml_compiler2_hir::semantic_index::DefinitionSite::Statement(stmt_id) => {
            // 1. Build the scope nesting path from cursor to enclosing Function.
            //    lambda_ranges: innermost-first Lambda scope ranges.
            //    func_range: the enclosing Function scope range.
            let scope_id = index.scope_at_offset(at_offset, None);
            let ancestors = index.ancestor_scopes(scope_id);

            let mut lambda_ranges_rev: Vec<text_size::TextRange> = Vec::new();
            let mut func_range: Option<text_size::TextRange> = None;
            for ancestor_id in &ancestors {
                let s = &index.scopes[ancestor_id.index() as usize];
                match s.kind {
                    ScopeKind::Lambda => lambda_ranges_rev.push(s.range),
                    ScopeKind::Function => {
                        func_range = Some(s.range);
                        break;
                    }
                    _ => {}
                }
            }

            // 2. Start from the outermost Function scope → find its body.
            let func_range = func_range?;
            let (func_local_id, _) = item_tree
                .functions
                .iter()
                .find(|(_, f)| f.span == func_range)?;
            let func_loc = FunctionLoc::new(db, file, *func_local_id);
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let baml_compiler2_hir::body::FunctionBody::Expr(ref top_body) = *body else {
                return None;
            };

            // 3. If cursor is directly in the Function (no lambda nesting), use it.
            if lambda_ranges_rev.is_empty() {
                extract_pat_from_stmt(top_body, stmt_id)
            } else {
                // Get the top-level source map and descend through nested lambdas.
                let top_source_map =
                    baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;

                // Reverse to get outermost→innermost order for descent.
                lambda_ranges_rev.reverse();

                let target_body =
                    descend_into_lambdas(top_body, &top_source_map, &lambda_ranges_rev)?;
                extract_pat_from_stmt(target_body, stmt_id)
            }
        }
        baml_compiler2_hir::semantic_index::DefinitionSite::PatternBinding(pat_id) => Some(pat_id),
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
        let inference = baml_compiler2_tir::inference::infer_scope_types(db, scope_id);
        if let Some(ty) = inference.binding_type(pat_id) {
            return Some(ty.clone());
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
    if let baml_compiler2_ast::Stmt::Let { pattern, .. } = stmt {
        Some(*pattern)
    } else {
        None
    }
}

/// Descend through nested lambda bodies following the given scope ranges.
///
/// `lambda_ranges` is ordered outermost→innermost. At each level, finds the
/// `Expr::Lambda` whose expression span (from the current body's source map)
/// matches the target range, then recurses into that lambda's body. This uses
/// the **same** source map the HIR builder used when creating scope ranges,
/// guaranteeing a match even for deeply nested testset/test lambdas.
fn descend_into_lambdas<'a>(
    body: &'a baml_compiler2_ast::ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    lambda_ranges: &[text_size::TextRange],
) -> Option<&'a baml_compiler2_ast::ExprBody> {
    if lambda_ranges.is_empty() {
        return Some(body);
    }
    let target_range = lambda_ranges[0];
    for (expr_id, expr) in body.exprs.iter() {
        if let baml_compiler2_ast::Expr::Lambda(func_def) = expr {
            let expr_span = source_map.expr_span(expr_id);
            if expr_span == target_range {
                if let Some(baml_compiler2_ast::FunctionBodyDef::Expr(
                    ref nested_body,
                    ref nested_sm,
                )) = func_def.body
                {
                    return descend_into_lambdas(nested_body, nested_sm, &lambda_ranges[1..]);
                }
            }
        }
    }
    None
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

    // ── Locals (innermost scope first) ───────────────────────────────────────
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);

    let mut sort_prefix = 0usize;
    for ancestor_id in index.ancestor_scopes(scope_id) {
        let bindings: &ScopeBindings = &index.scope_bindings[ancestor_id.index() as usize];

        // Let bindings (reverse source order so most-recent is first).
        for (name, _site, binding_range) in bindings.bindings.iter().rev() {
            // Only show bindings that are visible at the cursor position.
            if binding_range.start() <= offset {
                items.push(
                    Completion::new(name.as_str(), CompletionKind::Variable).with_sort(format!(
                        "{:03}_{}",
                        sort_prefix,
                        name.as_str()
                    )),
                );
                sort_prefix += 1;
            }
        }

        // Parameters.
        for (name, _idx) in &bindings.params {
            items.push(
                Completion::new(name.as_str(), CompletionKind::Variable)
                    .with_detail("parameter")
                    .with_sort(format!("{:03}_{}", sort_prefix, name.as_str())),
            );
            sort_prefix += 1;
        }
    }

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
                Definition::TemplateString(_) => (
                    CompletionKind::TemplateString,
                    "template_string".to_string(),
                ),
                Definition::Client(_) => (CompletionKind::Client, "client".to_string()),
                Definition::RetryPolicy(_) => {
                    (CompletionKind::RetryPolicy, "retry_policy".to_string())
                }
                Definition::Let(loc) => {
                    let item_tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
                    match item_tree[loc.id(db)].origin {
                        baml_compiler2_ast::ast::LetOrigin::Client => {
                            (CompletionKind::Client, "client".to_string())
                        }
                        baml_compiler2_ast::ast::LetOrigin::RetryPolicy => {
                            (CompletionKind::RetryPolicy, "retry_policy".to_string())
                        }
                        _ => continue,
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
            .with_sort("00_class"),
        Completion::new("enum", CompletionKind::Keyword)
            .with_detail("enum declaration")
            .with_sort("01_enum"),
        Completion::new("function", CompletionKind::Keyword)
            .with_detail("function declaration")
            .with_sort("02_function"),
        Completion::new("client", CompletionKind::Keyword)
            .with_detail("LLM client declaration")
            .with_sort("03_client"),
        Completion::new("generator", CompletionKind::Keyword)
            .with_detail("code generator declaration")
            .with_sort("04_generator"),
        Completion::new("test", CompletionKind::Keyword)
            .with_detail("test case declaration")
            .with_sort("05_test"),
        Completion::new("retry_policy", CompletionKind::Keyword)
            .with_detail("retry policy declaration")
            .with_sort("06_retry_policy"),
        Completion::new("template_string", CompletionKind::Keyword)
            .with_detail("template string declaration")
            .with_sort("07_template_string"),
        Completion::new("type", CompletionKind::Keyword)
            .with_detail("type alias declaration")
            .with_sort("08_type"),
    ]
}

// ── Empty file fallback ───────────────────────────────────────────────────────

/// Completions when the file is empty or the cursor is at a position where
/// `find_token_at_offset` returns `None`.
fn completions_at_empty_file(_db: &dyn Db, _file: SourceFile) -> Vec<Completion> {
    completions_for_top_level()
}

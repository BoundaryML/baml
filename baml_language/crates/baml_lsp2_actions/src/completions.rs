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

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::{SyntaxKind, SyntaxNode};
use baml_compiler2_hir::{
    contributions::Definition, loc::FunctionLoc, package::PackageId, scope::ScopeKind,
    semantic_index::ScopeBindings,
};
use baml_compiler2_ppir::package_items;
use baml_compiler2_tir::ty::Ty;
use rowan::NodeOrToken;
use text_size::TextSize;

use crate::{Db, utils};

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
    /// Cursor is after a `.` — field/method/variant access on a base expression.
    FieldAccess,
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
    let token = match utils::find_token_at_offset(db, file, offset) {
        Some(t) => t,
        None => return completions_at_empty_file(db, file),
    };

    let context = detect_context(&token, offset);

    match context {
        CompletionContext::TypePosition => completions_for_type_position(db, file, offset),
        CompletionContext::FieldAccess => completions_for_field_access(db, file, &token, offset),
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
        return CompletionContext::FieldAccess;
    }

    // Walk ancestors to detect the structural context.
    let mut node = token.parent();
    while let Some(current) = node {
        let kind = current.kind();

        match SyntaxKind::from(kind) {
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
/// indicating a field access completion context.
fn is_field_access_position(token: &baml_compiler_syntax::SyntaxToken) -> bool {
    // Only WORD tokens can be field names.
    if token.kind() != SyntaxKind::WORD {
        // Also check if cursor is right after a DOT (token is whitespace/empty after dot).
        // Walk the parent and look at siblings.
    }

    // Check previous sibling tokens in the parent node.
    let parent = match token.parent() {
        Some(p) => p,
        None => return false,
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
    if SyntaxKind::from(parent.kind()) == SyntaxKind::PATH_EXPR {
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
        let k = SyntaxKind::from(n.kind());
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
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
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
/// Extracts the base identifier (the `WORD` token before the `.`), resolves
/// it, then returns the appropriate members.
fn completions_for_field_access(
    db: &dyn Db,
    file: SourceFile,
    token: &baml_compiler_syntax::SyntaxToken,
    offset: TextSize,
) -> Vec<Completion> {
    // Find the base expression: the WORD token preceding the `.`.
    let base_name = match find_base_for_field_access(token) {
        Some(name) => name,
        None => return Vec::new(),
    };

    let base = Name::new(&base_name);

    // Resolve the base name in scope.
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(db, file, offset, &base);

    // Resolve the base to a type.
    let ty = match resolved {
        baml_compiler2_tir::resolve::ResolvedName::Item(def)
        | baml_compiler2_tir::resolve::ResolvedName::Builtin(def) => {
            // Item reference — the type is the item itself.
            definition_to_ty(db, def)
        }
        baml_compiler2_tir::resolve::ResolvedName::Local {
            definition_site: Some(site),
            ..
        } => {
            // Local variable — look up inferred type.
            local_variable_ty(db, file, offset, site)
        }
        _ => None,
    };

    let Some(ty) = ty else {
        return Vec::new();
    };

    completions_for_ty_members(db, &ty)
}

/// Returns completions for the members of `ty`.
fn completions_for_ty_members(db: &dyn Db, ty: &Ty) -> Vec<Completion> {
    match ty {
        Ty::Class(qn) => {
            // Find the class definition and return its fields and methods.
            let class_name = Name::new(qn.name.as_str());
            let pkg_info_name = qn.pkg.as_str();
            let pkg_id = PackageId::new(db, Name::new(pkg_info_name));
            let pkg = package_items(db, pkg_id);

            let class_def = pkg.lookup_type(&[class_name]);
            let Some(Definition::Class(class_loc)) = class_def else {
                return Vec::new();
            };

            let mut items = Vec::new();

            // Fields from resolved class fields.
            let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
            for (field_name, field_ty) in &resolved.fields {
                items.push(
                    Completion::new(field_name.as_str(), CompletionKind::Field)
                        .with_detail(utils::display_ty(field_ty))
                        .with_sort(format!("0_{}", field_name.as_str())),
                );
            }

            // Methods from item tree.
            let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
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

        Ty::Enum(qn) => {
            // Find the enum and return its variants.
            let enum_name = Name::new(qn.name.as_str());
            let pkg_id = PackageId::new(db, Name::new(qn.pkg.as_str()));
            let pkg = package_items(db, pkg_id);

            let enum_def = pkg.lookup_type(&[enum_name]);
            let Some(Definition::Enum(enum_loc)) = enum_def else {
                return Vec::new();
            };

            let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
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

        Ty::List(_) | Ty::EvolvingList(_) => {
            // Built-in list methods.
            builtin_list_completions()
        }

        Ty::Map(_, _) | Ty::EvolvingMap(_, _) => {
            // Built-in map methods.
            builtin_map_completions()
        }

        Ty::Primitive(baml_compiler2_tir::ty::PrimitiveType::String) => {
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

/// Find the base identifier before the `.` in a field access.
///
/// Given a token at position `bar` in `foo.bar`, returns `"foo"`.
/// Handles both `PATH_EXPR` (where siblings are in the parent) and
/// `FIELD_ACCESS_EXPR` (where the base is a child node).
fn find_base_for_field_access(token: &baml_compiler_syntax::SyntaxToken) -> Option<String> {
    let parent = token.parent()?;

    // Collect all tokens in the parent that come before a DOT that precedes our token.
    let mut dot_offset: Option<text_size::TextSize> = None;

    for child in parent.children_with_tokens() {
        if let NodeOrToken::Token(t) = &child {
            if t.kind() == SyntaxKind::DOT && t.text_range().end() <= token.text_range().start() {
                dot_offset = Some(t.text_range().start());
            }
        }
    }

    let dot_pos = dot_offset?;

    // Find the last WORD token before the dot.
    let mut base: Option<String> = None;
    for child in parent.children_with_tokens() {
        if let NodeOrToken::Token(t) = &child {
            if t.kind() == SyntaxKind::WORD && t.text_range().end() <= dot_pos {
                base = Some(t.text().to_string());
            }
        }
    }

    // Also check child nodes (for FIELD_ACCESS_EXPR where base is a sub-expression).
    if base.is_none() {
        for child in parent.children_with_tokens() {
            if let NodeOrToken::Node(n) = &child {
                if n.text_range().end() <= dot_pos {
                    // Take the last WORD token inside this sub-node.
                    let last_word = n
                        .descendants_with_tokens()
                        .filter_map(|d| {
                            if let NodeOrToken::Token(t) = d {
                                if t.kind() == SyntaxKind::WORD {
                                    Some(t)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .last();
                    if let Some(w) = last_word {
                        base = Some(w.text().to_string());
                    }
                }
            }
        }
    }

    base
}

/// Convert a `Definition` to its representative `Ty`.
///
/// Used by field-access completions to determine what fields/variants are
/// available on an item reference (e.g., `MyEnum.` → enum variants).
fn definition_to_ty(db: &dyn Db, def: Definition<'_>) -> Option<Ty> {
    match def {
        Definition::Class(class_loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
            let class = &item_tree[class_loc.id(db)];
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, class_loc.file(db));
            Some(Ty::Class(baml_compiler2_tir::ty::QualifiedTypeName {
                pkg: Name::new(pkg_info.package.as_str()),
                name: class.name.clone(),
            }))
        }
        Definition::Enum(enum_loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
            let enum_data = &item_tree[enum_loc.id(db)];
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, enum_loc.file(db));
            Some(Ty::Enum(baml_compiler2_tir::ty::QualifiedTypeName {
                pkg: Name::new(pkg_info.package.as_str()),
                name: enum_data.name.clone(),
            }))
        }
        _ => None,
    }
}

/// Look up the type of a local variable (let binding or parameter) at a scope position.
fn local_variable_ty(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    site: baml_compiler2_hir::semantic_index::DefinitionSite,
) -> Option<Ty> {
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);

    // Find the enclosing Function scope.
    let scope_id = index.scope_at_offset(at_offset);
    let enclosing_func_scope = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                ScopeKind::Function
            )
        })?;

    let func_scope_range = index.scopes[enclosing_func_scope.index() as usize].range;

    // Match scope range to a function in the item tree.
    let (func_local_id, _) = item_tree
        .functions
        .iter()
        .find(|(_, f)| f.span == func_scope_range)?;

    let func_loc = FunctionLoc::new(db, file, *func_local_id);
    let func_scope_salsa_id = index.scope_ids[enclosing_func_scope.index() as usize];
    let inference = baml_compiler2_tir::inference::infer_scope_types(db, func_scope_salsa_id);

    match site {
        baml_compiler2_hir::semantic_index::DefinitionSite::Parameter(param_idx) => {
            // Get declared type from function signature.
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            sig.params.get(param_idx).and_then(|(_, te)| {
                let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                let pkg_id = PackageId::new(db, pkg_info.package.clone());
                let pkg = package_items(db, pkg_id);
                let mut diags = Vec::new();
                Some(baml_compiler2_tir::lower_type_expr::lower_type_expr(
                    db, te, pkg, &mut diags,
                ))
            })
        }
        baml_compiler2_hir::semantic_index::DefinitionSite::Statement(stmt_id) => {
            // Find the PatId from the statement.
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
                return None;
            };
            let stmt = &expr_body.stmts[stmt_id];
            let pat_id = match stmt {
                baml_compiler2_ast::Stmt::Let { pattern, .. } => *pattern,
                _ => return None,
            };
            inference.binding_type(pat_id).cloned().or_else(|| {
                // Try other scopes (nested blocks).
                for scope_id in &index.scope_ids {
                    let inf = baml_compiler2_tir::inference::infer_scope_types(db, *scope_id);
                    if let Some(ty) = inf.binding_type(pat_id) {
                        return Some(ty.clone());
                    }
                }
                None
            })
        }
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

    // ── Locals (innermost scope first) ───────────────────────────────────────
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset);

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
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg = package_items(db, pkg_id);

    let local_sort_base = sort_prefix + 1000;

    for ns_items in pkg.namespaces.values() {
        for (name, def) in &ns_items.values {
            let (kind, detail) = match def {
                Definition::Function(_) => (CompletionKind::Function, "function"),
                Definition::TemplateString(_) => {
                    (CompletionKind::TemplateString, "template_string")
                }
                Definition::Client(_) => (CompletionKind::Client, "client"),
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

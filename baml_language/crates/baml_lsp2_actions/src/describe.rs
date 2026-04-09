//! `describe` — structured symbol description for CLI and agent use.
//!
//! The core `describe()` function takes a symbol name and produces a
//! `SymbolDescription` containing everything needed to understand the symbol:
//! shape (compact representation), full source body, docstring, signature-level
//! dependencies, and reference sites.
//!
//! This is a regular function (not a Salsa query). Internally it calls
//! Salsa-cached queries (`file_outline`, `file_item_tree`, `syntax_tree`, etc.).

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::DefinitionKind;
use baml_compiler_syntax::SyntaxKind;
use serde::Serialize;
use text_size::TextRange;

use crate::Db;
use crate::search::{SymbolInfo, search_symbols};
use crate::type_info::type_info_for_definition;
use crate::usages::usages_at;

// ── Types ────────────────────────────────────────────────────────────────────

/// Complete description of a symbol.
#[derive(Clone, Serialize)]
pub struct SymbolDescription {
    /// The symbol's name.
    pub name: String,
    /// Symbol kind (Class, Enum, Function, …).
    #[serde(serialize_with = "serialize_kind")]
    pub kind: DefinitionKind,
    /// The file where the symbol is defined.
    #[serde(serialize_with = "serialize_file")]
    pub file: SourceFile,
    /// Byte range of the name token.
    #[serde(serialize_with = "serialize_range")]
    pub name_span: TextRange,
    /// Byte range of the full item node in the CST.
    #[serde(serialize_with = "serialize_range")]
    pub item_range: TextRange,
    /// Compact shape representation (e.g. signature + field list).
    pub shape: String,
    /// Complete source text of the definition.
    pub full_body: String,
    /// Leading `///` docstring, if any.
    pub docstring: Option<String>,
    /// Resolved type string, if applicable (e.g. `(a: string) -> int` for functions).
    pub resolved_type: Option<String>,
    /// Symbols referenced in the signature (parameter types, return type, etc.).
    pub dependencies: Vec<DepRef>,
    /// Sites where this symbol is used.
    pub references: Vec<RefSite>,
}

/// A symbol referenced in the signature of another symbol.
#[derive(Clone, Serialize)]
pub struct DepRef {
    pub name: String,
    #[serde(serialize_with = "serialize_kind")]
    pub kind: DefinitionKind,
    #[serde(serialize_with = "serialize_file")]
    pub file: SourceFile,
    #[serde(serialize_with = "serialize_range")]
    pub name_span: TextRange,
}

/// A site where a symbol is used.
#[derive(Clone, Serialize)]
pub struct RefSite {
    #[serde(serialize_with = "serialize_file")]
    pub file: SourceFile,
    #[serde(serialize_with = "serialize_range")]
    pub range: TextRange,
    /// The full source line containing the reference.
    pub line_text: String,
    /// 1-based line number.
    pub line_number: usize,
}

// ── describe ─────────────────────────────────────────────────────────────────

/// Describe a symbol by exact name.
///
/// Searches all `files` for a symbol whose name matches `name` exactly
/// (case-sensitive). Returns descriptions for all matches (there may be
/// multiple if the name appears in different files, though BAML typically
/// has unique top-level names).
///
/// Returns an empty Vec if no symbol with that name exists.
pub fn describe(db: &dyn Db, files: &[SourceFile], name: &str) -> Vec<SymbolDescription> {
    // Find exact matches via search_symbols (which does case-insensitive
    // substring matching), then filter to exact case-sensitive matches.
    let candidates = search_symbols(db, files, name);

    // Prefer top-level symbols; fall back to members (fields, variants) if
    // no top-level match exists; finally try locals (parameters, let bindings).
    let top_level: Vec<&SymbolInfo> = candidates
        .iter()
        .filter(|s| s.name == name && s.container_name.is_none())
        .collect();

    if !top_level.is_empty() {
        return top_level
            .into_iter()
            .filter_map(|sym| describe_symbol(db, files, sym))
            .collect();
    }

    let members: Vec<&SymbolInfo> = candidates
        .iter()
        .filter(|s| s.name == name && s.container_name.is_some())
        .collect();

    if !members.is_empty() {
        return members
            .into_iter()
            .filter_map(|sym| describe_symbol(db, files, sym))
            .collect();
    }

    // No outline match — try locals (parameters and let bindings).
    describe_locals(db, files, name)
}

/// Build a full `SymbolDescription` for a single `SymbolInfo`.
fn describe_symbol(
    db: &dyn Db,
    files: &[SourceFile],
    sym: &SymbolInfo,
) -> Option<SymbolDescription> {
    if sym.kind.is_member() {
        describe_member(db, files, sym)
    } else {
        describe_top_level(db, files, sym)
    }
}

/// Describe a top-level symbol (class, function, enum, etc.).
fn describe_top_level(
    db: &dyn Db,
    files: &[SourceFile],
    sym: &SymbolInfo,
) -> Option<SymbolDescription> {
    let file = sym.file;
    let file_text = file.text(db);

    // ── CST body extraction ──────────────────────────────────────────────────
    let item_range = find_item_range(db, file, sym.name_span, sym.kind)?;
    let full_body = slice_text(file_text, item_range);

    // ── Shape generation ─────────────────────────────────────────────────────
    let shape = build_shape(db, file, sym);

    // ── Docstring extraction ─────────────────────────────────────────────────
    let docstring = extract_docstring(db, file, item_range);

    // ── Dependency discovery ─────────────────────────────────────────────────
    let dependencies = find_dependencies(db, files, file, sym);

    // ── Resolved type ────────────────────────────────────────────────────────
    let resolved_type = resolve_type_for_item(db, file, sym);

    // ── Reference finding ────────────────────────────────────────────────────
    let references = find_references(db, files, file, sym.name_span);

    Some(SymbolDescription {
        name: sym.name.clone(),
        kind: sym.kind,
        file,
        name_span: sym.name_span,
        item_range,
        shape,
        full_body,
        docstring,
        resolved_type,
        dependencies,
        references,
    })
}

/// Describe a member symbol (field, variant, method).
///
/// Shows the member itself, the containing class/enum as a dependency,
/// and references to the member.
fn describe_member(
    db: &dyn Db,
    files: &[SourceFile],
    sym: &SymbolInfo,
) -> Option<SymbolDescription> {
    let file = sym.file;
    let file_text = file.text(db);

    // Find the member's CST node (FIELD or ENUM_VARIANT).
    let member_range = find_member_range(db, file, sym.name_span, sym.kind)?;
    let full_body = slice_text(file_text, member_range);

    // Shape: "field_name type" or "VariantName" with container context.
    let shape = if let Some(ref container) = sym.container_name {
        format!("{}.{}", container, full_body.trim())
    } else {
        full_body.trim().to_string()
    };

    // Docstring from the member's CST node.
    let docstring = extract_docstring(db, file, member_range);

    // The containing class/enum is a dependency.
    let mut dependencies = Vec::new();
    if let Some(ref container_name) = sym.container_name {
        // Find the container symbol.
        let container_results = search_symbols(db, files, container_name);
        if let Some(container) = container_results
            .iter()
            .find(|s| s.name == *container_name && s.container_name.is_none())
        {
            dependencies.push(DepRef {
                name: container.name.clone(),
                kind: container.kind,
                file: container.file,
                name_span: container.name_span,
            });
        }
    }

    // Resolved type for members — look up via the parent class/enum rather than
    // `resolve_name_at`, which only handles top-level `Definition` items.
    let resolved_type = resolve_member_type(db, file, sym);

    // References to this field/variant.
    let references = find_references(db, files, file, sym.name_span);

    Some(SymbolDescription {
        name: sym.name.clone(),
        kind: sym.kind,
        file,
        name_span: sym.name_span,
        item_range: member_range,
        shape,
        full_body,
        docstring,
        resolved_type,
        dependencies,
        references,
    })
}

// ── Local variable lookup ────────────────────────────────────────────────────

/// Find and describe local variables (parameters and let bindings) by name.
///
/// Scans all functions in all files for parameters and let bindings matching
/// `name`. Returns a `SymbolDescription` for each match, with the containing
/// function as a dependency.
fn describe_locals(db: &dyn Db, files: &[SourceFile], name: &str) -> Vec<SymbolDescription> {
    let mut results = Vec::new();

    for &file in files {
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        let index = baml_compiler2_hir::file_semantic_index(db, file);

        for (&func_local_id, func) in &item_tree.functions {
            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, func_local_id);
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);

            // ── Check parameters ─────────────────────────────────────────
            for (param_idx, (param_name, param_type_expr)) in sig.params.iter().enumerate() {
                if param_name.as_str() != name {
                    continue;
                }

                let type_str = crate::utils::display_type_expr(param_type_expr);

                // Find the parameter's source span from the signature source map.
                let param_span = baml_compiler2_hir::signature::function_signature_source_map(
                    db, func_loc,
                )
                .param_spans
                .get(param_idx)
                .copied()
                .unwrap_or_else(|| text_size::TextRange::empty(func.span.start()));

                let func_name = func.name.as_str().to_string();

                // Find usages of this parameter within the function.
                let param_refs = usages_at(db, file, param_span.start())
                    .into_iter()
                    .filter(|loc| !(loc.file == file && loc.range == param_span))
                    .map(|loc| {
                        let text = loc.file.text(db);
                        let (line_number, line_text) = line_at_offset(text, loc.range.start());
                        RefSite {
                            file: loc.file,
                            range: loc.range,
                            line_text,
                            line_number,
                        }
                    })
                    .collect();

                // Get the full function body for context display.
                let file_text = file.text(db);
                let func_body = file_text
                    .get(usize::from(func.span.start())..usize::from(func.span.end()))
                    .unwrap_or("")
                    .to_string();

                results.push(SymbolDescription {
                    name: name.to_string(),
                    kind: DefinitionKind::Parameter,
                    file,
                    name_span: param_span,
                    item_range: func.span,
                    shape: format!("{name}: {type_str}"),
                    full_body: func_body,
                    docstring: None,
                    resolved_type: Some(type_str),
                    dependencies: vec![make_function_dep(db, file, func_local_id, &func_name)],
                    references: param_refs,
                });
            }

            // ── Check let bindings ───────────────────────────────────────
            // Find the function's scope in the semantic index.
            let func_scope_idx = index.scopes.iter().position(|s| {
                matches!(s.kind, baml_compiler2_hir::scope::ScopeKind::Function)
                    && s.name.as_ref() == Some(&func.name)
                    && s.range == func.span
            });

            let Some(func_scope_idx) = func_scope_idx else {
                continue;
            };

            // Collect bindings from the function scope AND all descendant
            // scopes (blocks, match arms, lambdas, etc.) so we don't miss
            // let bindings nested inside control flow.
            let func_scope = &index.scopes[func_scope_idx];
            let scope_indices = std::iter::once(func_scope_idx).chain(
                (func_scope.descendants.start.index()..func_scope.descendants.end.index())
                    .map(|i| i as usize),
            );

            for scope_idx in scope_indices {
                let bindings = &index.scope_bindings[scope_idx];

                for (binding_name, def_site, binding_span) in &bindings.bindings {
                    if binding_name.as_str() != name {
                        continue;
                    }

                    // Use the binding's own scope for type inference — this
                    // handles lambdas that get separate inference, while
                    // block-scoped lets fall back to the function's inference.
                    let scope_id = index.scope_ids[scope_idx];
                    let inference =
                        baml_compiler2_tir::inference::infer_scope_types(db, scope_id);

                    let type_str = match def_site {
                        baml_compiler2_hir::semantic_index::DefinitionSite::Statement(stmt_id) => {
                            let body = baml_compiler2_hir::body::function_body(db, func_loc);
                            if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                                if let baml_compiler2_ast::Stmt::Let { pattern, .. } = &expr_body.stmts[*stmt_id] {
                                    inference
                                        .binding_type(*pattern)
                                        .map(crate::utils::display_ty)
                                        .unwrap_or_else(|| "unknown".to_string())
                                } else {
                                    "unknown".to_string()
                                }
                            } else {
                                "unknown".to_string()
                            }
                        }
                        baml_compiler2_hir::semantic_index::DefinitionSite::PatternBinding(pat_id) => {
                            inference
                                .binding_type(*pat_id)
                                .map(crate::utils::display_ty)
                                .unwrap_or_else(|| "unknown".to_string())
                        }
                        baml_compiler2_hir::semantic_index::DefinitionSite::Parameter(_) => {
                            "unknown".to_string()
                        }
                    };

                    let func_name = func.name.as_str().to_string();

                    let binding_refs = usages_at(db, file, binding_span.start())
                        .into_iter()
                        .filter(|loc| !(loc.file == file && loc.range == *binding_span))
                        .map(|loc| {
                            let text = loc.file.text(db);
                            let (line_number, line_text) = line_at_offset(text, loc.range.start());
                            RefSite {
                                file: loc.file,
                                range: loc.range,
                                line_text,
                                line_number,
                            }
                        })
                        .collect();

                    let file_text = file.text(db);
                    let func_body = file_text
                        .get(usize::from(func.span.start())..usize::from(func.span.end()))
                        .unwrap_or("")
                        .to_string();

                    results.push(SymbolDescription {
                        name: name.to_string(),
                        kind: DefinitionKind::Binding,
                        file,
                        name_span: *binding_span,
                        item_range: func.span,
                        shape: format!("let {name}: {type_str}"),
                        full_body: func_body,
                        docstring: None,
                        resolved_type: Some(type_str),
                        dependencies: vec![make_function_dep(db, file, func_local_id, &func_name)],
                        references: binding_refs,
                    });
                }
            }
        }
    }

    results
}

/// Build a `DepRef` pointing to a function.
fn make_function_dep(
    db: &dyn crate::Db,
    file: SourceFile,
    func_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
    func_name: &str,
) -> DepRef {
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let source_map = baml_compiler2_hir::file_item_tree_source_map(db, file);
    let name_span = source_map
        .function_name_spans
        .get(&func_id)
        .copied()
        .unwrap_or_else(|| item_tree[func_id].span);
    DepRef {
        name: func_name.to_string(),
        kind: DefinitionKind::Function,
        file,
        name_span,
    }
}

// ── CST body extraction ──────────────────────────────────────────────────────

/// Map from `DefinitionKind` to the set of `SyntaxKinds` that represent item-level
/// CST nodes for that kind.
fn is_item_node(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CLASS_DEF
            | SyntaxKind::FUNCTION_DEF
            | SyntaxKind::ENUM_DEF
            | SyntaxKind::CLIENT_DEF
            | SyntaxKind::GENERATOR_DEF
            | SyntaxKind::TEST_DEF
            | SyntaxKind::TEST_EXPR_DEF
            | SyntaxKind::TESTSET_DEF
            | SyntaxKind::RETRY_POLICY_DEF
            | SyntaxKind::TEMPLATE_STRING_DEF
            | SyntaxKind::TYPE_ALIAS_DEF
    )
}

/// Find the text range of a member CST node (FIELD, `ENUM_VARIANT`) for a name span.
fn find_member_range(
    db: &dyn Db,
    file: SourceFile,
    name_span: TextRange,
    kind: DefinitionKind,
) -> Option<TextRange> {
    let target_syntax_kind = match kind {
        DefinitionKind::Field => SyntaxKind::FIELD,
        DefinitionKind::Variant => SyntaxKind::ENUM_VARIANT,
        _ => return None,
    };

    let tree = baml_compiler_parser::syntax_tree(db, file);

    let token = match tree.token_at_offset(name_span.start()) {
        rowan::TokenAtOffset::Single(t) => t,
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => return None,
    };

    token
        .parent_ancestors()
        .find(|n| n.kind() == target_syntax_kind)
        .map(|n| n.text_range())
}

/// Find the text range of the enclosing item CST node for a name span.
fn find_item_range(
    db: &dyn Db,
    file: SourceFile,
    name_span: TextRange,
    _kind: DefinitionKind,
) -> Option<TextRange> {
    let tree = baml_compiler_parser::syntax_tree(db, file);

    let token = match tree.token_at_offset(name_span.start()) {
        rowan::TokenAtOffset::Single(t) => t,
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => return None,
    };

    // Walk up to the enclosing item-level node.
    token
        .parent_ancestors()
        .find(|n| is_item_node(n.kind()))
        .map(|n| n.text_range())
}

// ── Shape generation ─────────────────────────────────────────────────────────

/// Build a compact shape string for a symbol.
///
/// Uses `TypeInfo` from the existing `type_info` module for structured data,
/// then formats it without the markdown code fences.
fn build_shape(db: &dyn Db, file: SourceFile, sym: &SymbolInfo) -> String {
    // Resolve the symbol to a Definition to reuse type_info_for_definition.
    let name = baml_base::Name::new(&sym.name);
    let resolved =
        baml_compiler2_tir::resolve::resolve_name_at(db, file, sym.name_span.start(), &name);

    let (baml_compiler2_tir::resolve::ResolvedName::Item(def)
    | baml_compiler2_tir::resolve::ResolvedName::Builtin(def)) = resolved
    else {
        return format!("{} {}", sym.kind.as_str(), sym.name);
    };

    let type_info = type_info_for_definition(db, def);
    // Reuse the hover markdown but strip the ```baml fences.
    let md = type_info.to_hover_markdown();
    md.trim()
        .strip_prefix("```baml\n")
        .and_then(|s| s.strip_suffix("\n```"))
        .unwrap_or(&md)
        .to_string()
}

// ── Type resolution ──────────────────────────────────────────────────────────

/// Resolve the type of a field or variant by looking it up in the parent
/// class/enum's resolved fields, rather than going through `resolve_name_at`
/// (which only handles top-level `Definition` items, not members).
fn resolve_member_type(db: &dyn Db, file: SourceFile, sym: &SymbolInfo) -> Option<String> {
    let container_name = sym.container_name.as_ref()?;
    let container_baml_name = baml_base::Name::new(container_name);
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(
        db,
        file,
        sym.name_span.start(),
        &container_baml_name,
    );

    let (baml_compiler2_tir::resolve::ResolvedName::Item(def)
    | baml_compiler2_tir::resolve::ResolvedName::Builtin(def)) = resolved
    else {
        return None;
    };

    match (sym.kind, def) {
        (DefinitionKind::Field, baml_compiler2_hir::contributions::Definition::Class(class_loc)) => {
            let resolved_fields =
                baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
            resolved_fields
                .fields
                .iter()
                .find(|(field_name, _, _)| field_name.as_str() == sym.name)
                .map(|(_, ty, _)| crate::utils::display_ty(ty))
        }
        (DefinitionKind::Variant, baml_compiler2_hir::contributions::Definition::Enum(_)) => {
            // Enum variants don't have a meaningful type beyond the enum itself.
            Some(container_name.clone())
        }
        _ => None,
    }
}

/// Resolve the type of a top-level symbol to a user-friendly string.
///
/// For functions: `(a: string, b: int) -> ReturnType`
/// For classes: field list with types
/// For enums: variant list
/// For type aliases: the expansion
/// For locals: the inferred type
fn resolve_type_for_item(db: &dyn Db, file: SourceFile, sym: &SymbolInfo) -> Option<String> {
    use crate::type_info::TypeInfo;

    let name = baml_base::Name::new(&sym.name);
    let resolved =
        baml_compiler2_tir::resolve::resolve_name_at(db, file, sym.name_span.start(), &name);

    let (baml_compiler2_tir::resolve::ResolvedName::Item(def)
    | baml_compiler2_tir::resolve::ResolvedName::Builtin(def)) = resolved
    else {
        return None;
    };

    let type_info = type_info_for_definition(db, def);
    match type_info {
        TypeInfo::Function {
            params,
            return_type,
            ..
        } => {
            let param_strs: Vec<String> =
                params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
            let ret = return_type
                .map(|r| format!(" -> {r}"))
                .unwrap_or_default();
            Some(format!("({}){}", param_strs.join(", "), ret))
        }
        TypeInfo::Class { fields, .. } => {
            if fields.is_empty() {
                Some("{}".to_string())
            } else {
                let field_strs: Vec<String> =
                    fields.iter().map(|(n, t)| format!("{n}: {t}")).collect();
                Some(format!("{{ {} }}", field_strs.join(", ")))
            }
        }
        TypeInfo::Enum { variants, .. } => Some(format!("{{ {} }}", variants.join(", "))),
        TypeInfo::TypeAlias { expansion, .. } => Some(expansion),
        TypeInfo::TemplateString { .. } => Some("template_string".to_string()),
        TypeInfo::LocalVar { ty, .. } => Some(ty),
        TypeInfo::OtherItem { kind, .. } => Some(kind.to_string()),
    }
}

// ── Docstring extraction ─────────────────────────────────────────────────────

/// Extract leading `///` doc comments from the CST node preceding the item.
fn extract_docstring(db: &dyn Db, file: SourceFile, item_range: TextRange) -> Option<String> {
    let tree = baml_compiler_parser::syntax_tree(db, file);

    // Find the item node.
    let token = match tree.token_at_offset(item_range.start()) {
        rowan::TokenAtOffset::Single(t) => t,
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => return None,
    };

    let item_node = token
        .parent_ancestors()
        .find(|n| is_item_node(n.kind()))?;

    // Walk backward through siblings/trivia before the item node looking for
    // consecutive /// comments.
    let mut doc_lines: Vec<String> = Vec::new();

    // Get all tokens before the item node's start that are trivia.
    // We look at preceding siblings of the item node.
    let mut prev = item_node.prev_sibling_or_token();
    while let Some(node_or_token) = prev {
        match node_or_token {
            rowan::NodeOrToken::Token(ref tok) => {
                match tok.kind() {
                    SyntaxKind::LINE_COMMENT => {
                        let text = tok.text();
                        if let Some(doc) = text.strip_prefix("///") {
                            // Strip one leading space if present.
                            let doc = doc.strip_prefix(' ').unwrap_or(doc);
                            doc_lines.push(doc.to_string());
                        } else {
                            // Regular comment, stop collecting.
                            break;
                        }
                    }
                    SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {
                        // Skip whitespace between doc comments.
                    }
                    _ => break,
                }
            }
            rowan::NodeOrToken::Node(_) => break,
        }
        prev = node_or_token.prev_sibling_or_token();
    }

    if doc_lines.is_empty() {
        return None;
    }

    // Lines were collected in reverse order.
    doc_lines.reverse();
    Some(doc_lines.join("\n"))
}

// ── Dependency discovery ─────────────────────────────────────────────────────

/// Find signature-level dependencies for a symbol.
///
/// For functions: parameter types, return type.
/// For classes: field types that are user-defined.
/// For other kinds: empty (self-contained or not applicable).
fn find_dependencies(db: &dyn Db, files: &[SourceFile], file: SourceFile, sym: &SymbolInfo) -> Vec<DepRef> {
    let name = baml_base::Name::new(&sym.name);
    let resolved =
        baml_compiler2_tir::resolve::resolve_name_at(db, file, sym.name_span.start(), &name);

    let (baml_compiler2_tir::resolve::ResolvedName::Item(def)
    | baml_compiler2_tir::resolve::ResolvedName::Builtin(def)) = resolved
    else {
        return Vec::new();
    };

    let mut deps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(sym.name.clone());

    match def {
        baml_compiler2_hir::contributions::Definition::Function(func_loc) => {
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            // Collect type names from params and return type.
            for (_param_name, type_expr) in &sig.params {
                collect_type_expr_deps(db, file, type_expr, &mut deps, &mut seen);
            }
            if let Some(ret) = &sig.return_type {
                collect_type_expr_deps(db, file, ret, &mut deps, &mut seen);
            }
        }
        baml_compiler2_hir::contributions::Definition::Class(class_loc) => {
            let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
            for (_field_name, ty, _attrs) in &resolved.fields {
                collect_ty_deps(db, files, ty, &mut deps, &mut seen);
            }
        }
        baml_compiler2_hir::contributions::Definition::Enum(_) => {
            // Enums are self-contained, no type dependencies.
        }
        baml_compiler2_hir::contributions::Definition::TypeAlias(alias_loc) => {
            // Walk the alias's target type expression.
            let item_tree = baml_compiler2_hir::file_item_tree(db, file);
            let alias = &item_tree[alias_loc.id(db)];
            if let Some(spanned_te) = &alias.type_expr {
                collect_type_expr_deps(db, file, &spanned_te.expr, &mut deps, &mut seen);
            }
        }
        baml_compiler2_hir::contributions::Definition::Client(client_loc) => {
            // Surface the retry policy reference if present.
            let item_tree = baml_compiler2_hir::file_item_tree(db, file);
            let client = &item_tree[client_loc.id(db)];
            if let Some(ref policy_name) = client.retry_policy_name {
                let name_str = policy_name.as_str().to_string();
                if seen.insert(name_str.clone()) {
                    if let Some(dep) = resolve_dep_from_outline(db, files, &name_str) {
                        deps.push(dep);
                    }
                }
            }
        }
        baml_compiler2_hir::contributions::Definition::Test(test_loc) => {
            // Extract the test's function references.
            let item_tree = baml_compiler2_hir::file_item_tree(db, file);
            let test = &item_tree[test_loc.id(db)];
            for func_name in &test.function_refs {
                let name_str = func_name.as_str().to_string();
                if seen.insert(name_str.clone()) {
                    if let Some(dep) = resolve_dep_from_outline(db, files, &name_str) {
                        deps.push(dep);
                    }
                }
            }
        }
        baml_compiler2_hir::contributions::Definition::TemplateString(_) => {
            // Template string params are plain names with no types; self-contained.
        }
        baml_compiler2_hir::contributions::Definition::RetryPolicy(_)
        | baml_compiler2_hir::contributions::Definition::Generator(_)
        | baml_compiler2_hir::contributions::Definition::Let(_) => {
            // These don't have meaningful type dependencies for display.
        }
    }

    deps
}

/// Walk a `TypeExpr` and collect user-defined type names as `DepRefs`.
fn collect_type_expr_deps(
    db: &dyn Db,
    file: SourceFile,
    te: &baml_compiler2_ast::TypeExpr,
    deps: &mut Vec<DepRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    use baml_compiler2_ast::TypeExpr;
    match te {
        TypeExpr::Path { segments, .. } => {
            if let Some(last) = segments.last() {
                let name_str = last.as_str().to_string();
                if seen.insert(name_str.clone()) {
                    // Try to resolve this name to find its definition.
                    if let Some(dep) = resolve_dep(db, file, &name_str) {
                        deps.push(dep);
                    }
                }
            }
        }
        TypeExpr::Optional { inner, .. } | TypeExpr::List { inner, .. } => {
            collect_type_expr_deps(db, file, inner, deps, seen);
        }
        TypeExpr::Map { key, value, .. } => {
            collect_type_expr_deps(db, file, key, deps, seen);
            collect_type_expr_deps(db, file, value, deps, seen);
        }
        TypeExpr::Union { variants, .. } => {
            for v in variants {
                collect_type_expr_deps(db, file, v, deps, seen);
            }
        }
        TypeExpr::Function { params, ret, .. } => {
            for p in params {
                collect_type_expr_deps(db, file, &p.ty, deps, seen);
            }
            collect_type_expr_deps(db, file, ret, deps, seen);
        }
        // Primitives and literals have no user-defined deps.
        _ => {}
    }
}

/// Walk a resolved Ty and collect user-defined type names as `DepRefs`.
fn collect_ty_deps(
    db: &dyn Db,
    files: &[SourceFile],
    ty: &baml_compiler2_tir::ty::Ty,
    deps: &mut Vec<DepRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    use baml_compiler2_tir::ty::Ty;
    match ty {
        Ty::Class(qtn, _) | Ty::Enum(qtn, _) | Ty::TypeAlias(qtn, _) => {
            let name_str = qtn.to_string();
            if seen.insert(name_str.clone()) {
                // Look up the definition location via outline search.
                if let Some(dep) = resolve_dep_from_outline(db, files, &name_str) {
                    deps.push(dep);
                }
            }
        }
        Ty::Optional(inner, _) | Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
            collect_ty_deps(db, files, inner, deps, seen);
        }
        Ty::Map(k, v, _) | Ty::EvolvingMap(k, v, _) => {
            collect_ty_deps(db, files, k, deps, seen);
            collect_ty_deps(db, files, v, deps, seen);
        }
        Ty::Union(members, _) => {
            for m in members {
                collect_ty_deps(db, files, m, deps, seen);
            }
        }
        Ty::Function { params, ret, .. } => {
            for (_name, ty) in params {
                collect_ty_deps(db, files, ty, deps, seen);
            }
            collect_ty_deps(db, files, ret, deps, seen);
        }
        _ => {}
    }
}

/// Try to resolve a type name to a `DepRef` by looking it up in the outline.
fn resolve_dep_from_outline(db: &dyn Db, files: &[SourceFile], name: &str) -> Option<DepRef> {
    // Search all files' outlines for a symbol matching `name`.
    for &file in files {
        let outline = crate::outline::file_outline(db, file);
        for item in outline {
            if item.name == name {
                return Some(DepRef {
                    name: item.name.clone(),
                    kind: item.kind,
                    file,
                    name_span: item.name_span,
                });
            }
        }
    }
    None
}

/// Try to resolve a name to a `DepRef` using name resolution.
fn resolve_dep(db: &dyn Db, context_file: SourceFile, name: &str) -> Option<DepRef> {
    let baml_name = baml_base::Name::new(name);
    // Use offset 0 — we just need scope-level resolution for the file.
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(
        db,
        context_file,
        text_size::TextSize::from(0),
        &baml_name,
    );

    let (baml_compiler2_tir::resolve::ResolvedName::Item(def)
    | baml_compiler2_tir::resolve::ResolvedName::Builtin(def)) = resolved
    else {
        return None;
    };

    let (dep_file, name_span) = crate::utils::definition_span(db, def)?;

    Some(DepRef {
        name: name.to_string(),
        kind: def.kind(),
        file: dep_file,
        name_span,
    })
}

// ── Reference finding ────────────────────────────────────────────────────────

/// Find all reference sites for a symbol, excluding the definition itself.
fn find_references(
    db: &dyn Db,
    _files: &[SourceFile],
    file: SourceFile,
    name_span: TextRange,
) -> Vec<RefSite> {
    let locations = usages_at(db, file, name_span.start());

    locations
        .into_iter()
        // Exclude the definition site itself.
        .filter(|loc| !(loc.file == file && loc.range == name_span))
        .map(|loc| {
            let text = loc.file.text(db);
            let (line_number, line_text) = line_at_offset(text, loc.range.start());
            RefSite {
                file: loc.file,
                range: loc.range,
                line_text,
                line_number,
            }
        })
        .collect()
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Slice a text range from file text, trimming leading blank lines.
fn slice_text(text: &str, range: TextRange) -> String {
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    if end <= text.len() {
        text[start..end].trim_start_matches('\n').to_string()
    } else {
        String::new()
    }
}

/// Find the 1-based line number and full line text at a byte offset.
fn line_at_offset(text: &str, offset: text_size::TextSize) -> (usize, String) {
    let offset: usize = offset.into();
    let offset = offset.min(text.len());

    let line_number = text[..offset].chars().filter(|&c| c == '\n').count() + 1;

    let line_start = text[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = text[offset..]
        .find('\n')
        .map(|p| offset + p)
        .unwrap_or(text.len());

    (line_number, text[line_start..line_end].trim_end().to_string())
}

// ── Serde helpers ────────────────────────────────────────────────────────────

// serde's `serialize_with` contract requires `&T` — suppress the copy-by-ref lint.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_kind<S: serde::Serializer>(
    kind: &DefinitionKind,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.serialize_str(kind.as_str())
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_file<S: serde::Serializer>(
    _file: &SourceFile,
    s: S,
) -> Result<S::Ok, S::Error> {
    // This stub implementation exists so that we can serialize `SymbolDescription`,
    // which contains a `SourceFile`, to json, without actually poluting it with the
    // whole source file.
    s.serialize_none()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_range<S: serde::Serializer>(
    range: &TextRange,
    s: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;
    let mut state = s.serialize_struct("Range", 2)?;
    state.serialize_field("start", &u32::from(range.start()))?;
    state.serialize_field("end", &u32::from(range.end()))?;
    state.end()
}

//! `type_at` — structured type/signature info at a cursor position.
//!
//! This is a regular function (not a Salsa query). It uses the Rowan CST to
//! find the token under the cursor, resolves the name via `resolve_name_at`,
//! and builds a `TypeInfo` value describing what the name refers to.
//!
//! ## Resolution cases
//!
//! - `ResolvedName::Item(Definition::Function(_))` — builds `TypeInfo::Function`
//!   with params and return type from `function_signature`.
//!
//! - `ResolvedName::Item(Definition::Class(_))` — builds `TypeInfo::Class`
//!   with field names and types from `resolve_class_fields`.
//!
//! - `ResolvedName::Item(Definition::Enum(_))` — builds `TypeInfo::Enum`
//!   with variant names from the item tree.
//!
//! - `ResolvedName::Item(Definition::TypeAlias(_))` — builds `TypeInfo::TypeAlias`
//!   with the expansion type from `resolve_type_alias`.
//!
//! - `ResolvedName::Item(Definition::TemplateString(_))` — builds
//!   `TypeInfo::TemplateString` (no further info available).
//!
//! - `ResolvedName::Item(Definition::Client(_) | Generator(_) | ...)` — builds
//!   `TypeInfo::OtherItem` with the kind label.
//!
//! - `ResolvedName::Local { definition_site: Some(Parameter(idx)) }` — builds
//!   `TypeInfo::LocalVar` with the parameter type from `function_signature`.
//!
//! - `ResolvedName::Local { definition_site: Some(Statement(stmt_id)) }` — builds
//!   `TypeInfo::LocalVar` with the binding type from `infer_scope_types`.
//!
//! - `ResolvedName::Builtin(def)` — same as the matching `Item` case above.
//!
//! - `ResolvedName::Unknown` or cursor not on a WORD token — returns `None`.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::{
    contributions::Definition, scope::ScopeKind, semantic_index::DefinitionSite,
};
use text_size::TextSize;

use crate::{Db, utils};

// ── TypeInfo ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParamInfo {
    pub name: String,
    pub ty: String,
    pub optional: bool,
}

impl FunctionParamInfo {
    pub fn render(&self) -> String {
        let optional = if self.optional { "?" } else { "" };
        format!("{}{}: {}", self.name, optional, self.ty)
    }
}

/// A class method, captured for the hover "has methods?" check and the
/// describe/test renderers. The full describe listing (with docstring and line
/// range) is carried separately by `describe::MethodRef`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSig {
    pub name: String,
    /// Canonical one-line signature, e.g. `function Greet(self) -> string`.
    pub signature: String,
    /// `true` when the first parameter is named `self`.
    pub is_instance: bool,
}

/// Structured type/signature info at a cursor position.
///
/// Returned by `type_at`. The LSP layer (`request.rs`) formats this into hover
/// markdown. Keeping it as a structured type makes it easy to format for
/// different output contexts (markdown, plain text, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeInfo {
    /// A function definition: name, parameters, return type.
    Function {
        name: String,
        params: Vec<FunctionParamInfo>,
        return_type: Option<String>,
        throws: Option<String>,
        note: Option<String>,
    },
    /// A class definition: name, fields (name + type string), implemented interfaces.
    Class {
        name: String,
        /// Generic type parameter names (e.g. `["T"]`). Rendered as `<T>` after
        /// the class name in the body block; empty for non-generic classes.
        generic_params: Vec<String>,
        fields: Vec<(String, String)>,
        implements: Vec<String>,
        /// Instance + static methods (signatures only). Drives the hover hint
        /// and feeds the test/describe renderers; not shown inline in hover.
        methods: Vec<MethodSig>,
        /// The class's full `///` docstring (all lines), if any.
        docstring: Option<String>,
        /// Canonical FQN for the hover "Run `baml describe …`" hint (`string`,
        /// `Foo`, `root.ns.Foo`, `baml.json.JsonObject`).
        canonical_fqn: String,
    },
    /// An enum definition: name, variants.
    Enum { name: String, variants: Vec<String> },
    /// A type alias: name + the expansion type string.
    TypeAlias { name: String, expansion: String },
    /// A template string: name only (no further type info).
    TemplateString { name: String },
    /// A local variable (let binding or parameter): name + inferred/declared type.
    LocalVar { name: String, ty: String },
    /// A non-structural top-level item (client, generator, test, `retry_policy`).
    OtherItem { name: String, kind: &'static str },
}

impl TypeInfo {
    /// The canonical BAML block for this item, without code fences, docstring,
    /// or any trailing hint/note. For a class this is the **fields-only** body
    /// (`class Foo {\n    bar: int,\n}`): methods are surfaced separately by
    /// `describe`, never inside the body block. Shared by
    /// [`Self::to_hover_markdown`] (which wraps it) and `describe::build_shape`.
    pub fn to_describe_block(&self) -> String {
        match self {
            TypeInfo::Function {
                name,
                params,
                return_type,
                throws,
                ..
            } => {
                let param_strs: Vec<String> =
                    params.iter().map(FunctionParamInfo::render).collect();
                let ret = return_type
                    .as_deref()
                    .map(|r| format!(" -> {r}"))
                    .unwrap_or_default();
                let throws = throws
                    .as_deref()
                    .map(|t| format!(" throws {t}"))
                    .unwrap_or_default();
                format!(
                    "function {}({}){}{throws}",
                    name,
                    param_strs.join(", "),
                    ret
                )
            }
            TypeInfo::Class {
                name,
                generic_params,
                fields,
                implements,
                ..
            } => {
                // Fields-only canonical body: `name: type,` with trailing comma,
                // 4-space indent. Methods are never rendered here.
                let generics = if generic_params.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", generic_params.join(", "))
                };
                let mut member_strs: Vec<String> = fields
                    .iter()
                    .map(|(n, t)| format!("    {n}: {t},"))
                    .collect();
                for implements_block in implements {
                    member_strs.extend(implements_block.lines().map(|line| format!("    {line}")));
                }

                if member_strs.is_empty() {
                    format!("class {name}{generics} {{}}")
                } else {
                    format!("class {name}{generics} {{\n{}\n}}", member_strs.join("\n"))
                }
            }
            TypeInfo::Enum { name, variants } => {
                let variant_strs: Vec<String> =
                    variants.iter().map(|v| format!("    {v}")).collect();
                if variant_strs.is_empty() {
                    format!("enum {name} {{}}")
                } else {
                    format!("enum {name} {{\n{}\n}}", variant_strs.join("\n"))
                }
            }
            TypeInfo::TypeAlias { name, expansion } => format!("type {name} = {expansion}"),
            TypeInfo::TemplateString { name } => format!("template_string {name}"),
            TypeInfo::LocalVar { name, ty } => format!("{name}: {ty}"),
            TypeInfo::OtherItem { name, kind } => format!("{kind} {name}"),
        }
    }

    /// Format this `TypeInfo` as hover markdown.
    ///
    /// The caller (request.rs) wraps the result in an LSP `MarkupContent`.
    pub fn to_hover_markdown(&self) -> String {
        match self {
            TypeInfo::Class {
                docstring,
                methods,
                canonical_fqn,
                ..
            } => {
                // Docstring lines live inside the fenced block, above the class.
                let mut inner = String::new();
                if let Some(doc) = docstring {
                    for line in doc.lines() {
                        inner.push_str("/// ");
                        inner.push_str(line);
                        inner.push('\n');
                    }
                }
                inner.push_str(&self.to_describe_block());
                let mut out = format!("```baml\n{inner}\n```");
                // Only a class with methods points the user at `baml describe`.
                if !methods.is_empty() {
                    out.push_str("\n\nRun `baml describe ");
                    out.push_str(canonical_fqn);
                    out.push_str("` for methods and details.");
                }
                out
            }
            TypeInfo::Function { note, .. } => {
                let mut out = format!("```baml\n{}\n```", self.to_describe_block());
                if let Some(note) = note {
                    out.push_str("\n\n");
                    out.push_str(note);
                }
                out
            }
            _ => format!("```baml\n{}\n```", self.to_describe_block()),
        }
    }
}

// ── type_at ───────────────────────────────────────────────────────────────────

/// Find structured type/signature info for the symbol at `offset` in `file`.
///
/// Regular function (not cached). The expensive work (`file_semantic_index`,
/// `function_signature`, `resolve_class_fields`, `infer_scope_types`) is
/// internally Salsa-cached.
///
/// Returns `None` if the cursor is not on an identifier, or if the name
/// cannot be resolved.
pub fn type_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Option<TypeInfo> {
    // ── Step 1: find the token at the cursor ─────────────────────────────────
    let token = utils::find_token_at_offset(db, file, offset)?;

    // Only WORD tokens can be names.
    if token.kind() != SyntaxKind::WORD {
        return None;
    }

    let name_text = token.text();
    let name = Name::new(name_text);

    // A let/for binding is not visible to expression resolution until after its
    // declaration statement. Hovers on the declaration token itself still need
    // to describe that binding.
    if let Some((site, lookup_offset)) = declaration_site_at(db, file, offset, &name) {
        return local_type_info(db, file, lookup_offset, &name, site);
    }

    // ── Step 2: resolve the name in scope ─────────────────────────────────────
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(db, file, offset, &name);

    // ── Step 3: build TypeInfo based on the resolution ────────────────────────
    match resolved {
        baml_compiler2_tir::resolve::ResolvedName::Item(def)
        | baml_compiler2_tir::resolve::ResolvedName::Builtin(def) => {
            Some(type_info_for_definition(db, def))
        }

        baml_compiler2_tir::resolve::ResolvedName::Local {
            name: local_name,
            definition_site: Some(site),
        } => local_type_info(db, file, offset, &local_name, site),

        baml_compiler2_tir::resolve::ResolvedName::Local {
            definition_site: None,
            ..
        }
        | baml_compiler2_tir::resolve::ResolvedName::Unknown => None,
    }
}

fn declaration_site_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    name: &Name,
) -> Option<(DefinitionSite, TextSize)> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);

    for ancestor_id in index.ancestor_scopes(scope_id) {
        let bindings = &index.scope_bindings[ancestor_id.index() as usize];
        for binding in bindings.bindings.iter().rev() {
            if &binding.name == name
                && (binding.name_range.contains(offset) || binding.name_range.end() == offset)
            {
                return Some((binding.site, binding.name_range.end()));
            }
        }
    }

    None
}

// ── type_info_for_definition ──────────────────────────────────────────────────

/// Build `TypeInfo` for a top-level item definition.
pub fn type_info_for_definition(db: &dyn Db, def: Definition<'_>) -> TypeInfo {
    match def {
        Definition::Function(func_loc) => {
            let file = func_loc.file(db);
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
            let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
            let iface = baml_compiler2_tir::package_interface::package_interface(db, pkg_id);
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            let function_name = sig.name.clone();

            let fallback = || {
                let params = sig
                    .params
                    .iter()
                    .map(|param| FunctionParamInfo {
                        name: param.name.as_str().to_string(),
                        ty: utils::display_type_expr(&param.ty),
                        optional: param.has_default,
                    })
                    .collect();
                let return_type = sig.return_type.as_ref().map(utils::display_type_expr);
                TypeInfo::Function {
                    name: sig.name.as_str().to_string(),
                    params,
                    return_type,
                    throws: sig.throws.as_ref().map(utils::display_type_expr),
                    note: None,
                }
            };

            let Some(exported) = iface.lookup_function(&pkg_info.namespace_path, &function_name)
            else {
                return fallback();
            };

            let params = exported
                .params
                .iter()
                .map(|param| FunctionParamInfo {
                    name: param
                        .name
                        .as_ref()
                        .map(|name| name.as_str().to_string())
                        .unwrap_or_else(|| "_".to_string()),
                    ty: display_surface_ty(db, file, &param.ty),
                    optional: param.is_optional(),
                })
                .collect();
            let return_type = Some(display_surface_ty(db, file, &exported.return_type));
            let throws = if exported.declared_throws.is_some()
                || !matches!(
                    exported.callable_throws,
                    baml_compiler2_tir::ty::Ty::Never { .. }
                ) {
                Some(display_surface_ty(db, file, &exported.callable_throws))
            } else {
                None
            };
            TypeInfo::Function {
                name: exported.name.as_str().to_string(),
                params,
                return_type,
                throws,
                note: callback_forwarding_note(exported),
            }
        }

        Definition::Class(class_loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
            let class_data = &item_tree[class_loc.id(db)];
            let class_name = class_data.name.as_str().to_string();

            // Use resolved field types (Salsa-cached), rendered canonically so
            // builtin companion classes collapse to their alias (`string`).
            let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
            let fields = resolved
                .fields
                .iter()
                .map(|(field_name, ty, _attrs)| {
                    (
                        field_name.as_str().to_string(),
                        utils::display_ty_canonical_for_file(db, class_loc.file(db), ty),
                    )
                })
                .collect();
            let implements = class_data
                .implements
                .iter()
                .map(render_implements_block)
                .collect();

            let qtn = baml_compiler2_tir::lower_type_expr::qualify_def(db, def, &class_data.name);
            let canonical_fqn = utils::canonical_fqn_string(&qtn);
            let methods = crate::describe::class_method_sigs(db, class_loc);

            let generic_params = class_data
                .generic_params
                .iter()
                .map(|n| n.as_str().to_string())
                .collect();

            TypeInfo::Class {
                name: class_name,
                generic_params,
                fields,
                implements,
                methods,
                docstring: class_data.docstring.clone(),
                canonical_fqn,
            }
        }

        Definition::Enum(enum_loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, enum_loc.file(db));
            let enum_data = &item_tree[enum_loc.id(db)];
            let variants = enum_data
                .variants
                .iter()
                .map(|v| v.name.as_str().to_string())
                .collect();
            TypeInfo::Enum {
                name: enum_data.name.as_str().to_string(),
                variants,
            }
        }

        Definition::Interface(iface_loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
            let iface = &item_tree[iface_loc.id(db)];
            TypeInfo::OtherItem {
                name: iface.name.as_str().to_string(),
                kind: "interface",
            }
        }

        Definition::TypeAlias(alias_loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, alias_loc.file(db));
            let alias_data = &item_tree[alias_loc.id(db)];
            let alias_name = alias_data.name.as_str().to_string();

            // Use the resolved (lowered) type for display.
            let resolved = baml_compiler2_tir::inference::resolve_type_alias(db, alias_loc);
            let expansion = utils::display_ty_for_file(db, alias_loc.file(db), &resolved.ty);

            TypeInfo::TypeAlias {
                name: alias_name,
                expansion,
            }
        }

        Definition::TemplateString(ts_loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, ts_loc.file(db));
            let ts_data = &item_tree[ts_loc.id(db)];
            TypeInfo::TemplateString {
                name: ts_data.name.as_str().to_string(),
            }
        }

        Definition::Client(loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "client",
            }
        }

        Definition::Test(loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "test",
            }
        }

        Definition::RetryPolicy(loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "retry_policy",
            }
        }

        Definition::Let(loc) => {
            let item_tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            let kind = match data.origin {
                baml_compiler2_ast::ast::LetOrigin::Client => "client",
                baml_compiler2_ast::ast::LetOrigin::RetryPolicy => "retry_policy",
                baml_compiler2_ast::ast::LetOrigin::Source => "let",
            };
            TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind,
            }
        }
    }
}

fn render_implements_block(block: &baml_compiler2_hir::item_tree::ImplementsBlock) -> String {
    let mut members = Vec::new();

    members.extend(block.field_links.iter().map(|link| {
        format!(
            "{} as {}",
            link.interface_field.as_str(),
            link.class_field.as_str()
        )
    }));
    members.extend(block.associated_type_bindings.iter().map(|binding| {
        let ty = binding
            .type_expr
            .as_ref()
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "unknown".to_string());
        format!("type {} = {}", binding.name.as_str(), ty)
    }));

    if members.is_empty() {
        format!("implements {} {{}}", block.target)
    } else {
        let members = members
            .into_iter()
            .map(|member| format!("    {member}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("implements {} {{\n{members}\n}}", block.target)
    }
}

fn display_surface_ty(db: &dyn Db, file: SourceFile, ty: &baml_compiler2_tir::ty::Ty) -> String {
    utils::display_ty_for_file(db, file, ty)
}

fn display_local_binding_ty(
    db: &dyn Db,
    file: SourceFile,
    ty: &baml_compiler2_tir::ty::Ty,
) -> String {
    utils::display_ty_for_file(db, file, ty)
}

fn function_param_matches_effect_slot(ty: &baml_compiler2_tir::ty::Ty, effect_name: &Name) -> bool {
    use baml_compiler2_tir::ty::Ty;

    match ty {
        Ty::Function { throws, .. } => matches!(
            throws.as_ref(),
            Ty::TypeVar(name, _) if name == effect_name
        ),
        Ty::Union(members, _) => {
            let mut matched = false;
            for member in members {
                if matches!(member, Ty::Null { .. }) {
                    continue;
                }
                if !function_param_matches_effect_slot(member, effect_name) {
                    return false;
                }
                matched = true;
            }
            matched
        }
        _ => false,
    }
}

fn callback_forwarding_note(
    exported: &baml_compiler2_tir::package_interface::ExportedFunction,
) -> Option<String> {
    use baml_compiler2_tir::ty::Ty;

    let throws_facts =
        baml_compiler2_tir::throw_inference::flatten_ty_to_facts(&exported.callable_throws);
    let throw_fact_refs = throws_facts.iter().collect::<Vec<_>>();
    let [only_fact] = throw_fact_refs.as_slice() else {
        return None;
    };
    let Ty::TypeVar(effect_name, _) = only_fact else {
        return None;
    };
    if !baml_compiler2_tir::ty::is_synthetic_effect_param(effect_name) {
        return None;
    }

    let mut matching_params = exported
        .params
        .iter()
        .filter(|param| function_param_matches_effect_slot(&param.ty, effect_name))
        .filter_map(|param| param.name.as_ref())
        .collect::<Vec<_>>();

    if matching_params.len() == 1 {
        let callback_name = matching_params.pop().expect("len checked");
        Some(format!(
            "Forwards whatever callback `{callback_name}` throws."
        ))
    } else {
        None
    }
}

// ── local_type_info ───────────────────────────────────────────────────────────

/// Build `TypeInfo::LocalVar` for a local variable (let binding or parameter).
fn local_type_info(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    name: &Name,
    site: DefinitionSite,
) -> Option<TypeInfo> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);

    // Find the enclosing Function scope.
    let scope_id = index.scope_at_offset(at_offset, None);
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

    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *func_local_id);

    match site {
        DefinitionSite::Parameter(param_idx) => {
            // Get the declared parameter type from the function signature.
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            let ty_str = sig
                .params
                .get(param_idx)
                .map(|param| utils::display_type_expr(&param.ty))
                .unwrap_or_else(|| "unknown".to_string());
            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: ty_str,
            })
        }

        DefinitionSite::Statement(stmt_id) => {
            // Look up the binding type from inferred scope types.
            // We need to go from `StmtId` → `PatId` → binding type.
            //
            // Use the function body to find the statement and extract the pat id,
            // then look up the type from infer_scope_types for the enclosing scope.
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let pat_id = body_stmt_to_pat_id(&body, stmt_id)?;

            // Get the scope containing at_offset (may be a nested block scope).
            // infer_scope_types is keyed by ScopeId. We need the function scope's
            // ScopeId to get the binding type, since bindings are stored per scope.
            let func_scope_id = index.scope_ids[enclosing_func_scope.index() as usize];
            let inference = baml_compiler2_tir::inference::infer_scope_types(db, func_scope_id);
            let ty_str = inference
                .binding_type(pat_id)
                .map(|ty| display_local_binding_ty(db, file, ty))
                .unwrap_or_else(|| {
                    // Try the use-site's ancestor scope chain — restricts the
                    // lookup to inferences for bodies that share the
                    // use-site's pattern arena. Iterating *every* scope in
                    // the file would, under PatId collisions across nested
                    // ExprBodies (e.g. two lambdas with the same arena
                    // index), surface the wrong type for hover/inlay hints.
                    find_binding_ty_in_scopes(db, index, scope_id, pat_id)
                        .map(|ty| display_local_binding_ty(db, file, &ty))
                        .unwrap_or_else(|| "unknown".to_string())
                });

            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: ty_str,
            })
        }

        DefinitionSite::PatternBinding(_) => {
            // Pattern bindings — report as local variable with unknown type for now.
            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: "unknown".to_string(),
            })
        }
    }
}

/// Extract the `PatId` for the binding introduced by `stmt_id`.
///
/// For local declaration statements, returns the pattern ID.
/// Returns `None` for other statement kinds.
fn body_stmt_to_pat_id(
    body: &baml_compiler2_hir::body::FunctionBody,
    stmt_id: baml_compiler2_ast::StmtId,
) -> Option<baml_compiler2_ast::PatId> {
    use baml_compiler2_hir::body::FunctionBody;
    let FunctionBody::Expr(expr_body) = body else {
        return None;
    };

    let stmt = &expr_body.stmts[stmt_id];
    match stmt {
        baml_compiler2_ast::Stmt::Let { pattern, .. }
        | baml_compiler2_ast::Stmt::For {
            binding: pattern, ..
        } => Some(*pattern),
        _ => None,
    }
}

/// Search the use-site's ancestor-scope chain for the binding type of
/// `pat_id`.
///
/// `PatId`s are arena-local to a function/lambda body, so iterating *all*
/// scopes in the file can surface a wrong-arena hit if two bodies happen to
/// allocate the same `PatId` index. Walking ancestors only — Function or
/// Lambda scopes that enclose `from_scope` — restricts the lookup to
/// inferences whose binding maps were populated from the use-site's own
/// arena. Mirrors the structure already used by
/// `completions.rs::find_binding_ty_for_local`.
fn find_binding_ty_in_scopes(
    db: &dyn Db,
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    from_scope: baml_compiler2_hir::scope::FileScopeId,
    pat_id: baml_compiler2_ast::PatId,
) -> Option<baml_compiler2_tir::ty::Ty> {
    for ancestor_id in index.ancestor_scopes(from_scope) {
        let scope_id = index.scope_ids[ancestor_id.index() as usize];
        let inference = baml_compiler2_tir::inference::infer_scope_types(db, scope_id);
        if let Some(ty) = inference.binding_type(pat_id) {
            return Some(ty.clone());
        }
    }
    None
}

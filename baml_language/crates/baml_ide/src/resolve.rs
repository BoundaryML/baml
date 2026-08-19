//! The addressing layer: *what symbol is at this position?*
//!
//! Every cursor feature (go-to-definition, find-references, hover) answers
//! two separate questions — "what is the cursor on?" and "what do I do with
//! it?". This module owns the first question, once: [`symbol_at`] turns a
//! position into a [`SymbolTarget`], and [`target_definition`] maps a target
//! to the location of its declaration's name token. The by-name entry
//! (`baml describe <name>`) resolves into the same [`SymbolTarget`] space via
//! the listing module, so extraction downstream never cares how a symbol was
//! addressed.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, EnumLoc, FunctionLoc, InterfaceLoc},
    scope::{FileScopeId, ScopeKind},
    semantic_index::{BindingId, BindingKind, FileSemanticIndex},
};
use baml_compiler2_ppir::{
    item_data,
    resolve::{ResolvedName, resolve_name_at},
};
use text_size::{TextRange, TextSize};

use crate::syntax;

// ── Location ─────────────────────────────────────────────────────────────────

/// A resolved source location: the target file and the byte range of the
/// name token (not the full item body). The LSP layer converts `file` to a
/// URI and `range` to an LSP `Range`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub file: SourceFile,
    pub range: TextRange,
}

// ── SymbolTarget ─────────────────────────────────────────────────────────────

/// The symbol a position resolves to, in a form every feature can share.
///
/// Members are addressed positionally (`field_index`, `variant_index`, …)
/// into the owning item's firewall data, so a target stays valid across the
/// readers (`class_data`, source maps) without re-resolving names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SymbolTarget<'db> {
    /// A top-level item (class, function, enum, type alias, …), user-defined
    /// or builtin.
    Item(Definition<'db>),
    /// A local binding (let, parameter, pattern, or catch binding).
    Local {
        /// The function whose body arena owns the binding.
        func: FunctionLoc<'db>,
        /// The enclosing `Function` scope (reference searches start here;
        /// lambda scopes nest below it and share the body arena).
        func_scope: FileScopeId,
        binding: BindingId,
    },
    /// A class field, by position in the class's field list.
    Field {
        class: ClassLoc<'db>,
        field_index: usize,
    },
    /// An enum variant, by position in the enum's variant list.
    Variant {
        enum_loc: EnumLoc<'db>,
        variant_index: usize,
    },
    /// A method with a concrete body: a class method, an `implements`-block
    /// method, or an interface *default* method.
    Method { func: FunctionLoc<'db> },
    /// A required (signature-only) interface method, by position.
    InterfaceRequiredMethod {
        iface: InterfaceLoc<'db>,
        method_index: usize,
    },
    /// An interface field declaration, by position.
    InterfaceField {
        iface: InterfaceLoc<'db>,
        field_index: usize,
    },
}

// ── symbol_at ────────────────────────────────────────────────────────────────

/// Resolve the symbol at `offset` in `file`.
///
/// Regular function (not cached) — the expensive parts
/// (`file_semantic_index`, `resolve_name_at`, inference) are Salsa-cached.
///
/// Resolution order:
/// 1. **Locals.** Checked before name resolution because a declaration's own
///    name token is deliberately not visible to `resolve_name_at` until
///    after its statement, yet the cursor sits exactly there when a user
///    asks for references on a `let`.
/// 2. **Scope names** via `resolve_name_at` (items, builtins).
/// 3. **Members**: field/variant declarations under the cursor, member
///    accesses, and constructor-literal keys, confirmed through inference.
pub fn symbol_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<SymbolTarget<'_>> {
    let token = syntax::find_token_at_offset(db, file, offset)?;

    // WORD tokens and keyword tokens may both be names in member position
    // (`obj.implements()`), so let the resolvers decide whether the text is
    // actually a symbol.
    if token.kind() != SyntaxKind::WORD && !token.kind().is_keyword() {
        return None;
    }

    let name_text = token.text();
    let name = Name::new(name_text);

    if let Some(target) = local_at(db, file, offset, &name) {
        return Some(target);
    }

    match resolve_name_at(db, file, offset, &name) {
        ResolvedName::Item(def) | ResolvedName::Builtin(def) => Some(SymbolTarget::Item(def)),
        // A local that `local_at` did not find has no recoverable binding
        // identity; member resolution cannot apply to a scope name either.
        ResolvedName::Local { .. } => None,
        ResolvedName::Unknown => member_at(db, file, offset, name_text),
    }
}

/// The local binding at `offset`, if any: either the declaration's own name
/// token (identified by its recorded name range) or a visible use site.
fn local_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
    name: &Name,
) -> Option<SymbolTarget<'db>> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);

    // Declaration tokens are intentionally not visible until after their
    // initializer/statement, so identify them by their recorded name range.
    let declaration = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find_map(|ancestor_id| {
            let bindings = &index.scope_bindings[ancestor_id.index() as usize];
            bindings
                .bindings
                .iter()
                .enumerate()
                .rev()
                .find(|(_, binding)| {
                    binding.name == *name
                        && (binding.name_range.contains(offset)
                            || binding.name_range.start() == offset)
                })
                .map(|(binding_idx, _)| BindingId::local(ancestor_id, binding_idx))
        });

    let binding = declaration.or_else(|| index.visible_binding_at(scope_id, offset, name))?;

    let func_scope = enclosing_function_scope(index, scope_id)?;
    // The recorded item↔scope link, not a span match: a span cannot tell a
    // function from its companions (they share one declaration span).
    let owner_scope = index.scope_ids[func_scope.index() as usize];
    let item_data::ScopeOwner::Function(func) = item_data::scope_owner(db, owner_scope)? else {
        return None;
    };

    Some(SymbolTarget::Local {
        func,
        func_scope,
        binding,
    })
}

/// The nearest enclosing `Function`-kind scope (inclusive walk from
/// `scope_id`'s ancestors).
pub(crate) fn enclosing_function_scope(
    index: &FileSemanticIndex<'_>,
    scope_id: FileScopeId,
) -> Option<FileScopeId> {
    index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                ScopeKind::Function
            )
        })
}

// ── member resolution ────────────────────────────────────────────────────────

/// Fallback resolution for member positions where `resolve_name_at` returns
/// `Unknown` — field/variant declarations, member accesses, constructor
/// keys, methods.
fn member_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
    token_text: &str,
) -> Option<SymbolTarget<'db>> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let scope = &index.scopes[scope_id.index() as usize];
    let owner_scope = index.scope_ids[scope_id.index() as usize];

    match scope.kind {
        // Cursor on a field declaration inside a class body.
        ScopeKind::Class => {
            let item_data::ScopeOwner::Class(class) = item_data::scope_owner(db, owner_scope)?
            else {
                return None;
            };
            let field_index = item_data::class_data(db, class)
                .fields
                .iter()
                .position(|field| field.name.as_str() == token_text)?;
            Some(SymbolTarget::Field { class, field_index })
        }
        // Cursor on a variant declaration inside an enum body.
        ScopeKind::Enum => {
            let item_data::ScopeOwner::Enum(enum_loc) = item_data::scope_owner(db, owner_scope)?
            else {
                return None;
            };
            let variant_index = item_data::enum_data(db, enum_loc)
                .variants
                .iter()
                .position(|variant| variant.name.as_str() == token_text)?;
            Some(SymbolTarget::Variant {
                enum_loc,
                variant_index,
            })
        }
        // Cursor in a function body: member access or constructor key.
        //
        // Known gap: a cursor on a declaration inside an *interface* body is
        // not addressable — the semantic index opens no scope for interface
        // bodies (no `ScopeKind` for them), so there is no recorded
        // item↔scope link to resolve through. Closing it needs the compiler
        // to open interface scopes the way class bodies do; interface
        // *usages* (virtual member accesses) resolve fine through inference.
        _ => {
            let func_scope = enclosing_function_scope(index, scope_id)?;
            let func_owner = index.scope_ids[func_scope.index() as usize];
            let item_data::ScopeOwner::Function(func_loc) = item_data::scope_owner(db, func_owner)?
            else {
                return None;
            };

            let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_owner)?;
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
                return None;
            };
            let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;

            member_access_at(db, offset, token_text, expr_body, &source_map, inference)
                .or_else(|| constructor_field_at(db, offset, expr_body, &source_map, inference))
        }
    }
}

/// Cursor on a `MemberAccess` expression or a non-root segment of a
/// multi-segment `Path` (`p.name`, `Status.Active`, `s.celebrate()`).
fn member_access_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    offset: TextSize,
    token_text: &str,
    expr_body: &baml_compiler2_ast::ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &baml_compiler2_hir_ty::infer::InferenceResult<'db>,
) -> Option<SymbolTarget<'db>> {
    use baml_compiler2_ast::Expr;

    // Best match = the innermost expression whose span contains the cursor
    // and whose member name matches the token. For `Path`s, also record
    // which segment the cursor is on.
    let mut best: Option<(baml_compiler2_ast::ExprId, TextRange, Option<usize>)> = None;
    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            Expr::MemberAccess { member, .. } => {
                if member.as_str() != token_text {
                    continue;
                }
                let span = source_map.expr_span(expr_id);
                if !span.contains(offset) && span.end() != offset {
                    continue;
                }
                if best.is_none_or(|(_, prev_span, _)| span.len() < prev_span.len()) {
                    best = Some((expr_id, span, None));
                }
            }
            Expr::Path(segments) if segments.len() >= 2 => {
                // Only segments[1..] can be member accesses — segments[0] is
                // the root name.
                let segment_idx = segments[1..]
                    .iter()
                    .enumerate()
                    .find(|(idx, seg)| {
                        if seg.as_str() != token_text {
                            return false;
                        }
                        let seg_span = source_map.path_segment_span(expr_id, *idx + 1);
                        seg_span.contains(offset) || seg_span.end() == offset
                    })
                    .map(|(idx, _)| idx + 1);
                if segment_idx.is_none() {
                    continue;
                }
                let span = source_map.expr_span(expr_id);
                if best.is_none_or(|(_, prev_span, _)| span.len() < prev_span.len()) {
                    best = Some((expr_id, span, segment_idx));
                }
            }
            _ => {}
        }
    }

    let (expr_id, _, path_seg_idx) = best?;

    // For multi-segment `Path`s the per-segment resolution ladder is
    // parallel to `segments` (entry 0 carries no resolution); `MemberAccess`
    // uses the expression-level resolution.
    let resolution = if let Some(seg_idx) = path_seg_idx {
        inference
            .path_resolutions
            .get(&expr_id)
            .and_then(|path| path.segments.get(seg_idx))
            .and_then(|step| step.resolution.as_ref())
            .or_else(|| inference.member_resolutions.get(&expr_id))
    } else {
        inference.member_resolutions.get(&expr_id)
    };

    member_resolution_target(db, resolution?)
}

/// Map an inference [`MemberResolution`] to a [`SymbolTarget`].
pub(crate) fn member_resolution_target<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    resolution: &baml_compiler2_hir_ty::infer::MemberResolution<'db>,
) -> Option<SymbolTarget<'db>> {
    use baml_compiler2_hir_ty::infer::MemberResolution;

    match resolution {
        MemberResolution::Field { class, field } => {
            // Read the canonical (PPIR) tree, not the HIR pre-expansion tree:
            // an inferred `class` can be a synthetic `$stream` class (the
            // type of a streamed partial), which is absent pre-expansion.
            // Its field name-spans alias the user-authored class's, so
            // navigation still lands on real source.
            let field_index = item_data::class_data(db, *class)
                .fields
                .iter()
                .position(|f| f.name == *field)?;
            Some(SymbolTarget::Field {
                class: *class,
                field_index,
            })
        }
        MemberResolution::Variant { enum_loc, variant } => {
            let variant_index = item_data::enum_data(db, *enum_loc)
                .variants
                .iter()
                .position(|v| v.name == *variant)?;
            Some(SymbolTarget::Variant {
                enum_loc: *enum_loc,
                variant_index,
            })
        }
        MemberResolution::Free { func } => Some(SymbolTarget::Item(Definition::Function(*func))),
        MemberResolution::BoundMethod { func, .. }
        | MemberResolution::UnboundMethod { func, .. }
        | MemberResolution::InterfaceConcreteMethod { func, .. } => {
            Some(SymbolTarget::Method { func: *func })
        }
        MemberResolution::InterfaceVirtualMethod { interface, method } => {
            // Only the slot (interface + name) is known statically: address
            // the declaration — the required signature, or the default
            // method's definition.
            let iface_data = item_data::interface_data(db, *interface);
            if let Some(method_index) = iface_data
                .required_methods
                .iter()
                .position(|m| m.name == *method)
            {
                return Some(SymbolTarget::InterfaceRequiredMethod {
                    iface: *interface,
                    method_index,
                });
            }
            let default_loc = *iface_data
                .default_methods
                .iter()
                .find(|&&fn_loc| item_data::function_data(db, fn_loc).name == *method)?;
            Some(SymbolTarget::Method { func: default_loc })
        }
        MemberResolution::InterfaceVirtualField {
            interface,
            field_index,
            ..
        } => Some(SymbolTarget::InterfaceField {
            iface: *interface,
            field_index: *field_index as usize,
        }),
        // Mounted rows deliberately carry no dependency SourceFile/span.
        MemberResolution::External(_)
        | MemberResolution::ExternalField { .. }
        | MemberResolution::ExternalVariant { .. }
        | MemberResolution::ExternalInterfaceVirtualField { .. } => None,
    }
}

/// Cursor on a constructor-literal key (`data:` in `Success { data: … }`).
fn constructor_field_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    offset: TextSize,
    expr_body: &baml_compiler2_ast::ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &baml_compiler2_hir_ty::infer::InferenceResult<'db>,
) -> Option<SymbolTarget<'db>> {
    use baml_compiler2_ast::Expr;
    use baml_type::Ty;

    for (expr_id, expr) in expr_body.exprs.iter() {
        let Expr::Object { fields, .. } = expr else {
            continue;
        };
        let span = source_map.expr_span(expr_id);
        if !span.contains(offset) && span.end() != offset {
            continue;
        }

        // The key whose recorded name span is under the cursor — positional,
        // not textual, so `Foo { name: name }` addresses the key, and a
        // matching key in a *nested* literal never wins by accident.
        let field = fields.iter().find(|field| {
            let name_span = source_map.object_field_name_span(expr_id, field.value);
            name_span.contains(offset) || name_span.end() == offset
        })?;

        let obj_ty = inference.type_of_expr.get(&expr_id)?.to_plain();
        let Ty::Class(ref qtn, _, _) = obj_ty else {
            return None;
        };

        let pkg_id = baml_compiler2_hir::package::PackageId::new(db, qtn.package().clone());
        let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
        let def = pkg_items.lookup_type(qtn.namespace(), qtn.name())?;
        let Definition::Class(class) = def else {
            return None;
        };

        let field_index = item_data::class_data(db, class)
            .fields
            .iter()
            .position(|f| f.name == field.name)?;
        return Some(SymbolTarget::Field { class, field_index });
    }
    None
}

// ── target_definition ────────────────────────────────────────────────────────

/// The location of a target's declaration name token.
pub fn target_definition<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    target: SymbolTarget<'db>,
) -> Option<Location> {
    match target {
        SymbolTarget::Item(def) => {
            let (file, range) = syntax::definition_span(db, def)?;
            Some(Location { file, range })
        }
        SymbolTarget::Local { func, binding, .. } => {
            let file = func.file(db);
            match binding.kind {
                BindingKind::Local(idx) => {
                    let index = baml_compiler2_hir::file_semantic_index(db, file);
                    let local = index.scope_bindings[binding.scope.index() as usize]
                        .bindings
                        .get(idx as usize)?;
                    Some(Location {
                        file,
                        range: local.name_range,
                    })
                }
                BindingKind::Parameter(idx) => {
                    let sig_map =
                        baml_compiler2_hir::signature::function_signature_source_map(db, func);
                    let range = sig_map.param_spans.get(idx).copied()?;
                    Some(Location { file, range })
                }
            }
        }
        SymbolTarget::Field { class, field_index } => {
            let range = *item_data::class_source_map(db, class)
                .field_name_spans
                .get(field_index)?;
            Some(Location {
                file: class.file(db),
                range,
            })
        }
        SymbolTarget::Variant {
            enum_loc,
            variant_index,
        } => {
            let range = *item_data::enum_source_map(db, enum_loc)
                .variant_name_spans
                .get(variant_index)?;
            Some(Location {
                file: enum_loc.file(db),
                range,
            })
        }
        SymbolTarget::Method { func } => Some(Location {
            file: func.file(db),
            range: item_data::function_source_map(db, func).name_span,
        }),
        SymbolTarget::InterfaceRequiredMethod {
            iface,
            method_index,
        } => {
            let range = item_data::interface_source_map(db, iface)
                .required_method_spans
                .get(method_index)?
                .name_span;
            Some(Location {
                file: iface.file(db),
                range,
            })
        }
        SymbolTarget::InterfaceField { iface, field_index } => {
            let range = *item_data::interface_source_map(db, iface)
                .field_name_spans
                .get(field_index)?;
            Some(Location {
                file: iface.file(db),
                range,
            })
        }
    }
}

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
use baml_compiler2_ast::Expr;
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
    /// An interface's associated-type declaration, by position.
    AssociatedType {
        iface: InterfaceLoc<'db>,
        assoc_index: usize,
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

    // A dispatching operator token (`+`, `<`, `[`) addresses the method the
    // operation invokes: the matching impl's override when the receiver's
    // static type pins one, else the `baml.ops` interface's declaration.
    if let Some(target) = operator_target_at(db, file, offset, token.kind()) {
        return Some(target);
    }

    // WORD tokens and keyword tokens may both be names in member position
    // (`obj.implements()`), so let the resolvers decide whether the text is
    // actually a symbol.
    if token.kind() != SyntaxKind::WORD && !token.kind().is_keyword() {
        return None;
    }

    let name_text = token.text();
    let name = Name::new(name_text);

    // A token inside a *recorded member span* (`e.message`'s member, a
    // non-root path segment, a constructor key) is a member, full stop:
    // it commits to inference-record resolution and never falls through to
    // bare-name lookup — a field spelled like a visible local must not
    // resolve to the local, and when inference has nothing, honest absence
    // beats wrong-name navigation.
    if let Some(member) = member_position_at(db, file, offset) {
        return member.target;
    }

    // Declaration-position names the scope tree cannot see: a method's own
    // name token (method names never enter a binding scope), an interface's
    // associated-type declarations, and an impl block's associated-type
    // bindings. (Interface fields and required-method names resolve through
    // the same rung via their recorded source-map spans.)
    if let Some(target) = declaration_name_at(db, file, offset) {
        return Some(target);
    }

    if let Some(target) = local_at(db, file, offset, &name) {
        return Some(target);
    }

    match resolve_name_at(db, file, offset, &name) {
        ResolvedName::Item(def) | ResolvedName::Builtin(def) => Some(SymbolTarget::Item(def)),
        // A local that `local_at` did not find has no recoverable binding
        // identity; member resolution cannot apply to a scope name either.
        ResolvedName::Local { .. } => None,
        ResolvedName::Unknown => member_at(db, file, offset, name_text)
            .or_else(|| qualified_item_at(db, file, offset, &token)),
    }
}

/// Resolve a dotted *name* around `token` (`baml.errors.Io` in a type
/// annotation or pattern, where no expression claims the token): the CST
/// dot-chain plus the compiler's qualified-path resolver.
fn qualified_item_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
    token: &baml_compiler_syntax::SyntaxToken,
) -> Option<SymbolTarget<'db>> {
    let chain = crate::syntax::dotted_chain_to(token);
    if chain.len() < 2 {
        return None;
    }
    match baml_compiler2_ppir::resolve::resolve_path_at(db, file, offset, &chain, None) {
        ResolvedName::Item(def) | ResolvedName::Builtin(def) => Some(SymbolTarget::Item(def)),
        ResolvedName::Local { .. } | ResolvedName::Unknown => None,
    }
}

/// The verdict for a token that occupies a recorded member span.
struct MemberPosition<'db> {
    /// The resolved target — `None` when inference has no record for the
    /// member (mid-edit, unresolvable receiver), which still means "this is
    /// a member, do not resolve it as a bare name".
    target: Option<SymbolTarget<'db>>,
}

/// Whether `offset` sits in a member position of the enclosing function
/// body — decided purely by the *recorded spans* of member accesses, dotted
/// path segments, and constructor keys.
fn member_position_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<MemberPosition<'_>> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let func_scope = enclosing_function_scope(index, scope_id)?;
    let func_owner = index.scope_ids[func_scope.index() as usize];
    let item_data::ScopeOwner::Function(func_loc) = item_data::scope_owner(db, func_owner)? else {
        return None;
    };
    let body = baml_compiler2_hir::body::function_body(db, func_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
        return None;
    };
    let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;

    // Innermost claim wins (`a.b(c.d)` — the argument's member span nests
    // inside the call's expression span).
    let mut claim: Option<(baml_compiler2_ast::ExprId, TextRange, Option<usize>)> = None;
    let mut narrower = |candidate: (baml_compiler2_ast::ExprId, TextRange, Option<usize>)| {
        if claim.is_none_or(|(_, previous, _)| candidate.1.len() < previous.len()) {
            claim = Some(candidate);
        }
    };
    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            Expr::MemberAccess { .. } => {
                let member_span = source_map.member_access_member_span(expr_id);
                // The accessor falls back to the whole expression span when
                // no member span was recorded; that would over-claim (the
                // receiver too), so require a strict sub-span.
                if member_span != source_map.expr_span(expr_id)
                    && (member_span.contains(offset) || member_span.end() == offset)
                {
                    narrower((expr_id, member_span, None));
                }
            }
            Expr::Path(segments) if segments.len() >= 2 => {
                for segment_idx in 1..segments.len() {
                    let span = source_map.path_segment_span(expr_id, segment_idx);
                    if span.contains(offset) || span.end() == offset {
                        narrower((expr_id, span, Some(segment_idx)));
                    }
                }
            }
            Expr::Object { fields, .. } => {
                for field in fields {
                    let span = source_map.object_field_name_span(expr_id, field.value);
                    if span.contains(offset) || span.end() == offset {
                        narrower((expr_id, span, None));
                    }
                }
            }
            _ => {}
        }
    }

    let (expr_id, _, segment_idx) = claim?;
    let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_owner);
    let target = match &expr_body.exprs[expr_id] {
        Expr::Object { .. } => constructor_field_at(db, offset, expr_body, &source_map, inference?),
        Expr::MemberAccess { .. } | Expr::Path(_) => {
            let inference = inference?;
            let resolution = match segment_idx {
                Some(idx) => inference
                    .path_resolutions
                    .get(&expr_id)
                    .and_then(|path| path.segments.get(idx))
                    .and_then(|step| step.resolution.as_ref())
                    .or_else(|| inference.member_resolutions.get(&expr_id)),
                None => inference.member_resolutions.get(&expr_id),
            };
            match resolution {
                Some(resolution) => member_resolution_target(db, resolution),
                // No inference record: a package/namespace-qualified NAME
                // (`baml.http.fetch`) rather than a value's member.
                // `resolve_path_at` is inert for value-rooted paths (a local
                // receiver never resolves as a package), so this cannot
                // reintroduce shadowing.
                None => {
                    let (Expr::Path(segments), Some(idx)) =
                        (&expr_body.exprs[expr_id], segment_idx)
                    else {
                        return Some(MemberPosition { target: None });
                    };
                    match baml_compiler2_ppir::resolve::resolve_path_at(
                        db,
                        file,
                        offset,
                        &segments[..=idx],
                        None,
                    ) {
                        ResolvedName::Item(def) | ResolvedName::Builtin(def) => {
                            Some(SymbolTarget::Item(def))
                        }
                        ResolvedName::Local { .. } | ResolvedName::Unknown => None,
                    }
                }
            }
        }
        // Only the three claiming kinds reach here.
        _ => None,
    };
    Some(MemberPosition { target })
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

/// Cursor on a dispatching operator token: the method the operation
/// invokes, from the compiler's own operator table (`hir_ty::ops`) and impl
/// resolver. The receiver's *static* type picks the impl (`1 + 2` navigates
/// to `Add for int`'s `add`); a receiver dispatch cannot pin statically
/// (union, existential, unbound var) falls back to the `baml.ops`
/// interface's method declaration — the static truth for dynamic dispatch.
fn operator_target_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
    kind: SyntaxKind,
) -> Option<SymbolTarget<'_>> {
    use baml_compiler2_ast::Expr;
    use baml_compiler2_hir_ty::ops;

    // Token kinds that can spell a dispatching operator. Everything else
    // exits before any body walk. `<`/`>`/`[`/`]` are ambiguous with
    // generics and array syntax; the enclosing-expression check below
    // disambiguates (only an operator sits in an operator expression's gap).
    if !matches!(
        kind,
        SyntaxKind::PLUS
            | SyntaxKind::MINUS
            | SyntaxKind::STAR
            | SyntaxKind::SLASH
            | SyntaxKind::PERCENT
            | SyntaxKind::AND
            | SyntaxKind::PIPE
            | SyntaxKind::CARET
            | SyntaxKind::LESS_LESS
            | SyntaxKind::GREATER_GREATER
            | SyntaxKind::EQUALS_EQUALS
            | SyntaxKind::NOT_EQUALS
            | SyntaxKind::LESS
            | SyntaxKind::GREATER
            | SyntaxKind::LESS_EQUALS
            | SyntaxKind::GREATER_EQUALS
            | SyntaxKind::PLUS_EQUALS
            | SyntaxKind::MINUS_EQUALS
            | SyntaxKind::STAR_EQUALS
            | SyntaxKind::SLASH_EQUALS
            | SyntaxKind::PERCENT_EQUALS
            | SyntaxKind::AND_EQUALS
            | SyntaxKind::PIPE_EQUALS
            | SyntaxKind::CARET_EQUALS
            | SyntaxKind::LESS_LESS_EQUALS
            | SyntaxKind::GREATER_GREATER_EQUALS
            | SyntaxKind::L_BRACKET
            | SyntaxKind::R_BRACKET
    ) {
        return None;
    }

    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let func_scope = enclosing_function_scope(index, scope_id)?;
    let func_owner = index.scope_ids[func_scope.index() as usize];
    let item_data::ScopeOwner::Function(func_loc) = item_data::scope_owner(db, func_owner)? else {
        return None;
    };
    let body = baml_compiler2_hir::body::function_body(db, func_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
        return None;
    };
    let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;

    // The innermost operator expression whose *operator gap* holds the
    // cursor: the expression's span contains the offset while no operand
    // subexpression does. Innermost wins so `a + b * c` addresses `*`.
    let mut claim: Option<(
        TextRange,
        ops::OperatorDispatch,
        baml_compiler2_ast::ExprId,
        Option<baml_compiler2_ast::ExprId>,
    )> = None;
    let mut narrower = |span: TextRange,
                        dispatch: ops::OperatorDispatch,
                        lhs: baml_compiler2_ast::ExprId,
                        rhs: Option<baml_compiler2_ast::ExprId>| {
        if claim.is_none_or(|(previous, ..)| span.len() < previous.len()) {
            claim = Some((span, dispatch, lhs, rhs));
        }
    };
    let inside = |operand: baml_compiler2_ast::ExprId| {
        let span = source_map.expr_span(operand);
        span.contains(offset) && span.end() != offset
    };
    for (expr_id, expr) in expr_body.exprs.iter() {
        let span = source_map.expr_span(expr_id);
        if !(span.contains(offset) || span.end() == offset) {
            continue;
        }
        match expr {
            Expr::Binary { op, lhs, rhs } => {
                if let Some(dispatch) = ops::binary_operator(*op)
                    && !inside(*lhs)
                    && !inside(*rhs)
                {
                    narrower(span, dispatch, *lhs, Some(*rhs));
                }
            }
            Expr::Unary { op, expr: operand } => {
                if let Some(dispatch) = ops::unary_operator(*op)
                    && !inside(*operand)
                {
                    narrower(span, dispatch, *operand, None);
                }
            }
            Expr::Index { base, index } => {
                if !inside(*base) && !inside(*index) {
                    narrower(span, ops::INDEX_DISPATCH, *base, Some(*index));
                }
            }
            _ => {}
        }
    }

    for (stmt_id, stmt) in expr_body.stmts.iter() {
        let baml_compiler2_ast::Stmt::AssignOp { target, op, value } = stmt else {
            continue;
        };
        let span = source_map.stmt_span(stmt_id);
        if (span.contains(offset) || span.end() == offset) && !inside(*target) && !inside(*value) {
            narrower(span, ops::assign_operator(*op), *target, Some(*value));
        }
    }

    let (_, dispatch, lhs, rhs) = claim?;
    let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_owner)?;
    let lhs_ty = inference.type_of_expr.get(&lhs)?;
    let rhs_ty = rhs.and_then(|rhs| inference.type_of_expr.get(&rhs));
    let func = ops::operator_method(db, dispatch, lhs_ty, rhs_ty)?;
    Some(SymbolTarget::Method { func })
}

/// Cursor on a declaration-position name the scope tree cannot see: a
/// method's own name token (class, impl, and interface bodies alike), an
/// interface's associated-type or field declarations, or an impl block's
/// associated-type binding (which addresses the interface's declaration —
/// the contract the binding fulfils).
fn declaration_name_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<SymbolTarget<'_>> {
    let hit = |span: TextRange| span.contains(offset) || span.end() == offset;

    // Method names. Free functions keep the ordinary name-resolution road;
    // methods have no scope entry, so their declarations resolve here.
    for func in item_data::file_functions(db, file) {
        if hit(item_data::function_source_map(db, *func).name_span)
            && item_data::method_owner(db, *func).is_some()
        {
            return Some(SymbolTarget::Method { func: *func });
        }
    }

    for iface in item_data::file_interfaces(db, file) {
        let source_map = item_data::interface_source_map(db, *iface);
        if !hit(source_map.span) {
            continue;
        }
        for (assoc_index, assoc) in source_map.associated_type_spans.iter().enumerate() {
            if hit(assoc.name_span) {
                return Some(SymbolTarget::AssociatedType {
                    iface: *iface,
                    assoc_index,
                });
            }
        }
        for (field_index, span) in source_map.field_name_spans.iter().enumerate() {
            if hit(*span) {
                return Some(SymbolTarget::InterfaceField {
                    iface: *iface,
                    field_index,
                });
            }
        }
    }

    // `type Assoc = …` in an `implements` block addresses the interface's
    // own declaration.
    for block in item_data::file_impls(db, file) {
        let source_map = item_data::impl_block_source_map(db, *block);
        if !hit(source_map.span) {
            continue;
        }
        let binding_index = source_map
            .associated_type_bindings
            .iter()
            .position(|binding| hit(binding.name_span))?;
        let data = item_data::impl_block_data(db, *block);
        let bound_name = &data.associated_type_bindings.get(binding_index)?.name;
        let facts = baml_compiler2_hir_ty::impls::impl_facts(db, *block).as_ref()?;
        let interface = &facts.interface.name;
        let package = baml_compiler2_hir::package::PackageId::new(db, interface.package().clone());
        let Some(Definition::Interface(iface)) = baml_compiler2_ppir::package_items(db, package)
            .lookup_type(interface.namespace(), interface.name())
        else {
            return None;
        };
        let assoc_index = item_data::interface_data(db, iface)
            .associated_types
            .iter()
            .position(|assoc| &assoc.name == bound_name)?;
        return Some(SymbolTarget::AssociatedType { iface, assoc_index });
    }

    None
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
        SymbolTarget::AssociatedType { iface, assoc_index } => {
            let range = item_data::interface_source_map(db, iface)
                .associated_type_spans
                .get(assoc_index)?
                .name_span;
            Some(Location {
                file: iface.file(db),
                range,
            })
        }
    }
}

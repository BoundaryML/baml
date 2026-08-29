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

    // Literal template text is prose: it addresses the template's driver
    // (tagged form), never a same-spelled local or item.
    if let Some(position) = template_position_at(db, file, offset) {
        return match position {
            TemplatePosition::Driver(func) => Some(SymbolTarget::Item(Definition::Function(func))),
            TemplatePosition::DefaultText => None,
        };
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

/// One member-position claim: the owning function (and its body scope),
/// the claiming expression, the claimed span, and the path segment index.
type Claim<'db> = (
    baml_compiler2_hir::loc::FunctionLoc<'db>,
    baml_compiler2_hir::scope::ScopeId<'db>,
    baml_compiler2_ast::ExprId,
    TextRange,
    Option<usize>,
);

/// Whether `offset` sits in a member position of the enclosing function
/// body — decided purely by the *recorded spans* of member accesses, dotted
/// path segments, and constructor keys.
fn member_position_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<MemberPosition<'_>> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);

    // Desugared expressions (a template's `string.from` concat, an llm
    // function's `baml.sap.parse` companion body) carry spans ALIASING the
    // source they were synthesized from — template prose, prompt text. A
    // span may claim only when the source at that span actually spells the
    // member's name; anything else is an alias, not an access.
    let text = file.text(db);
    let spells =
        |span: TextRange, name: &str| usize::from(span.end()) <= text.len() && &text[span] == name;

    // Innermost claim wins (`a.b(c.d)` — the argument's member span nests
    // inside the call's expression span), searched across EVERY candidate
    // function: an llm function's companions alias the prompt span, and the
    // real interpolation expressions may live in any of the family.
    let mut claim: Option<Claim<'_>> = None;
    let mut seen: Vec<baml_compiler2_hir::loc::FunctionLoc<'_>> = Vec::new();
    for scope_id in scope_candidates_at(db, file, index, offset) {
        let Some(func_scope) = enclosing_function_scope(index, scope_id) else {
            continue;
        };
        let func_owner = index.scope_ids[func_scope.index() as usize];
        let Some(item_data::ScopeOwner::Function(func_loc)) =
            item_data::scope_owner(db, func_owner)
        else {
            continue;
        };
        if seen.contains(&func_loc) {
            continue;
        }
        seen.push(func_loc);
        let body = baml_compiler2_ppir::function_body(db, func_loc);
        let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, func_loc) else {
            continue;
        };
        let mut push_claim =
            |expr_id: baml_compiler2_ast::ExprId, span: TextRange, seg: Option<usize>| {
                if claim
                    .as_ref()
                    .is_none_or(|(_, _, _, previous, _)| span.len() < previous.len())
                {
                    claim = Some((func_loc, func_owner, expr_id, span, seg));
                }
            };
        for (expr_id, expr) in expr_body.exprs.iter() {
            match expr {
                Expr::MemberAccess { member, .. } => {
                    let member_span = source_map.member_access_member_span(expr_id);
                    // The accessor falls back to the whole expression span
                    // when no member span was recorded; that would over-claim
                    // (the receiver too), so require a strict sub-span.
                    if member_span != source_map.expr_span(expr_id)
                        && (member_span.contains(offset) || member_span.end() == offset)
                        && spells(member_span, member.as_str())
                    {
                        push_claim(expr_id, member_span, None);
                    }
                }
                Expr::Path(segments) if segments.len() >= 2 => {
                    for (segment_idx, segment) in segments.iter().enumerate().skip(1) {
                        let span = source_map.path_segment_span(expr_id, segment_idx);
                        if (span.contains(offset) || span.end() == offset)
                            && spells(span, segment.as_str())
                        {
                            push_claim(expr_id, span, Some(segment_idx));
                        }
                    }
                }
                Expr::Object { fields, .. } => {
                    for field in fields {
                        let span = source_map.object_field_name_span(expr_id, field.value);
                        if (span.contains(offset) || span.end() == offset)
                            && spells(span, field.name.as_str())
                        {
                            push_claim(expr_id, span, None);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let (func_loc, func_owner, expr_id, _, segment_idx) = claim?;
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
        return None;
    };
    let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc)?;
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

/// The value whose member a `.` at `dot` reads: the body owner covering the
/// access and the receiver's recorded type.
///
/// Addressing, not resolution — the member itself may not be written yet
/// (`items.` mid-keystroke), which is exactly when completion asks. Both
/// spellings the lowering produces are handled: a `MemberAccess` names its
/// base directly, and a dotted `Path` records the type AFTER each segment,
/// so the segment ending at the dot carries the receiver.
pub fn receiver_at_dot<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    dot: TextSize,
) -> Option<(
    baml_compiler2_hir::body::BodyOwnerId<'db>,
    baml_type::interned::Ty,
)> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let text = file.text(db);
    // A receiver "reaches" the dot when only trivia separates them, so a
    // member access written across lines (`s\n    .len()`) addresses the
    // same value a single-line one does.
    let reaches = |end: TextSize| {
        end <= dot
            && text
                .get(usize::from(end)..usize::from(dot))
                .is_some_and(|between| between.chars().all(|c| c.is_whitespace() || c == '?'))
    };
    let mut seen: Vec<baml_compiler2_hir::loc::FunctionLoc<'db>> = Vec::new();
    // Innermost wins, the same contest `member_position_at` runs: a receiver
    // nested in an argument list claims over the call that encloses it.
    let mut best: Option<(
        baml_compiler2_hir::body::BodyOwnerId<'db>,
        baml_type::interned::Ty,
        TextRange,
    )> = None;
    for scope_id in scope_candidates_at(db, file, index, dot) {
        let Some(func_scope) = enclosing_function_scope(index, scope_id) else {
            continue;
        };
        let func_owner = index.scope_ids[func_scope.index() as usize];
        let Some(item_data::ScopeOwner::Function(func_loc)) =
            item_data::scope_owner(db, func_owner)
        else {
            continue;
        };
        if seen.contains(&func_loc) {
            continue;
        }
        seen.push(func_loc);
        let body = baml_compiler2_ppir::function_body(db, func_loc);
        let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, func_loc) else {
            continue;
        };
        let Some(inference) = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_owner) else {
            continue;
        };
        for (expr_id, expr) in expr_body.exprs.iter() {
            let found = match expr {
                Expr::MemberAccess { base, .. } => reaches(source_map.expr_span(*base).end())
                    .then(|| {
                        inference
                            .type_of_expr
                            .get(base)
                            .map(|ty| (ty.clone(), source_map.expr_span(*base)))
                    })
                    .flatten(),
                // `a?.b` reads a member of the NON-NULL payload — that is
                // what the optional access is for, and the recorded variant
                // is what says so.
                Expr::OptionalMemberAccess { base, .. } => {
                    reaches(source_map.expr_span(*base).end())
                        .then(|| {
                            inference.type_of_expr.get(base).map(|ty| {
                                (
                                    baml_type::interned::Ty::from_plain(
                                        &ty.to_plain().strip_null(),
                                    ),
                                    source_map.expr_span(*base),
                                )
                            })
                        })
                        .flatten()
                }
                Expr::Path(segments) => (0..segments.len()).find_map(|segment_idx| {
                    let span = source_map.path_segment_span(expr_id, segment_idx);
                    if !reaches(span.end()) {
                        return None;
                    }
                    let resolved = inference.path_resolutions.get(&expr_id)?;
                    let step = resolved.segments.get(segment_idx)?;
                    Some((step.ty.clone(), span))
                }),
                _ => None,
            };
            if let Some((ty, span)) = found
                && let Some(owner) = baml_compiler2_hir_ty::ide::owner_for_scope(db, func_owner)
                && best
                    .as_ref()
                    .is_none_or(|(_, _, previous)| span.len() < previous.len())
            {
                best = Some((owner, ty, span));
            }
        }
    }
    // A receiver the checker could not type addresses nothing: `baml.` is a
    // package qualifier, and inference records the error sentinel for the
    // `baml` it could not read as a value. Reporting that as a receiver
    // would answer "no members" for a dot that has plenty.
    best.filter(|(_, ty, _)| !matches!(ty.kind(), baml_type::interned::TyKind::Error { .. }))
        .map(|(owner, ty, _)| (owner, ty))
}

/// The type declaration a qualifier path names, when it names one:
/// `baml.iter.Range`, `root.Point`, or a builtin alias like `int` (whose
/// declaration is the companion class the language defines it by).
///
/// This is the middle rung of a dotted read — between a value receiver and a
/// namespace — and it is what makes UFCS addressable: `int.min(a, b)` names
/// the same member `a.min(b)` does.
pub fn type_qualifier_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    segments: &[Name],
) -> Option<Definition<'db>> {
    // A lowercase alias is the SOURCE spelling of a companion class; the
    // language's own table says which (`PrimitiveType::builtin_class_path`).
    if let [only] = segments
        && let Some(baml_type::BuiltinTypeName::Primitive(primitive)) =
            baml_type::BuiltinTypeName::from_alias(only.as_str())
    {
        let path: Vec<Name> = primitive
            .builtin_class_path()
            .iter()
            .map(Name::new)
            .collect();
        let (name, namespace) = path.split_last()?;
        let baml = baml_compiler2_hir::package::PackageId::new(db, Name::new("baml"));
        return baml_compiler2_ppir::package_items(db, baml).lookup_type(namespace, name);
    }

    let definition = baml_compiler2_ppir::resolve::qualified_type_at(db, file, segments)?;
    matches!(
        definition,
        Definition::Class(_) | Definition::Enum(_) | Definition::Interface(_)
    )
    .then_some(definition)
}

/// A call being written: the callee's function type and the argument labels
/// already spelled.
pub struct CallPosition {
    /// The callee's type, which carries the parameter names and which of
    /// them are optional (and therefore named-only).
    pub callee: baml_type::interned::Ty,
    /// Labels already written in this call — a name is offered once.
    pub written: Vec<Name>,
}

/// The call whose argument list opens at `open_paren`.
///
/// Addressed like [`receiver_at_dot`]: the callee's recorded span ends where
/// the `(` begins, so the call is found by an offset rather than by
/// re-resolving the callee's name.
pub fn call_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    open_paren: TextSize,
) -> Option<CallPosition> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let mut seen: Vec<baml_compiler2_hir::loc::FunctionLoc<'_>> = Vec::new();
    for scope_id in scope_candidates_at(db, file, index, open_paren) {
        let Some((body, source_map, inference)) = body_at(db, index, scope_id, &mut seen) else {
            continue;
        };
        let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        for expr in expr_body.exprs.values() {
            let Expr::Call { callee, args, .. } = expr else {
                continue;
            };
            if source_map.expr_span(*callee).end() != open_paren {
                continue;
            }
            let ty = inference.type_of_expr.get(callee)?;
            return Some(CallPosition {
                callee: ty.clone(),
                written: args.iter().filter_map(|arg| arg.label.clone()).collect(),
            });
        }
    }
    None
}

/// An object literal being written: the class it constructs and the fields
/// already spelled.
pub struct ObjectLiteralPosition<'db> {
    pub class: baml_compiler2_hir::loc::ClassLoc<'db>,
    pub written: Vec<Name>,
}

/// The object literal whose recorded span covers `offset`.
///
/// The literal's TYPE is the recorded one, so a constructor whose name the
/// reader is still typing resolves the same way inference resolved it —
/// nothing here re-resolves a type path.
pub fn object_literal_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<ObjectLiteralPosition<'db>> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let mut seen: Vec<baml_compiler2_hir::loc::FunctionLoc<'db>> = Vec::new();
    let mut best: Option<(ObjectLiteralPosition<'db>, TextRange)> = None;
    for scope_id in scope_candidates_at(db, file, index, offset) {
        let Some((body, source_map, inference)) = body_at(db, index, scope_id, &mut seen) else {
            continue;
        };
        let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        for (expr_id, expr) in expr_body.exprs.iter() {
            let Expr::Object { fields, .. } = expr else {
                continue;
            };
            let span = source_map.expr_span(expr_id);
            if !(span.contains(offset) || span.end() == offset) {
                continue;
            }
            let Some(baml_type::interned::TyKind::Class(qtn, ..)) = inference
                .type_of_expr
                .get(&expr_id)
                .map(baml_type::interned::Ty::kind)
            else {
                continue;
            };
            let facts = baml_compiler2_hir_ty::facts::Facts::new(db);
            let Some(Definition::Class(class)) = facts.definition_of(qtn) else {
                continue;
            };
            // Innermost wins: `Outer { inner: Inner { … } }`.
            if best
                .as_ref()
                .is_none_or(|(_, previous)| span.len() < previous.len())
            {
                best = Some((
                    ObjectLiteralPosition {
                        class,
                        written: fields.iter().map(|field| field.name.clone()).collect(),
                    },
                    span,
                ));
            }
        }
    }
    best.map(|(position, _)| position)
}

/// The body a scope belongs to, with everything a position query reads from
/// it. `seen` keeps a function's family (an llm function and its companions
/// share spans) from being walked twice.
fn body_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    index: &'db FileSemanticIndex<'db>,
    scope_id: FileScopeId,
    seen: &mut Vec<baml_compiler2_hir::loc::FunctionLoc<'db>>,
) -> Option<(
    std::sync::Arc<baml_compiler2_hir::body::FunctionBody>,
    baml_compiler2_ast::AstSourceMap,
    &'db baml_compiler2_hir_ty::infer::InferenceResult<'db>,
)> {
    let func_scope = enclosing_function_scope(index, scope_id)?;
    let func_owner = index.scope_ids[func_scope.index() as usize];
    let item_data::ScopeOwner::Function(func_loc) = item_data::scope_owner(db, func_owner)? else {
        return None;
    };
    if seen.contains(&func_loc) {
        return None;
    }
    seen.push(func_loc);
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc)?;
    let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_owner)?;
    Some((body, source_map, inference))
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
    for scope_id in scope_candidates_at(db, file, index, offset) {
        if let Some(target) = local_in_scope(db, index, scope_id, offset, name) {
            return Some(target);
        }
    }
    None
}

/// [`local_at`] against one candidate scope.
fn local_in_scope<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    index: &FileSemanticIndex<'db>,
    scope_id: FileScopeId,
    offset: TextSize,
    name: &Name,
) -> Option<SymbolTarget<'db>> {
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

/// Every scope that can own code at `offset`: the position-derived pick
/// first (the fast, unambiguous common case), then the body scope of every
/// function whose span covers the offset. An llm function's companions
/// alias their parent's span — `scope_at_offset`'s single pick among that
/// family is arbitrary, so position resolution must try them all.
fn scope_candidates_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    index: &FileSemanticIndex<'_>,
    offset: TextSize,
) -> Vec<FileScopeId> {
    let mut out = vec![index.scope_at_offset(offset, None)];
    for func in item_data::file_functions(db, file) {
        let span = item_data::function_source_map(db, *func).span;
        if !(span.contains(offset) || span.end() == offset) {
            continue;
        }
        if let Some(scope) = index.item_scope(baml_compiler2_hir::scope::ItemScopeOwner::Function(
            func.id(db),
        )) && !out.contains(&scope)
        {
            out.push(scope);
        }
    }
    out
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
            let body = baml_compiler2_ppir::function_body(db, func_loc);
            let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
                return None;
            };
            let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc)?;

            member_access_at(db, offset, token_text, expr_body, &source_map, inference)
                .or_else(|| constructor_field_at(db, offset, expr_body, &source_map, inference))
        }
    }
}

/// Where a cursor inside a backtick template's LITERAL TEXT points
/// (BEP-049): the prose between `${…}` holes is not code, so it addresses
/// the template machinery instead of same-spelled locals or items.
pub(crate) enum TemplatePosition<'db> {
    /// Tagged template (a `prompt`-tagged backtick): the tag driver — hover and
    /// go-to-definition land on the function whose return shape the
    /// template produces.
    Driver(FunctionLoc<'db>),
    /// Untagged template text (plain string interpolation), or a tagged
    /// template whose tag did not resolve: nothing to navigate to; hover
    /// documents the template form.
    DefaultText,
}

/// `Some` when `offset` sits in a backtick template's literal text. The tag
/// expression, `${…}` interpolations, and `${for}`/`${if}` headers are code
/// — they return `None` and resolve under the normal rules.
pub(crate) fn template_position_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<TemplatePosition<'_>> {
    use baml_compiler2_ast::{Expr, TemplateSegment, TemplateTag};

    // Any CODE region of the template under the cursor → normal resolution.
    fn in_code(
        segments: &[TemplateSegment],
        source_map: &baml_compiler2_ast::AstSourceMap,
        hit: &dyn Fn(TextRange) -> bool,
    ) -> bool {
        segments.iter().any(|segment| match segment {
            TemplateSegment::Text(_) => false,
            TemplateSegment::Interp(expr) => hit(source_map.expr_span(*expr)),
            TemplateSegment::For {
                binding,
                collection,
                body,
            } => {
                hit(source_map.pattern_span(*binding))
                    || hit(source_map.expr_span(*collection))
                    || in_code(body, source_map, hit)
            }
            TemplateSegment::CStyleFor {
                init,
                cond,
                step,
                body,
            } => {
                hit(source_map.stmt_span(*init))
                    || hit(source_map.expr_span(*cond))
                    || step.is_some_and(|step| hit(source_map.stmt_span(step)))
                    || in_code(body, source_map, hit)
            }
            TemplateSegment::If {
                branches,
                else_body,
            } => {
                branches.iter().any(|branch| {
                    hit(source_map.expr_span(branch.condition))
                        || in_code(&branch.body, source_map, hit)
                }) || else_body
                    .as_ref()
                    .is_some_and(|body| in_code(body, source_map, hit))
            }
        })
    }

    let hit = |span: TextRange| span.contains(offset) || span.end() == offset;

    // The innermost template across every function whose span holds the
    // cursor — scope lookup alone is ambiguous here, because an llm
    // function's synthesized companions alias the prompt's spans.
    let mut innermost: Option<(
        baml_compiler2_hir::loc::FunctionLoc<'_>,
        baml_compiler2_ast::ExprId,
        TextRange,
    )> = None;
    for func in item_data::file_functions(db, file) {
        if !hit(item_data::function_source_map(db, *func).span) {
            continue;
        }
        let body = baml_compiler2_ppir::function_body(db, *func);
        let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, *func) else {
            continue;
        };
        for (expr_id, expr) in expr_body.exprs.iter() {
            if !matches!(expr, Expr::Template { .. }) {
                continue;
            }
            let span = source_map.expr_span(expr_id);
            if hit(span)
                && innermost
                    .as_ref()
                    .is_none_or(|(_, _, previous)| span.len() < previous.len())
            {
                innermost = Some((*func, expr_id, span));
            }
        }
    }
    // No `Expr::Template` at the cursor: check the SPEC-LOWERED prompt form.
    // An llm function's `prompt:` never becomes a template expression — the
    // backtick flattens straight into the `@spec` companion ("both lower
    // through the same prompt`…` tagged template", lower_cst) — but the
    // synthesized prompt lambda opens a scope MARKED `is_template_body`
    // whose range is exactly the literal. Inside it, code regions are the
    // NON-synthetic nodes (interpolations, `${for}` headers — original
    // spans); everything else is prompt prose, addressed to the `ai.prompt`
    // driver the flattening mirrors.
    let Some((func_loc, template_id, _)) = innermost else {
        return llm_prompt_position_at(db, file, offset);
    };

    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
        return None;
    };
    let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc)?;
    let Expr::Template { tag, segments } = &expr_body.exprs[template_id] else {
        return None;
    };

    if in_code(segments, &source_map, &hit) {
        return None;
    }

    match tag {
        TemplateTag::Custom { tag, .. } => {
            if hit(source_map.expr_span(*tag)) {
                return None;
            }
            let func_owner = item_data::function_scope(db, func_loc)?;
            let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_owner)?;
            match inference.member_resolutions.get(tag) {
                Some(baml_compiler2_hir_ty::infer::MemberResolution::Free { func }) => {
                    Some(TemplatePosition::Driver(*func))
                }
                // Unresolved or non-free tags still block prose words from
                // resolving as code; hover documents the template form.
                _ => Some(TemplatePosition::DefaultText),
            }
        }
        TemplateTag::Default { .. } => Some(TemplatePosition::DefaultText),
    }
}

/// The tag driver of the template whose literal covers `offset` — for BOTH
/// positions (prose and `${…}` code). The driver's `body` callback params
/// (`ctx`, `role`) are name-injected into interpolations by inference with
/// no binding at the use site, so hover resolves them from this signature.
pub(crate) fn template_driver_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'_>> {
    use baml_compiler2_ast::{Expr, TemplateTag};

    let hit = |span: TextRange| span.contains(offset) || span.end() == offset;
    // The llm `prompt:` form — recorded geometry, driver is `ai.prompt`.
    if item_data::file_functions(db, file).iter().any(|func| {
        item_data::llm_prompt_spans(db, *func)
            .as_ref()
            .is_some_and(|spans| hit(spans.literal))
    }) {
        let package = baml_compiler2_hir::package::PackageId::new(db, Name::new("ai"));
        if let Some(Definition::Function(func)) =
            baml_compiler2_ppir::package_items(db, package).lookup_value(&[], &Name::new("prompt"))
        {
            return Some(func);
        }
        return None;
    }
    // A tagged template expression: the recorded tag resolution.
    for func in item_data::file_functions(db, file) {
        if !hit(item_data::function_source_map(db, *func).span) {
            continue;
        }
        let body = baml_compiler2_ppir::function_body(db, *func);
        let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, *func) else {
            continue;
        };
        for (expr_id, expr) in expr_body.exprs.iter() {
            let Expr::Template {
                tag: TemplateTag::Custom { tag, .. },
                ..
            } = expr
            else {
                continue;
            };
            if !hit(source_map.expr_span(expr_id)) {
                continue;
            }
            let func_owner = item_data::function_scope(db, *func)?;
            let inference = baml_compiler2_hir_ty::ide::infer_for_scope(db, func_owner)?;
            if let Some(baml_compiler2_hir_ty::infer::MemberResolution::Free { func }) =
                inference.member_resolutions.get(tag)
            {
                return Some(*func);
            }
        }
    }
    None
}

/// The spec-lowered llm-prompt arm of [`template_position_at`]: an llm
/// function's `prompt:` never becomes a template expression (the backtick
/// flattens straight into the `@spec` companion, whose spans alias the
/// literal), so prose-vs-code comes from the RECORDED prompt geometry
/// (`llm_prompt_spans`, captured at CST lowering). Prose addresses the
/// `ai.prompt` driver — the documented lowering contract ("both lower
/// through the same prompt`…` tagged template").
fn llm_prompt_position_at(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<TemplatePosition<'_>> {
    let hit = |span: TextRange| span.contains(offset) || span.end() == offset;
    let spans = item_data::file_functions(db, file)
        .iter()
        .find_map(|func| {
            item_data::llm_prompt_spans(db, *func)
                .as_ref()
                .filter(|spans| hit(spans.literal))
        })?;
    if spans.code.iter().any(|&span| hit(span)) {
        // `${…}` interpolations and block tags are code — normal rules.
        return None;
    }

    let package = baml_compiler2_hir::package::PackageId::new(db, Name::new("ai"));
    match baml_compiler2_ppir::package_items(db, package).lookup_value(&[], &Name::new("prompt")) {
        Some(Definition::Function(func)) => Some(TemplatePosition::Driver(func)),
        _ => Some(TemplatePosition::DefaultText),
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
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
        return None;
    };
    let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc)?;

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

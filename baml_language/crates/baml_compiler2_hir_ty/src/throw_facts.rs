//! Per-file throw-fact EXTRACTION (BEP-007's expensive half), relocated from
//! TIR's `throw_inference` during the TIR retirement.
//!
//! Extracts, for every function a file defines, its direct throw facts
//! (declared `throws` clause, else syntactic `throw` sites typed without
//! body inference), its same-package call edges, and whether a declared
//! clause firewalls propagation. The bytecode cache persists this output
//! verbatim per file and seeds it back on warm compiles; the solver half
//! that consumed it lives on only as the cache's change-detection input
//! (`callable_throws` is the runtime throws surface now).

use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::{AstSourceMap, BodyNode, Expr, ExprBody, Literal};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};
use baml_type::{Ty, TyAttr, throw_facts::FunctionThrowFacts};

use crate::{
    lower::qualify_def,
    package_interface::{ThrowFact, throw_set_key},
};

/// Per-file extraction output, wrapped so the tracked query can return by
/// reference (comparison-based salsa `Update`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileThrowFacts(pub Vec<FunctionThrowFacts>);

// Safety: comparison-based replacement for Salsa early cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for FileThrowFacts {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: pointer is Salsa-owned and valid for replacement.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Extract throw-analysis facts for every function defined in `file`
/// (top-level functions and class methods; interface default methods are
/// package-level defaults handled by the interface machinery, not solver
/// nodes).
///
/// This is the expensive half of throw inference (PPIR bodies + signature
/// lowering), isolated per file so it can be (a) memoized at file
/// granularity and (b) seeded from a previous compile: when the database
/// carries [`baml_compiler2_hir::inputs::SeededThrowFacts`] for this file, the seeds are
/// returned verbatim and the body is never walked. Facts are a pure
/// function of file content + name resolution; the bytecode cache only
/// seeds files whose content is unchanged and whose resolution-relevant
/// dependencies didn't change signature.
#[salsa::tracked(returns(ref))]
pub fn file_throw_facts(
    db: &dyn baml_compiler2_ppir::Db,
    file: baml_base::SourceFile,
) -> FileThrowFacts {
    // `seeds.by_path(db)` is a *tracked* read of the `SeededThrowFacts` input:
    // databases that seed (e.g. `ProjectDatabase`) hold the input from
    // construction (empty until seeded), so this memo records a dependency on
    // the seed map and a later `set_seeded_throw_facts` reliably invalidates it.
    // An absent/empty map yields no hit and falls through to honest extraction.
    if let Some(seeds) = db.seeded_throw_facts() {
        if let Some(facts) = seeds.by_path(db).get(&file.path(db).display().to_string()) {
            return FileThrowFacts(facts.clone());
        }
    }

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let func_ns = pkg_info.namespace_path;

    // Class methods (including `implements`-block methods, which
    // `class_data.methods` flattens in) and interface default methods are
    // not top-level solver entries under their own names.
    let mut member_ids = std::collections::HashSet::new();
    for class_loc in baml_compiler2_ppir::item_data::file_classes(db, file) {
        let class_data = baml_compiler2_ppir::item_data::class_data(db, *class_loc);
        member_ids.extend(class_data.methods.iter().copied());
    }
    for iface_loc in baml_compiler2_ppir::item_data::file_interfaces(db, file) {
        let iface_data = baml_compiler2_ppir::item_data::interface_data(db, *iface_loc);
        member_ids.extend(iface_data.default_methods.iter().copied());
    }
    // Out-of-body `implement<…> I for Y { … }` blocks: their methods dispatch
    // through the interface registry and were never solver nodes under their
    // bare names. (In-body `implements` methods are already covered above —
    // `class_data.methods` flattens them in.)
    for impl_loc in baml_compiler2_ppir::item_data::file_free_impls(db, file) {
        let block = baml_compiler2_ppir::item_data::impl_block_data(db, *impl_loc);
        member_ids.extend(block.methods.iter().copied());
    }

    let mut out = Vec::new();

    for func_loc in baml_compiler2_ppir::item_data::file_functions(db, file) {
        // Required interface methods are signature-only items; the throw
        // fixpoint saw no such functions before they were items, and their
        // conformance-checked implementors carry the real bodies.
        if baml_compiler2_ppir::item_data::is_required_interface_method(db, *func_loc) {
            continue;
        }
        if member_ids.contains(func_loc) {
            continue;
        }
        let func_loc = *func_loc;
        let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
        let short_name = &func_data.name;
        let key = function_key(db, func_loc, short_name);

        let (direct, has_declared_contract) = extract_direct_and_declared(
            db,
            pkg_items,
            &func_ns,
            func_loc,
            &func_data.type_refs,
            &func_data.params,
        );

        let body = baml_compiler2_ppir::function_body(db, func_loc);
        let call_edges =
            if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                collect_call_targets(expr_body)
            } else {
                BTreeSet::new()
            };

        out.push(FunctionThrowFacts {
            key,
            direct,
            call_edges,
            has_declared_contract,
        });
    }

    for class_loc in baml_compiler2_ppir::item_data::file_classes(db, file) {
        let class_data = baml_compiler2_ppir::item_data::class_data(db, *class_loc);
        let class_name = &class_data.name;
        for &func_loc in &class_data.methods {
            let method_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            let method_name = &method_data.name;
            // Key as "ClassName.method_name" (with namespace prefix if any).
            let method_short = Name::new(format!("{class_name}.{method_name}"));
            let key = function_key(db, func_loc, &method_short);

            let (direct, has_declared_contract) = extract_direct_and_declared(
                db,
                pkg_items,
                &func_ns,
                func_loc,
                &method_data.type_refs,
                &method_data.params,
            );

            let body = baml_compiler2_ppir::function_body(db, func_loc);
            let call_edges =
                if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                    // Rewrite "self.X" call targets to "ClassName.X" so edges
                    // connect to the correct graph nodes.
                    collect_call_targets(expr_body)
                        .into_iter()
                        .map(|t| rewrite_self_target(&t, class_name))
                        .collect()
                } else {
                    BTreeSet::new()
                };

            out.push(FunctionThrowFacts {
                key,
                direct,
                call_edges,
                has_declared_contract,
            });
        }
    }

    FileThrowFacts(out)
}

/// Compute a function's or method's direct throw set and whether it declares a
/// `throws` contract. A declared `throws` clause is a *closed* contract (#3983):
/// its lowered set is exactly what the function exposes to callers and replaces
/// any body-derived facts (the firewall); otherwise the direct set is collected
/// from the body. Shared by the top-level-function and class-method passes,
/// which differ only in which item supplies `params`.
fn extract_direct_and_declared<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    type_refs: &baml_compiler2_hir::type_ref::TypeRefStore,
    params: &[baml_compiler2_ppir::item_data::FunctionParamData],
) -> (BTreeSet<ThrowFact>, bool) {
    let sig = baml_compiler2_ppir::function_signature(db, func_loc);
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let file = func_loc.file(db);

    // The method's full frame and bounds (its own params plus the enclosing
    // owner's) so a `T`-reference or `T.member` projection in the clause or
    // a parameter type resolves instead of erasing.
    let scope_ctx = || {
        crate::lower::lower_ctx_for_file(db, file)
            .with_frame(crate::lower::function_generic_frame(db, func_loc))
            .with_bounds(crate::lower::function_generic_bounds(db, func_loc))
    };

    let declared_throws = sig.throws.as_ref().map(|te| {
        // A raw AST clause lowers through a scratch store (the MIR pattern).
        let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
        let id = builder.lower(te);
        let (store, _spans) = builder.finish();
        let lowered = scope_ctx().lower_type_ref(&store, id).to_plain();
        flatten_declared_ty_to_facts(&lowered)
    });

    let direct = if let Some(declared) = declared_throws.clone() {
        declared
    } else if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
        let param_types = lower_param_types(&scope_ctx(), type_refs, params);
        collect_direct_throws(db, pkg_items, ns_context, func_loc, expr_body, &param_types)
    } else {
        BTreeSet::new()
    };

    (direct, declared_throws.is_some())
}

fn function_key<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    short_name: &Name,
) -> Name {
    let file = func.file(db);
    let pkg = baml_compiler2_hir::file_package::file_package(db, file);
    throw_set_key(&pkg.namespace_path, short_name)
}

pub fn collect_direct_throws<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    body: &ExprBody,
    param_types: &[(Name, Ty)],
) -> BTreeSet<ThrowFact> {
    let mut facts = BTreeSet::new();
    let catch_arm_bodies = collect_catch_arm_bodies(body);
    // Rethrow detection is scoped by source span (a `throw e` rethrows only
    // inside the `catch (e)` arm that binds it), so fetch the span-bearing
    // source map — but only when the body actually has a `catch`. A `catch`-free
    // body needs no spans and stays insensitive to whitespace-only edits.
    let source_map = (!catch_arm_bodies.is_empty())
        .then(|| baml_compiler2_ppir::function_body_source_map(db, func_loc))
        .flatten();

    // Walk the body structurally rather than scanning the arena: a `throw`
    // written inside a lambda belongs to that lambda's throw set, not to this
    // function's. Only calling the lambda transfers the effect.
    for node in body_nodes(body) {
        let value = match node {
            BodyNode::Expr(id) => match &body.exprs[id] {
                Expr::Throw { value } => *value,
                _ => continue,
            },
            BodyNode::Stmt(id) => match &body.stmts[id] {
                baml_compiler2_ast::Stmt::Throw { value } => *value,
                _ => continue,
            },
        };
        if !is_catch_rethrow(value, body, source_map.as_ref(), &catch_arm_bodies) {
            facts.insert(throw_fact_from_expr(
                db,
                pkg_items,
                ns_context,
                param_types,
                value,
                body,
            ));
        }
    }

    facts
}

/// Lower a function's parameter declarations to `(name, Ty)` pairs.
///
/// Used so a `throw <param>` expression can be typed from the declaration site
/// without invoking body inference (which would cycle back through throw-set
/// computation). Parameters without a written type (e.g. `self`) are skipped.
fn lower_param_types(
    ctx: &crate::lower::LowerCtx<'_>,
    type_refs: &baml_compiler2_hir::type_ref::TypeRefStore,
    params: &[baml_compiler2_ppir::item_data::FunctionParamData],
) -> Vec<(Name, Ty)> {
    params
        .iter()
        .filter_map(|param| {
            let type_ref = param.type_ref?;
            let ty = ctx.lower_type_ref(type_refs, type_ref).to_plain();
            Some((param.name.clone(), ty))
        })
        .collect()
}

/// Whether `value` is the operand of a *rethrow* — a `throw e` whose `e` names
/// a `catch` clause binding in scope, as in `catch (e) { _ => throw e }`.
///
/// A rethrow re-raises a value already accounted for (the caught expression's
/// throws are collected independently), so it contributes no new fact; the bare
/// binding can't be resolved to a nameable type anyway. The check is scoped by
/// span containment: `throw e` is a rethrow only when it lies *inside* a
/// `catch (e)` arm body. That is what distinguishes it from `throw e` where `e`
/// is a same-named parameter or local *outside* the catch — which is a real
/// throw of `e`, and treating it as a rethrow would drop its type from the set.
fn is_catch_rethrow(
    value: baml_compiler2_ast::ExprId,
    body: &ExprBody,
    source_map: Option<&AstSourceMap>,
    catch_arm_bodies: &[(&str, baml_compiler2_ast::ExprId)],
) -> bool {
    let Some(source_map) = source_map else {
        return false;
    };
    let Expr::Path(segments) = &body.exprs[value] else {
        return false;
    };
    let [name] = segments.as_slice() else {
        return false;
    };
    let value_span = source_map.expr_span(value);
    catch_arm_bodies.iter().any(|(binding, arm_body)| {
        *binding == name.as_str() && source_map.expr_span(*arm_body).contains_range(value_span)
    })
}

pub fn collect_call_targets(body: &ExprBody) -> BTreeSet<Name> {
    let mut targets = BTreeSet::new();
    // Structural, not a flat arena scan: a call made inside a lambda body is an
    // edge from the *lambda*, so charging it to the enclosing function would
    // give the function the callee's throws without it ever calling anything.
    for node in body_nodes(body) {
        let BodyNode::Expr(id) = node else { continue };
        if let Expr::Call { callee, .. } = &body.exprs[id]
            && let Some(path) = expr_to_path(*callee, body)
        {
            let joined = path.iter().map(Name::as_str).collect::<Vec<_>>().join(".");
            targets.insert(Name::new(joined));
        }
    }
    targets
}

/// Convert a thrown expression to a `Ty` directly, using `pkg_items` to resolve
/// paths to their actual types (enum variants, classes, etc).
fn throw_fact_from_expr<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    param_types: &[(Name, Ty)],
    expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
) -> Ty {
    let fact = match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(_)) => Some(Ty::String {
            attr: TyAttr::default(),
        }),
        Expr::Literal(Literal::Int(_)) => Some(Ty::Int {
            attr: TyAttr::default(),
        }),
        Expr::Literal(Literal::Float(_)) => Some(Ty::Float {
            attr: TyAttr::default(),
        }),
        Expr::Literal(Literal::Bool(_)) => Some(Ty::Bool {
            attr: TyAttr::default(),
        }),
        Expr::Null => Some(Ty::Null {
            attr: TyAttr::default(),
        }),
        Expr::Path(segments) if !segments.is_empty() => {
            // A thrown bare identifier naming a parameter (`throw s`) carries
            // that parameter's declared type — it is a value, not a type path.
            if let [name] = segments.as_slice()
                && let Some((_, ty)) = param_types.iter().find(|(param, _)| param == name)
            {
                Some(ty.clone())
            } else {
                resolve_path_to_ty(db, pkg_items, ns_context, segments)
            }
        }
        Expr::MemberAccess { .. } => expr_to_path(expr_id, body)
            .and_then(|segments| resolve_path_to_ty(db, pkg_items, ns_context, &segments)),
        Expr::Object {
            type_name: path, ..
        } => resolve_path_to_ty(db, pkg_items, ns_context, path.segments()),
        _ => None,
    };
    // This lightweight, cycle-avoiding pass can't statically name every thrown
    // value: a call/binary/array/conditional result, or an unresolved path, has
    // no name to give. Over-approximate to the top type `unknown`
    // (`Unknown`): it is sound (a `catch` must handle the top type) and
    // has a runtime representation. Full inference types these precisely for
    // diagnostics; this set only feeds runtime throws metadata, where a
    // conservative bound is correct.
    fact.unwrap_or(Ty::Unknown {
        attr: TyAttr::default(),
    })
}

/// Rewrite a call target name from `self.X` to `ClassName.X`.
/// Other targets are returned unchanged.
fn rewrite_self_target(target: &Name, class_name: &Name) -> Name {
    let s = target.as_str();
    if let Some(rest) = s.strip_prefix("self.") {
        Name::new(format!("{class_name}.{rest}"))
    } else {
        target.clone()
    }
}

/// Resolve a path like `["Status", "HttpError"]` or `["ns", "Status", "HttpError"]`
/// or `["Status"]` to a `Ty`, or `None` when the path names nothing this pass
/// can see (or names a non-type definition).
fn resolve_path_to_ty<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    segments: &[Name],
) -> Option<Ty> {
    // Try treating the last segment as an enum variant and the prefix as
    // the enum path (e.g. `Status.HttpError` or `root.Status.Failed`).
    if segments.len() >= 2 {
        let enum_path = &segments[..segments.len() - 1];
        let variant = &segments[segments.len() - 1];
        let enum_name = enum_path.last().expect("enum_path is non-empty");
        let enum_ns = &enum_path[..enum_path.len() - 1];
        if let Some(def @ Definition::Enum(_)) =
            lookup_type_in_scope(db, pkg_items, ns_context, enum_ns, enum_name)
        {
            let qtn = qualify_def(db, def, enum_name);
            return Some(Ty::EnumVariant(qtn, variant.clone(), TyAttr::default()));
        }
    }

    // Otherwise resolve the full path as a type.
    let name = segments.last().expect("segments is non-empty");
    let seg_ns = &segments[..segments.len() - 1];
    let def = lookup_type_in_scope(db, pkg_items, ns_context, seg_ns, name)?;
    match def {
        Definition::Class(_) => Some(Ty::Class(
            qualify_def(db, def, name),
            vec![],
            TyAttr::default(),
        )),
        Definition::Enum(_) => Some(Ty::Enum(qualify_def(db, def, name), TyAttr::default())),
        Definition::TypeAlias(_) => {
            Some(Ty::TypeAlias(qualify_def(db, def, name), TyAttr::default()))
        }
        // `lookup_type_in_scope` searches the type namespace, so these are
        // unreachable for its results; either way they are not nameable as a
        // thrown type. Spelled out rather than wildcarded so a new
        // `Definition` variant has to be classified here.
        Definition::Interface(_)
        | Definition::Function(_)
        | Definition::TemplateString(_)
        | Definition::Client(_)
        | Definition::RetryPolicy(_)
        | Definition::Let(_) => None,
    }
}

/// Resolve `type_name` within namespace `ns`, applying the same fallbacks as
/// declaration path lowering so throw-set recovery resolves a path identically
/// whether it appears as an enum-variant prefix or a plain type:
///
/// - a bare name (`ns` empty) is tried in `ns_context` first, then at the
///   package root;
/// - a `root.`-prefixed `ns` is retried against the current package with the
///   prefix stripped (own-package alias);
/// - any other leading segment is treated as a sibling package name.
fn lookup_type_in_scope<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    ns: &[Name],
    type_name: &Name,
) -> Option<Definition<'db>> {
    let direct = if ns.is_empty() && !ns_context.is_empty() {
        pkg_items
            .lookup_type(ns_context, type_name)
            .or_else(|| pkg_items.lookup_type(ns, type_name))
    } else {
        pkg_items.lookup_type(ns, type_name)
    };
    direct.or_else(|| {
        let (first, rest) = ns.split_first()?;
        if first.as_str() == "root" {
            pkg_items.lookup_type(rest, type_name)
        } else {
            let pkg_id = PackageId::new(db, first.clone());
            let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
            pkg.lookup_type(rest, type_name)
        }
    })
}

/// Collect, for each `catch` clause binding, the `(binding-name, arm-body-expr)`
/// pairs whose arm-body span scopes the rethrows of that binding.
fn collect_catch_arm_bodies(body: &ExprBody) -> Vec<(&str, baml_compiler2_ast::ExprId)> {
    let mut arms = Vec::new();
    // Structural: a `catch` inside a lambda scopes only that lambda's rethrows.
    for node in body_nodes(body) {
        let BodyNode::Expr(id) = node else { continue };
        if let Expr::Catch { clauses, .. } = &body.exprs[id] {
            for clause in clauses {
                if let Some(name) = body.patterns[clause.binding].binding_name(&body.patterns) {
                    for &arm_id in &clause.arms {
                        arms.push((name.as_str(), body.catch_arms[arm_id].body));
                    }
                }
            }
        }
    }
    arms
}

/// Every node of `body` that belongs to *this* function, in pre-order.
///
/// Empty when the body has no root expression (a parse failure), which is the
/// same set a flat arena scan would have found meaningful.
fn body_nodes(body: &ExprBody) -> Vec<BodyNode> {
    body.root_expr
        .map(|root| body.reachable_excluding_lambdas(root))
        .unwrap_or_default()
}

fn expr_to_path(expr_id: baml_compiler2_ast::ExprId, body: &ExprBody) -> Option<Vec<Name>> {
    match &body.exprs[expr_id] {
        Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
        Expr::MemberAccess { base, member } => {
            let mut base_path = expr_to_path(*base, body)?;
            base_path.push(member.clone());
            Some(base_path)
        }
        _ => None,
    }
}

/// Flatten a DECLARED `throws` clause into its leaf facts. Unlike
/// `package_interface::flatten_ty_to_facts` (which preserves the surface as
/// written for the caller-facing `callable_throws` sets), the extraction
/// facts widen literal members to their primitives and drop `void` alongside
/// `never` — the shape TIR's solver seeded its nodes with, which the
/// bytecode cache's persisted fact format still assumes.
pub fn flatten_declared_ty_to_facts(ty: &Ty) -> BTreeSet<ThrowFact> {
    let mut out = BTreeSet::new();
    collect_widened_leaf_types(ty, &mut out);
    out
}

fn collect_widened_leaf_types(ty: &Ty, out: &mut BTreeSet<Ty>) {
    match ty {
        // Compound types: decompose
        Ty::Union(members, _) => {
            for member in members {
                collect_widened_leaf_types(member, out);
            }
        }
        // Literal types: widen to primitive for throw fact purposes
        Ty::Literal(lit, _, _) => {
            let attr = TyAttr::default();
            out.insert(match lit {
                Literal::Int(_) => Ty::Int { attr },
                Literal::Bigint(_) => Ty::Bigint { attr },
                Literal::Float(_) => Ty::Float { attr },
                Literal::String(_) => Ty::String { attr },
                Literal::Bool(_) => Ty::Bool { attr },
            });
        }
        // Bottom/void: no facts
        Ty::Never { .. } | Ty::Void { .. } => {}
        // Everything else: keep as-is
        _ => {
            out.insert(ty.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_ast::{Pattern, Stmt, TypeExprKind};
    use text_size::TextRange;

    use super::*;

    fn throwing_expr(body: &mut ExprBody, message: &str) -> baml_compiler2_ast::ExprId {
        let value = body
            .exprs
            .alloc(Expr::Literal(Literal::String(message.to_owned())));
        body.exprs.alloc(Expr::Throw { value })
    }

    #[test]
    fn hidden_runtime_operands_are_structural_throw_fact_nodes_once() {
        let mut body = ExprBody::default();
        let callee = body.exprs.alloc(Expr::Path(vec![Name::new("f")]));
        let call_throw = throwing_expr(&mut body, "call");
        let call = body.exprs.alloc(Expr::Call {
            callee,
            type_args: vec![
                TypeExprKind::Unreflect {
                    operand: Some(call_throw),
                    attrs: Vec::new(),
                }
                .at(TextRange::default()),
            ],
            args: Vec::new(),
        });
        let call_stmt = body.stmts.alloc(Stmt::Expr(call));

        let binding_throw = throwing_expr(&mut body, "binding");
        let binding = body.stmts.alloc(Stmt::TypeBinding {
            name: Name::new("T"),
            value: TypeExprKind::Unreflect {
                operand: Some(binding_throw),
                attrs: Vec::new(),
            }
            .at(TextRange::default()),
        });

        let scrutinee = body.exprs.alloc(Expr::Literal(Literal::Int(1)));
        let pattern_throw = throwing_expr(&mut body, "pattern");
        let pattern = body.patterns.alloc(Pattern::Unreflect(pattern_throw));
        let pattern_test = body.exprs.alloc(Expr::Is { scrutinee, pattern });
        let root = body.exprs.alloc(Expr::Block {
            stmts: vec![call_stmt, binding],
            tail_expr: Some(pattern_test),
        });
        body.root_expr = Some(root);

        let nodes = body_nodes(&body);
        for hidden in [call_throw, binding_throw, pattern_throw] {
            assert_eq!(
                nodes
                    .iter()
                    .filter(|node| **node == BodyNode::Expr(hidden))
                    .count(),
                1,
                "hidden operand {hidden:?} must participate exactly once",
            );
        }
    }
}

use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, ExprId, Stmt, StmtId};
use baml_compiler2_hir::package::PackageId;
use rustc_hash::FxHashMap;

use crate::{
    throw_inference::{flatten_ty_to_facts, function_throw_sets},
    ty::{Freshness, QualifiedTypeName, Ty, TyAttr},
};

pub(crate) fn collect_effective_throws<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
    catch_residual_throws: &FxHashMap<ExprId, BTreeSet<Ty>>,
    include_typevars: bool,
    unknown_on_unresolved_call: bool,
) -> BTreeSet<Ty> {
    let mut out = BTreeSet::new();
    if let Some(root) = body.root_expr {
        collect_effective_throws_from_expr(
            db,
            package_id,
            root,
            body,
            expressions,
            catch_residual_throws,
            include_typevars,
            unknown_on_unresolved_call,
            &mut out,
        );
    }
    out
}

#[derive(Clone, Copy)]
struct CallResolutionOptions {
    include_typevars: bool,
    unknown_on_unresolved_call: bool,
}

#[allow(clippy::too_many_arguments)]
fn collect_effective_throws_from_expr<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    expr_id: ExprId,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
    catch_residual_throws: &FxHashMap<ExprId, BTreeSet<Ty>>,
    include_typevars: bool,
    unknown_on_unresolved_call: bool,
    out: &mut BTreeSet<Ty>,
) {
    match &body.exprs[expr_id] {
        Expr::Throw { value } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *value,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_throw_facts_from_value(*value, body, expressions, out);
        }
        Expr::Call { callee, args } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *callee,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            for arg in args {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *arg,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
            collect_effective_throws_from_call(
                db,
                package_id,
                *callee,
                body,
                expressions,
                CallResolutionOptions {
                    include_typevars,
                    unknown_on_unresolved_call,
                },
                out,
            );
        }
        Expr::Catch { clauses, .. } => {
            if let Some(residual) = catch_residual_throws.get(&expr_id) {
                out.extend(residual.iter().cloned());
            }
            for clause in clauses {
                for arm_id in &clause.arms {
                    let arm = &body.catch_arms[*arm_id];
                    collect_effective_throws_from_expr(
                        db,
                        package_id,
                        arm.body,
                        body,
                        expressions,
                        catch_residual_throws,
                        include_typevars,
                        unknown_on_unresolved_call,
                        out,
                    );
                }
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *condition,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_effective_throws_from_expr(
                db,
                package_id,
                *then_branch,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            if let Some(else_expr) = else_branch {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *else_expr,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *scrutinee,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            for arm_id in arms {
                let arm = &body.match_arms[*arm_id];
                if let Some(guard) = arm.guard {
                    collect_effective_throws_from_expr(
                        db,
                        package_id,
                        guard,
                        body,
                        expressions,
                        catch_residual_throws,
                        include_typevars,
                        unknown_on_unresolved_call,
                        out,
                    );
                }
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    arm.body,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *lhs,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_effective_throws_from_expr(
                db,
                package_id,
                *rhs,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
        }
        Expr::Unary { expr, .. } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *expr,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
        }
        Expr::Object {
            fields, spreads, ..
        } => {
            for (_, value) in fields {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *value,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
            for spread in spreads {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    spread.expr,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Expr::Array { elements } => {
            for elem in elements {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *elem,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Expr::Map { entries } => {
            for (key, value) in entries {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *key,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *value,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Expr::Block { stmts, tail_expr } => {
            for stmt_id in stmts {
                collect_effective_throws_from_stmt(
                    db,
                    package_id,
                    *stmt_id,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
            if let Some(tail) = tail_expr {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *tail,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Expr::FieldAccess { base, .. } | Expr::OptionalFieldAccess { base, .. } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *base,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
        }
        Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *base,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_effective_throws_from_expr(
                db,
                package_id,
                *index,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
        }
        Expr::OptionalCall { callee, args } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *callee,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            for arg in args {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *arg,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Expr::OptionalChain { expr } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *expr,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
        }
        Expr::Lambda(_)
        | Expr::Literal(_)
        | Expr::ByteStringLiteral(_)
        | Expr::Null
        | Expr::Path(_)
        | Expr::Missing => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_effective_throws_from_stmt<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    stmt_id: StmtId,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
    catch_residual_throws: &FxHashMap<ExprId, BTreeSet<Ty>>,
    include_typevars: bool,
    unknown_on_unresolved_call: bool,
    out: &mut BTreeSet<Ty>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => collect_effective_throws_from_expr(
            db,
            package_id,
            *expr,
            body,
            expressions,
            catch_residual_throws,
            include_typevars,
            unknown_on_unresolved_call,
            out,
        ),
        Stmt::Let { initializer, .. } => {
            if let Some(init) = initializer {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *init,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Stmt::While {
            condition,
            body: while_body,
            after,
            ..
        } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *condition,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_effective_throws_from_expr(
                db,
                package_id,
                *while_body,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            if let Some(after_stmt) = after {
                collect_effective_throws_from_stmt(
                    db,
                    package_id,
                    *after_stmt,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Stmt::For {
            collection,
            body: for_body,
            ..
        } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *collection,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_effective_throws_from_expr(
                db,
                package_id,
                *for_body,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_effective_throws_from_expr(
                    db,
                    package_id,
                    *expr,
                    body,
                    expressions,
                    catch_residual_throws,
                    include_typevars,
                    unknown_on_unresolved_call,
                    out,
                );
            }
        }
        Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *target,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_effective_throws_from_expr(
                db,
                package_id,
                *value,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
        }
        Stmt::Throw { value } => {
            collect_effective_throws_from_expr(
                db,
                package_id,
                *value,
                body,
                expressions,
                catch_residual_throws,
                include_typevars,
                unknown_on_unresolved_call,
                out,
            );
            collect_throw_facts_from_value(*value, body, expressions, out);
        }
        Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
    }
}

fn collect_effective_throws_from_call<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    callee_expr_id: ExprId,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
    options: CallResolutionOptions,
    out: &mut BTreeSet<Ty>,
) {
    let type_level_facts = expressions.get(&callee_expr_id).and_then(|ty| match ty {
        Ty::Function { throws, .. } => Some(flatten_ty_to_facts(throws)),
        _ => None,
    });

    if let Some(facts) = type_level_facts {
        let filtered: BTreeSet<Ty> = facts
            .iter()
            .filter(|fact| options.include_typevars || !matches!(fact, Ty::TypeVar(_, _)))
            .cloned()
            .collect();
        if !filtered.is_empty() {
            out.extend(filtered);
            return;
        }
        if facts.iter().any(|fact| matches!(fact, Ty::TypeVar(_, _))) && !options.include_typevars {
            return;
        }
    }

    if let Some(target) = call_target_name(callee_expr_id, body, expressions) {
        let throws = function_throw_sets(db, package_id);
        if let Some(transitive) = throws.transitive_for(&target) {
            out.extend(transitive.iter().cloned());
            return;
        }
    }

    if options.unknown_on_unresolved_call
        && !matches!(expressions.get(&callee_expr_id), Some(Ty::Function { .. }))
    {
        out.insert(Ty::Unknown {
            attr: TyAttr::default(),
        });
    }
}

fn collect_throw_facts_from_value(
    value_expr_id: ExprId,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
    out: &mut BTreeSet<Ty>,
) {
    if let Some(thrown_ty) = expressions.get(&value_expr_id) {
        out.extend(flatten_ty_to_facts(thrown_ty));
        return;
    }

    match &body.exprs[value_expr_id] {
        Expr::Literal(lit) => {
            out.extend(flatten_ty_to_facts(&Ty::Literal(
                lit.clone(),
                Freshness::Regular,
                TyAttr::default(),
            )));
        }
        Expr::ByteStringLiteral(_) => {
            out.insert(Ty::Primitive(
                crate::ty::PrimitiveType::Uint8Array,
                TyAttr::default(),
            ));
        }
        Expr::Null => {
            out.insert(Ty::Primitive(
                crate::ty::PrimitiveType::Null,
                TyAttr::default(),
            ));
        }
        _ => {
            out.insert(Ty::Unknown {
                attr: TyAttr::default(),
            });
        }
    }
}

fn call_target_name(
    callee_expr_id: ExprId,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
) -> Option<Name> {
    match &body.exprs[callee_expr_id] {
        Expr::Path(segments) if !segments.is_empty() => Some(path_name(segments)),
        Expr::FieldAccess { base, field } => {
            if let Some(Ty::Class(qn, _)) = expressions.get(base) {
                Some(class_method_key(qn, field))
            } else {
                let mut segments = expr_to_path_segments(*base, body)?;
                segments.push(field.clone());
                Some(path_name(&segments))
            }
        }
        _ => None,
    }
}

fn expr_to_path_segments(expr_id: ExprId, body: &ExprBody) -> Option<Vec<Name>> {
    match &body.exprs[expr_id] {
        Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
        Expr::FieldAccess { base, field } => {
            let mut segments = expr_to_path_segments(*base, body)?;
            segments.push(field.clone());
            Some(segments)
        }
        _ => None,
    }
}

fn class_method_key(class_name: &QualifiedTypeName, method: &Name) -> Name {
    let ns = class_name.namespace();
    let key = if ns.is_empty() {
        format!("{}.{}", class_name.name(), method)
    } else {
        let ns_str = ns.iter().map(Name::as_str).collect::<Vec<_>>().join(".");
        format!("{ns_str}.{}.{}", class_name.name(), method)
    };
    Name::new(key)
}

fn path_name(segments: &[Name]) -> Name {
    if segments.len() == 1 {
        segments[0].clone()
    } else {
        Name::new(
            segments
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join("."),
        )
    }
}

use std::collections::{BTreeSet, HashMap};

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, ExprId, Stmt, StmtId};
use baml_compiler2_hir::package::PackageId;
use rustc_hash::FxHashMap;

use crate::{
    throw_inference::function_throw_sets,
    throws_semantics::{flatten_ty_to_facts, function_throws_facts},
    ty::{Freshness, QualifiedTypeName, Ty, TyAttr},
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_effective_throws<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
    catch_residual_throws: &FxHashMap<ExprId, BTreeSet<Ty>>,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    include_typevars: bool,
    unknown_on_unresolved_call: bool,
) -> BTreeSet<Ty> {
    let context = EffectiveThrowsContext {
        db,
        package_id,
        body,
        expressions,
        catch_residual_throws,
        aliases,
        include_typevars,
        unknown_on_unresolved_call,
    };
    body.root_expr
        .map(|root| context.collect_effective_throws_from_root_expr(root))
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_effective_throws_from_root_expr<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    root_expr: ExprId,
    body: &ExprBody,
    expressions: &FxHashMap<ExprId, Ty>,
    catch_residual_throws: &FxHashMap<ExprId, BTreeSet<Ty>>,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    include_typevars: bool,
    unknown_on_unresolved_call: bool,
) -> BTreeSet<Ty> {
    EffectiveThrowsContext {
        db,
        package_id,
        body,
        expressions,
        catch_residual_throws,
        aliases,
        include_typevars,
        unknown_on_unresolved_call,
    }
    .collect_effective_throws_from_root_expr(root_expr)
}

struct EffectiveThrowsContext<'a, 'db> {
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    body: &'a ExprBody,
    expressions: &'a FxHashMap<ExprId, Ty>,
    catch_residual_throws: &'a FxHashMap<ExprId, BTreeSet<Ty>>,
    aliases: &'a HashMap<QualifiedTypeName, Ty>,
    include_typevars: bool,
    unknown_on_unresolved_call: bool,
}

impl EffectiveThrowsContext<'_, '_> {
    fn collect_effective_throws_from_root_expr(&self, root_expr: ExprId) -> BTreeSet<Ty> {
        let mut out = BTreeSet::new();
        self.collect_effective_throws_from_expr(root_expr, &mut out);
        out
    }

    fn collect_effective_throws_from_expr(&self, expr_id: ExprId, out: &mut BTreeSet<Ty>) {
        match &self.body.exprs[expr_id] {
            Expr::Throw { value } => {
                self.collect_effective_throws_from_expr(*value, out);
                collect_throw_facts_from_value(*value, self.body, self.expressions, out);
            }
            Expr::Call { callee, args } | Expr::OptionalCall { callee, args } => {
                self.collect_effective_throws_from_expr(*callee, out);
                for arg in args {
                    self.collect_effective_throws_from_expr(*arg, out);
                }
                self.collect_effective_throws_from_call(*callee, out);
            }
            Expr::Catch { clauses, .. } => {
                if let Some(residual) = self.catch_residual_throws.get(&expr_id) {
                    out.extend(residual.iter().cloned());
                }
                for clause in clauses {
                    for arm_id in &clause.arms {
                        let arm = &self.body.catch_arms[*arm_id];
                        self.collect_effective_throws_from_expr(arm.body, out);
                    }
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.collect_effective_throws_from_expr(*condition, out);
                self.collect_effective_throws_from_expr(*then_branch, out);
                if let Some(else_expr) = else_branch {
                    self.collect_effective_throws_from_expr(*else_expr, out);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.collect_effective_throws_from_expr(*scrutinee, out);
                for arm_id in arms {
                    let arm = &self.body.match_arms[*arm_id];
                    if let Some(guard) = arm.guard {
                        self.collect_effective_throws_from_expr(guard, out);
                    }
                    self.collect_effective_throws_from_expr(arm.body, out);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.collect_effective_throws_from_expr(*lhs, out);
                self.collect_effective_throws_from_expr(*rhs, out);
            }
            Expr::Unary { expr, .. } | Expr::OptionalChain { expr } => {
                self.collect_effective_throws_from_expr(*expr, out);
            }
            Expr::Object {
                fields, spreads, ..
            } => {
                for (_, value) in fields {
                    self.collect_effective_throws_from_expr(*value, out);
                }
                for spread in spreads {
                    self.collect_effective_throws_from_expr(spread.expr, out);
                }
            }
            Expr::Array { elements } => {
                for elem in elements {
                    self.collect_effective_throws_from_expr(*elem, out);
                }
            }
            Expr::Map { entries } => {
                for (key, value) in entries {
                    self.collect_effective_throws_from_expr(*key, out);
                    self.collect_effective_throws_from_expr(*value, out);
                }
            }
            Expr::Block { stmts, tail_expr } => {
                for stmt_id in stmts {
                    self.collect_effective_throws_from_stmt(*stmt_id, out);
                }
                if let Some(tail) = tail_expr {
                    self.collect_effective_throws_from_expr(*tail, out);
                }
            }
            Expr::FieldAccess { base, .. } | Expr::OptionalFieldAccess { base, .. } => {
                self.collect_effective_throws_from_expr(*base, out);
            }
            Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
                self.collect_effective_throws_from_expr(*base, out);
                self.collect_effective_throws_from_expr(*index, out);
            }
            Expr::Lambda(_)
            | Expr::Literal(_)
            | Expr::ByteStringLiteral(_)
            | Expr::Null
            | Expr::Path(_)
            | Expr::Missing => {}
        }
    }

    fn collect_effective_throws_from_stmt(&self, stmt_id: StmtId, out: &mut BTreeSet<Ty>) {
        match &self.body.stmts[stmt_id] {
            Stmt::Expr(expr) => self.collect_effective_throws_from_expr(*expr, out),
            Stmt::Let { initializer, .. } => {
                if let Some(init) = initializer {
                    self.collect_effective_throws_from_expr(*init, out);
                }
            }
            Stmt::While {
                condition,
                body: while_body,
                after,
                ..
            } => {
                self.collect_effective_throws_from_expr(*condition, out);
                self.collect_effective_throws_from_expr(*while_body, out);
                if let Some(after_stmt) = after {
                    self.collect_effective_throws_from_stmt(*after_stmt, out);
                }
            }
            Stmt::For {
                collection,
                body: for_body,
                ..
            } => {
                self.collect_effective_throws_from_expr(*collection, out);
                self.collect_effective_throws_from_expr(*for_body, out);
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.collect_effective_throws_from_expr(*expr, out);
                }
            }
            Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
                self.collect_effective_throws_from_expr(*target, out);
                self.collect_effective_throws_from_expr(*value, out);
            }
            Stmt::Throw { value } => {
                self.collect_effective_throws_from_expr(*value, out);
                collect_throw_facts_from_value(*value, self.body, self.expressions, out);
            }
            Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
        }
    }

    fn collect_effective_throws_from_call(&self, callee_expr_id: ExprId, out: &mut BTreeSet<Ty>) {
        let type_level_facts = self
            .expressions
            .get(&callee_expr_id)
            .and_then(|ty| function_throws_facts(ty, self.aliases));

        if let Some(facts) = type_level_facts.as_ref() {
            let filtered: BTreeSet<Ty> = facts
                .iter()
                .filter(|fact| self.include_typevars || !matches!(fact, Ty::TypeVar(_, _)))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                out.extend(filtered);
                return;
            }
            if facts.iter().any(|fact| matches!(fact, Ty::TypeVar(_, _))) && !self.include_typevars
            {
                return;
            }
        }

        if let Some(target) = call_target_name(callee_expr_id, self.body, self.expressions) {
            let throws = function_throw_sets(self.db, self.package_id);
            if let Some(transitive) = throws.transitive_for(&target) {
                out.extend(transitive.iter().cloned());
                return;
            }
        }

        if self.unknown_on_unresolved_call && type_level_facts.is_none() {
            out.insert(Ty::Unknown {
                attr: TyAttr::default(),
            });
        }
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
        Expr::FieldAccess { base, field } | Expr::OptionalFieldAccess { base, field } => {
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
        Expr::FieldAccess { base, field } | Expr::OptionalFieldAccess { base, field } => {
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

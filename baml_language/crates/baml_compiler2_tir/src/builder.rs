//! Per-scope type inference builder.
//!
//! `TypeInferenceBuilder` is the mutable accumulator during a single scope's
//! type inference run. It walks the `ExprBody` arena for expressions belonging
//! to the scope being analyzed, recording inferred types in `expressions`.
//!
//! Implements bidirectional type checking:
//! - `infer_expr` (synthesis): compute type bottom-up
//! - `check_expr` (checking): verify against expected type top-down
//! - `check_stmt`: type-check a statement
//!
//! Key invariant: when encountering a lambda expression body, the builder does
//! NOT recurse into it — lambda bodies are separate scopes with their own
//! `infer_scope_types` Salsa query.

use std::collections::{BTreeSet, HashMap};

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, ExprId, PatId, Stmt, StmtId, TypeExpr};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
    scope::ScopeId,
};
use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::{
    infer_context::{InferContext, RelatedLocation, TirTypeError, TypeCheckDiagnostics},
    ty::{Freshness, PrimitiveType, Ty},
};

/// Format an f64 as a string suitable for a float literal.
/// Returns `None` for non-finite values (inf, NaN).
fn format_float(v: f64) -> Option<String> {
    if !v.is_finite() {
        return None;
    }
    let s = format!("{v}");
    // Ensure the string always has a decimal point so it reads as float.
    if s.contains('.') {
        Some(s)
    } else {
        Some(format!("{v}.0"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternMatchStrength {
    NoMatch,
    MayMatch,
    DefiniteMatch,
}

#[derive(Debug, Default, Clone)]
struct ThrowPatternMatches {
    may_match: BTreeSet<String>,
    definitely_handled: BTreeSet<String>,
}

/// Per-scope inference builder.
///
/// Created at the start of `infer_scope_types`, discarded when done.
/// Modeled after Ty's `TypeInferenceBuilder`.
pub struct TypeInferenceBuilder<'db> {
    /// Diagnostic sink.
    context: InferContext<'db>,
    /// Expression types being built up.
    expressions: FxHashMap<ExprId, Ty>,
    /// Binding types: the type a variable is bound to (may differ from the
    /// initializer expression type due to widening or annotation).
    bindings: FxHashMap<PatId, Ty>,
    /// Package items for cross-file name resolution.
    package_items: &'db PackageItems<'db>,
    /// Current package ID (for throw-set queries).
    package_id: PackageId<'db>,
    /// The scope being analyzed (kept for future use).
    #[allow(dead_code)]
    scope: ScopeId<'db>,
    /// Declared return type for the function (used to check return statements).
    declared_return_ty: Option<Ty>,
    /// Local variable bindings: name → inferred type (flow-sensitive, updated
    /// by narrowing and assignments).
    locals: FxHashMap<Name, Ty>,
    /// Declared types: name → the type from the parameter annotation or
    /// explicit `let` type annotation. Written once per variable, never
    /// modified by narrowing or assignment. Used to validate assignments
    /// (the declared type is the upper bound for what can be assigned).
    ///
    /// Only populated for variables with explicit type annotations (params
    /// always have annotations; `let` bindings only when annotated).
    /// Unannotated `let` bindings (including evolving containers) are NOT
    /// tracked here — there's no user-stated contract to enforce.
    declared_types: FxHashMap<Name, Ty>,
    /// Resolved type alias map: alias qualified name → expanded Ty.
    /// Used by the normalizer for structural subtype checking.
    aliases: HashMap<crate::ty::QualifiedTypeName, Ty>,
    /// Residual throw facts for each catch expression after applying all clauses.
    catch_residual_throws: FxHashMap<ExprId, BTreeSet<String>>,
}

impl<'db> TypeInferenceBuilder<'db> {
    pub fn new(
        context: InferContext<'db>,
        package_items: &'db PackageItems<'db>,
        package_id: PackageId<'db>,
        scope: ScopeId<'db>,
        aliases: HashMap<crate::ty::QualifiedTypeName, Ty>,
    ) -> Self {
        Self {
            context,
            expressions: FxHashMap::default(),
            bindings: FxHashMap::default(),
            package_items,
            package_id,
            scope,
            declared_return_ty: None,
            locals: FxHashMap::default(),
            declared_types: FxHashMap::default(),
            aliases,
            catch_residual_throws: FxHashMap::default(),
        }
    }

    /// Finish building and return the accumulated results.
    pub fn finish(
        self,
    ) -> (
        FxHashMap<ExprId, Ty>,
        FxHashMap<PatId, Ty>,
        TypeCheckDiagnostics<'db>,
    ) {
        let diagnostics = self.context.finish();
        (self.expressions, self.bindings, diagnostics)
    }

    /// Set the declared return type (for return statement checking).
    pub fn set_return_type(&mut self, ty: Ty) {
        self.declared_return_ty = Some(ty);
    }

    /// Report a type error at a raw source span (for type annotations).
    pub fn report_at_span(&self, error: TirTypeError, span: text_size::TextRange) {
        self.context.report_at_span(error, span);
    }

    /// Add a local variable binding (e.g. function parameters).
    ///
    /// Also records the declared type (parameters always have annotations).
    /// Uses `entry().or_insert()` so repeated calls (e.g. from narrowing
    /// save/restore) don't overwrite the original declared type.
    pub fn add_local(&mut self, name: Name, ty: Ty) {
        self.declared_types
            .entry(name.clone())
            .or_insert_with(|| ty.clone());
        self.locals.insert(name, ty);
    }

    /// Record the type of an expression.
    pub fn record_expr_type(&mut self, expr_id: ExprId, ty: Ty) {
        self.expressions.insert(expr_id, ty);
    }

    // ── Bidirectional Type Checking ─────────────────────────────────────────

    /// Synthesis mode: compute the type of an expression bottom-up.
    pub fn infer_expr(&mut self, expr_id: ExprId, body: &ExprBody) -> Ty {
        let expr = &body.exprs[expr_id];
        let ty = match expr {
            Expr::Literal(lit) => self.infer_literal(lit),
            Expr::Null => Ty::Primitive(PrimitiveType::Null),
            Expr::Path(segments) => self.infer_path(segments.as_slice(), body, expr_id),
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // Infer the condition first so its type is in `self.expressions`.
                self.infer_expr(*condition, body);

                // Extract narrowings from the condition expression.
                let narrowings =
                    crate::narrowing::extract_narrowings(*condition, body, &self.expressions);

                // Apply then-branch narrowings, saving originals.
                let saved = crate::narrowing::apply_then_narrowings(&narrowings, &mut self.locals);

                let then_ty = self.infer_expr(*then_branch, body);

                // Restore originals and apply else-branch narrowings.
                crate::narrowing::restore_and_apply_else(&narrowings, &saved, &mut self.locals);

                let result_ty = if let Some(else_id) = else_branch {
                    let else_ty = self.infer_expr(*else_id, body);
                    self.join_types(&then_ty, &else_ty)
                } else {
                    Ty::Void
                };

                // Restore original types after the if expression.
                crate::narrowing::restore_narrowings(saved, &mut self.locals);

                result_ty
            }
            Expr::Call { callee, args } => {
                // Check for container mutation method (e.g. x.push(val))
                // before general callee inference, since resolve_member
                // doesn't know about built-in List/Map methods.
                if let Some(result_ty) =
                    self.try_container_method_call(expr_id, *callee, args, body)
                {
                    result_ty
                } else {
                    // Determine whether this is a method call (callee is a FieldAccess).
                    // Method calls have a `self` parameter in the resolved function type
                    // that must be skipped when checking the explicit argument count.
                    let is_method_call = matches!(&body.exprs[*callee], Expr::FieldAccess { .. });

                    let callee_ty = self.infer_expr(*callee, body);
                    for arg in args {
                        self.infer_expr(*arg, body);
                    }
                    match &callee_ty {
                        Ty::Function { params, ret } => {
                            // For method calls, skip the `self` parameter when
                            // comparing against the number of explicit arguments.
                            let effective_params = if is_method_call {
                                crate::generics::skip_self_param(params)
                            } else {
                                params.as_slice()
                            };
                            if effective_params.len() != args.len() {
                                self.context.report_simple(
                                    TirTypeError::ArgumentCountMismatch {
                                        expected: effective_params.len(),
                                        got: args.len(),
                                    },
                                    expr_id,
                                );
                            }
                            *ret.clone()
                        }
                        Ty::Unknown | Ty::Error => Ty::Unknown,
                        _ => {
                            self.context.report_simple(
                                TirTypeError::NotCallable {
                                    ty: callee_ty.clone(),
                                },
                                expr_id,
                            );
                            Ty::Unknown
                        }
                    }
                }
            }
            Expr::Block { stmts, tail_expr } => {
                let mut diverged_at: Option<(usize, StmtId)> = None;
                for (i, stmt_id) in stmts.iter().enumerate() {
                    if self.check_stmt_with_early_return_narrowing(*stmt_id, body) {
                        diverged_at = Some((i, *stmt_id));
                        break;
                    }
                }
                if let Some((div_idx, div_stmt)) = diverged_at {
                    let remaining =
                        stmts.len() - div_idx - 1 + if tail_expr.is_some() { 1 } else { 0 };
                    if remaining > 0 {
                        self.context.report_at_stmt(
                            crate::infer_context::TirTypeError::DeadCode {
                                after: div_stmt,
                                unreachable_count: remaining,
                            },
                            div_stmt,
                        );
                    }
                    Ty::Never
                } else {
                    tail_expr
                        .map(|e| self.infer_expr(e, body))
                        .unwrap_or(Ty::Void)
                }
            }
            Expr::FieldAccess { base, field } => {
                // Check for primitive-type static method access first:
                // `image.from_url(...)` where `image` is a type name, not a value.
                if let Some(ty) = self.try_primitive_static_access(*base, field, body) {
                    ty
                } else {
                    let base_ty = self.infer_expr(*base, body);
                    self.resolve_member(&base_ty, field, expr_id)
                }
            }
            Expr::Array { elements } => {
                let elem_types: Vec<Ty> =
                    elements.iter().map(|e| self.infer_expr(*e, body)).collect();
                let elem_ty = self.join_all(&elem_types);
                Ty::List(Box::new(elem_ty))
            }
            Expr::Map { entries } => {
                let mut key_types = Vec::new();
                let mut val_types = Vec::new();
                for (k, v) in entries {
                    key_types.push(self.infer_expr(*k, body));
                    val_types.push(self.infer_expr(*v, body));
                }
                let key_ty = self.join_all(&key_types);
                let val_ty = self.join_all(&val_types);
                Ty::Map(Box::new(key_ty), Box::new(val_ty))
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.infer_expr(*lhs, body);
                let rhs_ty = self.infer_expr(*rhs, body);
                self.infer_binary_op(op, &lhs_ty, &rhs_ty, expr_id)
            }
            Expr::Unary { op, expr } => {
                let operand_ty = self.infer_expr(*expr, body);
                self.infer_unary_op(op, &operand_ty, expr_id)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => self.infer_match_expr(expr_id, *scrutinee, arms, body),
            Expr::Catch { base, clauses } => self.infer_catch_expr(expr_id, *base, clauses, body),
            Expr::Throw { value } => {
                self.infer_expr(*value, body);
                Ty::Never
            }
            Expr::Object {
                type_name, fields, ..
            } => {
                for (_, expr_id) in fields {
                    self.infer_expr(*expr_id, body);
                }
                type_name
                    .as_ref()
                    .and_then(|n| {
                        self.package_items.lookup_type(&[n.clone()]).map(|def| {
                            Ty::Class(crate::lower_type_expr::qualify_def(
                                self.context.db(),
                                def,
                                n,
                            ))
                        })
                    })
                    .unwrap_or(Ty::Unknown)
            }
            Expr::Index { base, index } => {
                let base_ty = self.infer_expr(*base, body);
                self.infer_expr(*index, body);
                match base_ty {
                    Ty::List(elem_ty) | Ty::EvolvingList(elem_ty) => *elem_ty,
                    Ty::Map(_, val_ty) | Ty::EvolvingMap(_, val_ty) => *val_ty,
                    Ty::Unknown | Ty::Error => Ty::Unknown,
                    _ => {
                        self.context.report_simple(
                            TirTypeError::NotIndexable {
                                ty: base_ty.clone(),
                            },
                            expr_id,
                        );
                        Ty::Unknown
                    }
                }
            }
            Expr::Missing => Ty::Unknown,
        };
        self.record_expr_type(expr_id, ty.clone());
        ty
    }

    /// Checking mode: verify an expression against an expected type.
    pub fn check_expr(&mut self, expr_id: ExprId, body: &ExprBody, expected: &Ty) -> Ty {
        let expr = &body.exprs[expr_id];
        match expr {
            // Block: check the tail expression against expected type
            Expr::Block { stmts, tail_expr } => {
                let mut diverged_at: Option<(usize, StmtId)> = None;
                for (i, stmt_id) in stmts.iter().enumerate() {
                    if self.check_stmt_with_early_return_narrowing(*stmt_id, body) {
                        diverged_at = Some((i, *stmt_id));
                        break;
                    }
                }
                let ty = if let Some((div_idx, div_stmt)) = diverged_at {
                    let remaining =
                        stmts.len() - div_idx - 1 + if tail_expr.is_some() { 1 } else { 0 };
                    if remaining > 0 {
                        self.context.report_at_stmt(
                            crate::infer_context::TirTypeError::DeadCode {
                                after: div_stmt,
                                unreachable_count: remaining,
                            },
                            div_stmt,
                        );
                    }
                    Ty::Never
                } else if let Some(tail) = tail_expr {
                    self.check_expr(*tail, body, expected)
                } else if !matches!(expected, Ty::Unknown | Ty::Void) {
                    // No tail expression, no divergence — block falls through
                    // without producing a value. Report missing return.
                    self.context.report_simple(
                        TirTypeError::MissingReturn {
                            expected: expected.clone(),
                        },
                        expr_id,
                    );
                    expected.clone()
                } else {
                    Ty::Void
                };
                self.record_expr_type(expr_id, ty.clone());
                ty
            }
            // If: check both branches against expected type
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // Infer the condition first so its type is in `self.expressions`.
                self.infer_expr(*condition, body);

                // Extract narrowings from the condition expression.
                let narrowings =
                    crate::narrowing::extract_narrowings(*condition, body, &self.expressions);

                // Apply then-branch narrowings, saving originals.
                let saved = crate::narrowing::apply_then_narrowings(&narrowings, &mut self.locals);

                let then_ty = self.check_expr(*then_branch, body, expected);

                // Restore originals and apply else-branch narrowings.
                crate::narrowing::restore_and_apply_else(&narrowings, &saved, &mut self.locals);

                let ty = if let Some(else_id) = else_branch {
                    let else_ty = self.check_expr(*else_id, body, expected);
                    self.join_types(&then_ty, &else_ty)
                } else {
                    if !matches!(expected, Ty::Void | Ty::Unknown) {
                        self.context
                            .report_simple(TirTypeError::VoidUsedAsValue, expr_id);
                    }
                    Ty::Void
                };

                // Restore original types after the if expression.
                crate::narrowing::restore_narrowings(saved, &mut self.locals);

                self.record_expr_type(expr_id, ty.clone());
                ty
            }
            Expr::Array { elements } => {
                let elem_ty = match expected {
                    Ty::List(e) | Ty::EvolvingList(e) => Some(e),
                    _ => None,
                };
                if let Some(elem_ty) = elem_ty {
                    for e in elements {
                        self.check_expr(*e, body, elem_ty);
                    }
                    let ty = expected.clone();
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    self.infer_expr(expr_id, body)
                }
            }
            // Object: if expected is Class(name), check fields
            Expr::Object { fields, .. } => {
                if let Ty::Class(_) = expected {
                    for (_field_name, field_expr) in fields {
                        self.infer_expr(*field_expr, body);
                    }
                    let ty = expected.clone();
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    self.infer_expr(expr_id, body)
                }
            }
            Expr::Map { entries } => {
                let kv = match expected {
                    Ty::Map(k, v) | Ty::EvolvingMap(k, v) => Some((k, v)),
                    _ => None,
                };
                if let Some((key_ty, val_ty)) = kv {
                    for (k, v) in entries {
                        self.check_expr(*k, body, key_ty);
                        self.check_expr(*v, body, val_ty);
                    }
                    let ty = expected.clone();
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    self.infer_expr(expr_id, body)
                }
            }
            // Literal checked against a literal type: compare values directly.
            // On match, strip freshness → Regular. On mismatch, fall through
            // to the default infer-then-check path which will report the error.
            Expr::Literal(lit) if matches!(expected, Ty::Literal(..)) => {
                use crate::ty::Freshness;
                let expected_lit = match expected {
                    Ty::Literal(v, _) => v,
                    _ => unreachable!(),
                };
                if lit == expected_lit {
                    let ty = Ty::Literal(expected_lit.clone(), Freshness::Regular);
                    self.record_expr_type(expr_id, ty.clone());
                    ty
                } else {
                    // Value doesn't match — infer (produces fresh literal) and
                    // let the subtype check report the error.
                    let inferred = self.infer_expr(expr_id, body);
                    if !self.is_subtype(&inferred, expected) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: expected.clone(),
                                got: inferred.clone(),
                            },
                            expr_id,
                            Vec::new(),
                        );
                    }
                    inferred
                }
            }
            // All other expressions: infer then subtype-check
            _ => {
                let inferred = self.infer_expr(expr_id, body);
                if matches!(inferred, Ty::Void) && !matches!(expected, Ty::Void | Ty::Unknown) {
                    self.context
                        .report_simple(TirTypeError::VoidUsedAsValue, expr_id);
                } else if !self.is_subtype(&inferred, expected) {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: expected.clone(),
                            got: inferred.clone(),
                        },
                        expr_id,
                        Vec::new(),
                    );
                }
                inferred
            }
        }
    }

    /// Type-check a statement. Returns `true` if the statement diverges
    /// (i.e. control flow never reaches the next statement).
    pub fn check_stmt(&mut self, stmt_id: StmtId, body: &ExprBody) -> bool {
        let stmt = &body.stmts[stmt_id];
        match stmt {
            Stmt::Expr(expr_id) => {
                let ty = self.infer_expr(*expr_id, body);
                matches!(ty, Ty::Never)
            }
            Stmt::Let {
                pattern,
                initializer,
                type_annotation,
                ..
            } => {
                // Track whether this let has an explicit annotation (for declared_types).
                let mut ann_ty_for_decl: Option<Ty> = None;
                let init_ty = if let Some(init) = initializer {
                    if let Some(ann_idx) = type_annotation {
                        let mut diags = Vec::new();
                        let ann_ty = crate::lower_type_expr::lower_type_expr(
                            self.context.db(),
                            &body.type_annotations[*ann_idx],
                            self.package_items,
                            &mut diags,
                        );
                        for diag in diags {
                            self.context.report_at_type_annot(diag, *ann_idx);
                        }
                        let ty = self.check_expr(*init, body, &ann_ty);
                        if matches!(ty, Ty::Void) {
                            self.context
                                .report_simple(TirTypeError::VoidUsedAsValue, *init);
                        }
                        ann_ty_for_decl = Some(ann_ty);
                        Some(ty)
                    } else {
                        let ty = self.infer_expr(*init, body);
                        if matches!(ty, Ty::Void) {
                            self.context
                                .report_simple(TirTypeError::VoidUsedAsValue, *init);
                        }
                        // No annotation → no declared type (evolving containers etc.)
                        Some(ty.widen_fresh().make_evolving())
                    }
                } else {
                    None
                };
                // Track local variable binding for name resolution
                let diverges = matches!(init_ty, Some(Ty::Never));
                if let Some(ty) = init_ty {
                    self.bindings.insert(*pattern, ty.clone());
                    let pat = &body.patterns[*pattern];
                    let name = match pat {
                        baml_compiler2_ast::Pattern::Binding(name) => Some(name),
                        baml_compiler2_ast::Pattern::TypedBinding { name, .. } => Some(name),
                        _ => None,
                    };
                    if let Some(name) = name {
                        self.locals.insert(name.clone(), ty);
                        // Record declared type only for annotated let-bindings.
                        if let Some(decl_ty) = ann_ty_for_decl {
                            self.declared_types.insert(name.clone(), decl_ty);
                        }
                    }
                }
                diverges
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    if let Some(ret_ty) = &self.declared_return_ty {
                        let ret_ty = ret_ty.clone();
                        self.check_expr(*e, body, &ret_ty);
                    } else {
                        self.infer_expr(*e, body);
                    }
                }
                true // return always diverges
            }
            Stmt::Throw { value } => {
                self.infer_expr(*value, body);
                true
            }
            Stmt::While {
                condition,
                body: while_body,
                ..
            } => {
                self.infer_expr(*condition, body);
                self.infer_expr(*while_body, body);
                false
            }
            Stmt::Assign { target, value } => {
                // Check for container index mutation: x[i] = val
                if self.try_index_assign_mutation(*target, *value, body) {
                    return false;
                }
                // For simple variable assignment (x = val), check against the
                // variable's *declared* type, not its potentially-narrowed type.
                // Narrowing may have refined x: int? → null inside an if-branch,
                // but assignment should still accept any value assignable to int?.
                let declared_ty = self.get_declared_type(*target, body);
                let value_ty = self.infer_expr(*value, body);
                if let Some(ref decl_ty) = declared_ty {
                    if !matches!(decl_ty, Ty::Unknown | Ty::Error)
                        && !matches!(value_ty, Ty::Unknown | Ty::Error)
                        && !self.is_subtype(&value_ty, decl_ty)
                    {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: decl_ty.clone(),
                                got: value_ty.clone(),
                            },
                            *value,
                            Vec::new(),
                        );
                    }
                    // Update the local to the assigned value's type (invalidates narrowing)
                    if let Expr::Path(segments) = &body.exprs[*target] {
                        if segments.len() == 1 {
                            self.locals.insert(segments[0].clone(), value_ty);
                        }
                    }
                } else {
                    self.infer_expr(*target, body);
                    self.infer_expr(*value, body);
                }
                false
            }
            Stmt::AssignOp { target, op, value } => {
                let target_ty = self.infer_expr(*target, body);
                let value_ty = self.infer_expr(*value, body);
                let binary_op = Self::assign_op_to_binary_op(op);
                let result_ty = self.infer_binary_op(&binary_op, &target_ty, &value_ty, *target);
                // Re-record the value expression with the result type so the
                // display shows the operation result, not the raw RHS literal.
                self.record_expr_type(*value, result_ty);
                false
            }
            Stmt::Assert { condition } => {
                self.infer_expr(*condition, body);
                false
            }
            Stmt::Break | Stmt::Continue => true, // break/continue diverge
            Stmt::Missing | Stmt::HeaderComment { .. } => false,
        }
    }

    // ── Early-return narrowing ────────────────────────────────────────────────

    /// Type-check a statement, applying early-return narrowing when applicable.
    ///
    /// This wraps `check_stmt` and adds special handling for the pattern:
    ///
    /// ```baml
    /// if (x == null) { return ...; }
    /// // x is non-null here
    /// ```
    ///
    /// When a `Stmt::Expr(Expr::If { ... })` is processed:
    /// - If the then-branch always diverges (return/break/continue)
    /// - And the overall statement does NOT diverge (no else, or else does not diverge)
    ///
    /// Then the else-branch narrowings are applied to the locals map, narrowing
    /// the variable types for the remainder of the enclosing block.
    ///
    /// For all other statements, delegates to `check_stmt`.
    fn check_stmt_with_early_return_narrowing(&mut self, stmt_id: StmtId, body: &ExprBody) -> bool {
        let stmt = &body.stmts[stmt_id];

        // Only special-case `Stmt::Expr(Expr::If { ... })`
        if let baml_compiler2_ast::Stmt::Expr(if_expr_id) = stmt {
            let if_expr = &body.exprs[*if_expr_id];
            if let Expr::If {
                condition,
                then_branch,
                else_branch,
            } = if_expr
            {
                let condition = *condition;
                let then_branch = *then_branch;
                let else_branch = *else_branch;

                // Infer the condition to populate its type in self.expressions.
                // (infer_expr for the full Expr::If will also do this, but we
                // need narrowings before calling check_stmt.)
                //
                // We call check_stmt normally — it calls infer_expr(Expr::If),
                // which already applies and restores narrowings for the branches.
                // After check_stmt returns, we check if the then-branch diverged
                // and, if so, apply the else-narrowings permanently.

                // Extract narrowings from the condition. We need the condition
                // type to be recorded first, so we infer it here. Note that
                // infer_expr for the Expr::If will re-infer it (idempotent: the
                // type is recorded and cached in self.expressions).
                self.infer_expr(condition, body);
                let narrowings =
                    crate::narrowing::extract_narrowings(condition, body, &self.expressions);

                // Run the normal check_stmt (which handles the full Expr::If
                // including inner narrowing for the branches).
                let stmt_diverges = self.check_stmt(stmt_id, body);

                // After check_stmt, inspect whether the then-branch diverged.
                // If it did diverge AND the overall if didn't (either no else
                // branch, or else also diverged but then the whole stmt would
                // have diverged too), apply the else-narrowings to locals.
                if !narrowings.is_empty() {
                    let then_ty = self.expressions.get(&then_branch);
                    let then_diverged = matches!(then_ty, Some(Ty::Never));

                    if then_diverged && !stmt_diverges {
                        // The then-branch always diverges but execution can
                        // continue after this statement — so the else-narrowings
                        // now hold for the rest of the block.
                        crate::narrowing::apply_post_diverge_narrowings(
                            &narrowings,
                            &mut self.locals,
                        );
                    }

                    // If there's no else and the then-branch diverges, we also
                    // want to apply the else-narrowing even when the overall if
                    // might not diverge (it diverges only if then always diverges
                    // and there's no else, which is covered above).
                    let _ = else_branch; // already handled via stmt_diverges check
                }

                return stmt_diverges;
            }
        }

        // Default: delegate to check_stmt
        self.check_stmt(stmt_id, body)
    }

    // ── Helper methods ────────────────────────────────────────────────────────

    /// Look up the *declared* type of an assignment target.
    ///
    /// Returns the original type from the parameter annotation or `let` type
    /// annotation — unaffected by narrowing. Returns `None` for unannotated
    /// let-bindings (including evolving containers) or non-simple targets.
    fn get_declared_type(&self, target: ExprId, body: &ExprBody) -> Option<Ty> {
        if let Expr::Path(segments) = &body.exprs[target] {
            if segments.len() == 1 {
                return self.declared_types.get(&segments[0]).cloned();
            }
        }
        None
    }

    fn infer_match_expr(
        &mut self,
        match_expr_id: ExprId,
        scrutinee_expr_id: ExprId,
        arms: &[baml_compiler2_ast::MatchArmId],
        body: &ExprBody,
    ) -> Ty {
        let scrutinee_ty = self.infer_expr(scrutinee_expr_id, body);
        let scrutinee_name = match &body.exprs[scrutinee_expr_id] {
            Expr::Path(segments) if segments.len() == 1 => Some(segments[0].clone()),
            _ => None,
        };

        let required_cases = self.required_match_cases(&scrutinee_ty);
        let mut covered_cases = BTreeSet::new();
        let mut catch_all_seen = false;
        let mut arm_types = Vec::new();

        for arm_id in arms {
            let arm = &body.match_arms[*arm_id];
            let pattern_id = arm.pattern;

            let arm_cases = self.pattern_match_cases(pattern_id, &scrutinee_ty, body, arm.body);
            let mut unreachable = catch_all_seen;
            if !unreachable && arm.guard.is_none() {
                if let Some(required) = &required_cases {
                    if !arm_cases.is_empty()
                        && arm_cases
                            .iter()
                            .all(|c| covered_cases.contains(c) || !required.contains(c))
                    {
                        unreachable = true;
                    }
                }
            }
            if unreachable {
                self.context
                    .report_simple(TirTypeError::UnreachableArm, arm.body);
            }

            let narrowed_scrutinee_ty =
                self.pattern_narrowed_type(pattern_id, &scrutinee_ty, body, arm.body);
            let mut saved = Vec::new();

            if let Some(name) = &scrutinee_name {
                saved.push((name.clone(), self.locals.get(name).cloned()));
                self.locals
                    .insert(name.clone(), narrowed_scrutinee_ty.clone());
            }

            if let Some((bind_name, bind_ty)) = self.pattern_binding_for_arm(
                pattern_id,
                &scrutinee_ty,
                &narrowed_scrutinee_ty,
                body,
            ) {
                saved.push((bind_name.clone(), self.locals.get(&bind_name).cloned()));
                self.locals.insert(bind_name, bind_ty);
            }

            if let Some(guard_expr) = arm.guard {
                self.infer_expr(guard_expr, body);
            }

            let arm_ty = self.infer_expr(arm.body, body);
            arm_types.push(arm_ty);

            for (name, previous) in saved {
                if let Some(prev_ty) = previous {
                    self.locals.insert(name, prev_ty);
                } else {
                    self.locals.remove(&name);
                }
            }

            if arm.guard.is_none() {
                if self.pattern_covers_all_match(pattern_id, &scrutinee_ty, body, arm.body) {
                    catch_all_seen = true;
                    if let Some(required) = &required_cases {
                        covered_cases = required.clone();
                    }
                } else if let Some(required) = &required_cases {
                    covered_cases.extend(arm_cases.into_iter().filter(|c| required.contains(c)));
                }
            }
        }

        if let Some(required) = required_cases {
            if !catch_all_seen {
                let missing: Vec<String> = required
                    .difference(&covered_cases)
                    .map(std::string::ToString::to_string)
                    .collect();
                if !missing.is_empty() {
                    self.context.report_simple(
                        TirTypeError::NonExhaustiveMatch {
                            scrutinee_type: scrutinee_ty.clone(),
                            missing_cases: missing,
                        },
                        match_expr_id,
                    );
                }
            }
        }

        self.join_all(&arm_types)
    }

    fn infer_catch_expr(
        &mut self,
        catch_expr_id: ExprId,
        base_expr_id: ExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        body: &ExprBody,
    ) -> Ty {
        let base_ty = self.infer_expr(base_expr_id, body);
        let mut result_members = vec![base_ty.clone()];
        let mut residual = self.catch_base_throw_types(base_expr_id, body);

        for clause in clauses {
            let binding_name = match &body.patterns[clause.binding] {
                baml_compiler2_ast::Pattern::Binding(name) => Some(name.clone()),
                baml_compiler2_ast::Pattern::TypedBinding { name, ty } => {
                    if let Some(banned) = crate::throw_inference::is_banned_catch_binding_type(ty) {
                        self.context.report_simple(
                            TirTypeError::InvalidCatchBindingType {
                                type_name: banned.to_string(),
                            },
                            base_expr_id,
                        );
                    }
                    Some(name.clone())
                }
                _ => None,
            };

            for &arm_id in &clause.arms {
                let arm = &body.catch_arms[arm_id];
                let matches =
                    self.match_throw_types_for_pattern(arm.pattern, &residual, body, arm.body);
                if matches.may_match.is_empty() {
                    self.context
                        .report_warning_simple(TirTypeError::UnreachableArm, arm.body);
                }

                let narrowed_binding_ty = self.throw_facts_to_ty(&matches.may_match);
                let mut saved = Vec::new();
                if let Some(name) = &binding_name {
                    saved.push((name.clone(), self.locals.get(name).cloned()));
                    self.locals
                        .insert(name.clone(), narrowed_binding_ty.clone());
                }
                if let Some((arm_bind_name, arm_bind_ty)) = self.pattern_binding_for_arm(
                    arm.pattern,
                    &narrowed_binding_ty,
                    &narrowed_binding_ty,
                    body,
                ) {
                    saved.push((
                        arm_bind_name.clone(),
                        self.locals.get(&arm_bind_name).cloned(),
                    ));
                    self.locals.insert(arm_bind_name, arm_bind_ty);
                }

                let arm_ty = self.infer_expr(arm.body, body);
                result_members.push(arm_ty);

                for (name, previous) in saved {
                    if let Some(prev_ty) = previous {
                        self.locals.insert(name, prev_ty);
                    } else {
                        self.locals.remove(&name);
                    }
                }

                for handled in &matches.definitely_handled {
                    residual.remove(handled);
                }
            }

            if matches!(
                clause.kind,
                baml_compiler2_ast::CatchClauseKind::CatchAll
                    | baml_compiler2_ast::CatchClauseKind::CatchAllPanics
            ) {
                residual.clear();
            }
        }

        self.catch_residual_throws
            .insert(catch_expr_id, residual.clone());
        self.join_all(&result_members)
    }

    /// Validate declared `throws` against effective escaping throws from the body.
    pub fn check_throws_contract(
        &mut self,
        body: &ExprBody,
        declared_throws: Option<&TypeExpr>,
        throws_span: Option<TextRange>,
        fallback_span: TextRange,
    ) {
        let Some(declared_expr) = declared_throws else {
            return;
        };

        let mut diags = Vec::new();
        let declared_ty = crate::lower_type_expr::lower_type_expr(
            self.context.db(),
            declared_expr,
            self.package_items,
            &mut diags,
        );
        let span = throws_span.unwrap_or(fallback_span);
        for diag in diags {
            self.context.report_at_span(diag, span);
        }

        let declared = crate::throw_inference::throw_facts_from_ty(&declared_ty);
        let effective = self.collect_effective_throws(body);

        let mut extra: Vec<String> = effective.difference(&declared).cloned().collect();
        let mut extraneous: Vec<String> = declared.difference(&effective).cloned().collect();
        extra.sort();
        extraneous.sort();

        if !extra.is_empty() {
            self.context.report_at_span(
                TirTypeError::ThrowsContractViolation {
                    declared: declared_ty.clone(),
                    extra_types: extra,
                },
                span,
            );
        }
        if !extraneous.is_empty() {
            self.context.report_warning_at_span(
                TirTypeError::ExtraneousThrowsDeclaration {
                    extra_types: extraneous,
                },
                span,
            );
        }
    }

    fn required_match_cases(&self, ty: &Ty) -> Option<BTreeSet<String>> {
        match ty {
            Ty::Primitive(PrimitiveType::Bool) => {
                Some(BTreeSet::from(["true".to_string(), "false".to_string()]))
            }
            Ty::Primitive(PrimitiveType::Null) => Some(BTreeSet::from(["null".to_string()])),
            Ty::Literal(lit, _) => Some(BTreeSet::from([self.literal_case_name(lit)])),
            Ty::Enum(enum_name) => Some(
                self.lookup_enum_variants(enum_name)
                    .into_iter()
                    .map(|variant| format!("{}.{}", enum_name.name, variant))
                    .collect(),
            ),
            Ty::Optional(inner) => {
                let mut cases = self.required_match_cases(inner)?;
                cases.insert("null".to_string());
                Some(cases)
            }
            Ty::Union(members) => {
                let mut out = BTreeSet::new();
                for member in members {
                    let Some(member_cases) = self.required_match_cases(member) else {
                        return None;
                    };
                    out.extend(member_cases);
                }
                Some(out)
            }
            Ty::Never => Some(BTreeSet::new()),
            _ => None,
        }
    }

    fn pattern_match_cases(
        &mut self,
        pattern_id: PatId,
        scrutinee_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> BTreeSet<String> {
        let pattern = &body.patterns[pattern_id];
        match pattern {
            baml_compiler2_ast::Pattern::Binding(name) => {
                if name.as_str() == "_" {
                    self.required_match_cases(scrutinee_ty).unwrap_or_default()
                } else if self.is_bare_type_sugar_binding(name) {
                    let narrowed =
                        self.pattern_narrowed_type(pattern_id, scrutinee_ty, body, at_expr);
                    self.required_match_cases(&narrowed).unwrap_or_default()
                } else {
                    self.required_match_cases(scrutinee_ty).unwrap_or_default()
                }
            }
            baml_compiler2_ast::Pattern::TypedBinding { .. } => {
                let narrowed = self.pattern_narrowed_type(pattern_id, scrutinee_ty, body, at_expr);
                if self.is_subtype(scrutinee_ty, &narrowed) {
                    self.required_match_cases(scrutinee_ty).unwrap_or_default()
                } else {
                    self.required_match_cases(&narrowed).unwrap_or_default()
                }
            }
            baml_compiler2_ast::Pattern::Literal(lit) => {
                BTreeSet::from([self.literal_case_name(lit)])
            }
            baml_compiler2_ast::Pattern::Null => BTreeSet::from(["null".to_string()]),
            baml_compiler2_ast::Pattern::EnumVariant { enum_name, variant } => {
                BTreeSet::from([format!("{}.{}", enum_name, variant)])
            }
            baml_compiler2_ast::Pattern::Union(parts) => {
                let mut out = BTreeSet::new();
                for part in parts {
                    out.extend(self.pattern_match_cases(*part, scrutinee_ty, body, at_expr));
                }
                out
            }
        }
    }

    fn pattern_covers_all_match(
        &mut self,
        pattern_id: PatId,
        scrutinee_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> bool {
        match &body.patterns[pattern_id] {
            baml_compiler2_ast::Pattern::Binding(name) => {
                !self.is_bare_type_sugar_binding(name) || name.as_str() == "_"
            }
            baml_compiler2_ast::Pattern::TypedBinding { .. } => {
                let narrowed = self.pattern_narrowed_type(pattern_id, scrutinee_ty, body, at_expr);
                self.is_subtype(scrutinee_ty, &narrowed)
            }
            baml_compiler2_ast::Pattern::Union(parts) => {
                if let Some(required) = self.required_match_cases(scrutinee_ty) {
                    let mut covered = BTreeSet::new();
                    for part in parts {
                        covered.extend(self.pattern_match_cases(
                            *part,
                            scrutinee_ty,
                            body,
                            at_expr,
                        ));
                    }
                    required.iter().all(|c| covered.contains(c))
                } else {
                    false
                }
            }
            _ => {
                if let Some(required) = self.required_match_cases(scrutinee_ty) {
                    let covered = self.pattern_match_cases(pattern_id, scrutinee_ty, body, at_expr);
                    required.iter().all(|c| covered.contains(c))
                } else {
                    false
                }
            }
        }
    }

    fn pattern_binding_for_arm(
        &self,
        pattern_id: PatId,
        scrutinee_ty: &Ty,
        narrowed_ty: &Ty,
        body: &ExprBody,
    ) -> Option<(Name, Ty)> {
        match &body.patterns[pattern_id] {
            baml_compiler2_ast::Pattern::Binding(name) => {
                if name.as_str() == "_" || self.is_bare_type_sugar_binding(name) {
                    None
                } else {
                    Some((name.clone(), scrutinee_ty.clone()))
                }
            }
            baml_compiler2_ast::Pattern::TypedBinding { name, .. } => {
                Some((name.clone(), narrowed_ty.clone()))
            }
            _ => None,
        }
    }

    fn pattern_narrowed_type(
        &mut self,
        pattern_id: PatId,
        scrutinee_ty: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> Ty {
        match &body.patterns[pattern_id] {
            baml_compiler2_ast::Pattern::Binding(name) => {
                if self.is_bare_type_sugar_binding(name) {
                    self.lower_pattern_type_expr(&TypeExpr::Path(vec![name.clone()]), at_expr)
                } else {
                    scrutinee_ty.clone()
                }
            }
            baml_compiler2_ast::Pattern::TypedBinding { ty, .. } => {
                self.lower_pattern_type_expr(ty, at_expr)
            }
            baml_compiler2_ast::Pattern::Literal(lit) => {
                Ty::Literal(lit.clone(), crate::ty::Freshness::Regular)
            }
            baml_compiler2_ast::Pattern::Null => Ty::Primitive(PrimitiveType::Null),
            baml_compiler2_ast::Pattern::EnumVariant { enum_name, variant } => {
                if let Ty::Enum(qn) = scrutinee_ty {
                    if qn.name == *enum_name {
                        return Ty::EnumVariant(qn.clone(), variant.clone());
                    }
                }
                if let Some(def) = self.package_items.lookup_type(&[enum_name.clone()]) {
                    if matches!(def, Definition::Enum(_)) {
                        return Ty::EnumVariant(
                            crate::lower_type_expr::qualify_def(self.context.db(), def, enum_name),
                            variant.clone(),
                        );
                    }
                }
                Ty::Unknown
            }
            baml_compiler2_ast::Pattern::Union(parts) => {
                let mut tys = Vec::new();
                for part in parts {
                    tys.push(self.pattern_narrowed_type(*part, scrutinee_ty, body, at_expr));
                }
                self.join_all(&tys)
            }
        }
    }

    fn lower_pattern_type_expr(&mut self, expr: &TypeExpr, at_expr: ExprId) -> Ty {
        let mut diags = Vec::new();
        let ty = crate::lower_type_expr::lower_type_expr(
            self.context.db(),
            expr,
            self.package_items,
            &mut diags,
        );
        for diag in diags {
            self.context.report_simple(diag, at_expr);
        }
        ty
    }

    fn literal_case_name(&self, lit: &baml_base::Literal) -> String {
        match lit {
            baml_base::Literal::Int(v) => v.to_string(),
            baml_base::Literal::Float(v) => v.clone(),
            baml_base::Literal::String(v) => format!("{v:?}"),
            baml_base::Literal::Bool(v) => v.to_string(),
        }
    }

    fn is_bare_type_sugar_binding(&self, name: &Name) -> bool {
        matches!(
            name.as_str(),
            "int" | "float" | "string" | "bool" | "null" | "image" | "audio" | "video" | "pdf"
        ) || self.package_items.lookup_type(&[name.clone()]).is_some()
    }

    fn catch_base_throw_types(&self, base_expr_id: ExprId, body: &ExprBody) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        self.collect_throw_facts_from_expr(base_expr_id, body, &mut out);
        out
    }

    fn throw_facts_to_ty(&self, facts: &BTreeSet<String>) -> Ty {
        if facts.is_empty() {
            return Ty::Never;
        }
        let tys: Vec<Ty> = facts.iter().map(|f| self.throw_fact_to_ty(f)).collect();
        self.join_all(&tys)
    }

    fn throw_fact_to_ty(&self, fact: &str) -> Ty {
        match fact {
            "int" => Ty::Primitive(PrimitiveType::Int),
            "float" => Ty::Primitive(PrimitiveType::Float),
            "string" => Ty::Primitive(PrimitiveType::String),
            "bool" => Ty::Primitive(PrimitiveType::Bool),
            "null" => Ty::Primitive(PrimitiveType::Null),
            "unknown" => Ty::Unknown,
            _ => {
                let name = Name::new(fact);
                if let Some(def) = self.package_items.lookup_type(&[name.clone()]) {
                    match def {
                        Definition::Class(_) => Ty::Class(crate::lower_type_expr::qualify_def(
                            self.context.db(),
                            def,
                            &name,
                        )),
                        Definition::Enum(_) => Ty::Enum(crate::lower_type_expr::qualify_def(
                            self.context.db(),
                            def,
                            &name,
                        )),
                        Definition::TypeAlias(_) => Ty::TypeAlias(
                            crate::lower_type_expr::qualify_def(self.context.db(), def, &name),
                        ),
                        _ => Ty::Unknown,
                    }
                } else {
                    Ty::Unknown
                }
            }
        }
    }

    fn match_throw_types_for_pattern(
        &mut self,
        pattern_id: PatId,
        throw_types: &BTreeSet<String>,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> ThrowPatternMatches {
        let mut out = ThrowPatternMatches::default();
        for throw_fact in throw_types {
            match self.pattern_match_strength(pattern_id, throw_fact, body, at_expr) {
                PatternMatchStrength::NoMatch => {}
                PatternMatchStrength::MayMatch => {
                    out.may_match.insert(throw_fact.clone());
                }
                PatternMatchStrength::DefiniteMatch => {
                    out.may_match.insert(throw_fact.clone());
                    out.definitely_handled.insert(throw_fact.clone());
                }
            }
        }
        out
    }

    fn pattern_match_strength(
        &mut self,
        pattern_id: PatId,
        throw_fact: &str,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> PatternMatchStrength {
        let pattern = &body.patterns[pattern_id];
        match pattern {
            baml_compiler2_ast::Pattern::Binding(name) => {
                if self.is_bare_type_sugar_binding(name) {
                    let lowered =
                        self.lower_pattern_type_expr(&TypeExpr::Path(vec![name.clone()]), at_expr);
                    if self.ty_matches_throw_fact(&lowered, throw_fact) {
                        PatternMatchStrength::DefiniteMatch
                    } else if throw_fact == "unknown" {
                        PatternMatchStrength::MayMatch
                    } else {
                        PatternMatchStrength::NoMatch
                    }
                } else {
                    PatternMatchStrength::DefiniteMatch
                }
            }
            baml_compiler2_ast::Pattern::TypedBinding { ty, .. } => {
                let lowered = self.lower_pattern_type_expr(ty, at_expr);
                if self.ty_matches_throw_fact(&lowered, throw_fact) {
                    PatternMatchStrength::DefiniteMatch
                } else if throw_fact == "unknown" {
                    PatternMatchStrength::MayMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
            baml_compiler2_ast::Pattern::Literal(lit) => {
                if self.literal_throw_fact(lit) == throw_fact || throw_fact == "unknown" {
                    PatternMatchStrength::DefiniteMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
            baml_compiler2_ast::Pattern::Null => {
                if throw_fact == "null" || throw_fact == "unknown" {
                    PatternMatchStrength::DefiniteMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
            baml_compiler2_ast::Pattern::EnumVariant { enum_name, variant } => {
                let exact = format!("{enum_name}.{variant}");
                if throw_fact == exact || throw_fact == enum_name.as_str() {
                    PatternMatchStrength::DefiniteMatch
                } else if throw_fact == "unknown" {
                    PatternMatchStrength::MayMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
            baml_compiler2_ast::Pattern::Union(parts) => {
                let mut saw_may = false;
                for part in parts {
                    match self.pattern_match_strength(*part, throw_fact, body, at_expr) {
                        PatternMatchStrength::DefiniteMatch => {
                            return PatternMatchStrength::DefiniteMatch;
                        }
                        PatternMatchStrength::MayMatch => saw_may = true,
                        PatternMatchStrength::NoMatch => {}
                    }
                }
                if saw_may {
                    PatternMatchStrength::MayMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
        }
    }

    fn ty_matches_throw_fact(&self, ty: &Ty, throw_fact: &str) -> bool {
        match ty {
            Ty::Primitive(p) => p.to_string() == throw_fact,
            Ty::Literal(lit, _) => self.literal_throw_fact(lit) == throw_fact,
            Ty::Optional(inner) => {
                throw_fact == "null" || self.ty_matches_throw_fact(inner, throw_fact)
            }
            Ty::Union(parts) => parts
                .iter()
                .any(|part| self.ty_matches_throw_fact(part, throw_fact)),
            Ty::Class(qn) | Ty::Enum(qn) | Ty::TypeAlias(qn) => {
                throw_fact == qn.name.as_str() || throw_fact == qn.to_string()
            }
            Ty::EnumVariant(qn, variant) => {
                throw_fact == format!("{}.{}", qn.name, variant) || throw_fact == qn.name.as_str()
            }
            Ty::BuiltinUnknown | Ty::Unknown | Ty::Error => true,
            Ty::Never | Ty::Void => false,
            Ty::List(_)
            | Ty::Map(_, _)
            | Ty::EvolvingList(_)
            | Ty::EvolvingMap(_, _)
            | Ty::Function { .. }
            | Ty::RustType => false,
        }
    }

    fn literal_throw_fact(&self, lit: &baml_base::Literal) -> &'static str {
        match lit {
            baml_base::Literal::Int(_) => "int",
            baml_base::Literal::Float(_) => "float",
            baml_base::Literal::String(_) => "string",
            baml_base::Literal::Bool(_) => "bool",
        }
    }

    fn collect_effective_throws(&self, body: &ExprBody) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        if let Some(root) = body.root_expr {
            self.collect_effective_throws_from_expr(root, body, &mut out);
        }
        out
    }

    fn collect_effective_throws_from_expr(
        &self,
        expr_id: ExprId,
        body: &ExprBody,
        out: &mut BTreeSet<String>,
    ) {
        match &body.exprs[expr_id] {
            Expr::Throw { value } => {
                self.collect_effective_throws_from_expr(*value, body, out);
                self.collect_throw_facts_from_value(*value, out);
            }
            Expr::Call { callee, args } => {
                self.collect_effective_throws_from_expr(*callee, body, out);
                for arg in args {
                    self.collect_effective_throws_from_expr(*arg, body, out);
                }
                if let Some(target) = self.call_target_name(*callee, body) {
                    let throws = crate::throw_inference::function_throw_sets(
                        self.context.db(),
                        self.package_id,
                    );
                    if let Some(transitive) = throws.transitive_for(&target) {
                        out.extend(transitive.iter().cloned());
                    }
                }
            }
            Expr::Catch { clauses, .. } => {
                if let Some(residual) = self.catch_residual_throws.get(&expr_id) {
                    out.extend(residual.iter().cloned());
                }
                for clause in clauses {
                    for arm_id in &clause.arms {
                        let arm = &body.catch_arms[*arm_id];
                        self.collect_effective_throws_from_expr(arm.body, body, out);
                    }
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.collect_effective_throws_from_expr(*condition, body, out);
                self.collect_effective_throws_from_expr(*then_branch, body, out);
                if let Some(else_expr) = else_branch {
                    self.collect_effective_throws_from_expr(*else_expr, body, out);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.collect_effective_throws_from_expr(*scrutinee, body, out);
                for arm_id in arms {
                    let arm = &body.match_arms[*arm_id];
                    if let Some(guard) = arm.guard {
                        self.collect_effective_throws_from_expr(guard, body, out);
                    }
                    self.collect_effective_throws_from_expr(arm.body, body, out);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.collect_effective_throws_from_expr(*lhs, body, out);
                self.collect_effective_throws_from_expr(*rhs, body, out);
            }
            Expr::Unary { expr, .. } => {
                self.collect_effective_throws_from_expr(*expr, body, out);
            }
            Expr::Object {
                fields, spreads, ..
            } => {
                for (_, value) in fields {
                    self.collect_effective_throws_from_expr(*value, body, out);
                }
                for spread in spreads {
                    self.collect_effective_throws_from_expr(spread.expr, body, out);
                }
            }
            Expr::Array { elements } => {
                for elem in elements {
                    self.collect_effective_throws_from_expr(*elem, body, out);
                }
            }
            Expr::Map { entries } => {
                for (key, value) in entries {
                    self.collect_effective_throws_from_expr(*key, body, out);
                    self.collect_effective_throws_from_expr(*value, body, out);
                }
            }
            Expr::Block { stmts, tail_expr } => {
                for stmt_id in stmts {
                    self.collect_effective_throws_from_stmt(*stmt_id, body, out);
                }
                if let Some(tail) = tail_expr {
                    self.collect_effective_throws_from_expr(*tail, body, out);
                }
            }
            Expr::FieldAccess { base, .. } => {
                self.collect_effective_throws_from_expr(*base, body, out);
            }
            Expr::Index { base, index } => {
                self.collect_effective_throws_from_expr(*base, body, out);
                self.collect_effective_throws_from_expr(*index, body, out);
            }
            Expr::Literal(_) | Expr::Null | Expr::Path(_) | Expr::Missing => {}
        }
    }

    fn collect_effective_throws_from_stmt(
        &self,
        stmt_id: StmtId,
        body: &ExprBody,
        out: &mut BTreeSet<String>,
    ) {
        match &body.stmts[stmt_id] {
            Stmt::Expr(expr) => self.collect_effective_throws_from_expr(*expr, body, out),
            Stmt::Let { initializer, .. } => {
                if let Some(init) = initializer {
                    self.collect_effective_throws_from_expr(*init, body, out);
                }
            }
            Stmt::While {
                condition,
                body: while_body,
                after,
                ..
            } => {
                self.collect_effective_throws_from_expr(*condition, body, out);
                self.collect_effective_throws_from_expr(*while_body, body, out);
                if let Some(after_stmt) = after {
                    self.collect_effective_throws_from_stmt(*after_stmt, body, out);
                }
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.collect_effective_throws_from_expr(*expr, body, out);
                }
            }
            Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
                self.collect_effective_throws_from_expr(*target, body, out);
                self.collect_effective_throws_from_expr(*value, body, out);
            }
            Stmt::Assert { condition } => {
                self.collect_effective_throws_from_expr(*condition, body, out);
            }
            Stmt::Throw { value } => {
                self.collect_effective_throws_from_expr(*value, body, out);
                self.collect_throw_facts_from_value(*value, out);
            }
            Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
        }
    }

    fn collect_throw_facts_from_expr(
        &self,
        expr_id: ExprId,
        body: &ExprBody,
        out: &mut BTreeSet<String>,
    ) {
        match &body.exprs[expr_id] {
            Expr::Throw { value } => {
                self.collect_throw_facts_from_expr(*value, body, out);
                self.collect_throw_facts_from_value(*value, out);
            }
            Expr::Call { callee, args } => {
                self.collect_throw_facts_from_expr(*callee, body, out);
                for arg in args {
                    self.collect_throw_facts_from_expr(*arg, body, out);
                }
                if let Some(target) = self.call_target_name(*callee, body) {
                    let throws = crate::throw_inference::function_throw_sets(
                        self.context.db(),
                        self.package_id,
                    );
                    if let Some(transitive) = throws.transitive_for(&target) {
                        out.extend(transitive.iter().cloned());
                    }
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.collect_throw_facts_from_expr(*condition, body, out);
                self.collect_throw_facts_from_expr(*then_branch, body, out);
                if let Some(else_expr) = else_branch {
                    self.collect_throw_facts_from_expr(*else_expr, body, out);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.collect_throw_facts_from_expr(*scrutinee, body, out);
                for arm_id in arms {
                    let arm = &body.match_arms[*arm_id];
                    if let Some(guard) = arm.guard {
                        self.collect_throw_facts_from_expr(guard, body, out);
                    }
                    self.collect_throw_facts_from_expr(arm.body, body, out);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.collect_throw_facts_from_expr(*lhs, body, out);
                self.collect_throw_facts_from_expr(*rhs, body, out);
            }
            Expr::Unary { expr, .. } => self.collect_throw_facts_from_expr(*expr, body, out),
            Expr::Object {
                fields, spreads, ..
            } => {
                for (_, value) in fields {
                    self.collect_throw_facts_from_expr(*value, body, out);
                }
                for spread in spreads {
                    self.collect_throw_facts_from_expr(spread.expr, body, out);
                }
            }
            Expr::Array { elements } => {
                for elem in elements {
                    self.collect_throw_facts_from_expr(*elem, body, out);
                }
            }
            Expr::Map { entries } => {
                for (key, value) in entries {
                    self.collect_throw_facts_from_expr(*key, body, out);
                    self.collect_throw_facts_from_expr(*value, body, out);
                }
            }
            Expr::Block { stmts, tail_expr } => {
                for stmt in stmts {
                    self.collect_throw_facts_from_stmt(*stmt, body, out);
                }
                if let Some(tail) = tail_expr {
                    self.collect_throw_facts_from_expr(*tail, body, out);
                }
            }
            Expr::FieldAccess { base, .. } => self.collect_throw_facts_from_expr(*base, body, out),
            Expr::Index { base, index } => {
                self.collect_throw_facts_from_expr(*base, body, out);
                self.collect_throw_facts_from_expr(*index, body, out);
            }
            Expr::Catch { base, .. } => {
                self.collect_throw_facts_from_expr(*base, body, out);
            }
            Expr::Literal(_) | Expr::Null | Expr::Path(_) | Expr::Missing => {}
        }
    }

    fn collect_throw_facts_from_stmt(
        &self,
        stmt_id: StmtId,
        body: &ExprBody,
        out: &mut BTreeSet<String>,
    ) {
        match &body.stmts[stmt_id] {
            Stmt::Expr(expr_id) => self.collect_throw_facts_from_expr(*expr_id, body, out),
            Stmt::Let { initializer, .. } => {
                if let Some(init) = initializer {
                    self.collect_throw_facts_from_expr(*init, body, out);
                }
            }
            Stmt::While {
                condition,
                body: while_body,
                after,
                ..
            } => {
                self.collect_throw_facts_from_expr(*condition, body, out);
                self.collect_throw_facts_from_expr(*while_body, body, out);
                if let Some(after_stmt) = after {
                    self.collect_throw_facts_from_stmt(*after_stmt, body, out);
                }
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.collect_throw_facts_from_expr(*expr, body, out);
                }
            }
            Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
                self.collect_throw_facts_from_expr(*target, body, out);
                self.collect_throw_facts_from_expr(*value, body, out);
            }
            Stmt::Assert { condition } => self.collect_throw_facts_from_expr(*condition, body, out),
            Stmt::Throw { value } => {
                self.collect_throw_facts_from_expr(*value, body, out);
                self.collect_throw_facts_from_value(*value, out);
            }
            Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
        }
    }

    fn collect_throw_facts_from_value(&self, value_expr_id: ExprId, out: &mut BTreeSet<String>) {
        let unknown_ty = Ty::Unknown;
        let thrown_ty = self.expressions.get(&value_expr_id).unwrap_or(&unknown_ty);
        out.extend(crate::throw_inference::throw_facts_from_ty(thrown_ty));
    }

    fn call_target_name(&self, callee_expr_id: ExprId, body: &ExprBody) -> Option<Name> {
        let segments = self.expr_to_path_segments(callee_expr_id, body)?;
        if segments.is_empty() {
            return None;
        }
        Some(Name::new(
            segments
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join("."),
        ))
    }

    fn expr_to_path_segments(&self, expr_id: ExprId, body: &ExprBody) -> Option<Vec<Name>> {
        match &body.exprs[expr_id] {
            Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
            Expr::FieldAccess { base, field } => {
                let mut base_segments = self.expr_to_path_segments(*base, body)?;
                base_segments.push(field.clone());
                Some(base_segments)
            }
            _ => None,
        }
    }

    /// Extract the short name from a qualified type name for package item lookups.
    fn unqualify(qn: &crate::ty::QualifiedTypeName) -> Name {
        qn.name.clone()
    }

    fn infer_literal(&self, lit: &baml_base::Literal) -> Ty {
        Ty::Literal(lit.clone(), Freshness::Fresh)
    }

    fn infer_path(&mut self, segments: &[Name], _body: &ExprBody, expr_id: ExprId) -> Ty {
        // After AST lowering, Path is always single-segment (a bare identifier).
        // Multi-segment paths like Color.Red or x.field are desugared to FieldAccess chains.
        debug_assert!(
            segments.len() == 1,
            "multi-segment Path should have been desugared to FieldAccess: {:?}",
            segments
        );
        if segments.len() == 1 {
            let name = &segments[0];
            let ty = self.infer_single_name(name);
            if matches!(ty, Ty::Unknown)
                && !self.locals.contains_key(name)
                && self.package_items.lookup_value(&[name.clone()]).is_none()
                && self.package_items.lookup_type(&[name.clone()]).is_none()
            {
                self.context
                    .report_simple(TirTypeError::UnresolvedName { name: name.clone() }, expr_id);
            }
            ty
        } else {
            Ty::Unknown
        }
    }

    /// Resolve a single name to its type.
    ///
    /// Checks local variables first, then value namespace (functions), then
    /// type namespace (classes, enums).
    fn infer_single_name(&self, name: &Name) -> Ty {
        if let Some(ty) = self.locals.get(name) {
            return match ty {
                Ty::EvolvingList(inner) => Ty::List(inner.clone()),
                Ty::EvolvingMap(k, v) => Ty::Map(k.clone(), v.clone()),
                other => other.clone(),
            };
        }
        // Check value namespace (functions, template strings)
        if let Some(def) = self.package_items.lookup_value(&[name.clone()]) {
            match def {
                Definition::Function(func_loc) => {
                    // Get function signature to build the function type
                    let db = self.context.db();
                    let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
                    let mut diags = Vec::new();
                    let ty = Ty::Function {
                        params: sig
                            .params
                            .iter()
                            .map(|(n, te)| {
                                (
                                    Some(n.clone()),
                                    crate::lower_type_expr::lower_type_expr(
                                        db,
                                        te,
                                        self.package_items,
                                        &mut diags,
                                    ),
                                )
                            })
                            .collect(),
                        ret: Box::new(
                            sig.return_type
                                .as_ref()
                                .map(|te| {
                                    crate::lower_type_expr::lower_type_expr(
                                        db,
                                        te,
                                        self.package_items,
                                        &mut diags,
                                    )
                                })
                                .unwrap_or(Ty::Unknown),
                        ),
                    };
                    // Note: diags from referenced function signatures are not
                    // reported here — they'll be reported at the definition site.
                    ty
                }
                _ => Ty::Unknown,
            }
        } else if let Some(def) = self.package_items.lookup_type(&[name.clone()]) {
            let db = self.context.db();
            match def {
                Definition::Class(_) => {
                    Ty::Class(crate::lower_type_expr::qualify_def(db, def, name))
                }
                Definition::Enum(_) => Ty::Enum(crate::lower_type_expr::qualify_def(db, def, name)),
                Definition::TypeAlias(_) => {
                    Ty::TypeAlias(crate::lower_type_expr::qualify_def(db, def, name))
                }
                _ => Ty::Unknown,
            }
        } else {
            Ty::Unknown
        }
    }

    /// Resolve a member access on a known base type.
    ///
    /// For class types, checks data fields. For enum types, validates variants.
    /// For builtin container types (`Ty::List`, `Ty::Map`) and `Ty::Primitive(String)`,
    /// bridges to the `.baml`-declared builtin classes via `resolve_builtin_method`.
    /// Emits `UnresolvedMember` diagnostics when the base type is known but
    /// the member doesn't exist.
    pub fn resolve_member(&mut self, base_ty: &Ty, member: &Name, at: ExprId) -> Ty {
        match base_ty {
            Ty::Class(class_name) => {
                // Check class fields
                let class_fields = self.lookup_class_fields(class_name);
                if let Some(field_ty) = class_fields.get(member) {
                    return field_ty.clone();
                }

                // Check class methods via the item tree (methods are stored
                // directly on the Class entry, not in the package namespace).
                if let Some(ty) = self.lookup_class_method(class_name, member) {
                    return ty;
                }

                // Known class but member not found — error
                let class_def = self
                    .package_items
                    .lookup_type(&[Self::unqualify(class_name)]);
                let related = class_def
                    .map(|def| vec![(RelatedLocation::Item(def), "class defined here")])
                    .unwrap_or_default();
                self.context.report(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                    related,
                );
                Ty::Unknown
            }
            Ty::Enum(enum_name) => {
                // Validate that the variant exists
                let variants = self.lookup_enum_variants(enum_name);
                if variants.contains(member) {
                    return Ty::EnumVariant(enum_name.clone(), member.clone());
                }

                // Known enum but variant not found — error
                let enum_def = self
                    .package_items
                    .lookup_type(&[Self::unqualify(enum_name)]);
                let related = enum_def
                    .map(|def| vec![(RelatedLocation::Item(def), "enum defined here")])
                    .unwrap_or_default();
                self.context.report(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                    related,
                );
                Ty::Unknown
            }
            Ty::List(element_ty) => {
                // Bridge: int[] → Array<int> — resolve via builtin Array class.
                self.resolve_builtin_method(&["Array"], &[element_ty.as_ref().clone()], member)
                    .unwrap_or_else(|| {
                        self.context.report_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown
                    })
            }
            Ty::Map(key_ty, val_ty) => {
                // Bridge: map<string, int> → Map<string, int>
                self.resolve_builtin_method(
                    &["Map"],
                    &[key_ty.as_ref().clone(), val_ty.as_ref().clone()],
                    member,
                )
                .unwrap_or_else(|| {
                    self.context.report_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown
                })
            }
            Ty::Primitive(PrimitiveType::String)
            | Ty::Literal(baml_base::Literal::String(_), _) => {
                // Bridge: string / "literal" → String class
                self.resolve_builtin_method(&["String"], &[], member)
                    .unwrap_or_else(|| {
                        self.context.report_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown
                    })
            }
            Ty::Primitive(
                p @ (PrimitiveType::Image
                | PrimitiveType::Audio
                | PrimitiveType::Video
                | PrimitiveType::Pdf),
            ) => {
                // Bridge: each media primitive → its own builtin class in baml.media
                self.resolve_builtin_method(p.builtin_class_path(), &[], member)
                    .unwrap_or_else(|| {
                        self.context.report_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown
                    })
            }
            Ty::Union(members) => {
                // For union types, try to resolve the field on each member.
                // If ALL members have the field, return Union(resolved_types).
                // If any member is missing the field, report per-member errors.
                let members = members.clone();
                let resolved: Vec<(Ty, Option<Ty>)> = members
                    .iter()
                    .map(|m| (m.clone(), self.try_resolve_member_on_ty(m, member)))
                    .collect();

                if resolved.iter().all(|(_, r)| r.is_some()) {
                    // All members have the field — return union of resolved types
                    let field_tys: Vec<Ty> =
                        resolved.into_iter().map(|(_, r)| r.unwrap()).collect();
                    Ty::Union(field_tys)
                } else {
                    // Report an error for each member that's missing the field
                    for (member_ty, result) in &resolved {
                        if result.is_none() {
                            self.context.report_simple(
                                TirTypeError::UnresolvedMember {
                                    base_type: member_ty.clone(),
                                    member: member.clone(),
                                },
                                at,
                            );
                        }
                    }
                    Ty::Unknown
                }
            }
            Ty::Unknown => {
                // Base type unknown — can't resolve member, but don't emit error
                // (the base type error was already reported upstream)
                Ty::Unknown
            }
            _ => {
                // Other types (other primitives, etc.) — no members
                self.context.report_simple(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                );
                Ty::Unknown
            }
        }
    }

    /// Try to resolve a member on a type without emitting diagnostics.
    ///
    /// Returns `Some(Ty)` if the member exists, `None` if it doesn't.
    /// Used by `resolve_member` for union type handling.
    fn try_resolve_member_on_ty(&self, ty: &Ty, member: &Name) -> Option<Ty> {
        match ty {
            Ty::Class(class_name) => {
                let fields = self.lookup_class_fields(class_name);
                if let Some(field_ty) = fields.get(member) {
                    return Some(field_ty.clone());
                }
                if let Some(method_ty) = self.lookup_class_method(class_name, member) {
                    return Some(method_ty);
                }
                None
            }
            Ty::List(element_ty) => {
                self.resolve_builtin_method(&["Array"], &[element_ty.as_ref().clone()], member)
            }
            Ty::Map(key_ty, val_ty) => self.resolve_builtin_method(
                &["Map"],
                &[key_ty.as_ref().clone(), val_ty.as_ref().clone()],
                member,
            ),
            Ty::Primitive(PrimitiveType::String)
            | Ty::Literal(baml_base::Literal::String(_), _) => {
                self.resolve_builtin_method(&["String"], &[], member)
            }
            Ty::Primitive(
                p @ (PrimitiveType::Image
                | PrimitiveType::Audio
                | PrimitiveType::Video
                | PrimitiveType::Pdf),
            ) => self.resolve_builtin_method(p.builtin_class_path(), &[], member),
            Ty::Optional(inner) => {
                // Drill through Optional to resolve the member on the inner type
                self.try_resolve_member_on_ty(inner, member)
            }
            Ty::Unknown => {
                // Unknown propagates — treat as if the field exists with Unknown type
                Some(Ty::Unknown)
            }
            _ => None,
        }
    }

    /// Look up class fields from the package items (via item tree).
    ///
    /// Returns a map of field name → resolved field type.
    fn lookup_class_fields(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
    ) -> FxHashMap<Name, Ty> {
        let mut result = FxHashMap::default();
        let short = Self::unqualify(class_name);
        if let Some(def) = self.package_items.lookup_type(&[short]) {
            if let Definition::Class(class_loc) = def {
                let file = class_loc.file(self.context.db());
                let item_tree = baml_compiler2_hir::file_item_tree(self.context.db(), file);
                let class_data = &item_tree[class_loc.id(self.context.db())];
                for field in &class_data.fields {
                    let mut diags = Vec::new();
                    let field_ty = field
                        .type_expr
                        .as_ref()
                        .map(|te| {
                            let ty = crate::lower_type_expr::lower_type_expr(
                                self.context.db(),
                                &te.expr,
                                self.package_items,
                                &mut diags,
                            );
                            for diag in diags.drain(..) {
                                self.context.report_at_span(diag, te.span);
                            }
                            ty
                        })
                        .unwrap_or(Ty::Unknown);
                    result.insert(field.name.clone(), field_ty);
                }
            }
        }
        result
    }

    /// Look up a class method by name from the item tree.
    ///
    /// Methods are stored on the `Class` entry directly (not in the package
    /// namespace), so we resolve the class, iterate its method IDs, and match
    /// by name.
    fn lookup_class_method(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        method_name: &Name,
    ) -> Option<Ty> {
        let short = Self::unqualify(class_name);
        let def = self.package_items.lookup_type(&[short])?;
        let Definition::Class(class_loc) = def else {
            return None;
        };
        let db = self.context.db();
        let file = class_loc.file(db);
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];

        for &method_id in &class_data.methods {
            let method_data = &item_tree[method_id];
            if method_data.name == *method_name {
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
                let mut diags = Vec::new();
                let ty = Ty::Function {
                    params: sig
                        .params
                        .iter()
                        .map(|(n, te)| {
                            (
                                Some(n.clone()),
                                crate::lower_type_expr::lower_type_expr(
                                    db,
                                    te,
                                    self.package_items,
                                    &mut diags,
                                ),
                            )
                        })
                        .collect(),
                    ret: Box::new(
                        sig.return_type
                            .as_ref()
                            .map(|te| {
                                crate::lower_type_expr::lower_type_expr(
                                    db,
                                    te,
                                    self.package_items,
                                    &mut diags,
                                )
                            })
                            .unwrap_or(Ty::Unknown),
                    ),
                };
                // Note: diags from method signatures are reported at definition site.
                return Some(ty);
            }
        }
        None
    }

    /// Check if a FieldAccess base is a primitive type name used for static
    /// method access (e.g. `image.from_url(...)`, `pdf.from_base64(...)`).
    ///
    /// Returns `Some(method_ty)` if the base is a recognized primitive type name
    /// and the field is a valid static method on the corresponding builtin class.
    /// Returns `None` to fall through to normal FieldAccess resolution.
    fn try_primitive_static_access(
        &self,
        base_id: ExprId,
        field: &Name,
        body: &ExprBody,
    ) -> Option<Ty> {
        let base_expr = &body.exprs[base_id];
        let Expr::Path(segments) = base_expr else {
            return None;
        };
        if segments.len() != 1 {
            return None;
        }
        let name = segments[0].as_str();

        // Map lowercase primitive type names to their builtin class paths.
        let class_path: &[&str] = match name {
            "image" => &["media", "Image"],
            "audio" => &["media", "Audio"],
            "video" => &["media", "Video"],
            "pdf" => &["media", "Pdf"],
            "string" => &["String"],
            _ => return None,
        };

        self.resolve_builtin_method(class_path, &[], field)
    }

    /// Resolve a method or field on a builtin class declared in the `"baml"` package.
    ///
    /// 1. Fetches `package_items(db, "baml")`.
    /// 2. Looks up `class_name` in the root namespace.
    /// 3. Binds the class's `generic_params` to `type_args` (e.g. `{T → int}`).
    /// 4. Searches the class methods for `member_name`, lowering the method's
    ///    parameter and return types with type variable substitution applied.
    /// 5. Falls back to checking class fields.
    ///
    /// Returns `None` if the class or member is not found.
    fn resolve_builtin_method(
        &self,
        class_path: &[&str],
        type_args: &[Ty],
        member_name: &Name,
    ) -> Option<Ty> {
        let db = self.context.db();
        let baml_pkg_id =
            baml_compiler2_hir::package::PackageId::new(db, baml_base::Name::new("baml"));
        let baml_items = baml_compiler2_hir::package::package_items(db, baml_pkg_id);

        // Look up the class by path (e.g. &["Array"] or &["media", "Image"]).
        let path: Vec<Name> = class_path.iter().map(|s| baml_base::Name::new(s)).collect();
        let def = baml_items.lookup_type(&path)?;
        let baml_compiler2_hir::contributions::Definition::Class(class_loc) = def else {
            return None;
        };

        let file = class_loc.file(db);
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];

        // Bind generic type variables: e.g. {T → int} for Array<int>.
        let bindings = crate::generics::bind_type_vars(&class_data.generic_params, type_args);

        // Search methods first.
        for &method_id in &class_data.methods {
            let method_data = &item_tree[method_id];
            if method_data.name == *member_name {
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
                let mut diags = Vec::new();
                let params: Vec<(Option<Name>, Ty)> = sig
                    .params
                    .iter()
                    .map(|(n, te)| {
                        let ty = crate::generics::lower_type_expr_with_generics(
                            db,
                            te,
                            self.package_items,
                            &bindings,
                            &mut diags,
                        );
                        (Some(n.clone()), ty)
                    })
                    .collect();
                let ret = sig
                    .return_type
                    .as_ref()
                    .map(|te| {
                        crate::generics::lower_type_expr_with_generics(
                            db,
                            te,
                            self.package_items,
                            &bindings,
                            &mut diags,
                        )
                    })
                    .unwrap_or(Ty::Void);
                // Discard diags — they will be reported at the definition site
                // (the builtin .baml stub). We don't want to spam user code
                // with unresolved-type errors from builtin signatures.
                drop(diags);
                return Some(Ty::Function {
                    params,
                    ret: Box::new(ret),
                });
            }
        }

        // Fall back to fields (e.g. Request.method, Request.url).
        for field in &class_data.fields {
            if field.name == *member_name {
                let mut diags = Vec::new();
                let field_ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        crate::generics::lower_type_expr_with_generics(
                            db,
                            &te.expr,
                            self.package_items,
                            &bindings,
                            &mut diags,
                        )
                    })
                    .unwrap_or(Ty::Unknown);
                drop(diags);
                return Some(field_ty);
            }
        }

        None
    }

    /// Look up enum variants from the package items (via item tree).
    fn lookup_enum_variants(&self, enum_name: &crate::ty::QualifiedTypeName) -> Vec<Name> {
        let short = Self::unqualify(enum_name);
        if let Some(def) = self.package_items.lookup_type(&[short]) {
            if let Definition::Enum(enum_loc) = def {
                let file = enum_loc.file(self.context.db());
                let item_tree = baml_compiler2_hir::file_item_tree(self.context.db(), file);
                let enum_data = &item_tree[enum_loc.id(self.context.db())];
                return enum_data.variants.iter().map(|v| v.name.clone()).collect();
            }
        }
        Vec::new()
    }

    // ── Evolving Container Mutations ─────────────────────────────────────────

    /// Extract the local variable name from an expression, if it's a simple
    /// single-segment path that refers to a known local.
    fn expr_local_name(&self, expr_id: ExprId, body: &ExprBody) -> Option<Name> {
        match &body.exprs[expr_id] {
            Expr::Path(segments) if segments.len() == 1 => {
                let name = &segments[0];
                if self.locals.contains_key(name) {
                    Some(name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Try to handle a container mutation method call: x.push(val) / x.append(val).
    ///
    /// This is the "evolving path" for container mutations — it intercepts
    /// `.push()` / `.append()` and index assignment *before* normal method
    /// resolution via `resolve_builtin_method`. See the doc comment on
    /// `Ty::EvolvingList` for why two paths exist.
    ///
    /// If the callee is `base.push(arg)` or `base.append(arg)` where base is a
    /// local with type `List(T)` or `EvolvingList(T)`:
    /// - If `T == Never`: first establishment → update local to `[Evolving]List(arg_ty)`
    /// - If `arg_ty <: T`: ok
    /// - Otherwise: type error
    ///
    /// Returns `Some(return_ty)` if handled, `None` to fall through to the
    /// builtin method resolution path.
    fn try_container_method_call(
        &mut self,
        call_expr_id: ExprId,
        callee_id: ExprId,
        args: &[ExprId],
        body: &ExprBody,
    ) -> Option<Ty> {
        // After AST lowering, method calls are always FieldAccess:
        //   x.push(val) → Call { callee: FieldAccess { base: Path(["x"]), field: "push" }, ... }
        let (base_id, local_name, method_name) = match &body.exprs[callee_id] {
            Expr::FieldAccess { base, field } => {
                let name = self.expr_local_name(*base, body)?;
                (*base, name, field.clone())
            }
            _ => return None,
        };

        let local_ty = self.locals.get(&local_name)?.clone();

        match method_name.as_str() {
            "push" | "append" if args.len() == 1 => {
                let (elem_ty, is_evolving) = match &local_ty {
                    Ty::EvolvingList(elem) => (elem, true),
                    Ty::List(elem) => (elem, false),
                    _ => return None,
                };

                let arg_ty = self.infer_expr(args[0], body);
                let widened_arg = arg_ty.clone().widen_fresh();

                if matches!(**elem_ty, Ty::Never) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingList(Box::new(widened_arg))
                    } else {
                        Ty::List(Box::new(widened_arg))
                    };
                    self.locals.insert(local_name, new_ty);
                } else if !self.is_subtype(&widened_arg, elem_ty) {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: *elem_ty.clone(),
                            got: widened_arg,
                        },
                        args[0],
                        Vec::new(),
                    );
                }

                self.record_expr_type(base_id, local_ty);
                self.record_expr_type(callee_id, Ty::Unknown);
                let result = Ty::Primitive(PrimitiveType::Null);
                self.record_expr_type(call_expr_id, result.clone());
                Some(result)
            }
            _ => None,
        }
    }

    /// Try to handle index assignment mutation: x[i] = val on List or Map locals.
    ///
    /// For `List(Never)`: first element establishes element type.
    /// For `Map(Never, Never)`: first entry establishes key and value types.
    ///
    /// Returns `true` if handled, `false` to fall through to general case.
    fn try_index_assign_mutation(
        &mut self,
        target_id: ExprId,
        value_id: ExprId,
        body: &ExprBody,
    ) -> bool {
        let (base_id, index_id) = match &body.exprs[target_id] {
            Expr::Index { base, index } => (*base, *index),
            _ => return false,
        };

        let local_name = match self.expr_local_name(base_id, body) {
            Some(n) => n,
            None => return false,
        };
        let local_ty = match self.locals.get(&local_name) {
            Some(ty) => ty.clone(),
            None => return false,
        };

        match &local_ty {
            Ty::List(elem_ty) | Ty::EvolvingList(elem_ty) => {
                let is_evolving = matches!(local_ty, Ty::EvolvingList(_));
                let index_ty = self.infer_expr(index_id, body);
                let val_ty = self.infer_expr(value_id, body);
                let widened_val = val_ty.clone().widen_fresh();

                if matches!(**elem_ty, Ty::Never) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingList(Box::new(widened_val.clone()))
                    } else {
                        Ty::List(Box::new(widened_val.clone()))
                    };
                    self.locals.insert(local_name, new_ty);
                } else if !self.is_subtype(&widened_val, elem_ty) {
                    self.context.report(
                        TirTypeError::TypeMismatch {
                            expected: *elem_ty.clone(),
                            got: widened_val.clone(),
                        },
                        value_id,
                        Vec::new(),
                    );
                }

                self.record_expr_type(base_id, local_ty);
                self.record_expr_type(index_id, index_ty);
                self.record_expr_type(target_id, widened_val);
                self.record_expr_type(value_id, val_ty);
                true
            }
            Ty::Map(key_ty, val_ty) | Ty::EvolvingMap(key_ty, val_ty) => {
                let is_evolving = matches!(local_ty, Ty::EvolvingMap(_, _));
                let index_ty = self.infer_expr(index_id, body);
                let value_ty = self.infer_expr(value_id, body);
                let widened_key = index_ty.clone().widen_fresh();
                let widened_val = value_ty.clone().widen_fresh();

                if matches!(**key_ty, Ty::Never) && matches!(**val_ty, Ty::Never) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingMap(
                            Box::new(widened_key.clone()),
                            Box::new(widened_val.clone()),
                        )
                    } else {
                        Ty::Map(Box::new(widened_key.clone()), Box::new(widened_val.clone()))
                    };
                    self.locals.insert(local_name, new_ty);
                } else {
                    if !self.is_subtype(&widened_key, key_ty) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: *key_ty.clone(),
                                got: widened_key.clone(),
                            },
                            index_id,
                            Vec::new(),
                        );
                    }
                    if !self.is_subtype(&widened_val, val_ty) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: *val_ty.clone(),
                                got: widened_val.clone(),
                            },
                            value_id,
                            Vec::new(),
                        );
                    }
                }

                self.record_expr_type(base_id, local_ty);
                self.record_expr_type(index_id, index_ty);
                self.record_expr_type(target_id, widened_val);
                self.record_expr_type(value_id, value_ty);
                true
            }
            _ => false,
        }
    }

    fn join_types(&self, a: &Ty, b: &Ty) -> Ty {
        if matches!(a, Ty::Never) {
            return b.clone();
        }
        if matches!(b, Ty::Never) {
            return a.clone();
        }
        if matches!(a, Ty::Void) || matches!(b, Ty::Void) {
            return Ty::Void;
        }
        if a == b {
            return a.clone();
        }
        // Same literal value, different freshness → normalize to Regular
        if let (Ty::Literal(lit_a, _), Ty::Literal(lit_b, _)) = (a, b) {
            if lit_a == lit_b {
                return Ty::Literal(lit_a.clone(), crate::ty::Freshness::Regular);
            }
        }
        // Build a flat union, deduplicating members
        let mut members = Vec::new();
        let mut push = |ty: &Ty| {
            if let Ty::Union(inner) = ty {
                for m in inner {
                    if !members.contains(m) {
                        members.push(m.clone());
                    }
                }
            } else if !members.contains(ty) {
                members.push(ty.clone());
            }
        };
        push(a);
        push(b);
        if members.len() == 1 {
            members.into_iter().next().unwrap()
        } else {
            Ty::Union(members)
        }
    }

    fn join_all(&self, types: &[Ty]) -> Ty {
        if types.is_empty() {
            return Ty::Never;
        }
        types
            .iter()
            .skip(1)
            .fold(types[0].clone(), |acc, t| self.join_types(&acc, t))
    }

    /// Subtype check — delegates to the normalizer which resolves type aliases
    /// and performs equirecursive structural subtyping.
    fn is_subtype(&self, sub: &Ty, sup: &Ty) -> bool {
        crate::normalize::is_subtype_of(sub, sup, &self.aliases)
    }

    fn infer_binary_op(
        &mut self,
        op: &baml_compiler2_ast::BinaryOp,
        lhs: &Ty,
        rhs: &Ty,
        at: ExprId,
    ) -> Ty {
        // Try constant folding on two literals first.
        if let Some(folded) = Self::try_fold_binary(op, lhs, rhs) {
            return folded;
        }
        use baml_compiler2_ast::BinaryOp;
        match op {
            // Comparison / equality → bool
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Instanceof => Ty::Primitive(PrimitiveType::Bool),

            // Logical → bool
            BinaryOp::And | BinaryOp::Or => Ty::Primitive(PrimitiveType::Bool),

            // Arithmetic: result type depends on operands
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let result = Self::infer_arithmetic(op, lhs, rhs);
                if matches!(result, Ty::Unknown)
                    && !matches!(lhs, Ty::Unknown | Ty::Error)
                    && !matches!(rhs, Ty::Unknown | Ty::Error)
                {
                    self.context.report_simple(
                        TirTypeError::InvalidBinaryOp {
                            op: op.clone(),
                            lhs: lhs.clone(),
                            rhs: rhs.clone(),
                        },
                        at,
                    );
                }
                result
            }

            // Bitwise → int
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => Ty::Primitive(PrimitiveType::Int),
        }
    }

    /// Determine the result type of an arithmetic operation (non-literal fallback).
    ///
    /// String concatenation is only valid for `Add`; other arithmetic ops on
    /// strings are invalid and return `Unknown` (triggering an error upstream).
    fn infer_arithmetic(op: &baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Ty {
        let base_ty = |ty: &Ty| -> Option<PrimitiveType> {
            match ty {
                Ty::Primitive(p) => Some(p.clone()),
                Ty::Literal(lit, _) => Some(PrimitiveType::from_literal(lit)),
                _ => None,
            }
        };
        match (base_ty(lhs), base_ty(rhs)) {
            (Some(PrimitiveType::Float), _) | (_, Some(PrimitiveType::Float)) => {
                Ty::Primitive(PrimitiveType::Float)
            }
            (Some(PrimitiveType::Int), Some(PrimitiveType::Int)) => {
                Ty::Primitive(PrimitiveType::Int)
            }
            (Some(PrimitiveType::String), _) | (_, Some(PrimitiveType::String)) => {
                // String concatenation only for Add
                if matches!(op, baml_compiler2_ast::BinaryOp::Add) {
                    Ty::Primitive(PrimitiveType::String)
                } else {
                    Ty::Unknown
                }
            }
            _ => Ty::Unknown,
        }
    }

    fn infer_unary_op(&mut self, op: &baml_compiler2_ast::UnaryOp, operand: &Ty, at: ExprId) -> Ty {
        // Try constant folding on a literal first.
        if let Some(folded) = Self::try_fold_unary(op, operand) {
            return folded;
        }
        match op {
            baml_compiler2_ast::UnaryOp::Not => Ty::Primitive(PrimitiveType::Bool),
            baml_compiler2_ast::UnaryOp::Neg => match operand {
                Ty::Primitive(PrimitiveType::Int) => Ty::Primitive(PrimitiveType::Int),
                Ty::Primitive(PrimitiveType::Float) => Ty::Primitive(PrimitiveType::Float),
                Ty::Unknown | Ty::Error => Ty::Unknown,
                _ => {
                    self.context.report_simple(
                        TirTypeError::InvalidUnaryOp {
                            op: op.clone(),
                            operand: operand.clone(),
                        },
                        at,
                    );
                    Ty::Unknown
                }
            },
        }
    }

    // ── Constant Folding ─────────────────────────────────────────────────────

    fn merge_freshness(a: crate::ty::Freshness, b: crate::ty::Freshness) -> crate::ty::Freshness {
        use crate::ty::Freshness;
        match (a, b) {
            (Freshness::Regular, Freshness::Regular) => Freshness::Regular,
            _ => Freshness::Fresh,
        }
    }

    /// Try to fold a unary operation on a literal into a new literal.
    fn try_fold_unary(op: &baml_compiler2_ast::UnaryOp, operand: &Ty) -> Option<Ty> {
        use crate::ty::LiteralValue;
        let (lit, f) = match operand {
            Ty::Literal(lit, f) => (lit, *f),
            _ => return None,
        };
        match op {
            baml_compiler2_ast::UnaryOp::Neg => match lit {
                LiteralValue::Int(n) => Some(Ty::Literal(LiteralValue::Int(n.checked_neg()?), f)),
                LiteralValue::Float(s) => {
                    let v: f64 = s.parse().ok()?;
                    Some(Ty::Literal(LiteralValue::Float(format_float(-v)?), f))
                }
                _ => None,
            },
            baml_compiler2_ast::UnaryOp::Not => match lit {
                LiteralValue::Bool(b) => Some(Ty::Literal(LiteralValue::Bool(!b), f)),
                _ => None,
            },
        }
    }

    /// Try to fold a binary operation on two literals into a new literal.
    fn try_fold_binary(op: &baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<Ty> {
        use baml_compiler2_ast::BinaryOp;

        use crate::ty::LiteralValue;

        let (lhs_lit, lhs_f) = match lhs {
            Ty::Literal(lit, f) => (lit, *f),
            _ => return None,
        };
        let (rhs_lit, rhs_f) = match rhs {
            Ty::Literal(lit, f) => (lit, *f),
            _ => return None,
        };
        let f = Self::merge_freshness(lhs_f, rhs_f);

        // Int × Int
        if let (LiteralValue::Int(a), LiteralValue::Int(b)) = (lhs_lit, rhs_lit) {
            let (a, b) = (*a, *b);
            return match op {
                BinaryOp::Add => Some(Ty::Literal(LiteralValue::Int(a.checked_add(b)?), f)),
                BinaryOp::Sub => Some(Ty::Literal(LiteralValue::Int(a.checked_sub(b)?), f)),
                BinaryOp::Mul => Some(Ty::Literal(LiteralValue::Int(a.checked_mul(b)?), f)),
                BinaryOp::Div => Some(Ty::Literal(LiteralValue::Int(a.checked_div(b)?), f)),
                BinaryOp::Mod => Some(Ty::Literal(LiteralValue::Int(a.checked_rem(b)?), f)),
                BinaryOp::BitAnd => Some(Ty::Literal(LiteralValue::Int(a & b), f)),
                BinaryOp::BitOr => Some(Ty::Literal(LiteralValue::Int(a | b), f)),
                BinaryOp::BitXor => Some(Ty::Literal(LiteralValue::Int(a ^ b), f)),
                BinaryOp::Shl => {
                    let shift = u32::try_from(b).ok()?;
                    Some(Ty::Literal(LiteralValue::Int(a.checked_shl(shift)?), f))
                }
                BinaryOp::Shr => {
                    let shift = u32::try_from(b).ok()?;
                    Some(Ty::Literal(LiteralValue::Int(a.checked_shr(shift)?), f))
                }
                BinaryOp::Eq => Some(Ty::Literal(LiteralValue::Bool(a == b), f)),
                BinaryOp::Ne => Some(Ty::Literal(LiteralValue::Bool(a != b), f)),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f)),
                BinaryOp::Le => Some(Ty::Literal(LiteralValue::Bool(a <= b), f)),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f)),
                BinaryOp::Ge => Some(Ty::Literal(LiteralValue::Bool(a >= b), f)),
                _ => None,
            };
        }

        // Float × Float
        if let (LiteralValue::Float(a_s), LiteralValue::Float(b_s)) = (lhs_lit, rhs_lit) {
            let a: f64 = a_s.parse().ok()?;
            let b: f64 = b_s.parse().ok()?;
            return match op {
                BinaryOp::Add => Some(Ty::Literal(LiteralValue::Float(format_float(a + b)?), f)),
                BinaryOp::Sub => Some(Ty::Literal(LiteralValue::Float(format_float(a - b)?), f)),
                BinaryOp::Mul => Some(Ty::Literal(LiteralValue::Float(format_float(a * b)?), f)),
                BinaryOp::Div if b != 0.0 => {
                    Some(Ty::Literal(LiteralValue::Float(format_float(a / b)?), f))
                }
                BinaryOp::Mod if b != 0.0 => {
                    Some(Ty::Literal(LiteralValue::Float(format_float(a % b)?), f))
                }
                BinaryOp::Eq => Some(Ty::Literal(LiteralValue::Bool(a == b), f)),
                BinaryOp::Ne => Some(Ty::Literal(LiteralValue::Bool(a != b), f)),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f)),
                BinaryOp::Le => Some(Ty::Literal(LiteralValue::Bool(a <= b), f)),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f)),
                BinaryOp::Ge => Some(Ty::Literal(LiteralValue::Bool(a >= b), f)),
                _ => None,
            };
        }

        // Bool × Bool
        if let (LiteralValue::Bool(a), LiteralValue::Bool(b)) = (lhs_lit, rhs_lit) {
            let (a, b) = (*a, *b);
            return match op {
                BinaryOp::And => Some(Ty::Literal(LiteralValue::Bool(a && b), f)),
                BinaryOp::Or => Some(Ty::Literal(LiteralValue::Bool(a || b), f)),
                BinaryOp::Eq => Some(Ty::Literal(LiteralValue::Bool(a == b), f)),
                BinaryOp::Ne => Some(Ty::Literal(LiteralValue::Bool(a != b), f)),
                _ => None,
            };
        }

        // String × String
        if let (LiteralValue::String(a), LiteralValue::String(b)) = (lhs_lit, rhs_lit) {
            return match op {
                BinaryOp::Add => Some(Ty::Literal(LiteralValue::String(format!("{a}{b}")), f)),
                BinaryOp::Eq => Some(Ty::Literal(LiteralValue::Bool(a == b), f)),
                BinaryOp::Ne => Some(Ty::Literal(LiteralValue::Bool(a != b), f)),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f)),
                BinaryOp::Le => Some(Ty::Literal(LiteralValue::Bool(a <= b), f)),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f)),
                BinaryOp::Ge => Some(Ty::Literal(LiteralValue::Bool(a >= b), f)),
                _ => None,
            };
        }

        None
    }

    fn assign_op_to_binary_op(op: &baml_compiler2_ast::AssignOp) -> baml_compiler2_ast::BinaryOp {
        use baml_compiler2_ast::{AssignOp, BinaryOp};
        match op {
            AssignOp::Add => BinaryOp::Add,
            AssignOp::Sub => BinaryOp::Sub,
            AssignOp::Mul => BinaryOp::Mul,
            AssignOp::Div => BinaryOp::Div,
            AssignOp::Mod => BinaryOp::Mod,
            AssignOp::BitAnd => BinaryOp::BitAnd,
            AssignOp::BitOr => BinaryOp::BitOr,
            AssignOp::BitXor => BinaryOp::BitXor,
            AssignOp::Shl => BinaryOp::Shl,
            AssignOp::Shr => BinaryOp::Shr,
        }
    }
}

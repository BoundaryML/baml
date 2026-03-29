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
use rustc_hash::{FxHashMap, FxHashSet};
use text_size::TextRange;

use crate::{
    infer_context::{InferContext, RelatedLocation, TirTypeError, TypeCheckDiagnostics},
    package_interface::PackageResolutionContext,
    ty::{Freshness, PrimitiveType, Ty, TyAttr},
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
#[allow(clippy::enum_variant_names)]
enum PatternMatchStrength {
    NoMatch,
    MayMatch,
    DefiniteMatch,
}

#[derive(Debug, Default, Clone)]
struct ThrowPatternMatches {
    may_match: BTreeSet<Ty>,
    definitely_handled: BTreeSet<Ty>,
}

/// Result of resolving a member on a builtin class (Array, Map, String, media types).
/// Distinguishes methods (which have locs) from fields (which are just types).
enum BuiltinResolution<'db> {
    Method {
        ty: Ty,
        class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    Field(Ty),
}

impl BuiltinResolution<'_> {
    fn into_ty(self) -> Ty {
        match self {
            BuiltinResolution::Method { ty, .. } => ty,
            BuiltinResolution::Field(ty) => ty,
        }
    }
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
    /// Member resolutions: for field-access expressions that resolved to a
    /// class field, enum variant, method, or free function — records the
    /// structural path so MIR can emit the correct `QualifiedName` and LSP
    /// can navigate to the definition.
    resolutions: FxHashMap<ExprId, crate::inference::MemberResolution<'db>>,
    /// Resolution context: own `PackageItems` + dependency `PackageInterfaces`.
    res_ctx: &'db PackageResolutionContext<'db>,
    /// Convenience: own package items (from `res_ctx`).
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
    /// Namespace path for the file being analyzed (e.g. `["env"]` for `baml/env.baml`).
    ns_context: Vec<Name>,
    /// Residual throw facts for each catch expression after applying all clauses.
    catch_residual_throws: FxHashMap<ExprId, BTreeSet<Ty>>,
    /// Match expressions that the exhaustiveness checker determined cover all cases.
    exhaustive_matches: FxHashSet<ExprId>,
    /// Generic type parameters in scope for this function (e.g. `["T"]` for
    /// `function foo<T>(...)`). Used when lowering type annotations inside the
    /// function body so that `T` resolves to `Ty::TypeVar("T", TyAttr::default())` rather than
    /// `Ty::Unknown`.
    pub generic_params: Vec<Name>,
}

impl<'db> TypeInferenceBuilder<'db> {
    pub fn new(
        context: InferContext<'db>,
        res_ctx: &'db PackageResolutionContext<'db>,
        package_id: PackageId<'db>,
        scope: ScopeId<'db>,
        aliases: HashMap<crate::ty::QualifiedTypeName, Ty>,
    ) -> Self {
        let db = context.db();
        let package_items = res_ctx.own_items;
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, scope.file(db));
        let ns_context = pkg_info.namespace_path;
        Self {
            context,
            expressions: FxHashMap::default(),
            bindings: FxHashMap::default(),
            resolutions: FxHashMap::default(),
            res_ctx,
            package_items,
            package_id,
            scope,
            declared_return_ty: None,
            locals: FxHashMap::default(),
            declared_types: FxHashMap::default(),
            aliases,
            ns_context,
            catch_residual_throws: FxHashMap::default(),
            exhaustive_matches: FxHashSet::default(),
            generic_params: Vec::new(),
        }
    }

    /// Set the generic type parameters for this function scope.
    pub fn set_generic_params(&mut self, params: Vec<Name>) {
        self.generic_params = params;
    }

    /// Finish building and return the accumulated results.
    #[allow(clippy::type_complexity)]
    pub fn finish(
        self,
    ) -> (
        FxHashMap<ExprId, Ty>,
        FxHashMap<PatId, Ty>,
        FxHashMap<ExprId, crate::inference::MemberResolution<'db>>,
        FxHashSet<ExprId>,
        TypeCheckDiagnostics<'db>,
    ) {
        let diagnostics = self.context.finish();
        (
            self.expressions,
            self.bindings,
            self.resolutions,
            self.exhaustive_matches,
            diagnostics,
        )
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
            Expr::Literal(lit) => Self::infer_literal(lit),
            Expr::Null => Ty::Primitive(PrimitiveType::Null, TyAttr::default()),
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
                    Self::join_types(&then_ty, &else_ty)
                } else {
                    Ty::Void {
                        attr: TyAttr::default(),
                    }
                };

                // Restore original types after the if expression.
                crate::narrowing::restore_narrowings(saved, &mut self.locals);

                result_ty
            }
            Expr::Call { .. } => {
                // Delegate to check_expr with Ty::Unknown so the generic
                // inference logic in check_expr handles all call expressions.
                self.check_expr(
                    expr_id,
                    body,
                    &Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                )
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
                    let remaining = stmts.len() - div_idx - 1 + usize::from(tail_expr.is_some());
                    if remaining > 0 {
                        self.context.report_at_stmt(
                            crate::infer_context::TirTypeError::DeadCode {
                                after: div_stmt,
                                unreachable_count: remaining,
                            },
                            div_stmt,
                        );
                    }
                    Ty::Never {
                        attr: TyAttr::default(),
                    }
                } else {
                    tail_expr
                        .map(|e| self.infer_expr(e, body))
                        .unwrap_or(Ty::Void {
                            attr: TyAttr::default(),
                        })
                }
            }
            Expr::FieldAccess { base, field } => {
                // Check for primitive-type static method access first:
                // `image.from_url(...)` where `image` is a type name, not a value.
                if let Some(ty) = self.try_primitive_static_access(expr_id, *base, field, body) {
                    ty
                // Check for package access: `baml.Array.length`, `env.get`, etc.
                } else if let Some(ty) = self.try_package_access(expr_id, *base, field, body) {
                    ty
                } else {
                    let base_ty = self.infer_expr(*base, body);
                    self.resolve_member(&base_ty, field, expr_id)
                }
            }
            Expr::Array { elements } => {
                let elem_types: Vec<Ty> =
                    elements.iter().map(|e| self.infer_expr(*e, body)).collect();
                let elem_ty = Self::join_all(&elem_types);
                Ty::List(Box::new(elem_ty), TyAttr::default())
            }
            Expr::Map { entries } => {
                let mut key_types = Vec::new();
                let mut val_types = Vec::new();
                for (k, v) in entries {
                    key_types.push(self.infer_expr(*k, body));
                    val_types.push(self.infer_expr(*v, body));
                }
                let key_ty = Self::join_all(&key_types);
                let val_ty = Self::join_all(&val_types);
                Ty::Map(Box::new(key_ty), Box::new(val_ty), TyAttr::default())
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.infer_expr(*lhs, body);
                let rhs_ty = self.infer_expr(*rhs, body);
                self.infer_binary_op(*op, &lhs_ty, &rhs_ty, expr_id)
            }
            Expr::Unary { op, expr } => {
                let operand_ty = self.infer_expr(*expr, body);
                self.infer_unary_op(*op, &operand_ty, expr_id)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => self.infer_match_expr(expr_id, *scrutinee, arms, body),
            Expr::Catch { base, clauses } => {
                self.infer_catch_expr(expr_id, *base, clauses, body, None)
            }
            Expr::Throw { value } => {
                self.infer_expr(*value, body);
                Ty::Never {
                    attr: TyAttr::default(),
                }
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
                        self.package_items
                            .lookup_type(&self.ns_context, n)
                            .map(|def| {
                                Ty::Class(
                                    crate::lower_type_expr::qualify_def(self.context.db(), def, n),
                                    TyAttr::default(),
                                )
                            })
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    })
            }
            Expr::Index { base, index } => {
                let base_ty = self.infer_expr(*base, body);
                self.infer_expr(*index, body);
                match base_ty {
                    Ty::List(elem_ty, _) | Ty::EvolvingList(elem_ty, _) => *elem_ty,
                    Ty::Map(_, val_ty, _) | Ty::EvolvingMap(_, val_ty, _) => *val_ty,
                    Ty::Unknown { attr: _ } | Ty::Error { attr: _ } => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                    _ => {
                        self.context.report_simple(
                            TirTypeError::NotIndexable {
                                ty: base_ty.clone(),
                            },
                            expr_id,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    }
                }
            }
            Expr::Lambda(func_def) => {
                // Synthesis mode: no expected type available.
                // All param types MUST be annotated; unannotated params produce an error.
                let mut param_tys: Vec<(Option<baml_base::Name>, Ty)> = Vec::new();

                for param in &func_def.params {
                    let param_ty = match &param.type_expr {
                        Some(te) => {
                            let mut diags = Vec::new();
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                self.context.db(),
                                &te.expr,
                                self.package_items,
                                &self.ns_context,
                                &self.generic_params,
                                &mut diags,
                            )
                        }
                        None => {
                            // No annotation and no expected type → error
                            self.context.report_simple(
                                TirTypeError::CannotInferLambdaParamType {
                                    param_name: param.name.clone(),
                                },
                                expr_id,
                            );
                            Ty::Unknown {
                                attr: TyAttr::default(),
                            }
                        }
                    };
                    param_tys.push((Some(param.name.clone()), param_ty));
                }

                // Lower optional return type annotation
                let return_annotation = func_def.return_type.as_ref().map(|te| {
                    let mut diags = Vec::new();
                    crate::lower_type_expr::lower_type_expr_in_ns(
                        self.context.db(),
                        &te.expr,
                        self.package_items,
                        &self.ns_context,
                        &self.generic_params,
                        &mut diags,
                    )
                });

                // Infer the lambda body using save/restore approach
                let ret_ty = self.infer_lambda_body(
                    func_def,
                    &param_tys,
                    return_annotation.as_ref(),
                    expr_id,
                );

                Ty::Function {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                    attr: TyAttr::default(),
                }
            }
            Expr::Missing => Ty::Unknown {
                attr: TyAttr::default(),
            },
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
                    let remaining = stmts.len() - div_idx - 1 + usize::from(tail_expr.is_some());
                    if remaining > 0 {
                        self.context.report_at_stmt(
                            crate::infer_context::TirTypeError::DeadCode {
                                after: div_stmt,
                                unreachable_count: remaining,
                            },
                            div_stmt,
                        );
                    }
                    Ty::Never {
                        attr: TyAttr::default(),
                    }
                } else if let Some(tail) = tail_expr {
                    self.check_expr(*tail, body, expected)
                } else if !matches!(expected, Ty::Unknown { .. } | Ty::Void { .. }) {
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
                    Ty::Void {
                        attr: TyAttr::default(),
                    }
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
                    Self::join_types(&then_ty, &else_ty)
                } else {
                    if !matches!(expected, Ty::Void { .. } | Ty::Unknown { .. }) {
                        self.context
                            .report_simple(TirTypeError::VoidUsedAsValue, expr_id);
                    }
                    Ty::Void {
                        attr: TyAttr::default(),
                    }
                };

                // Restore original types after the if expression.
                crate::narrowing::restore_narrowings(saved, &mut self.locals);

                self.record_expr_type(expr_id, ty.clone());
                ty
            }
            Expr::Array { elements } => {
                let elem_ty = match expected {
                    Ty::List(e, _) | Ty::EvolvingList(e, _) => Some(e),
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
                if let Ty::Class(_, _) = expected {
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
                    Ty::Map(k, v, _) | Ty::EvolvingMap(k, v, _) => Some((k, v)),
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
                let Ty::Literal(expected_lit, _, _) = expected else {
                    unreachable!()
                };
                if lit == expected_lit {
                    let ty =
                        Ty::Literal(expected_lit.clone(), Freshness::Regular, TyAttr::default());
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
            // Call expressions: generic type inference + argument checking.
            Expr::Call { callee, args } => {
                // Container mutation fast path (e.g. x.push(val) on EvolvingList)
                if let Some(result_ty) =
                    self.try_container_method_call(expr_id, *callee, args, body)
                {
                    self.record_expr_type(expr_id, result_ty.clone());
                    return result_ty;
                }

                let is_method_call = matches!(&body.exprs[*callee], Expr::FieldAccess { .. });
                let callee_ty = self.infer_expr(*callee, body);

                match &callee_ty {
                    Ty::Function { params, ret, .. } => {
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

                        let mut bindings = FxHashMap::default();

                        // Phase 0: reverse-infer from expected return type (low priority).
                        // Skip when expected is Unknown/Error — it provides no
                        // information and would pollute forward-inferred bindings
                        // via union_ty (e.g. T = unknown | Node instead of Node).
                        if crate::generics::contains_typevar(ret)
                            && !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. })
                        {
                            crate::generics::infer_bindings(ret, expected, &mut bindings);
                        }

                        // Phase 1: forward-infer from arguments (high priority, overrides)
                        for ((_, param_ty), arg) in effective_params.iter().zip(args.iter()) {
                            let substituted = crate::generics::substitute_ty(param_ty, &bindings);
                            let arg_ty = if !crate::generics::contains_typevar(&substituted) {
                                // Fully concrete — use contextual typing
                                self.check_expr(*arg, body, &substituted)
                            } else if let Ty::Function {
                                params: fn_params, ..
                            } = &substituted
                            {
                                // Partially-resolved function type: check if all param
                                // types are concrete even though return may have type vars.
                                // This enables `map(items, (x) -> { x * 2 })` where the
                                // expected type is `(int) -> U`.
                                let all_params_concrete = fn_params
                                    .iter()
                                    .all(|(_, t)| !crate::generics::contains_typevar(t));
                                if all_params_concrete {
                                    self.check_expr(*arg, body, &substituted)
                                } else {
                                    self.infer_expr(*arg, body)
                                }
                            } else {
                                // TypeVar not yet resolved — just infer
                                self.infer_expr(*arg, body)
                            };
                            crate::generics::infer_bindings(param_ty, &arg_ty, &mut bindings);
                        }

                        // Infer any extra args beyond param count (error recovery)
                        for arg in args.iter().skip(effective_params.len()) {
                            self.infer_expr(*arg, body);
                        }

                        // Phase 2: substitute return type and erase unresolved typevars
                        let substituted_ret = crate::generics::substitute_ty(ret, &bindings);
                        let mut erase_diags = Vec::new();
                        let result = crate::generics::erase_unresolved_typevars(
                            &substituted_ret,
                            &mut erase_diags,
                        );
                        for d in erase_diags {
                            self.context.report_simple(d, expr_id);
                        }

                        // Subtype check against expected type (skip if we did generic
                        // inference — the inference already accounts for expected)
                        if bindings.is_empty()
                            && !matches!(expected, Ty::Unknown { .. } | Ty::Error { .. })
                            && !self.is_subtype(&result, expected)
                        {
                            self.context.report(
                                TirTypeError::TypeMismatch {
                                    expected: expected.clone(),
                                    got: result.clone(),
                                },
                                expr_id,
                                Vec::new(),
                            );
                        }

                        self.record_expr_type(expr_id, result.clone());
                        result
                    }
                    Ty::Unknown { .. } | Ty::Error { .. } => {
                        for arg in args {
                            self.infer_expr(*arg, body);
                        }
                        let ty = Ty::Unknown {
                            attr: TyAttr::default(),
                        };
                        self.record_expr_type(expr_id, ty.clone());
                        ty
                    }
                    _ => {
                        self.context.report_simple(
                            TirTypeError::NotCallable {
                                ty: callee_ty.clone(),
                            },
                            expr_id,
                        );
                        for arg in args {
                            self.infer_expr(*arg, body);
                        }
                        let ty = Ty::Unknown {
                            attr: TyAttr::default(),
                        };
                        self.record_expr_type(expr_id, ty.clone());
                        ty
                    }
                }
            }
            // Catch: propagate expected type to the base expression
            Expr::Catch { base, clauses } => {
                self.infer_catch_expr(expr_id, *base, clauses, body, Some(expected))
            }
            // Lambda: bidirectional checking against expected function type
            Expr::Lambda(func_def) => {
                match expected {
                    Ty::Function {
                        params: expected_params,
                        ret: expected_ret,
                        ..
                    } => {
                        // Checking mode: decompose expected function type.
                        // Arity check
                        if func_def.params.len() != expected_params.len() {
                            self.context.report_simple(
                                TirTypeError::ArgumentCountMismatch {
                                    expected: expected_params.len(),
                                    got: func_def.params.len(),
                                },
                                expr_id,
                            );
                        }

                        // Determine param types: annotation takes precedence, else use expected
                        let mut param_tys: Vec<(Option<baml_base::Name>, Ty)> = Vec::new();
                        for (i, param) in func_def.params.iter().enumerate() {
                            let expected_param_ty = expected_params
                                .get(i)
                                .map(|(_, ty)| ty.clone())
                                .unwrap_or(Ty::Unknown {
                                    attr: TyAttr::default(),
                                });

                            let param_ty = match &param.type_expr {
                                Some(te) => {
                                    let mut diags = Vec::new();
                                    let annotated = crate::lower_type_expr::lower_type_expr_in_ns(
                                        self.context.db(),
                                        &te.expr,
                                        self.package_items,
                                        &self.ns_context,
                                        &self.generic_params,
                                        &mut diags,
                                    );
                                    // Check annotation is compatible with expected
                                    if !self.is_subtype(&expected_param_ty, &annotated) {
                                        self.context.report(
                                            TirTypeError::TypeMismatch {
                                                expected: expected_param_ty.clone(),
                                                got: annotated.clone(),
                                            },
                                            expr_id,
                                            Vec::new(),
                                        );
                                    }
                                    annotated
                                }
                                None => {
                                    // Bidirectional inference: use expected param type
                                    expected_param_ty
                                }
                            };
                            param_tys.push((Some(param.name.clone()), param_ty));
                        }

                        // Determine return type: annotation > expected
                        let return_annotation = func_def.return_type.as_ref().map(|te| {
                            let mut diags = Vec::new();
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                self.context.db(),
                                &te.expr,
                                self.package_items,
                                &self.ns_context,
                                &self.generic_params,
                                &mut diags,
                            )
                        });
                        let effective_ret =
                            return_annotation.as_ref().unwrap_or(expected_ret.as_ref());

                        // Infer/check the lambda body using save/restore approach
                        let ret_ty = self.infer_lambda_body(
                            func_def,
                            &param_tys,
                            Some(effective_ret),
                            expr_id,
                        );

                        let result = Ty::Function {
                            params: param_tys,
                            ret: Box::new(ret_ty),
                            attr: TyAttr::default(),
                        };
                        self.record_expr_type(expr_id, result.clone());
                        result
                    }
                    _ => {
                        // Non-function expected type: fall through to infer-then-check
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
            }
            // All other expressions: infer then subtype-check
            _ => {
                let inferred = self.infer_expr(expr_id, body);
                if matches!(inferred, Ty::Void { .. })
                    && !matches!(expected, Ty::Void { .. } | Ty::Unknown { .. })
                {
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
                matches!(ty, Ty::Never { .. })
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
                        let ann_ty = crate::lower_type_expr::lower_type_expr_in_ns(
                            self.context.db(),
                            &body.type_annotations[*ann_idx],
                            self.package_items,
                            &self.ns_context,
                            &self.generic_params,
                            &mut diags,
                        );
                        for diag in diags {
                            self.context.report_at_type_annot(diag, *ann_idx);
                        }
                        let ty = self.check_expr(*init, body, &ann_ty);
                        if matches!(ty, Ty::Void { .. }) {
                            self.context
                                .report_simple(TirTypeError::VoidUsedAsValue, *init);
                        }
                        ann_ty_for_decl = Some(ann_ty);
                        Some(ty)
                    } else {
                        let ty = self.infer_expr(*init, body);
                        if matches!(ty, Ty::Void { .. }) {
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
                let diverges = matches!(init_ty, Some(Ty::Never { .. }));
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
            // Design note: Stmt::For is kept as a first-class construct (not desugared
            // to While) so we can produce for-loop-specific diagnostics ("cannot iterate
            // over type X") and preserve iteration semantics for downstream codegen.
            // Desugaring to index-based basic blocks happens at MIR lowering time.
            Stmt::For {
                binding,
                collection,
                body: for_body,
            } => {
                // 1. Infer the collection type
                let coll_ty = self.infer_expr(*collection, body);

                // 2. Derive the element type from the collection
                let elem_ty = match &coll_ty {
                    Ty::List(elem, _) => *elem.clone(),
                    Ty::EvolvingList(elem, _) => *elem.clone(),
                    _ => {
                        self.context
                            .report_simple(TirTypeError::NotIterable { ty: coll_ty }, *collection);
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    }
                };

                // 3. Bind the loop variable to the element type
                let pat = &body.patterns[*binding];
                let name = match pat {
                    baml_compiler2_ast::Pattern::Binding(name) => Some(name.clone()),
                    baml_compiler2_ast::Pattern::TypedBinding { name, .. } => Some(name.clone()),
                    _ => None,
                };
                self.bindings.insert(*binding, elem_ty.clone());
                if let Some(name) = name {
                    self.locals.insert(name, elem_ty);
                }

                // 4. Check the body
                self.infer_expr(*for_body, body);
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
                    if !matches!(decl_ty, Ty::Unknown { .. } | Ty::Error { .. })
                        && !matches!(value_ty, Ty::Unknown { .. } | Ty::Error { .. })
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
                let binary_op = Self::assign_op_to_binary_op(*op);
                let result_ty = self.infer_binary_op(binary_op, &target_ty, &value_ty, *target);
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
                    let then_diverged = matches!(then_ty, Some(Ty::Never { .. }));

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
                        covered_cases.clone_from(required);
                    }
                } else if let Some(required) = &required_cases {
                    covered_cases.extend(arm_cases.into_iter().filter(|c| required.contains(c)));
                }
            }
        }

        if let Some(required) = required_cases {
            if catch_all_seen {
                // Catch-all covers everything — exhaustive.
                self.exhaustive_matches.insert(match_expr_id);
            } else {
                let missing: Vec<String> = required
                    .difference(&covered_cases)
                    .map(std::string::ToString::to_string)
                    .collect();
                if missing.is_empty() {
                    // All required cases covered explicitly — exhaustive.
                    self.exhaustive_matches.insert(match_expr_id);
                } else {
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

        Self::join_all(&arm_types)
    }

    fn infer_catch_expr(
        &mut self,
        catch_expr_id: ExprId,
        base_expr_id: ExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        body: &ExprBody,
        expected: Option<&Ty>,
    ) -> Ty {
        let base_ty = if let Some(expected) = expected {
            self.check_expr(base_expr_id, body, expected)
        } else {
            self.infer_expr(base_expr_id, body)
        };
        let mut result_members = vec![base_ty];
        let mut residual = self.catch_base_throw_types(base_expr_id, body);

        for clause in clauses {
            // Compute the clause-level binding type from the current residual throw set.
            // This is the type of the error variable (e.g. `e` in `catch (e)`).
            let clause_binding_ty = if residual.is_empty() {
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            } else {
                Self::facts_to_ty(&residual)
            };
            // Record the clause binding type in the bindings map so MIR can read it.
            self.bindings.insert(clause.binding, clause_binding_ty);

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

                let narrowed_binding_ty = Self::facts_to_ty(&matches.may_match);
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
        Self::join_all(&result_members)
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
        let declared_ty = crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            declared_expr,
            self.package_items,
            &self.ns_context,
            &self.generic_params,
            &mut diags,
        );
        let span = throws_span.unwrap_or(fallback_span);
        for diag in diags {
            self.context.report_at_span(diag, span);
        }

        let declared = crate::throw_inference::flatten_ty_to_facts(&declared_ty);
        let effective = self.collect_effective_throws(body);

        let mut extra: Vec<String> = effective
            .difference(&declared)
            .map(std::string::ToString::to_string)
            .collect();
        let mut extraneous: Vec<String> = declared
            .difference(&effective)
            .map(std::string::ToString::to_string)
            .collect();
        extra.sort();
        extraneous.sort();

        if !extra.is_empty() {
            self.context.report_at_span(
                TirTypeError::ThrowsContractViolation {
                    declared: declared_ty,
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
            Ty::Primitive(PrimitiveType::Bool, _) => {
                Some(BTreeSet::from(["true".to_string(), "false".to_string()]))
            }
            Ty::Primitive(PrimitiveType::Null, _) => Some(BTreeSet::from(["null".to_string()])),
            Ty::Literal(lit, _, _) => Some(BTreeSet::from([Self::literal_case_name(lit)])),
            Ty::Enum(enum_name, _) => Some(
                self.lookup_enum_variants(enum_name)
                    .into_iter()
                    .map(|variant| format!("{enum_name}.{variant}"))
                    .collect(),
            ),
            Ty::Optional(inner, _) => {
                let mut cases = self.required_match_cases(inner)?;
                cases.insert("null".to_string());
                Some(cases)
            }
            Ty::Union(members, _) => {
                let mut out = BTreeSet::new();
                for member in members {
                    let member_cases = self.required_match_cases(member)?;
                    out.extend(member_cases);
                }
                Some(out)
            }
            Ty::Never { .. } => Some(BTreeSet::new()),
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
                BTreeSet::from([Self::literal_case_name(lit)])
            }
            baml_compiler2_ast::Pattern::Null => BTreeSet::from(["null".to_string()]),
            baml_compiler2_ast::Pattern::EnumVariant { enum_name, variant } => {
                // Use the qualified name from scrutinee_ty when the pattern's
                // enum_name matches (bare or namespace-qualified). For
                // `root.Status.Ok`, enum_name is "root.Status" — strip the
                // `root.` prefix and compare against the QTN's ns+name path.
                let qualified_enum = match scrutinee_ty {
                    Ty::Enum(qtn, _) if Self::enum_name_matches(enum_name, qtn) => qtn.to_string(),
                    _ => enum_name.to_string(),
                };
                BTreeSet::from([format!("{qualified_enum}.{variant}")])
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
                    self.lower_pattern_type_expr(
                        &TypeExpr::Path {
                            segments: vec![name.clone()],
                            attrs: vec![],
                        },
                        at_expr,
                    )
                } else {
                    scrutinee_ty.clone()
                }
            }
            baml_compiler2_ast::Pattern::TypedBinding { ty, .. } => {
                self.lower_pattern_type_expr(ty, at_expr)
            }
            baml_compiler2_ast::Pattern::Literal(lit) => Ty::Literal(
                lit.clone(),
                crate::ty::Freshness::Regular,
                TyAttr::default(),
            ),
            baml_compiler2_ast::Pattern::Null => {
                Ty::Primitive(PrimitiveType::Null, TyAttr::default())
            }
            baml_compiler2_ast::Pattern::EnumVariant { enum_name, variant } => {
                if let Ty::Enum(qn, _) = scrutinee_ty {
                    if Self::enum_name_matches(enum_name, qn) {
                        return Ty::EnumVariant(qn.clone(), variant.clone(), TyAttr::default());
                    }
                }
                if let Some(def) = self.package_items.lookup_type(&self.ns_context, enum_name) {
                    if matches!(def, Definition::Enum(_)) {
                        return Ty::EnumVariant(
                            crate::lower_type_expr::qualify_def(self.context.db(), def, enum_name),
                            variant.clone(),
                            TyAttr::default(),
                        );
                    }
                }
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            baml_compiler2_ast::Pattern::Union(parts) => {
                let mut tys = Vec::new();
                for part in parts {
                    tys.push(self.pattern_narrowed_type(*part, scrutinee_ty, body, at_expr));
                }
                Self::join_all(&tys)
            }
        }
    }

    fn lower_pattern_type_expr(&mut self, expr: &TypeExpr, at_expr: ExprId) -> Ty {
        let mut diags = Vec::new();
        let ty = crate::lower_type_expr::lower_type_expr_in_ns(
            self.context.db(),
            expr,
            self.package_items,
            &self.ns_context,
            &self.generic_params,
            &mut diags,
        );
        for diag in diags {
            self.context.report_simple(diag, at_expr);
        }
        ty
    }

    fn literal_case_name(lit: &baml_base::Literal) -> String {
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
        ) || self
            .package_items
            .lookup_type(&self.ns_context, name)
            .is_some()
    }

    /// Check if a pattern's `enum_name` (which may be dotted like `"root.Status"` or
    /// `"root.llm.Status"`) refers to the same enum as a `QualifiedTypeName`.
    fn enum_name_matches(enum_name: &Name, qtn: &crate::ty::QualifiedTypeName) -> bool {
        // Bare name match: "Status" == qtn.name()
        if qtn.name() == enum_name {
            return true;
        }
        // Dotted name: split on "." and compare
        let parts: Vec<&str> = enum_name.as_str().split('.').collect();
        if parts.len() < 2 {
            return false;
        }
        let name = parts[parts.len() - 1];
        let path = &parts[..parts.len() - 1];
        // Strip leading "root" if present
        let ns_parts = if path.first() == Some(&"root") {
            &path[1..]
        } else {
            path
        };
        // Compare namespace and name
        name == qtn.name().as_str()
            && ns_parts.len() == qtn.namespace().len()
            && ns_parts
                .iter()
                .zip(qtn.namespace().iter())
                .all(|(a, b): (&&str, &Name)| *a == b.as_str())
    }

    fn catch_base_throw_types(&self, base_expr_id: ExprId, body: &ExprBody) -> BTreeSet<Ty> {
        let mut out = BTreeSet::new();
        self.collect_throw_facts_from_expr(base_expr_id, body, &mut out);
        out
    }

    /// Join a set of throw fact types into a single type.
    fn facts_to_ty(facts: &BTreeSet<Ty>) -> Ty {
        if facts.is_empty() {
            return Ty::Never {
                attr: TyAttr::default(),
            };
        }
        let tys: Vec<Ty> = facts.iter().cloned().collect();
        Self::join_all(&tys)
    }

    fn match_throw_types_for_pattern(
        &mut self,
        pattern_id: PatId,
        throw_types: &BTreeSet<Ty>,
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
        throw_fact: &Ty,
        body: &ExprBody,
        at_expr: ExprId,
    ) -> PatternMatchStrength {
        let is_unknown = matches!(
            throw_fact,
            Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. }
        );
        let pattern = &body.patterns[pattern_id];
        match pattern {
            baml_compiler2_ast::Pattern::Binding(name) => {
                if self.is_bare_type_sugar_binding(name) {
                    let lowered = self.lower_pattern_type_expr(
                        &TypeExpr::Path {
                            segments: vec![name.clone()],
                            attrs: vec![],
                        },
                        at_expr,
                    );
                    if Self::ty_covers_fact(&lowered, throw_fact) {
                        PatternMatchStrength::DefiniteMatch
                    } else if is_unknown {
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
                if Self::ty_covers_fact(&lowered, throw_fact) {
                    PatternMatchStrength::DefiniteMatch
                } else if is_unknown {
                    PatternMatchStrength::MayMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
            baml_compiler2_ast::Pattern::Literal(lit) => {
                let lit_ty = Ty::Primitive(PrimitiveType::from_literal(lit), TyAttr::default());
                if &lit_ty == throw_fact || is_unknown {
                    PatternMatchStrength::DefiniteMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
            baml_compiler2_ast::Pattern::Null => {
                if matches!(throw_fact, Ty::Primitive(PrimitiveType::Null, _)) || is_unknown {
                    PatternMatchStrength::DefiniteMatch
                } else {
                    PatternMatchStrength::NoMatch
                }
            }
            baml_compiler2_ast::Pattern::EnumVariant { enum_name, variant } => {
                let matches_variant = match throw_fact {
                    Ty::EnumVariant(qn, v, _) => {
                        Self::enum_name_matches(enum_name, qn) && v == variant
                    }
                    Ty::Enum(qn, _) => Self::enum_name_matches(enum_name, qn),
                    _ => false,
                };
                if matches_variant {
                    PatternMatchStrength::DefiniteMatch
                } else if is_unknown {
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

    /// Check if a pattern type covers a throw fact type.
    fn ty_covers_fact(pattern_ty: &Ty, fact: &Ty) -> bool {
        match pattern_ty {
            Ty::Primitive(p, _) => match fact {
                Ty::Primitive(fp, _) => p == fp,
                Ty::Literal(lit, _, _) => *p == PrimitiveType::from_literal(lit),
                _ => false,
            },
            Ty::Literal(lit, _, _) => {
                let widened = Ty::Primitive(PrimitiveType::from_literal(lit), TyAttr::default());
                &widened == fact
            }
            Ty::Optional(inner, _) => {
                matches!(fact, Ty::Primitive(PrimitiveType::Null, _))
                    || Self::ty_covers_fact(inner, fact)
            }
            Ty::Union(parts, _) => parts.iter().any(|part| Self::ty_covers_fact(part, fact)),
            Ty::Class(qn, _) => matches!(fact, Ty::Class(fqn, _) if fqn == qn),
            Ty::Enum(qn, _) => match fact {
                Ty::Enum(fqn, _) => fqn == qn,
                Ty::EnumVariant(fqn, _, _) => fqn == qn,
                _ => false,
            },
            Ty::TypeAlias(qn, _) => matches!(fact, Ty::TypeAlias(fqn, _) if fqn == qn),
            Ty::EnumVariant(qn, variant, _) => {
                matches!(fact, Ty::EnumVariant(fqn, fv, _) if fqn == qn && fv == variant)
                    || matches!(fact, Ty::Enum(fqn, _) if fqn == qn)
            }
            Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } | Ty::Error { .. } => true,
            _ => false,
        }
    }

    fn collect_effective_throws(&self, body: &ExprBody) -> BTreeSet<Ty> {
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
        out: &mut BTreeSet<Ty>,
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
            Expr::Lambda(_) | Expr::Literal(_) | Expr::Null | Expr::Path(_) | Expr::Missing => {}
        }
    }

    fn collect_effective_throws_from_stmt(
        &self,
        stmt_id: StmtId,
        body: &ExprBody,
        out: &mut BTreeSet<Ty>,
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
            Stmt::For {
                collection,
                body: for_body,
                ..
            } => {
                self.collect_effective_throws_from_expr(*collection, body, out);
                self.collect_effective_throws_from_expr(*for_body, body, out);
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
        out: &mut BTreeSet<Ty>,
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
            Expr::Lambda(_) | Expr::Literal(_) | Expr::Null | Expr::Path(_) | Expr::Missing => {}
        }
    }

    fn collect_throw_facts_from_stmt(
        &self,
        stmt_id: StmtId,
        body: &ExprBody,
        out: &mut BTreeSet<Ty>,
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
            Stmt::For {
                collection,
                body: for_body,
                ..
            } => {
                self.collect_throw_facts_from_expr(*collection, body, out);
                self.collect_throw_facts_from_expr(*for_body, body, out);
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

    fn collect_throw_facts_from_value(&self, value_expr_id: ExprId, out: &mut BTreeSet<Ty>) {
        let unknown_ty = Ty::Unknown {
            attr: TyAttr::default(),
        };
        let thrown_ty = self.expressions.get(&value_expr_id).unwrap_or(&unknown_ty);
        out.extend(crate::throw_inference::flatten_ty_to_facts(thrown_ty));
    }

    fn call_target_name(&self, callee_expr_id: ExprId, body: &ExprBody) -> Option<Name> {
        let segments = Self::expr_to_path_segments(callee_expr_id, body)?;
        if segments.len() < 2 {
            // Single-segment path (free function) — return as-is.
            return if segments.is_empty() {
                None
            } else {
                Some(segments[0].clone())
            };
        }
        // Multi-segment: receiver.method — resolve the receiver's type to get
        // the class name so the target matches throw_inference's "Class.method" keys.
        let receiver = &segments[0];
        let method = &segments[1];
        if let Some(Ty::Class(qn, _)) = self.locals.get(receiver) {
            let ns = qn.namespace();
            let key = if ns.is_empty() {
                format!("{}.{}", qn.name(), method)
            } else {
                let ns_str = ns.iter().map(Name::as_str).collect::<Vec<_>>().join(".");
                format!("{}.{}.{}", ns_str, qn.name(), method)
            };
            Some(Name::new(key))
        } else {
            // Receiver not a known local or not a class — fall back to raw path
            Some(Name::new(
                segments
                    .iter()
                    .map(Name::as_str)
                    .collect::<Vec<_>>()
                    .join("."),
            ))
        }
    }

    fn expr_to_path_segments(expr_id: ExprId, body: &ExprBody) -> Option<Vec<Name>> {
        match &body.exprs[expr_id] {
            Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
            Expr::FieldAccess { base, field } => {
                let mut base_segments = Self::expr_to_path_segments(*base, body)?;
                base_segments.push(field.clone());
                Some(base_segments)
            }
            _ => None,
        }
    }

    fn infer_literal(lit: &baml_base::Literal) -> Ty {
        Ty::Literal(lit.clone(), Freshness::Fresh, TyAttr::default())
    }

    fn infer_path(&mut self, segments: &[Name], _body: &ExprBody, expr_id: ExprId) -> Ty {
        if segments.len() == 1 {
            let name = &segments[0];
            let ty = self.infer_single_name(name);
            // Namespace shorthands like `env`, `sys`, `http` etc. can appear as
            // the base of a FieldAccess expression (e.g. `env.get("KEY")`), where
            // the parent will route them to the `"baml"` package.  Don't emit
            // `UnresolvedName` for these bare identifiers — the parent expression
            // is responsible for resolution and will emit an error if the member
            // doesn't exist.
            let is_baml_ns_shorthand = matches!(
                name.as_str(),
                "env"
                    | "sys"
                    | "http"
                    | "math"
                    | "fs"
                    | "net"
                    | "media"
                    | "llm"
                    | "errors"
                    | "unstable"
            );
            if matches!(ty, Ty::Unknown { .. })
                && !self.locals.contains_key(name)
                && self
                    .package_items
                    .lookup_value(&self.ns_context, name)
                    .is_none()
                && self
                    .package_items
                    .lookup_type(&self.ns_context, name)
                    .is_none()
                && !is_baml_ns_shorthand
            {
                self.context
                    .report_simple(TirTypeError::UnresolvedName { name: name.clone() }, expr_id);
            }
            ty
        } else if segments.len() >= 2 {
            self.infer_multi_segment_path(segments, expr_id)
        } else {
            Ty::Unknown {
                attr: TyAttr::default(),
            }
        }
    }

    /// Resolve a multi-segment path like `baml.llm.render_prompt` or `root.sys.panic`.
    ///
    /// The first segment is either a literal package name or `root`
    /// (which maps to the current file's package).
    fn infer_multi_segment_path(&mut self, segments: &[Name], expr_id: ExprId) -> Ty {
        let first = &segments[0];
        if self.locals.contains_key(first) {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        let db = self.context.db();
        let pkg_name = if first.as_str() == "root" {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, self.scope.file(db));
            pkg_info.package
        } else {
            first.clone()
        };
        let Some(pkg_items) = self.res_ctx.items_for_package(db, &pkg_name) else {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        };

        if pkg_items.namespaces.is_empty() {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }

        let after_pkg = &segments[1..];
        self.resolve_package_item(pkg_items, after_pkg, expr_id)
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            })
    }

    /// Shared helper: resolve a value or type within a package's namespace.
    ///
    /// `path` contains all segments after the package name. The last segment
    /// is the item name; preceding segments are the namespace path.
    ///
    /// Used by both `infer_multi_segment_path` (for `Expr::Path`) and
    /// `try_package_access` (for `Expr::FieldAccess` chains).
    fn resolve_package_item(
        &mut self,
        pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
        path: &[Name],
        expr_id: ExprId,
    ) -> Option<Ty> {
        use baml_compiler2_hir::contributions::Definition;

        if path.is_empty() {
            return None;
        }

        // Try as a value (function) in a nested namespace
        let item = path.last().expect("non-empty path");
        let lookup_val = pkg_items.lookup_value(&path[..path.len() - 1], item);
        if let Some(Definition::Function(func_loc)) = lookup_val {
            let db = self.context.db();
            let item_tree_for_func = baml_compiler2_ppir::file_item_tree(db, func_loc.file(db));
            let func_data_for_sig = &item_tree_for_func[func_loc.id(db)];
            let generic_params = &func_data_for_sig.generic_params;
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, func_loc.file(db));
            let ns_context = pkg_info.namespace_path;
            self.resolutions.insert(
                expr_id,
                crate::inference::MemberResolution::Free { func_loc },
            );
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            let mut diags = Vec::new();
            let ty = Ty::Function {
                params: sig
                    .params
                    .iter()
                    .map(|(n, te)| {
                        (
                            Some(n.clone()),
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                te,
                                pkg_items,
                                &ns_context,
                                generic_params,
                                &mut diags,
                            ),
                        )
                    })
                    .collect(),
                ret: Box::new(
                    sig.return_type
                        .as_ref()
                        .map(|te| {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                te,
                                pkg_items,
                                &ns_context,
                                generic_params,
                                &mut diags,
                            )
                        })
                        .unwrap_or(Ty::Unknown {
                            attr: TyAttr::default(),
                        }),
                ),
                attr: TyAttr::default(),
            };
            return Some(ty);
        }

        // Try as a type (class/enum)
        if let Some(def) = pkg_items.lookup_type(&path[..path.len() - 1], item) {
            let db = self.context.db();
            let name = item;
            match def {
                Definition::Class(_) => {
                    let class_qtn = crate::lower_type_expr::qualify_def(db, def, name);
                    return Some(Ty::Class(class_qtn, TyAttr::default()));
                }
                Definition::Enum(_) => {
                    let enum_qtn = crate::lower_type_expr::qualify_def(db, def, name);
                    return Some(Ty::Enum(enum_qtn, TyAttr::default()));
                }
                _ => {}
            }
        }

        None
    }

    /// Resolve a single name to its type.
    ///
    /// Checks local variables first, then value namespace (functions), then
    /// type namespace (classes, enums).
    fn infer_single_name(&self, name: &Name) -> Ty {
        if let Some(ty) = self.locals.get(name) {
            return match ty {
                Ty::EvolvingList(inner, attr) => Ty::List(inner.clone(), attr.clone()),
                Ty::EvolvingMap(k, v, attr) => Ty::Map(k.clone(), v.clone(), attr.clone()),
                other => other.clone(),
            };
        }
        if let Some(def) = self.package_items.lookup_value(&self.ns_context, name) {
            match def {
                Definition::Function(func_loc) => {
                    // Get function signature to build the function type
                    let db = self.context.db();
                    let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
                    let item_tree = baml_compiler2_ppir::file_item_tree(db, func_loc.file(db));
                    let func_data = &item_tree[func_loc.id(db)];
                    let generic_params = &func_data.generic_params;
                    let sig_ns =
                        baml_compiler2_hir::file_package::file_package(db, func_loc.file(db))
                            .namespace_path;
                    let mut diags = Vec::new();

                    // Note: diags from referenced function signatures are not
                    // reported here — they'll be reported at the definition site.
                    Ty::Function {
                        params: sig
                            .params
                            .iter()
                            .map(|(n, te)| {
                                (
                                    Some(n.clone()),
                                    crate::lower_type_expr::lower_type_expr_in_ns(
                                        db,
                                        te,
                                        self.package_items,
                                        &sig_ns,
                                        generic_params,
                                        &mut diags,
                                    ),
                                )
                            })
                            .collect(),
                        ret: Box::new(
                            sig.return_type
                                .as_ref()
                                .map(|te| {
                                    crate::lower_type_expr::lower_type_expr_in_ns(
                                        db,
                                        te,
                                        self.package_items,
                                        &sig_ns,
                                        generic_params,
                                        &mut diags,
                                    )
                                })
                                .unwrap_or(Ty::Unknown {
                                    attr: TyAttr::default(),
                                }),
                        ),
                        attr: TyAttr::default(),
                    }
                }
                _ => Ty::Unknown {
                    attr: TyAttr::default(),
                },
            }
        } else if let Some(def) = self.package_items.lookup_type(&self.ns_context, name) {
            let db = self.context.db();
            match def {
                Definition::Class(_) => Ty::Class(
                    crate::lower_type_expr::qualify_def(db, def, name),
                    TyAttr::default(),
                ),
                Definition::Enum(_) => Ty::Enum(
                    crate::lower_type_expr::qualify_def(db, def, name),
                    TyAttr::default(),
                ),
                Definition::TypeAlias(_) => Ty::TypeAlias(
                    crate::lower_type_expr::qualify_def(db, def, name),
                    TyAttr::default(),
                ),
                _ => Ty::Unknown {
                    attr: TyAttr::default(),
                },
            }
        } else {
            Ty::Unknown {
                attr: TyAttr::default(),
            }
        }
    }

    /// Resolve a member access on a known base type.
    ///
    /// For class types, checks data fields. For enum types, validates variants.
    /// For builtin container types (`Ty::List`, `Ty::Map`) and `Ty::Primitive(String, TyAttr::default())`,
    /// bridges to the `.baml`-declared builtin classes via `resolve_builtin_method`.
    /// Emits `UnresolvedMember` diagnostics when the base type is known but
    /// the member doesn't exist.
    pub fn resolve_member(&mut self, base_ty: &Ty, member: &Name, at: ExprId) -> Ty {
        match base_ty {
            Ty::Class(class_name, _) => {
                // Check class fields
                let class_fields = self.lookup_class_fields(class_name);
                if let Some(field_ty) = class_fields.get(member) {
                    // Store field resolution for LSP navigation
                    if let Some(class_loc) = self.resolve_class_loc(class_name) {
                        self.resolutions.insert(
                            at,
                            crate::inference::MemberResolution::Field {
                                class_loc,
                                field_name: member.clone(),
                            },
                        );
                    }
                    return field_ty.clone();
                }

                // Check class methods via the item tree (methods are stored
                // directly on the Class entry, not in the package namespace).
                if let Some((ty, class_loc, func_loc)) =
                    self.lookup_class_method(class_name, member)
                {
                    self.resolutions.insert(
                        at,
                        crate::inference::MemberResolution::Method {
                            class_loc,
                            func_loc,
                        },
                    );
                    return ty;
                }

                // Known class but member not found — error
                let class_def = self
                    .package_items
                    .lookup_type(class_name.namespace(), class_name.name());
                let related = class_def
                    .map(|def| vec![(RelatedLocation::Item(def), "class defined here")])
                    .unwrap_or_default();
                self.context.report_at_member(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                    related,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::Enum(enum_name, _) => {
                // Validate that the variant exists
                let variants = self.lookup_enum_variants(enum_name);
                if variants.contains(member) {
                    // Store variant resolution for LSP navigation
                    if let Some(enum_loc) = self.resolve_enum_loc(enum_name) {
                        self.resolutions.insert(
                            at,
                            crate::inference::MemberResolution::Variant {
                                enum_loc,
                                variant_name: member.clone(),
                            },
                        );
                    }
                    return Ty::EnumVariant(enum_name.clone(), member.clone(), TyAttr::default());
                }

                // Known enum but variant not found — error
                let enum_def = self
                    .package_items
                    .lookup_type(enum_name.namespace(), enum_name.name());
                let related = enum_def
                    .map(|def| vec![(RelatedLocation::Item(def), "enum defined here")])
                    .unwrap_or_default();
                self.context.report_at_member(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                    related,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::List(element_ty, _) => {
                // Bridge: int[] → Array<int> — resolve via builtin Array class.
                self.resolve_builtin_member(&["Array"], &[element_ty.as_ref().clone()], member, at)
                    .unwrap_or_else(|| {
                        self.context.report_at_member_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    })
            }
            Ty::Map(key_ty, val_ty, _) => {
                // Bridge: map<string, int> → Map<string, int>
                self.resolve_builtin_member(
                    &["Map"],
                    &[key_ty.as_ref().clone(), val_ty.as_ref().clone()],
                    member,
                    at,
                )
                .unwrap_or_else(|| {
                    self.context.report_at_member_simple(
                        TirTypeError::UnresolvedMember {
                            base_type: base_ty.clone(),
                            member: member.clone(),
                        },
                        at,
                    );
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                })
            }
            Ty::Primitive(PrimitiveType::String, _)
            | Ty::Literal(baml_base::Literal::String(_), _, _) => {
                // Bridge: string / "literal" → String class
                self.resolve_builtin_member(&["String"], &[], member, at)
                    .unwrap_or_else(|| {
                        self.context.report_at_member_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    })
            }
            Ty::Primitive(
                p @ (PrimitiveType::Image
                | PrimitiveType::Audio
                | PrimitiveType::Video
                | PrimitiveType::Pdf),
                _,
            ) => {
                // Bridge: each media primitive → its own builtin class in baml.media
                self.resolve_builtin_member(p.builtin_class_path(), &[], member, at)
                    .unwrap_or_else(|| {
                        self.context.report_at_member_simple(
                            TirTypeError::UnresolvedMember {
                                base_type: base_ty.clone(),
                                member: member.clone(),
                            },
                            at,
                        );
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        }
                    })
            }
            Ty::Union(members, _) => {
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
                    // TODO(TyAttr): This union is synthesized from per-member field resolutions,
                    // not a transform of the original union. The original union's attr describes
                    // the union-as-a-whole (e.g. @stream.done), but the result describes field
                    // types which may have different streaming semantics. TyAttr::default() is
                    // probably correct but needs confirmation.
                    Ty::Union(field_tys, TyAttr::default())
                } else {
                    // Report an error for each member that's missing the field
                    for (member_ty, result) in &resolved {
                        if result.is_none() {
                            self.context.report_at_member_simple(
                                TirTypeError::UnresolvedMember {
                                    base_type: member_ty.clone(),
                                    member: member.clone(),
                                },
                                at,
                            );
                        }
                    }
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            }
            Ty::TypeAlias(qtn, _) => {
                // Expand the alias to its concrete type, then recurse.
                if let Some(expanded) = self.aliases.get(qtn) {
                    let expanded = expanded.clone();
                    return self.resolve_member(&expanded, member, at);
                }
                // Alias not in map (cyclic or unresolved) — treat as Unknown
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            Ty::Unknown { .. } => {
                // Base type unknown — can't resolve member, but don't emit error
                // (the base type error was already reported upstream)
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
            _ => {
                // Other types (other primitives, etc.) — no members
                self.context.report_at_member_simple(
                    TirTypeError::UnresolvedMember {
                        base_type: base_ty.clone(),
                        member: member.clone(),
                    },
                    at,
                );
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        }
    }

    /// Try to resolve a member on a type without emitting diagnostics.
    ///
    /// Returns `Some(Ty)` if the member exists, `None` if it doesn't.
    /// Used by `resolve_member` for union type handling.
    fn try_resolve_member_on_ty(&self, ty: &Ty, member: &Name) -> Option<Ty> {
        match ty {
            Ty::Class(class_name, _) => {
                let fields = self.lookup_class_fields(class_name);
                if let Some(field_ty) = fields.get(member) {
                    return Some(field_ty.clone());
                }
                if let Some((method_ty, _, _)) = self.lookup_class_method(class_name, member) {
                    return Some(method_ty);
                }
                None
            }
            Ty::List(element_ty, _) => self
                .resolve_builtin_method(&["Array"], &[element_ty.as_ref().clone()], member)
                .map(BuiltinResolution::into_ty),
            Ty::Map(key_ty, val_ty, _) => self
                .resolve_builtin_method(
                    &["Map"],
                    &[key_ty.as_ref().clone(), val_ty.as_ref().clone()],
                    member,
                )
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(PrimitiveType::String, _)
            | Ty::Literal(baml_base::Literal::String(_), _, _) => self
                .resolve_builtin_method(&["String"], &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Primitive(
                p @ (PrimitiveType::Image
                | PrimitiveType::Audio
                | PrimitiveType::Video
                | PrimitiveType::Pdf),
                _,
            ) => self
                .resolve_builtin_method(p.builtin_class_path(), &[], member)
                .map(BuiltinResolution::into_ty),
            Ty::Optional(inner, _) => {
                // Drill through Optional to resolve the member on the inner type
                self.try_resolve_member_on_ty(inner, member)
            }
            Ty::TypeAlias(qtn, _) => {
                if let Some(expanded) = self.aliases.get(qtn) {
                    let expanded = expanded.clone();
                    self.try_resolve_member_on_ty(&expanded, member)
                } else {
                    None
                }
            }
            Ty::Unknown { .. } => {
                // Unknown propagates — treat as if the field exists with Unknown type
                Some(Ty::Unknown {
                    attr: TyAttr::default(),
                })
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
        let Some(pkg_items_for_class) = self.resolve_class_pkg_items(class_name.package()) else {
            return result;
        };
        if let Some(Definition::Class(class_loc)) =
            pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())
        {
            let file = class_loc.file(self.context.db());
            let ns_context =
                baml_compiler2_hir::file_package::file_package(self.context.db(), file)
                    .namespace_path;
            let item_tree = baml_compiler2_ppir::file_item_tree(self.context.db(), file);
            let class_data = &item_tree[class_loc.id(self.context.db())];
            for field in &class_data.fields {
                let mut diags = Vec::new();
                let field_ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        let ty = crate::lower_type_expr::lower_type_expr_in_ns(
                            self.context.db(),
                            &te.expr,
                            pkg_items_for_class,
                            &ns_context,
                            &class_data.generic_params,
                            &mut diags,
                        );
                        for diag in diags.drain(..) {
                            self.context.report_at_span(diag, te.span);
                        }
                        ty
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                result.insert(field.name.clone(), field_ty);
            }
        }
        result
    }

    /// Fetch `PackageItems` for the package that owns a class type.
    ///
    /// Returns `self.package_items` when the class is in the current package,
    /// or loads the correct foreign package's items when the class lives in a
    /// declared dependency package.
    fn resolve_class_pkg_items(
        &self,
        class_pkg: &baml_base::Name,
    ) -> Option<&'db baml_compiler2_hir::package::PackageItems<'db>> {
        let db = self.context.db();
        self.res_ctx.items_for_package(db, class_pkg)
    }

    /// Resolve a `QualifiedTypeName` to a `ClassLoc` via `package_items` lookup.
    fn resolve_class_loc(
        &self,
        qtn: &crate::ty::QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::ClassLoc<'db>> {
        let pkg_items = self.resolve_class_pkg_items(qtn.package())?;
        match pkg_items.lookup_type(qtn.namespace(), qtn.name())? {
            Definition::Class(class_loc) => Some(class_loc),
            _ => None,
        }
    }

    /// Resolve a `QualifiedTypeName` to an `EnumLoc` via `package_items` lookup.
    fn resolve_enum_loc(
        &self,
        qtn: &crate::ty::QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::EnumLoc<'db>> {
        let db = self.context.db();
        let items = if *qtn.package() == self.package_id.name(db) {
            self.package_items
        } else {
            self.res_ctx.items_for_package(db, qtn.package())?
        };
        match items.lookup_type(qtn.namespace(), qtn.name())? {
            Definition::Enum(enum_loc) => Some(enum_loc),
            _ => None,
        }
    }

    /// Look up a class method by name from the item tree.
    ///
    /// Methods are stored on the `Class` entry directly (not in the package
    /// namespace), so we resolve the class, iterate its method IDs, and match
    /// by name. Returns the method type along with the class and function locs
    /// so callers can record a `MemberResolution`.
    fn lookup_class_method(
        &self,
        class_name: &crate::ty::QualifiedTypeName,
        method_name: &Name,
    ) -> Option<(
        Ty,
        baml_compiler2_hir::loc::ClassLoc<'db>,
        baml_compiler2_hir::loc::FunctionLoc<'db>,
    )> {
        let pkg_items_for_class = self.resolve_class_pkg_items(class_name.package())?;
        let def = pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())?;
        let Definition::Class(class_loc) = def else {
            return None;
        };
        let db = self.context.db();
        let file = class_loc.file(db);
        let ns_context = baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];

        for &method_id in &class_data.methods {
            let method_data = &item_tree[method_id];
            if method_data.name == *method_name {
                let mut all_generic_params = class_data.generic_params.clone();
                all_generic_params.extend(method_data.generic_params.iter().cloned());
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
                let mut diags = Vec::new();
                let class_ty = Ty::Class(class_name.clone(), TyAttr::default());
                let ty = Ty::Function {
                    params: sig
                        .params
                        .iter()
                        .map(|(n, te)| {
                            let param_ty = if n.as_str() == "self"
                                && matches!(te, baml_compiler2_ast::TypeExpr::Unknown { .. })
                            {
                                // self with no annotation → use the enclosing class type
                                class_ty.clone()
                            } else {
                                crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    te,
                                    pkg_items_for_class,
                                    &ns_context,
                                    &all_generic_params,
                                    &mut diags,
                                )
                            };
                            (Some(n.clone()), param_ty)
                        })
                        .collect(),
                    ret: Box::new(
                        sig.return_type
                            .as_ref()
                            .map(|te| {
                                crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    te,
                                    pkg_items_for_class,
                                    &ns_context,
                                    &all_generic_params,
                                    &mut diags,
                                )
                            })
                            .unwrap_or(Ty::Unknown {
                                attr: TyAttr::default(),
                            }),
                    ),
                    attr: TyAttr::default(),
                };
                // Note: diags from method signatures are reported at definition site.
                return Some((ty, class_loc, func_loc));
            }
        }
        None
    }

    /// Check if a `FieldAccess` base is a primitive type name used for static
    /// method access (e.g. `image.from_url(...)`, `pdf.from_base64(...)`).
    ///
    /// Returns `Some(method_ty)` if the base is a recognized primitive type name
    /// and the field is a valid static method on the corresponding builtin class.
    /// Returns `None` to fall through to normal `FieldAccess` resolution.
    /// Try to resolve a `FieldAccess` chain rooted at `Path(["baml"])` as a
    /// builtin package access: `baml.Array.length`, `baml.media.Image.from_url`.
    ///
    /// Walks the `FieldAccess` chain to extract the class path and member name,
    /// then delegates to `resolve_builtin_member`.
    /// Try to resolve a `FieldAccess` chain as a package access path.
    ///
    /// Walks the `FieldAccess` chain to find a root `Path([pkg_name])` where
    /// `pkg_name` is a known package. Then resolves intermediate segments
    /// through the package's namespace to find a type (class/enum), and
    /// finally uses `resolve_member` on that type for the final field.
    ///
    /// Handles patterns like:
    /// - `baml.Array.length` -- package=`baml`, path=`["Array"]`, member=`length`
    /// - `baml.media.Image.from_url` -- package=`baml`, path=`["media","Image"]`, member=`from_url`
    /// - `env.get` → package="env", path=[], member="get" (free function)
    fn try_package_access(
        &mut self,
        at: ExprId,
        base_id: ExprId,
        member: &Name,
        body: &ExprBody,
    ) -> Option<Ty> {
        // Walk the chain to collect segments before this member access.
        let mut segments: Vec<Name> = Vec::new();
        let mut current = base_id;
        loop {
            match &body.exprs[current] {
                Expr::Path(path_segments) if path_segments.len() == 1 => {
                    segments.push(path_segments[0].clone());
                    break;
                }
                Expr::FieldAccess { base, field } => {
                    segments.push(field.clone());
                    current = *base;
                }
                _ => return None,
            }
        }

        // Reverse so we have root-to-leaf order: ["baml", "Array"] or ["env"]
        segments.reverse();

        if segments.is_empty() {
            return None;
        }

        // Check if the root segment is a known package (not a local variable)
        let first = &segments[0];
        if self.locals.contains_key(first) {
            return None;
        }

        let db = self.context.db();
        let resolved_pkg_name = if first.as_str() == "root" {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, self.scope.file(db));
            pkg_info.package
        } else {
            first.clone()
        };

        // Try to resolve the package. If not found, check if the first segment
        // is a known namespace shorthand for the `"baml"` package (e.g. `env.get`
        // → `baml.env.get`, `sys.panic` → `baml.sys.panic`).
        let baml_ns_shorthands: &[&str] = &[
            "env", "sys", "http", "math", "fs", "net", "media", "llm", "errors", "unstable",
        ];
        let (pkg_items, item_path_owned): (
            &baml_compiler2_hir::package::PackageItems<'db>,
            Vec<Name>,
        ) = if let Some(items) = self.res_ctx.items_for_package(db, &resolved_pkg_name) {
            // Found the package directly.
            if items.namespaces.is_empty() {
                return None;
            }
            let ip = segments[1..].to_vec();
            (items, ip)
        } else if baml_ns_shorthands.contains(&first.as_str()) {
            // `env.X` → treat as `baml.env.X`: look up in the `"baml"` package
            // with the namespace prefix prepended to the item path.
            let baml_name = Name::new("baml");
            let baml_items = self.res_ctx.items_for_package(db, &baml_name)?;
            // Prepend the namespace segment (`first`) to the item path.
            let mut ip = vec![first.clone()];
            ip.extend_from_slice(&segments[1..]);
            (baml_items, ip)
        } else {
            return None;
        };

        let item_path: &[Name] = &item_path_owned;

        // Record types for intermediate expressions (so MIR doesn't panic on them).
        let mut cur = base_id;
        loop {
            match &body.exprs[cur] {
                Expr::Path(_) => {
                    self.record_expr_type(
                        cur,
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        },
                    );
                    break;
                }
                Expr::FieldAccess { base, .. } => {
                    self.record_expr_type(
                        cur,
                        Ty::Unknown {
                            attr: TyAttr::default(),
                        },
                    );
                    cur = *base;
                }
                _ => break,
            }
        }

        // When there is a non-empty item_path, try class/enum member resolution
        // first (e.g. `baml.Array.length` → Array class, then method "length").
        if !item_path.is_empty() {
            let item_name = item_path.last().expect("non-empty item_path");
            if let Some(def) = pkg_items.lookup_type(&item_path[..item_path.len() - 1], item_name) {
                match def {
                    Definition::Class(_class_loc) => {
                        if first.as_str() == "root"
                            || first.as_str() == self.package_id.name(db).as_str()
                        {
                            let class_qtn = crate::lower_type_expr::qualify_def(db, def, item_name);
                            let base_ty = Ty::Class(class_qtn, TyAttr::default());
                            return Some(self.resolve_member(&base_ty, member, at));
                        }
                        let class_path: Vec<&str> =
                            item_path.iter().map(smol_str::SmolStr::as_str).collect();
                        return self
                            .resolve_builtin_member(&class_path, &[], member, at)
                            .or_else(|| {
                                let class_qtn =
                                    crate::lower_type_expr::qualify_def(db, def, item_name);
                                let base_ty = Ty::Class(class_qtn, TyAttr::default());
                                Some(self.resolve_member(&base_ty, member, at))
                            });
                    }
                    Definition::Enum(_) => {
                        let enum_qtn = crate::lower_type_expr::qualify_def(db, def, item_name);
                        let base_ty = Ty::Enum(enum_qtn, TyAttr::default());
                        return Some(self.resolve_member(&base_ty, member, at));
                    }
                    _ => {}
                }
            }
        }

        // Fall back to the shared value/type resolution for free functions
        // in nested namespaces (e.g. `env.get`, `baml.llm.render_prompt`).
        let full_path: Vec<Name> = item_path
            .iter()
            .chain(std::iter::once(member))
            .cloned()
            .collect();
        self.resolve_package_item(pkg_items, &full_path, at)
    }

    fn try_primitive_static_access(
        &mut self,
        at: ExprId,
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

        self.resolve_builtin_member(class_path, &[], field, at)
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
    /// Wrapper around `resolve_builtin_method` that also stores a `MemberResolution`
    /// when the result is a method (not a field).
    fn resolve_builtin_member(
        &mut self,
        class_path: &[&str],
        type_args: &[Ty],
        member_name: &Name,
        at: ExprId,
    ) -> Option<Ty> {
        let result = self.resolve_builtin_method(class_path, type_args, member_name)?;
        match result {
            BuiltinResolution::Method {
                ty,
                class_loc,
                func_loc,
            } => {
                self.resolutions.insert(
                    at,
                    crate::inference::MemberResolution::Method {
                        class_loc,
                        func_loc,
                    },
                );
                Some(ty)
            }
            BuiltinResolution::Field(ty) => Some(ty),
        }
    }

    fn resolve_builtin_method(
        &self,
        class_path: &[&str],
        type_args: &[Ty],
        member_name: &Name,
    ) -> Option<BuiltinResolution<'db>> {
        let db = self.context.db();
        let baml_items = self
            .res_ctx
            .items_for_package(db, &baml_base::Name::new("baml"))?;

        // Look up the class by path (e.g. &["Array"] or &["media", "Image"]).
        let path: Vec<Name> = class_path.iter().map(baml_base::Name::new).collect();
        let item = path.last().expect("non-empty class_path");
        let def = baml_items.lookup_type(&path[..path.len() - 1], item)?;
        let baml_compiler2_hir::contributions::Definition::Class(class_loc) = def else {
            return None;
        };

        let file = class_loc.file(db);
        let stub_pkg = baml_compiler2_hir::file_package::file_package(db, file);
        let stub_ns: &[Name] = &stub_pkg.namespace_path;
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
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
                // Build the class type for self parameter resolution.
                // For generics, apply type_args (e.g. Array<int>).
                let builtin_class_ty = if type_args.is_empty() {
                    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                    Ty::Class(
                        crate::ty::QualifiedTypeName::new_with_generic_params(
                            pkg_info.package,
                            pkg_info.namespace_path,
                            class_data.name.clone(),
                            class_data.generic_params.clone(),
                        ),
                        TyAttr::default(),
                    )
                } else if type_args.len() == 1 {
                    // Single type arg: Array<T> → List(T), special-case common containers
                    match class_data.name.as_str() {
                        "Array" => Ty::List(Box::new(type_args[0].clone()), TyAttr::default()),
                        _ => {
                            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                            Ty::Class(
                                crate::ty::QualifiedTypeName::new(
                                    pkg_info.package,
                                    pkg_info.namespace_path,
                                    class_data.name.clone(),
                                ),
                                TyAttr::default(),
                            )
                        }
                    }
                } else if type_args.len() == 2 {
                    match class_data.name.as_str() {
                        "Map" => Ty::Map(
                            Box::new(type_args[0].clone()),
                            Box::new(type_args[1].clone()),
                            TyAttr::default(),
                        ),
                        _ => {
                            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                            Ty::Class(
                                crate::ty::QualifiedTypeName::new(
                                    pkg_info.package,
                                    pkg_info.namespace_path,
                                    class_data.name.clone(),
                                ),
                                TyAttr::default(),
                            )
                        }
                    }
                } else {
                    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
                    Ty::Class(
                        crate::ty::QualifiedTypeName::new(
                            pkg_info.package,
                            pkg_info.namespace_path,
                            class_data.name.clone(),
                        ),
                        TyAttr::default(),
                    )
                };

                let params: Vec<(Option<Name>, Ty)> = sig
                    .params
                    .iter()
                    .map(|(n, te)| {
                        let ty = if n.as_str() == "self"
                            && matches!(te, baml_compiler2_ast::TypeExpr::Unknown { .. })
                        {
                            builtin_class_ty.clone()
                        } else {
                            crate::generics::lower_type_expr_with_generics(
                                db,
                                te,
                                self.package_items,
                                stub_ns,
                                &bindings,
                                &mut diags,
                            )
                        };
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
                            stub_ns,
                            &bindings,
                            &mut diags,
                        )
                    })
                    .unwrap_or(Ty::Void {
                        attr: TyAttr::default(),
                    });
                // Discard diags — they will be reported at the definition site
                // (the builtin .baml stub). We don't want to spam user code
                // with unresolved-type errors from builtin signatures.
                drop(diags);
                return Some(BuiltinResolution::Method {
                    ty: Ty::Function {
                        params,
                        ret: Box::new(ret),
                        attr: TyAttr::default(),
                    },
                    class_loc,
                    func_loc,
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
                            stub_ns,
                            &bindings,
                            &mut diags,
                        )
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    });
                drop(diags);
                return Some(BuiltinResolution::Field(field_ty));
            }
        }

        None
    }

    /// Look up enum variants from the package items (via item tree).
    ///
    /// Uses the enum's qualified package to find it in the correct package,
    /// not just the current file's package.
    fn lookup_enum_variants(&self, enum_name: &crate::ty::QualifiedTypeName) -> Vec<Name> {
        let db = self.context.db();

        // Resolve the package that owns the enum via res_ctx.
        let items = if *enum_name.package() == self.package_id.name(db) {
            self.package_items
        } else {
            match self.res_ctx.items_for_package(db, enum_name.package()) {
                Some(items) => items,
                None => return Vec::new(),
            }
        };

        if let Some(Definition::Enum(enum_loc)) =
            items.lookup_type(enum_name.namespace(), enum_name.name())
        {
            let file = enum_loc.file(db);
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            let enum_data = &item_tree[enum_loc.id(db)];
            return enum_data.variants.iter().map(|v| v.name.clone()).collect();
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
                let (elem_ty, is_evolving, container_attr) = match &local_ty {
                    Ty::EvolvingList(elem, attr) => (elem, true, attr.clone()),
                    Ty::List(elem, attr) => (elem, false, attr.clone()),
                    _ => return None,
                };

                let arg_ty = self.infer_expr(args[0], body);
                let widened_arg = arg_ty.widen_fresh();

                if matches!(**elem_ty, Ty::Never { .. }) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingList(Box::new(widened_arg), container_attr)
                    } else {
                        Ty::List(Box::new(widened_arg), container_attr)
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

                // Record a MemberResolution so MIR emits a proper method call
                // instead of a dynamic map lookup.
                let effective_elem = match &local_ty {
                    Ty::EvolvingList(e, _) | Ty::List(e, _) => e.as_ref().clone(),
                    _ => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                };
                self.resolve_builtin_member(&["Array"], &[effective_elem], &method_name, callee_id);

                self.record_expr_type(base_id, local_ty);
                self.record_expr_type(
                    callee_id,
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                );
                let result = Ty::Primitive(PrimitiveType::Null, TyAttr::default());
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

        let Some(local_name) = self.expr_local_name(base_id, body) else {
            return false;
        };
        let local_ty = match self.locals.get(&local_name) {
            Some(ty) => ty.clone(),
            None => return false,
        };

        match &local_ty {
            Ty::List(elem_ty, container_attr) | Ty::EvolvingList(elem_ty, container_attr) => {
                let is_evolving = matches!(local_ty, Ty::EvolvingList(_, _));
                let container_attr = container_attr.clone();
                let index_ty = self.infer_expr(index_id, body);
                let val_ty = self.infer_expr(value_id, body);
                let widened_val = val_ty.clone().widen_fresh();

                if matches!(**elem_ty, Ty::Never { .. }) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingList(Box::new(widened_val.clone()), container_attr)
                    } else {
                        Ty::List(Box::new(widened_val.clone()), container_attr)
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
            Ty::Map(key_ty, val_ty, container_attr)
            | Ty::EvolvingMap(key_ty, val_ty, container_attr) => {
                let is_evolving = matches!(local_ty, Ty::EvolvingMap(_, _, _));
                let container_attr = container_attr.clone();
                let index_ty = self.infer_expr(index_id, body);
                let value_ty = self.infer_expr(value_id, body);
                let widened_key = index_ty.clone().widen_fresh();
                let widened_val = value_ty.clone().widen_fresh();

                if matches!(**key_ty, Ty::Never { .. }) && matches!(**val_ty, Ty::Never { .. }) {
                    let new_ty = if is_evolving {
                        Ty::EvolvingMap(
                            Box::new(widened_key),
                            Box::new(widened_val.clone()),
                            container_attr,
                        )
                    } else {
                        Ty::Map(
                            Box::new(widened_key),
                            Box::new(widened_val.clone()),
                            container_attr,
                        )
                    };
                    self.locals.insert(local_name, new_ty);
                } else {
                    if !self.is_subtype(&widened_key, key_ty) {
                        self.context.report(
                            TirTypeError::TypeMismatch {
                                expected: *key_ty.clone(),
                                got: widened_key,
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

    fn join_types(a: &Ty, b: &Ty) -> Ty {
        if matches!(a, Ty::Never { .. }) {
            return b.clone();
        }
        if matches!(b, Ty::Never { .. }) {
            return a.clone();
        }
        if matches!(a, Ty::Void { .. }) || matches!(b, Ty::Void { .. }) {
            // TODO(TyAttr): Control-flow merge where either branch is Void — neither
            // branch's attr is obviously "the right one" to keep. May need a merge
            // operation on TyAttr, or default may be correct for synthesized types.
            return Ty::Void {
                attr: TyAttr::default(),
            };
        }
        if a == b {
            return a.clone();
        }
        // Same literal value, different freshness → normalize to Regular
        if let (Ty::Literal(lit_a, _, _), Ty::Literal(lit_b, _, _)) = (a, b) {
            if lit_a == lit_b {
                // TODO(TyAttr): Same literal from two branches with different freshness.
                // Could pick either side's attr or merge them; unclear which is correct.
                return Ty::Literal(
                    lit_a.clone(),
                    crate::ty::Freshness::Regular,
                    TyAttr::default(),
                );
            }
        }
        // Build a flat union, deduplicating members
        let mut members = Vec::new();
        let mut push = |ty: &Ty| {
            if let Ty::Union(inner, _) = ty {
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
            // TODO(TyAttr): Novel union synthesized from two different branch types.
            // No single original attr to preserve. Same question as union_ty in generics.rs.
            Ty::Union(members, TyAttr::default())
        }
    }

    fn join_all(types: &[Ty]) -> Ty {
        if types.is_empty() {
            return Ty::Never {
                attr: TyAttr::default(),
            };
        }
        types
            .iter()
            .skip(1)
            .fold(types[0].clone(), |acc, t| Self::join_types(&acc, t))
    }

    /// Subtype check — delegates to the normalizer which resolves type aliases
    /// and performs equirecursive structural subtyping.
    fn is_subtype(&self, sub: &Ty, sup: &Ty) -> bool {
        crate::normalize::is_subtype_of(sub, sup, &self.aliases)
    }

    fn infer_binary_op(
        &mut self,
        op: baml_compiler2_ast::BinaryOp,
        lhs: &Ty,
        rhs: &Ty,
        at: ExprId,
    ) -> Ty {
        use baml_compiler2_ast::BinaryOp;
        // Try constant folding on two literals first.
        if let Some(folded) = Self::try_fold_binary(op, lhs, rhs) {
            return folded;
        }
        match op {
            // Comparison / equality → bool
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Instanceof => Ty::Primitive(PrimitiveType::Bool, TyAttr::default()),

            // Logical → bool
            BinaryOp::And | BinaryOp::Or => Ty::Primitive(PrimitiveType::Bool, TyAttr::default()),

            // Arithmetic: result type depends on operands
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let result = Self::infer_arithmetic(op, lhs, rhs);
                if matches!(result, Ty::Unknown { .. })
                    && !matches!(lhs, Ty::Unknown { .. } | Ty::Error { .. })
                    && !matches!(rhs, Ty::Unknown { .. } | Ty::Error { .. })
                {
                    self.context.report_simple(
                        TirTypeError::InvalidBinaryOp {
                            op,
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
            | BinaryOp::Shr => Ty::Primitive(PrimitiveType::Int, TyAttr::default()),
        }
    }

    /// Determine the result type of an arithmetic operation (non-literal fallback).
    ///
    /// String concatenation is only valid for `Add`; other arithmetic ops on
    /// strings are invalid and return `Unknown` (triggering an error upstream).
    fn infer_arithmetic(op: baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Ty {
        fn promote(a: PrimitiveType, b: &PrimitiveType) -> Option<PrimitiveType> {
            if a == *b {
                return Some(a);
            }
            match (&a, &b) {
                (PrimitiveType::Int, PrimitiveType::Float)
                | (PrimitiveType::Float, PrimitiveType::Int) => Some(PrimitiveType::Float),
                _ => None,
            }
        }

        fn base_ty(ty: &Ty) -> Option<PrimitiveType> {
            match ty {
                Ty::Primitive(p, _) => Some(p.clone()),
                Ty::Literal(lit, _, _) => Some(PrimitiveType::from_literal(lit)),
                Ty::Union(members, _) => {
                    let mut result: Option<PrimitiveType> = None;
                    for m in members {
                        let p = base_ty(m)?;
                        result = Some(match result {
                            None => p,
                            Some(existing) => promote(existing, &p)?,
                        });
                    }
                    result
                }
                _ => None,
            }
        }

        match (base_ty(lhs), base_ty(rhs)) {
            (Some(PrimitiveType::Float), _) | (_, Some(PrimitiveType::Float)) => {
                Ty::Primitive(PrimitiveType::Float, TyAttr::default())
            }
            (Some(PrimitiveType::Int), Some(PrimitiveType::Int)) => {
                Ty::Primitive(PrimitiveType::Int, TyAttr::default())
            }
            (Some(PrimitiveType::String), _) | (_, Some(PrimitiveType::String)) => {
                // String concatenation only for Add
                if matches!(op, baml_compiler2_ast::BinaryOp::Add) {
                    Ty::Primitive(PrimitiveType::String, TyAttr::default())
                } else {
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            }
            _ => Ty::Unknown {
                attr: TyAttr::default(),
            },
        }
    }

    fn infer_unary_op(&mut self, op: baml_compiler2_ast::UnaryOp, operand: &Ty, at: ExprId) -> Ty {
        // Try constant folding on a literal first.
        if let Some(folded) = Self::try_fold_unary(op, operand) {
            return folded;
        }
        let operand_attr = operand.attr().clone();
        match op {
            baml_compiler2_ast::UnaryOp::Not => Ty::Primitive(PrimitiveType::Bool, operand_attr),
            baml_compiler2_ast::UnaryOp::Neg => match operand {
                Ty::Primitive(PrimitiveType::Int, attr) => {
                    Ty::Primitive(PrimitiveType::Int, attr.clone())
                }
                Ty::Primitive(PrimitiveType::Float, attr) => {
                    Ty::Primitive(PrimitiveType::Float, attr.clone())
                }
                Ty::Unknown { attr } | Ty::Error { attr } => Ty::Unknown { attr: attr.clone() },
                _ => {
                    self.context.report_simple(
                        TirTypeError::InvalidUnaryOp {
                            op,
                            operand: operand.clone(),
                        },
                        at,
                    );
                    Ty::Unknown { attr: operand_attr }
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
    fn try_fold_unary(op: baml_compiler2_ast::UnaryOp, operand: &Ty) -> Option<Ty> {
        use crate::ty::LiteralValue;
        let (lit, f) = match operand {
            Ty::Literal(lit, f, _) => (lit, *f),
            _ => return None,
        };
        match op {
            baml_compiler2_ast::UnaryOp::Neg => match lit {
                LiteralValue::Int(n) => Some(Ty::Literal(
                    LiteralValue::Int(n.checked_neg()?),
                    f,
                    TyAttr::default(),
                )),
                LiteralValue::Float(s) => {
                    let v: f64 = s.parse().ok()?;
                    Some(Ty::Literal(
                        LiteralValue::Float(format_float(-v)?),
                        f,
                        TyAttr::default(),
                    ))
                }
                _ => None,
            },
            baml_compiler2_ast::UnaryOp::Not => match lit {
                LiteralValue::Bool(b) => {
                    Some(Ty::Literal(LiteralValue::Bool(!b), f, TyAttr::default()))
                }
                _ => None,
            },
        }
    }

    /// Try to fold a binary operation on two literals into a new literal.
    fn try_fold_binary(op: baml_compiler2_ast::BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<Ty> {
        use baml_compiler2_ast::BinaryOp;

        use crate::ty::LiteralValue;

        let (lhs_lit, lhs_f) = match lhs {
            Ty::Literal(lit, f, _) => (lit, *f),
            _ => return None,
        };
        let (rhs_lit, rhs_f) = match rhs {
            Ty::Literal(lit, f, _) => (lit, *f),
            _ => return None,
        };
        let f = Self::merge_freshness(lhs_f, rhs_f);

        // Int × Int
        if let (LiteralValue::Int(a), LiteralValue::Int(b)) = (lhs_lit, rhs_lit) {
            let (a, b) = (*a, *b);
            return match op {
                BinaryOp::Add => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_add(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Sub => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_sub(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mul => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_mul(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Div => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_div(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mod => Some(Ty::Literal(
                    LiteralValue::Int(a.checked_rem(b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::BitAnd => {
                    Some(Ty::Literal(LiteralValue::Int(a & b), f, TyAttr::default()))
                }
                BinaryOp::BitOr => {
                    Some(Ty::Literal(LiteralValue::Int(a | b), f, TyAttr::default()))
                }
                BinaryOp::BitXor => {
                    Some(Ty::Literal(LiteralValue::Int(a ^ b), f, TyAttr::default()))
                }
                BinaryOp::Shl => {
                    let shift = u32::try_from(b).ok()?;
                    Some(Ty::Literal(
                        LiteralValue::Int(a.checked_shl(shift)?),
                        f,
                        TyAttr::default(),
                    ))
                }
                BinaryOp::Shr => {
                    let shift = u32::try_from(b).ok()?;
                    Some(Ty::Literal(
                        LiteralValue::Int(a.checked_shr(shift)?),
                        f,
                        TyAttr::default(),
                    ))
                }
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f, TyAttr::default())),
                BinaryOp::Le => Some(Ty::Literal(
                    LiteralValue::Bool(a <= b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f, TyAttr::default())),
                BinaryOp::Ge => Some(Ty::Literal(
                    LiteralValue::Bool(a >= b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        // Float × Float
        if let (LiteralValue::Float(a_s), LiteralValue::Float(b_s)) = (lhs_lit, rhs_lit) {
            let a: f64 = a_s.parse().ok()?;
            let b: f64 = b_s.parse().ok()?;
            return match op {
                BinaryOp::Add => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a + b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Sub => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a - b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mul => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a * b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Div if b != 0.0 => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a / b)?),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Mod if b != 0.0 => Some(Ty::Literal(
                    LiteralValue::Float(format_float(a % b)?),
                    f,
                    TyAttr::default(),
                )),
                #[allow(clippy::float_cmp)] // Intentional: folding literal float equality
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                #[allow(clippy::float_cmp)] // Intentional: folding literal float inequality
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f, TyAttr::default())),
                BinaryOp::Le => Some(Ty::Literal(
                    LiteralValue::Bool(a <= b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f, TyAttr::default())),
                BinaryOp::Ge => Some(Ty::Literal(
                    LiteralValue::Bool(a >= b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        // Bool × Bool
        if let (LiteralValue::Bool(a), LiteralValue::Bool(b)) = (lhs_lit, rhs_lit) {
            let (a, b) = (*a, *b);
            return match op {
                BinaryOp::And => Some(Ty::Literal(
                    LiteralValue::Bool(a && b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Or => Some(Ty::Literal(
                    LiteralValue::Bool(a || b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        // String × String
        if let (LiteralValue::String(a), LiteralValue::String(b)) = (lhs_lit, rhs_lit) {
            return match op {
                BinaryOp::Add => Some(Ty::Literal(
                    LiteralValue::String(format!("{a}{b}")),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Eq => Some(Ty::Literal(
                    LiteralValue::Bool(a == b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Ne => Some(Ty::Literal(
                    LiteralValue::Bool(a != b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Lt => Some(Ty::Literal(LiteralValue::Bool(a < b), f, TyAttr::default())),
                BinaryOp::Le => Some(Ty::Literal(
                    LiteralValue::Bool(a <= b),
                    f,
                    TyAttr::default(),
                )),
                BinaryOp::Gt => Some(Ty::Literal(LiteralValue::Bool(a > b), f, TyAttr::default())),
                BinaryOp::Ge => Some(Ty::Literal(
                    LiteralValue::Bool(a >= b),
                    f,
                    TyAttr::default(),
                )),
                _ => None,
            };
        }

        None
    }

    fn assign_op_to_binary_op(op: baml_compiler2_ast::AssignOp) -> baml_compiler2_ast::BinaryOp {
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

    /// Infer/check a lambda body using a save/restore approach.
    ///
    /// Saves the current locals, `declared_types`, `declared_return_ty`, and
    /// `generic_params`, seeds lambda params on top (captures work naturally
    /// because parent locals remain visible), then infers or checks the lambda
    /// body root expression.
    ///
    /// NOTE: The lambda body has its own `ExprBody` arena with `ExprId`s starting
    /// at 0, which may collide with the parent body's `ExprId`s in `self.expressions`.
    /// This is acceptable for the first iteration — LSP/MIR do not yet consume
    /// the per-scope `ScopeInference` for lambda scopes.
    ///
    /// Returns the inferred return type.
    fn infer_lambda_body(
        &mut self,
        func_def: &baml_compiler2_ast::FunctionDef,
        param_tys: &[(Option<baml_base::Name>, Ty)],
        expected_ret: Option<&Ty>,
        _lambda_expr_id: ExprId,
    ) -> Ty {
        use baml_compiler2_ast::FunctionBodyDef;

        // Get the lambda's ExprBody
        let Some(FunctionBodyDef::Expr(lambda_body, _source_map)) = &func_def.body else {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        };

        let Some(root_expr) = lambda_body.root_expr else {
            return Ty::Void {
                attr: TyAttr::default(),
            };
        };

        // Save current state
        let saved_locals = self.locals.clone();
        let saved_declared = self.declared_types.clone();
        let saved_return_ty = self.declared_return_ty.clone();
        let saved_generic_params = self.generic_params.clone();

        // Extend generic params with the lambda's own generic params
        let mut new_generic_params = self.generic_params.clone();
        new_generic_params.extend(func_def.generic_params.iter().cloned());
        self.generic_params = new_generic_params;

        // Seed lambda params (captures remain accessible via parent locals)
        for (name_opt, ty) in param_tys {
            if let Some(name) = name_opt {
                self.add_local(name.clone(), ty.clone());
            }
        }

        // Set return type context for return statement checking inside lambda
        if let Some(ret) = expected_ret {
            self.declared_return_ty = Some(ret.clone());
        } else {
            self.declared_return_ty = None;
        }

        // Infer or check the lambda body
        let ret_ty = if let Some(expected) = expected_ret {
            if matches!(expected, Ty::Unknown { .. } | Ty::TypeVar(_, _)) {
                // Expected return is unknown or a type var — just infer
                self.infer_expr(root_expr, lambda_body)
            } else {
                self.check_expr(root_expr, lambda_body, expected)
            }
        } else {
            self.infer_expr(root_expr, lambda_body)
        };

        // Restore parent state
        self.locals = saved_locals;
        self.declared_types = saved_declared;
        self.declared_return_ty = saved_return_ty;
        self.generic_params = saved_generic_params;

        ret_ty
    }
}

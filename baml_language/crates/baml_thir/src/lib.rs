//! Typed High-level Intermediate Representation.
//!
//! Provides type checking and inference for BAML.
//!
//! # Architecture
//!
//! The THIR layer performs bidirectional type checking:
//! - **Inference (synthesize)**: Compute the type of an expression from its structure
//! - **Checking**: Verify an expression has an expected type
//!
//! This follows patterns from rust-analyzer and ruff for incremental type checking.

use std::collections::HashMap;

use baml_base::{Name, Span};
use baml_diagnostics::compiler_error::TypeError;
use baml_hir::{ExprBody, ExprId, FunctionBody, FunctionSignature, Pattern, StmtId};

mod lower;
pub mod pretty;
mod types;

pub use lower::lower_type_ref;
pub use pretty::{expr_to_string, render_body_tree, render_function_tree};
pub use types::*;

//
// ──────────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for THIR queries.
///
/// This trait extends `baml_hir::Db` and provides access to all THIR-related
/// Salsa queries, including type inference and the initial typing context.
#[salsa::db]
pub trait Db: baml_hir::Db {}

// ============================================================================
// Type Inference Results
// ============================================================================

/// Result of type inference for a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceResult<'db> {
    /// Inferred return type of the function.
    pub return_type: Ty<'db>,
    /// Types of parameters.
    pub param_types: HashMap<Name, Ty<'db>>,
    /// Types inferred for each expression.
    pub expr_types: HashMap<ExprId, Ty<'db>>,
    /// Type checking errors.
    pub errors: Vec<TypeError<Ty<'db>>>,
}

// ============================================================================
// Type Context
// ============================================================================

/// Context for type inference, tracking scopes and accumulated results.
pub struct TypeContext<'db> {
    db: &'db dyn Db,
    /// Stack of variable scopes (innermost last).
    scopes: Vec<HashMap<Name, Ty<'db>>>,
    /// Inferred types for expressions.
    expr_types: HashMap<ExprId, Ty<'db>>,
    /// Accumulated type errors.
    errors: Vec<TypeError<Ty<'db>>>,
}

impl<'db> TypeContext<'db> {
    /// Create a new type context with an initial scope of global bindings.
    ///
    /// The initial scope typically contains top-level function types, allowing
    /// function calls to be properly typed. Pass an empty `HashMap` for no globals.
    pub fn new(db: &'db dyn Db, globals: HashMap<Name, Ty<'db>>) -> Self {
        TypeContext {
            db,
            scopes: vec![globals],
            expr_types: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Push a new scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current scope.
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a variable in the current scope.
    pub fn define(&mut self, name: Name, ty: Ty<'db>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Look up a variable in the scope chain.
    pub fn lookup(&self, name: &Name) -> Option<&Ty<'db>> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Record the type of an expression.
    pub fn set_expr_type(&mut self, expr: ExprId, ty: Ty<'db>) {
        self.expr_types.insert(expr, ty);
    }

    /// Get the type of an expression.
    #[allow(dead_code)]
    pub fn get_expr_type(&self, expr: ExprId) -> Option<&Ty<'db>> {
        self.expr_types.get(&expr)
    }

    /// Add a type error.
    pub fn push_error(&mut self, error: TypeError<Ty<'db>>) {
        self.errors.push(error);
    }

    /// Get the database reference.
    pub fn db(&self) -> &'db dyn Db {
        self.db
    }
}

// ============================================================================
// Type Inference
// ============================================================================

/// Infer types for a function body.
///
/// This is the main entry point for type inference. It takes a pre-lowered
/// function body and infers types for all expressions.
///
/// The `globals` parameter provides types for top-level functions, allowing
/// function calls to be properly typed. Pass `None` if no global context is needed.
///
/// Note: In a full implementation, this would be a Salsa tracked function.
/// For now, it's a regular function that takes the body directly.
pub fn infer_function_body<'db>(
    db: &'db dyn Db,
    body: &FunctionBody,
    param_types: HashMap<Name, Ty<'db>>,
    expected_return: &Ty<'db>,
    globals: Option<HashMap<Name, Ty<'db>>>,
) -> InferenceResult<'db> {
    let mut ctx = TypeContext::new(db, globals.unwrap_or_default());

    // Add parameters to the current scope (on top of globals)
    for (name, ty) in &param_types {
        ctx.define(name.clone(), ty.clone());
    }

    // Type check the body
    let return_type = match body {
        FunctionBody::Expr(expr_body) => {
            if let Some(root_expr) = expr_body.root_expr {
                infer_expr(&mut ctx, root_expr, expr_body)
            } else {
                Ty::Void
            }
        }
        FunctionBody::Llm(_) => {
            // LLM functions return their declared return type
            expected_return.clone()
        }
        FunctionBody::Missing => Ty::Unknown,
    };

    // Check return type matches (if we have span info, we'd report errors here)
    if !return_type.is_subtype_of(expected_return)
        && !return_type.is_unknown()
        && !expected_return.is_unknown()
    {
        // We'd need the span of the function body for this error
        // For now, we skip this check
    }

    InferenceResult {
        return_type,
        param_types,
        expr_types: ctx.expr_types,
        errors: ctx.errors,
    }
}

/// Infer types for a function given its signature and body.
///
/// This is the entry point for type inference from the test suite.
/// It takes pre-fetched signature and body data, allowing the caller (`baml_db`)
/// to handle the Salsa queries for fetching this data.
///
/// The `globals` parameter provides types for top-level functions, allowing
/// function calls to be properly typed. Pass `None` if no global context is needed.
pub fn infer_function<'db>(
    db: &'db dyn Db,
    signature: &FunctionSignature,
    body: &FunctionBody,
    globals: Option<HashMap<Name, Ty<'db>>>,
) -> InferenceResult<'db> {
    // Convert parameter TypeRefs to Tys
    let param_types: HashMap<Name, Ty<'db>> = signature
        .params
        .iter()
        .map(|param| {
            let ty = lower_type_ref(db, &param.type_ref);
            (param.name.clone(), ty)
        })
        .collect();

    // Convert return type
    let expected_return = lower_type_ref(db, &signature.return_type);

    // Delegate to the body inference function
    infer_function_body(db, body, param_types, &expected_return, globals)
}

/// Infer the type of an expression (synthesize mode).
fn infer_expr<'db>(ctx: &mut TypeContext<'db>, expr_id: ExprId, body: &ExprBody) -> Ty<'db> {
    use baml_hir::Expr;

    let expr = &body.exprs[expr_id];

    // Create a placeholder span for errors (ideally we'd track spans in ExprBody)
    let span = Span::new(
        baml_base::FileId::new(0),
        text_size::TextRange::empty(0.into()),
    );

    let ty = match expr {
        Expr::Literal(lit) => infer_literal(lit),

        Expr::Path(name) => {
            if let Some(ty) = ctx.lookup(name) {
                ty.clone()
            } else {
                ctx.push_error(TypeError::UnknownVariable {
                    name: name.to_string(),
                    span,
                });
                Ty::Unknown
            }
        }

        Expr::Binary { lhs, op, rhs } => {
            let lhs_ty = infer_expr(ctx, *lhs, body);
            let rhs_ty = infer_expr(ctx, *rhs, body);
            infer_binary_op(ctx, *op, &lhs_ty, &rhs_ty, span)
        }

        Expr::Unary { op, expr: inner } => {
            let inner_ty = infer_expr(ctx, *inner, body);
            infer_unary_op(ctx, *op, &inner_ty, span)
        }

        Expr::Call { callee, args } => {
            let callee_ty = infer_expr(ctx, *callee, body);

            // Infer argument types
            let arg_types: Vec<Ty<'db>> =
                args.iter().map(|arg| infer_expr(ctx, *arg, body)).collect();

            // If the callee is a function type, check arguments and return the return type
            match &callee_ty {
                Ty::Function { params, ret } => {
                    // Check argument count
                    if arg_types.len() != params.len() {
                        ctx.push_error(TypeError::ArgumentCountMismatch {
                            expected: params.len(),
                            found: arg_types.len(),
                            span,
                        });
                    }

                    // Check argument types
                    for (arg_ty, param_ty) in arg_types.iter().zip(params.iter()) {
                        if !arg_ty.is_subtype_of(param_ty) {
                            ctx.push_error(TypeError::TypeMismatch {
                                expected: param_ty.clone(),
                                found: arg_ty.clone(),
                                span, // Ideally we'd have the span of each arg
                            });
                        }
                    }

                    // Return the function's return type
                    (**ret).clone()
                }
                Ty::Unknown => Ty::Unknown,
                _ => {
                    ctx.push_error(TypeError::NotCallable {
                        ty: callee_ty,
                        span,
                    });
                    Ty::Unknown
                }
            }
        }

        Expr::FieldAccess { base, field } => {
            let base_ty = infer_expr(ctx, *base, body);
            infer_field_access(ctx, &base_ty, field, span)
        }

        Expr::Index { base, index } => {
            let base_ty = infer_expr(ctx, *base, body);
            let index_ty = infer_expr(ctx, *index, body);
            infer_index_access(ctx, &base_ty, &index_ty, span)
        }

        Expr::Array { elements } => {
            if elements.is_empty() {
                Ty::List(Box::new(Ty::Unknown))
            } else {
                // Infer element type from first element
                let elem_ty = infer_expr(ctx, elements[0], body);
                // Check all elements have compatible types
                for &elem in &elements[1..] {
                    let other_ty = infer_expr(ctx, elem, body);
                    if !other_ty.is_subtype_of(&elem_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: elem_ty.clone(),
                            found: other_ty,
                            span,
                        });
                    }
                }
                Ty::List(Box::new(elem_ty))
            }
        }

        Expr::Object {
            type_name: _,
            fields,
        } => {
            // Infer field types
            for (_, value_expr) in fields {
                infer_expr(ctx, *value_expr, body);
            }
            // For now, return Unknown since we don't have class resolution
            Ty::Unknown
        }

        Expr::Block { stmts, tail_expr } => {
            ctx.push_scope();

            // Type check statements
            for &stmt_id in stmts {
                check_stmt(ctx, stmt_id, body);
            }

            // Type of block is type of tail expression
            let result = if let Some(tail) = tail_expr {
                infer_expr(ctx, *tail, body)
            } else {
                Ty::Void
            };

            ctx.pop_scope();
            result
        }

        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            // Condition must be bool
            let cond_ty = infer_expr(ctx, *condition, body);
            if !cond_ty.is_subtype_of(&Ty::Bool) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Bool,
                    found: cond_ty,
                    span,
                });
            }

            // Infer branch types
            let then_ty = infer_expr(ctx, *then_branch, body);
            let else_ty = if let Some(else_expr) = else_branch {
                infer_expr(ctx, *else_expr, body)
            } else {
                Ty::Void
            };

            // Result is union of branches (simplified)
            if then_ty == else_ty {
                then_ty
            } else if else_branch.is_none() {
                // if without else returns optional
                Ty::Union(vec![then_ty, Ty::Null])
            } else {
                Ty::Union(vec![then_ty, else_ty])
            }
        }

        Expr::Missing => Ty::Unknown,
    };

    ctx.set_expr_type(expr_id, ty.clone());
    ty
}

/// Infer the type of a literal.
fn infer_literal(lit: &baml_hir::Literal) -> Ty<'static> {
    match lit {
        baml_hir::Literal::Int(_) => Ty::Int,
        baml_hir::Literal::Float(_) => Ty::Float,
        baml_hir::Literal::String(_) => Ty::String,
        baml_hir::Literal::Bool(_) => Ty::Bool,
        baml_hir::Literal::Null => Ty::Null,
    }
}

/// Infer the result type of a binary operation.
fn infer_binary_op<'db>(
    ctx: &mut TypeContext<'db>,
    op: baml_hir::BinaryOp,
    lhs: &Ty<'db>,
    rhs: &Ty<'db>,
    span: Span,
) -> Ty<'db> {
    use baml_hir::BinaryOp::{
        Add, And, BitAnd, BitOr, BitXor, Div, Eq, Ge, Gt, Le, Lt, Mod, Mul, Ne, Or, Shl, Shr, Sub,
    };

    match op {
        // Arithmetic operations
        Add | Sub | Mul | Div | Mod => match (lhs, rhs) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) => Ty::Float,
            (Ty::Int, Ty::Float) => Ty::Float,
            (Ty::Float, Ty::Int) => Ty::Float,
            _ => {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        },

        // Comparison operations
        Eq | Ne => Ty::Bool,

        Lt | Le | Gt | Ge => {
            if (lhs.is_subtype_of(&Ty::Int) || lhs.is_subtype_of(&Ty::Float))
                && (rhs.is_subtype_of(&Ty::Int) || rhs.is_subtype_of(&Ty::Float))
            {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }

        // Logical operations
        And | Or => {
            if lhs.is_subtype_of(&Ty::Bool) && rhs.is_subtype_of(&Ty::Bool) {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }

        // Bitwise operations
        BitAnd | BitOr | BitXor | Shl | Shr => {
            if lhs.is_subtype_of(&Ty::Int) && rhs.is_subtype_of(&Ty::Int) {
                Ty::Int
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }
    }
}

/// Infer the result type of a unary operation.
fn infer_unary_op<'db>(
    ctx: &mut TypeContext<'db>,
    op: baml_hir::UnaryOp,
    operand: &Ty<'db>,
    span: Span,
) -> Ty<'db> {
    use baml_hir::UnaryOp::{Neg, Not};

    match op {
        Not => {
            if operand.is_subtype_of(&Ty::Bool) {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidUnaryOp {
                    op: "!".to_string(),
                    operand: operand.clone(),
                    span,
                });
                Ty::Error
            }
        }
        Neg => {
            if operand.is_subtype_of(&Ty::Int) {
                Ty::Int
            } else if operand.is_subtype_of(&Ty::Float) {
                Ty::Float
            } else {
                ctx.push_error(TypeError::InvalidUnaryOp {
                    op: "-".to_string(),
                    operand: operand.clone(),
                    span,
                });
                Ty::Error
            }
        }
    }
}

/// Infer the type of a field access.
fn infer_field_access<'db>(
    ctx: &mut TypeContext<'db>,
    base: &Ty<'db>,
    field: &Name,
    span: Span,
) -> Ty<'db> {
    match base {
        Ty::Class(_class_id) => {
            // TODO: Look up field in class using ItemTree
            // For now, return Unknown
            Ty::Unknown
        }
        Ty::Unknown => Ty::Unknown,
        _ => {
            ctx.push_error(TypeError::NoSuchField {
                ty: base.clone(),
                field: field.to_string(),
                span,
            });
            Ty::Unknown
        }
    }
}

/// Infer the type of an index access.
fn infer_index_access<'db>(
    ctx: &mut TypeContext<'db>,
    base: &Ty<'db>,
    index: &Ty<'db>,
    span: Span,
) -> Ty<'db> {
    match base {
        Ty::List(elem) => {
            // Index must be int
            if !index.is_subtype_of(&Ty::Int) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int,
                    found: index.clone(),
                    span,
                });
            }
            (**elem).clone()
        }
        Ty::Map { key, value } => {
            // Index must match key type
            if !index.is_subtype_of(key) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: (**key).clone(),
                    found: index.clone(),
                    span,
                });
            }
            (**value).clone()
        }
        Ty::String => {
            // String indexing returns a character (string of length 1)
            if !index.is_subtype_of(&Ty::Int) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int,
                    found: index.clone(),
                    span,
                });
            }
            Ty::String
        }
        Ty::Unknown => Ty::Unknown,
        _ => {
            ctx.push_error(TypeError::NotIndexable {
                ty: base.clone(),
                span,
            });
            Ty::Unknown
        }
    }
}

/// Type check a statement.
fn check_stmt(ctx: &mut TypeContext<'_>, stmt_id: StmtId, body: &ExprBody) {
    use baml_hir::Stmt;

    let stmt = &body.stmts[stmt_id];

    match stmt {
        Stmt::Let {
            pattern,
            type_annotation,
            initializer,
        } => {
            let ty = if let Some(init) = initializer {
                let init_ty = infer_expr(ctx, *init, body);

                // If there's a type annotation, check it matches
                if let Some(annot) = type_annotation {
                    let annot_ty = lower_type_ref(ctx.db(), annot);
                    if !init_ty.is_subtype_of(&annot_ty) {
                        let span = Span::new(
                            baml_base::FileId::new(0),
                            text_size::TextRange::empty(0.into()),
                        );
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: annot_ty.clone(),
                            found: init_ty,
                            span,
                        });
                    }
                    annot_ty
                } else {
                    init_ty
                }
            } else if let Some(annot) = type_annotation {
                lower_type_ref(ctx.db(), annot)
            } else {
                Ty::Unknown
            };

            // Extract variable name from pattern
            let pat = &body.patterns[*pattern];
            match pat {
                Pattern::Binding(name) => {
                    ctx.define(name.clone(), ty);
                }
            }
        }

        Stmt::Expr(expr) => {
            infer_expr(ctx, *expr, body);
        }

        Stmt::Return(expr) => {
            if let Some(e) = expr {
                infer_expr(ctx, *e, body);
            }
            // TODO: Check return type matches function signature
        }

        Stmt::While {
            condition,
            body: while_body,
        } => {
            let cond_ty = infer_expr(ctx, *condition, body);
            if !cond_ty.is_subtype_of(&Ty::Bool) {
                let span = Span::new(
                    baml_base::FileId::new(0),
                    text_size::TextRange::empty(0.into()),
                );
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Bool,
                    found: cond_ty,
                    span,
                });
            }
            infer_expr(ctx, *while_body, body);
        }

        Stmt::ForIn {
            pattern,
            iterator,
            body: for_body,
        } => {
            let iter_ty = infer_expr(ctx, *iterator, body);

            // Extract element type from iterator
            let elem_ty = match &iter_ty {
                Ty::List(elem) => (**elem).clone(),
                _ => Ty::Unknown,
            };

            ctx.push_scope();

            // Bind the loop variable
            let pat = &body.patterns[*pattern];
            match pat {
                Pattern::Binding(name) => {
                    ctx.define(name.clone(), elem_ty);
                }
            }

            infer_expr(ctx, *for_body, body);
            ctx.pop_scope();
        }

        Stmt::ForCStyle {
            initializer,
            condition,
            update,
            body: for_body,
        } => {
            ctx.push_scope();

            if let Some(init_stmt) = initializer {
                check_stmt(ctx, *init_stmt, body);
            }

            if let Some(cond) = condition {
                let cond_ty = infer_expr(ctx, *cond, body);
                if !cond_ty.is_subtype_of(&Ty::Bool) {
                    let span = Span::new(
                        baml_base::FileId::new(0),
                        text_size::TextRange::empty(0.into()),
                    );
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: Ty::Bool,
                        found: cond_ty,
                        span,
                    });
                }
            }

            if let Some(upd) = update {
                infer_expr(ctx, *upd, body);
            }

            infer_expr(ctx, *for_body, body);
            ctx.pop_scope();
        }

        Stmt::Missing => {}
    }
}

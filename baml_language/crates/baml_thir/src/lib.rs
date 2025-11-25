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

use baml_base::{Name, Span};
use baml_hir::{ClassId, EnumId, Expr, ExprBody, ExprId, FunctionBody, FunctionId, StmtId};
use std::collections::HashMap;

mod lower;
mod types;

pub use lower::lower_type_ref;
pub use types::*;

// ============================================================================
// Type Inference Results
// ============================================================================

/// Result of type inference for a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceResult {
    /// Inferred return type of the function.
    pub return_type: Ty,
    /// Types of parameters.
    pub param_types: HashMap<Name, Ty>,
    /// Types inferred for each expression.
    pub expr_types: HashMap<ExprId, Ty>,
    /// Type checking errors.
    pub errors: Vec<TypeError>,
}

impl InferenceResult {
    /// Create an empty inference result.
    pub fn empty() -> Self {
        InferenceResult {
            return_type: Ty::Unknown,
            param_types: HashMap::new(),
            expr_types: HashMap::new(),
            errors: Vec::new(),
        }
    }
}

// ============================================================================
// Type Errors
// ============================================================================

/// Type errors that can occur during type checking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeError {
    /// Type mismatch between expected and found types.
    TypeMismatch { expected: Ty, found: Ty, span: Span },
    /// Reference to an unknown type name.
    UnknownType { name: String, span: Span },
    /// Reference to an unknown variable.
    UnknownVariable { name: String, span: Span },
    /// Invalid binary operation.
    InvalidBinaryOp {
        op: String,
        lhs: Ty,
        rhs: Ty,
        span: Span,
    },
    /// Invalid unary operation.
    InvalidUnaryOp { op: String, operand: Ty, span: Span },
    /// Wrong number of arguments in function call.
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },
    /// Calling a non-callable type.
    NotCallable { ty: Ty, span: Span },
    /// Field access on non-class type.
    NoSuchField { ty: Ty, field: String, span: Span },
    /// Index access on non-indexable type.
    NotIndexable { ty: Ty, span: Span },
}

impl baml_base::Diagnostic for TypeError {
    fn message(&self) -> String {
        match self {
            TypeError::TypeMismatch {
                expected, found, ..
            } => {
                format!("Type mismatch: expected {expected}, found {found}")
            }
            TypeError::UnknownType { name, .. } => {
                format!("Unknown type: {name}")
            }
            TypeError::UnknownVariable { name, .. } => {
                format!("Unknown variable: {name}")
            }
            TypeError::InvalidBinaryOp { op, lhs, rhs, .. } => {
                format!("Cannot apply operator '{op}' to types {lhs} and {rhs}")
            }
            TypeError::InvalidUnaryOp { op, operand, .. } => {
                format!("Cannot apply operator '{op}' to type {operand}")
            }
            TypeError::ArgumentCountMismatch {
                expected, found, ..
            } => {
                format!("Expected {expected} arguments, found {found}")
            }
            TypeError::NotCallable { ty, .. } => {
                format!("Type {ty} is not callable")
            }
            TypeError::NoSuchField { ty, field, .. } => {
                format!("Type {ty} has no field '{field}'")
            }
            TypeError::NotIndexable { ty, .. } => {
                format!("Type {ty} is not indexable")
            }
        }
    }

    fn span(&self) -> Option<Span> {
        let span = match self {
            TypeError::TypeMismatch { span, .. }
            | TypeError::UnknownType { span, .. }
            | TypeError::UnknownVariable { span, .. }
            | TypeError::InvalidBinaryOp { span, .. }
            | TypeError::InvalidUnaryOp { span, .. }
            | TypeError::ArgumentCountMismatch { span, .. }
            | TypeError::NotCallable { span, .. }
            | TypeError::NoSuchField { span, .. }
            | TypeError::NotIndexable { span, .. } => span,
        };
        Some(*span)
    }

    fn severity(&self) -> baml_base::Severity {
        baml_base::Severity::Error
    }
}

// ============================================================================
// Type Context
// ============================================================================

/// Context for type inference, tracking scopes and accumulated results.
pub struct TypeContext<'db> {
    db: &'db dyn salsa::Database,
    /// Stack of variable scopes (innermost last).
    scopes: Vec<HashMap<Name, Ty>>,
    /// Inferred types for expressions.
    expr_types: HashMap<ExprId, Ty>,
    /// Accumulated type errors.
    errors: Vec<TypeError>,
}

impl<'db> TypeContext<'db> {
    /// Create a new type context.
    pub fn new(db: &'db dyn salsa::Database) -> Self {
        TypeContext {
            db,
            scopes: vec![HashMap::new()], // Start with one scope
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
    pub fn define(&mut self, name: Name, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Look up a variable in the scope chain.
    pub fn lookup(&self, name: &Name) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Record the type of an expression.
    pub fn set_expr_type(&mut self, expr: ExprId, ty: Ty) {
        self.expr_types.insert(expr, ty.clone());
    }

    /// Get the type of an expression.
    pub fn get_expr_type(&self, expr: ExprId) -> Option<&Ty> {
        self.expr_types.get(&expr)
    }

    /// Add a type error.
    pub fn push_error(&mut self, error: TypeError) {
        self.errors.push(error);
    }

    /// Get the database reference.
    pub fn db(&self) -> &'db dyn salsa::Database {
        self.db
    }
}

// ============================================================================
// Type Inference
// ============================================================================

/// Infer types for a function.
pub fn infer_function(db: &dyn salsa::Database, func: FunctionId) -> InferenceResult {
    let func_data = baml_hir::function_data(db, func.clone());
    let body = baml_hir::function_body(db, func);

    let mut ctx = TypeContext::new(db);

    // Add parameters to the initial scope
    let mut param_types = HashMap::new();
    for param in &func_data.params {
        let ty = lower_type_ref(db, &param.ty);
        ctx.define(param.name.clone(), ty.clone());
        param_types.insert(param.name.clone(), ty);
    }

    // Get expected return type
    let expected_return = lower_type_ref(db, &func_data.return_type);

    // Type check the body
    let return_type = match &*body {
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

    // Check return type matches
    if !return_type.is_subtype_of(&expected_return) && !return_type.is_unknown() {
        // We'd need the span of the function body for this error
        // For now, we skip this check or use a placeholder span
    }

    InferenceResult {
        return_type,
        param_types,
        expr_types: ctx.expr_types,
        errors: ctx.errors,
    }
}

/// Infer the type of an expression (synthesize mode).
fn infer_expr(ctx: &mut TypeContext, expr_id: ExprId, body: &ExprBody) -> Ty {
    let expr = match body.get_expr(expr_id) {
        Some(e) => e,
        None => return Ty::Unknown,
    };

    let span = body.expr_span(expr_id).unwrap_or_else(|| {
        Span::new(
            baml_base::FileId::new(0),
            text_size::TextRange::empty(0.into()),
        )
    });

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
            let _callee_ty = infer_expr(ctx, *callee, body);
            // Infer argument types
            for arg in args {
                infer_expr(ctx, *arg, body);
            }
            // For now, return Unknown since we don't have function type info
            Ty::Unknown
        }

        Expr::MethodCall {
            receiver,
            method: _,
            args,
        } => {
            let _receiver_ty = infer_expr(ctx, *receiver, body);
            for arg in args {
                infer_expr(ctx, *arg, body);
            }
            Ty::Unknown
        }

        Expr::Field { base, field } => {
            let base_ty = infer_expr(ctx, *base, body);
            infer_field_access(ctx, &base_ty, field, span)
        }

        Expr::Index { base, index } => {
            let base_ty = infer_expr(ctx, *base, body);
            let index_ty = infer_expr(ctx, *index, body);
            infer_index_access(ctx, &base_ty, &index_ty, span)
        }

        Expr::Array(elements) => {
            if elements.is_empty() {
                Ty::List(Box::new(Ty::Unknown))
            } else {
                // Infer element type from first element
                let elem_ty = infer_expr(ctx, elements[0], body);
                // Check all elements have compatible types
                for &elem in &elements[1..] {
                    let other_ty = infer_expr(ctx, elem, body);
                    if !other_ty.is_subtype_of(&elem_ty) {
                        let elem_span = body.expr_span(elem).unwrap_or(span);
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: elem_ty.clone(),
                            found: other_ty,
                            span: elem_span,
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
                let cond_span = body.expr_span(*condition).unwrap_or(span);
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Bool,
                    found: cond_ty,
                    span: cond_span,
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

        Expr::Match {
            expr: match_expr,
            arms,
        } => {
            let _scrutinee_ty = infer_expr(ctx, *match_expr, body);

            let mut arm_types = Vec::new();
            for arm in arms {
                // TODO: Check pattern against scrutinee type
                if let Some(guard) = &arm.guard {
                    infer_expr(ctx, *guard, body);
                }
                let arm_ty = infer_expr(ctx, arm.body, body);
                arm_types.push(arm_ty);
            }

            // Result is union of all arm types
            if arm_types.is_empty() {
                Ty::Unknown
            } else if arm_types.iter().all(|t| t == &arm_types[0]) {
                arm_types.pop().unwrap()
            } else {
                Ty::Union(arm_types)
            }
        }

        Expr::Lambda {
            params,
            body: lambda_body,
        } => {
            ctx.push_scope();

            // Add parameters to scope
            for (name, ty_ref) in params {
                let ty = ty_ref
                    .as_ref()
                    .map(|t| lower_type_ref(ctx.db(), t))
                    .unwrap_or(Ty::Unknown);
                ctx.define(name.clone(), ty);
            }

            // Infer body type
            let _body_ty = infer_expr(ctx, *lambda_body, body);

            ctx.pop_scope();

            // Return Unknown since we don't have function types yet
            Ty::Unknown
        }

        Expr::StringInterpolation(parts) => {
            // Check that interpolated expressions exist
            for part in parts {
                if let baml_hir::StringPart::Expr(expr) = part {
                    infer_expr(ctx, *expr, body);
                }
            }
            Ty::String
        }

        Expr::Missing => Ty::Unknown,
    };

    ctx.set_expr_type(expr_id, ty.clone());
    ty
}

/// Infer the type of a literal.
fn infer_literal(lit: &baml_hir::Literal) -> Ty {
    match lit {
        baml_hir::Literal::Int(_) => Ty::Int,
        baml_hir::Literal::Float(_) => Ty::Float,
        baml_hir::Literal::String(_) => Ty::String,
        baml_hir::Literal::Bool(_) => Ty::Bool,
        baml_hir::Literal::Null => Ty::Null,
    }
}

/// Infer the result type of a binary operation.
fn infer_binary_op(
    ctx: &mut TypeContext,
    op: baml_hir::BinaryOp,
    lhs: &Ty,
    rhs: &Ty,
    span: Span,
) -> Ty {
    use baml_hir::BinaryOp::*;

    match op {
        // Arithmetic operations
        Add | Sub | Mul | Div | Mod => {
            if lhs.is_subtype_of(&Ty::Int) && rhs.is_subtype_of(&Ty::Int) {
                Ty::Int
            } else if lhs.is_subtype_of(&Ty::Float) && rhs.is_subtype_of(&Ty::Float) {
                Ty::Float
            } else if lhs.is_subtype_of(&Ty::Int) && rhs.is_subtype_of(&Ty::Float) {
                Ty::Float
            } else if lhs.is_subtype_of(&Ty::Float) && rhs.is_subtype_of(&Ty::Int) {
                Ty::Float
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{:?}", op),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }

        // Comparison operations
        Eq | Ne => Ty::Bool,

        Lt | Le | Gt | Ge => {
            if (lhs.is_subtype_of(&Ty::Int) || lhs.is_subtype_of(&Ty::Float))
                && (rhs.is_subtype_of(&Ty::Int) || rhs.is_subtype_of(&Ty::Float))
            {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{:?}", op),
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
                    op: format!("{:?}", op),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }

        // String concatenation
        Concat => {
            if lhs.is_subtype_of(&Ty::String) && rhs.is_subtype_of(&Ty::String) {
                Ty::String
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: "++".to_string(),
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
fn infer_unary_op(ctx: &mut TypeContext, op: baml_hir::UnaryOp, operand: &Ty, span: Span) -> Ty {
    use baml_hir::UnaryOp::*;

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
fn infer_field_access(ctx: &mut TypeContext, base: &Ty, field: &Name, span: Span) -> Ty {
    match base {
        Ty::Class(class_id) => {
            // Look up field in class
            let class_data = baml_hir::class_data(ctx.db(), class_id.clone());
            for class_field in &class_data.fields {
                if &class_field.name == field {
                    return lower_type_ref(ctx.db(), &class_field.ty);
                }
            }
            ctx.push_error(TypeError::NoSuchField {
                ty: base.clone(),
                field: field.to_string(),
                span,
            });
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
fn infer_index_access(ctx: &mut TypeContext, base: &Ty, index: &Ty, span: Span) -> Ty {
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
fn check_stmt(ctx: &mut TypeContext, stmt_id: StmtId, body: &ExprBody) {
    let stmt = match body.get_stmt(stmt_id) {
        Some(s) => s,
        None => return,
    };

    match stmt {
        baml_hir::Stmt::Let {
            name,
            type_annotation,
            initializer,
        } => {
            let ty = if let Some(init) = initializer {
                let init_ty = infer_expr(ctx, *init, body);

                // If there's a type annotation, check it matches
                if let Some(annot) = type_annotation {
                    let annot_ty = lower_type_ref(ctx.db(), annot);
                    if !init_ty.is_subtype_of(&annot_ty) {
                        let span = body.expr_span(*init).unwrap_or_else(|| {
                            Span::new(
                                baml_base::FileId::new(0),
                                text_size::TextRange::empty(0.into()),
                            )
                        });
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

            ctx.define(name.clone(), ty);
        }

        baml_hir::Stmt::Expr(expr) => {
            infer_expr(ctx, *expr, body);
        }

        baml_hir::Stmt::Return(expr) => {
            if let Some(e) = expr {
                infer_expr(ctx, *e, body);
            }
            // TODO: Check return type matches function signature
        }
    }
}

// ============================================================================
// Helper Functions (for backwards compatibility)
// ============================================================================

/// Helper function for class type resolution.
pub fn class_type(db: &dyn salsa::Database, class: ClassId) -> Ty {
    let _data = baml_hir::class_data(db, class.clone());
    Ty::Class(class)
}

/// Helper function for enum type resolution.
pub fn enum_type(_db: &dyn salsa::Database, enum_id: EnumId) -> Ty {
    Ty::Enum(enum_id)
}

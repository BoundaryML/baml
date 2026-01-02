//! Pretty printing for THIR (Typed HIR).
//!
//! This module provides tree-based visualization of the typed intermediate
//! representation, showing expression structure with inferred types.

use std::fmt::Write;

use baml_diagnostics::compiler_error::TypeError;
use baml_hir::{
    BinaryOp, Expr, ExprBody, ExprId, FunctionBody, FunctionSignature, Literal, LlmBody, Pattern,
    Stmt, StmtId, UnaryOp,
};

use super::Ty;
use crate::{InferenceResult, lower_type_ref};

/// Renders a function's THIR as a tree showing expression structure with types.
///
/// # Example output
/// ```text
/// function Foo(x: int) -> int
/// ├─ param x: int
/// └─ Block(2 stmts + tail): int
///    ├─ Let y: int
///    │  └─ Binary(Add): int
///    │     ├─ Literal(Int(1)): int
///    │     └─ Path(x): int
///    └─ Path(y): int
/// ```
pub fn render_function_tree(
    db: &dyn baml_hir::Db,
    func_name: &str,
    signature: &FunctionSignature,
    body: &FunctionBody,
    result: &InferenceResult<'_>,
) -> String {
    let mut output = String::new();
    let mut renderer = TreeRenderer::new(db, &mut output);
    renderer.render_function(func_name, signature, body, result);
    output
}

/// Renders just a function body's THIR as a tree.
pub fn render_body_tree(
    db: &dyn baml_hir::Db,
    body: &FunctionBody,
    result: &InferenceResult<'_>,
) -> String {
    let mut output = String::new();
    let mut renderer = TreeRenderer::new(db, &mut output);
    renderer.render_body(body, result);
    output
}

/// Internal tree renderer.
struct TreeRenderer<'a, 'db> {
    db: &'db dyn baml_hir::Db,
    output: &'a mut String,
    /// Tracks whether each depth level has more siblings coming.
    /// `true` means there are more siblings (draw │), `false` means it was the last child (draw space).
    continuation: Vec<bool>,
}

impl<'a, 'db> TreeRenderer<'a, 'db> {
    fn new(db: &'db dyn baml_hir::Db, output: &'a mut String) -> Self {
        Self {
            db,
            output,
            continuation: Vec::new(),
        }
    }

    fn render_function(
        &mut self,
        func_name: &str,
        signature: &FunctionSignature,
        body: &FunctionBody,
        result: &InferenceResult<'_>,
    ) {
        // Function header
        let return_type = lower_type_ref(self.db, &signature.return_type);
        let params: Vec<String> = signature
            .params
            .iter()
            .map(|p| {
                let ty = lower_type_ref(self.db, &p.type_ref);
                format!("{}: {}", p.name, ty)
            })
            .collect();

        writeln!(
            self.output,
            "function {}({}) -> {}",
            func_name,
            params.join(", "),
            return_type
        )
        .ok();

        // Show parameters as tree nodes
        let param_count = signature.params.len();
        for (i, param) in signature.params.iter().enumerate() {
            let param_ty = lower_type_ref(self.db, &param.type_ref);
            let is_last = i == param_count - 1 && matches!(body, FunctionBody::Missing);
            let prefix = if is_last { "└─" } else { "├─" };
            writeln!(self.output, "{} param {}: {}", prefix, param.name, param_ty).ok();
        }

        // Render body
        self.render_body(body, result);

        // Show errors if any
        if !result.errors.is_empty() {
            writeln!(self.output, "  Errors:").ok();
            for error in &result.errors {
                writeln!(self.output, "    • {}", short_display(error)).ok();
            }
        }
    }

    fn render_body(&mut self, body: &FunctionBody, result: &InferenceResult<'_>) {
        match body {
            FunctionBody::Expr(expr_body) => {
                if let Some(root_expr) = expr_body.root_expr {
                    self.render_expr(root_expr, expr_body, result, true);
                }
            }
            FunctionBody::Llm(llm_body) => {
                self.render_llm_body(llm_body);
            }
            FunctionBody::Missing => {
                writeln!(self.output, "└─ <missing body>").ok();
            }
        }
    }

    fn render_llm_body(&mut self, llm_body: &LlmBody) {
        let client = llm_body
            .client
            .as_ref()
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "none".to_string());
        writeln!(self.output, "└─ LLM Body (client: {client})").ok();
    }

    fn render_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        result: &InferenceResult<'_>,
        is_last: bool,
    ) {
        let expr = &body.exprs[expr_id];
        let ty = result
            .expr_types
            .get(&expr_id)
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "?".to_string());

        let prefix = self.make_prefix(is_last);
        let expr_desc = TreeRenderer::describe_expr(expr, &ty);
        writeln!(self.output, "{prefix}{expr_desc}").ok();

        // Track continuation for children: if this node is_last, children don't need │
        self.push_continuation(!is_last);
        self.render_expr_children(expr, body, result);
        self.pop_continuation();
    }

    fn describe_expr(expr: &Expr, ty: &str) -> String {
        match expr {
            Expr::Literal(lit) => format!("Literal({lit:?}): {ty}"),
            Expr::Path(segments) => {
                let path = segments
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                format!("Path({path}): {ty}")
            }
            Expr::Binary { op, .. } => format!("Binary({op:?}): {ty}"),
            Expr::Unary { op, .. } => format!("Unary({op:?}): {ty}"),
            Expr::Call { .. } => format!("Call: {ty}"),
            Expr::FieldAccess { field, .. } => format!("FieldAccess(.{field}): {ty}"),
            Expr::Index { .. } => format!("Index: {ty}"),
            Expr::Array { elements } => format!("Array[{}]: {}", elements.len(), ty),
            Expr::Object { type_name, fields } => {
                let name = type_name
                    .as_ref()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();
                format!("Object({} {{ {} fields }}): {}", name, fields.len(), ty)
            }
            Expr::Block { stmts, tail_expr } => {
                let tail = if tail_expr.is_some() { " + tail" } else { "" };
                format!("Block({} stmts{}): {}", stmts.len(), tail, ty)
            }
            Expr::If { else_branch, .. } => {
                let has_else = if else_branch.is_some() { " + else" } else { "" };
                format!("If{has_else}: {ty}")
            }
            Expr::Match { arms, .. } => {
                format!("Match({} arms): {}", arms.len(), ty)
            }
            Expr::Missing => format!("<missing>: {ty}"),
        }
    }

    fn render_expr_children(&mut self, expr: &Expr, body: &ExprBody, result: &InferenceResult<'_>) {
        match expr {
            Expr::Binary { lhs, rhs, .. } => {
                self.render_expr(*lhs, body, result, false);
                self.render_expr(*rhs, body, result, true);
            }
            Expr::Unary { expr: inner, .. } => {
                self.render_expr(*inner, body, result, true);
            }
            Expr::Call { callee, args } => {
                self.render_expr(*callee, body, result, args.is_empty());
                for (i, arg) in args.iter().enumerate() {
                    self.render_expr(*arg, body, result, i == args.len() - 1);
                }
            }
            Expr::FieldAccess { base, .. } => {
                self.render_expr(*base, body, result, true);
            }
            Expr::Index { base, index } => {
                self.render_expr(*base, body, result, false);
                self.render_expr(*index, body, result, true);
            }
            Expr::Array { elements } => {
                for (i, elem) in elements.iter().enumerate() {
                    self.render_expr(*elem, body, result, i == elements.len() - 1);
                }
            }
            Expr::Object { fields, .. } => {
                for (i, (name, value)) in fields.iter().enumerate() {
                    let is_last = i == fields.len() - 1;
                    let field_prefix = self.make_prefix(is_last);
                    writeln!(self.output, "{field_prefix}{name}:").ok();
                    self.push_continuation(!is_last);
                    self.render_expr(*value, body, result, true);
                    self.pop_continuation();
                }
            }
            Expr::Block { stmts, tail_expr } => {
                for (i, stmt_id) in stmts.iter().enumerate() {
                    let is_last = tail_expr.is_none() && i == stmts.len() - 1;
                    self.render_stmt(*stmt_id, body, result, is_last);
                }
                if let Some(tail) = tail_expr {
                    self.render_expr(*tail, body, result, true);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // Condition
                let cond_prefix = self.make_prefix(false);
                writeln!(self.output, "{cond_prefix}condition:").ok();
                self.push_continuation(true);
                self.render_expr(*condition, body, result, true);
                self.pop_continuation();

                // Then branch
                let then_is_last = else_branch.is_none();
                let then_prefix = self.make_prefix(then_is_last);
                writeln!(self.output, "{then_prefix}then:").ok();
                self.push_continuation(!then_is_last);
                self.render_expr(*then_branch, body, result, true);
                self.pop_continuation();

                // Else branch
                if let Some(else_expr) = else_branch {
                    let else_prefix = self.make_prefix(true);
                    writeln!(self.output, "{else_prefix}else:").ok();
                    self.push_continuation(false);
                    self.render_expr(*else_expr, body, result, true);
                    self.pop_continuation();
                }
            }
            Expr::Match { scrutinee, arms } => {
                // Render scrutinee
                let scrut_prefix = self.make_prefix(arms.is_empty());
                writeln!(self.output, "{scrut_prefix}scrutinee:").ok();
                self.push_continuation(!arms.is_empty());
                self.render_expr(*scrutinee, body, result, true);
                self.pop_continuation();

                // Render each arm
                for (i, arm) in arms.iter().enumerate() {
                    let is_last_arm = i == arms.len() - 1;
                    let arm_prefix = self.make_prefix(is_last_arm);
                    writeln!(self.output, "{arm_prefix}arm[{i}]:").ok();
                    self.push_continuation(!is_last_arm);
                    // Render arm body
                    self.render_expr(arm.body, body, result, true);
                    self.pop_continuation();
                }
            }
            Expr::Literal(_) | Expr::Path(_) | Expr::Missing => {
                // Leaf nodes, no children
            }
        }
    }

    fn render_stmt(
        &mut self,
        stmt_id: StmtId,
        body: &ExprBody,
        result: &InferenceResult<'_>,
        is_last: bool,
    ) {
        let stmt = &body.stmts[stmt_id];
        let prefix = self.make_prefix(is_last);

        match stmt {
            Stmt::Let {
                pattern,
                type_annotation,
                initializer,
                ..
            } => {
                let pat = &body.patterns[*pattern];
                let var_name = match pat {
                    Pattern::Binding(name) => name.to_string(),
                    Pattern::TypedBinding { name, ty } => format!("{name}: {ty:?}"),
                    Pattern::Literal(lit) => format!("{lit:?}"),
                    Pattern::EnumVariant { enum_name, variant } => format!("{enum_name}.{variant}"),
                    Pattern::Union(pats) => format!("union[{}]", pats.len()),
                };

                let ty_str = if let Some(type_ref) = type_annotation {
                    let ty = lower_type_ref(self.db, type_ref);
                    format!(": {ty}")
                } else if let Some(init) = initializer {
                    result
                        .expr_types
                        .get(init)
                        .map(|t| format!(": {t}"))
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                writeln!(self.output, "{prefix}Let {var_name}{ty_str}").ok();

                if let Some(init) = initializer {
                    self.push_continuation(!is_last);
                    self.render_expr(*init, body, result, true);
                    self.pop_continuation();
                }
            }
            Stmt::Expr(expr_id) => {
                writeln!(self.output, "{prefix}ExprStmt").ok();
                self.push_continuation(!is_last);
                self.render_expr(*expr_id, body, result, true);
                self.pop_continuation();
            }
            Stmt::Return(expr) => {
                writeln!(self.output, "{prefix}Return").ok();
                if let Some(e) = expr {
                    self.push_continuation(!is_last);
                    self.render_expr(*e, body, result, true);
                    self.pop_continuation();
                }
            }
            Stmt::While {
                condition,
                body: while_body,
                after,
                origin,
            } => {
                let origin_str = match origin {
                    baml_hir::LoopOrigin::While => "While",
                    baml_hir::LoopOrigin::ForLoop => "While (from for-loop)",
                };
                writeln!(self.output, "{prefix}{origin_str}").ok();
                self.push_continuation(!is_last);
                self.render_expr(*condition, body, result, false);
                self.render_expr(*while_body, body, result, after.is_none());
                if let Some(after_stmt) = after {
                    self.render_stmt(*after_stmt, body, result, true);
                }
                self.pop_continuation();
            }
            Stmt::Break => {
                writeln!(self.output, "{prefix}Break").ok();
            }
            Stmt::Continue => {
                writeln!(self.output, "{prefix}Continue").ok();
            }
            Stmt::Assign { target, value } => {
                writeln!(self.output, "{prefix}Assign").ok();
                self.push_continuation(!is_last);
                self.render_expr(*target, body, result, false);
                self.render_expr(*value, body, result, true);
                self.pop_continuation();
            }
            Stmt::AssignOp { target, op, value } => {
                writeln!(self.output, "{prefix}AssignOp ({op:?})").ok();
                self.push_continuation(!is_last);
                self.render_expr(*target, body, result, false);
                self.render_expr(*value, body, result, true);
                self.pop_continuation();
            }
            Stmt::Missing => {
                writeln!(self.output, "{prefix}<missing stmt>").ok();
            }
        }
    }

    fn make_prefix(&self, is_last: bool) -> String {
        let mut p = String::new();
        // Use continuation state to determine whether to draw │ or space
        for &has_more in &self.continuation {
            if has_more {
                p.push_str("│  ");
            } else {
                p.push_str("   ");
            }
        }
        p.push_str(if is_last { "└─ " } else { "├─ " });
        p
    }

    /// Push a new continuation level. `has_more` indicates if there are more siblings at this level.
    fn push_continuation(&mut self, has_more: bool) {
        self.continuation.push(has_more);
    }

    /// Pop the current continuation level.
    fn pop_continuation(&mut self) {
        self.continuation.pop();
    }
}

/// Converts an expression to an inline string representation (for compact display).
pub fn expr_to_string(expr_id: ExprId, body: &ExprBody) -> String {
    let expr = &body.exprs[expr_id];

    match expr {
        Expr::Literal(lit) => match lit {
            Literal::Int(n) => n.to_string(),
            Literal::Float(s) => s.clone(),
            Literal::String(s) => format!("\"{s}\""),
            Literal::Bool(b) => b.to_string(),
            Literal::Null => "null".to_string(),
        },
        Expr::Path(segments) => segments
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("."),
        Expr::Binary { op, lhs, rhs } => {
            let lhs_str = expr_to_string(*lhs, body);
            let rhs_str = expr_to_string(*rhs, body);
            let op_str = binary_op_to_str(*op);
            format!("{lhs_str} {op_str} {rhs_str}")
        }
        Expr::Unary { op, expr: inner } => {
            let inner_str = expr_to_string(*inner, body);
            let op_str = unary_op_to_str(*op);
            format!("{op_str}{inner_str}")
        }
        Expr::Call { callee, args } => {
            let callee_str = expr_to_string(*callee, body);
            let args_str: Vec<String> = args.iter().map(|a| expr_to_string(*a, body)).collect();
            format!("{}({})", callee_str, args_str.join(", "))
        }
        Expr::FieldAccess { base, field } => {
            let base_str = expr_to_string(*base, body);
            format!("{base_str}.{field}")
        }
        Expr::Index { base, index } => {
            let base_str = expr_to_string(*base, body);
            let index_str = expr_to_string(*index, body);
            format!("{base_str}[{index_str}]")
        }
        Expr::Array { elements } => {
            let elems: Vec<String> = elements.iter().map(|e| expr_to_string(*e, body)).collect();
            format!("[{}]", elems.join(", "))
        }
        Expr::Object { type_name, fields } => {
            let name = type_name
                .as_ref()
                .map(|n| format!("{n} "))
                .unwrap_or_default();
            let field_strs: Vec<String> = fields
                .iter()
                .map(|(n, v)| format!("{}: {}", n, expr_to_string(*v, body)))
                .collect();
            format!("{}{{ {} }}", name, field_strs.join(", "))
        }
        Expr::Block { .. } => "{ ... }".to_string(),
        Expr::If { .. } => "if ... { ... }".to_string(),
        Expr::Match { arms, .. } => format!("match {{ {} arms }}", arms.len()),
        Expr::Missing => "<missing>".to_string(),
    }
}

fn binary_op_to_str(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
    }
}

fn unary_op_to_str(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "!",
        UnaryOp::Neg => "-",
    }
}

pub fn short_display(error: &TypeError<Ty<'_>>) -> String {
    match error {
        TypeError::TypeMismatch {
            expected, found, ..
        } => format!("Expected {expected}. Found {found}"),
        TypeError::UnknownType { name, .. } => format!("Unknown type {name}"),
        TypeError::UnknownVariable { name, .. } => format!("Unknown type {name}"),
        TypeError::InvalidBinaryOp { op, lhs, rhs, .. } => {
            format!("Invalid op {op} for {lhs} and {rhs}")
        }
        TypeError::InvalidUnaryOp { op, operand, .. } => format!("Invalid op {op} for {operand}"),
        TypeError::ArgumentCountMismatch {
            expected, found, ..
        } => format!("Expected {expected} args, found {found}"),
        TypeError::NotCallable { ty, .. } => format!("{ty} is not callable"),
        TypeError::NotIndexable { ty, .. } => format!("{ty} is not indexable"),
        TypeError::NoSuchField { ty, field, .. } => format!("{ty} has no field {field}"),
        TypeError::NonExhaustiveMatch {
            scrutinee_type,
            missing_cases,
            ..
        } => {
            let missing = missing_cases.join(", ");
            format!("Non-exhaustive match on {scrutinee_type}: missing {missing}")
        }
        TypeError::UnreachableArm { .. } => "Unreachable match arm".to_string(),
        TypeError::UnknownEnumVariant {
            enum_name,
            variant_name,
            ..
        } => format!("Enum '{enum_name}' has no variant '{variant_name}'"),
    }
}

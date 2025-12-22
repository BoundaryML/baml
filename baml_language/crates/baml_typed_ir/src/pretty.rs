//! Pretty printing for TypedIR expressions.
//!
//! This module provides human-readable output of the TypedIR tree,
//! useful for debugging and testing.

use crate::{AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, Pattern, UnaryOp};

/// Pretty print an expression body.
pub fn pretty_print(body: &ExprBody) -> String {
    let mut printer = PrettyPrinter::new(body);
    printer.print_expr(body.root, 0);
    printer.output
}

struct PrettyPrinter<'a> {
    body: &'a ExprBody,
    output: String,
}

impl<'a> PrettyPrinter<'a> {
    fn new(body: &'a ExprBody) -> Self {
        Self {
            body,
            output: String::new(),
        }
    }

    fn indent(&mut self, level: usize) {
        for _ in 0..level {
            self.output.push_str("  ");
        }
    }

    fn print_expr(&mut self, id: ExprId, level: usize) {
        let expr = self.body.expr(id);
        let ty = self.body.ty(id);

        match expr {
            Expr::Literal(lit) => {
                self.indent(level);
                match lit {
                    Literal::Int(n) => self.output.push_str(&format!("{n}")),
                    Literal::Float(s) => self.output.push_str(s),
                    Literal::String(s) => self.output.push_str(&format!("{s:?}")),
                    Literal::Bool(b) => self.output.push_str(&format!("{b}")),
                    Literal::Null => self.output.push_str("null"),
                }
                self.output.push_str(&format!(" : {ty}"));
            }

            Expr::Unit => {
                self.indent(level);
                self.output.push_str("()");
            }

            Expr::Var(name) => {
                self.indent(level);
                self.output.push_str(&format!("{name} : {ty}"));
            }

            Expr::Path(segments) => {
                self.indent(level);
                let path: Vec<_> = segments.iter().map(|s| s.to_string()).collect();
                self.output.push_str(&path.join("."));
                self.output.push_str(&format!(" : {ty}"));
            }

            Expr::Let {
                pattern,
                ty: let_ty,
                value,
                body,
            } => {
                self.indent(level);
                let pat = self.body.pattern(*pattern);
                let pat_name = match pat {
                    Pattern::Binding(name) => name.to_string(),
                };
                self.output.push_str(&format!("let {pat_name}: {let_ty} =\n"));
                self.print_expr(*value, level + 1);
                self.output.push_str("\n");
                self.indent(level);
                self.output.push_str("in\n");
                self.print_expr(*body, level + 1);
            }

            Expr::Seq { first, second } => {
                self.print_expr(*first, level);
                self.output.push_str(";\n");
                self.print_expr(*second, level);
            }

            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.indent(level);
                self.output.push_str("if\n");
                self.print_expr(*condition, level + 1);
                self.output.push_str("\n");
                self.indent(level);
                self.output.push_str("then\n");
                self.print_expr(*then_branch, level + 1);
                if let Some(else_b) = else_branch {
                    self.output.push_str("\n");
                    self.indent(level);
                    self.output.push_str("else\n");
                    self.print_expr(*else_b, level + 1);
                }
            }

            Expr::While { condition, body } => {
                self.indent(level);
                self.output.push_str("while\n");
                self.print_expr(*condition, level + 1);
                self.output.push_str("\n");
                self.indent(level);
                self.output.push_str("do\n");
                self.print_expr(*body, level + 1);
            }

            Expr::Return(expr) => {
                self.indent(level);
                self.output.push_str("return");
                if let Some(e) = expr {
                    self.output.push('\n');
                    self.print_expr(*e, level + 1);
                }
            }

            Expr::Break => {
                self.indent(level);
                self.output.push_str("break");
            }

            Expr::Continue => {
                self.indent(level);
                self.output.push_str("continue");
            }

            Expr::Assign { target, value } => {
                self.indent(level);
                self.output.push_str("assign\n");
                self.print_expr(*target, level + 1);
                self.output.push_str("\n");
                self.indent(level);
                self.output.push_str(":=\n");
                self.print_expr(*value, level + 1);
            }

            Expr::AssignOp { target, op, value } => {
                self.indent(level);
                let op_str = match op {
                    AssignOp::Add => "+=",
                    AssignOp::Sub => "-=",
                    AssignOp::Mul => "*=",
                    AssignOp::Div => "/=",
                    AssignOp::Mod => "%=",
                    AssignOp::BitAnd => "&=",
                    AssignOp::BitOr => "|=",
                    AssignOp::BitXor => "^=",
                    AssignOp::Shl => "<<=",
                    AssignOp::Shr => ">>=",
                };
                self.output.push_str(&format!("assign-op {op_str}\n"));
                self.print_expr(*target, level + 1);
                self.output.push('\n');
                self.print_expr(*value, level + 1);
            }

            Expr::Binary { op, lhs, rhs } => {
                self.indent(level);
                let op_str = match op {
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
                };
                self.output.push_str(&format!("({op_str}) : {ty}\n"));
                self.print_expr(*lhs, level + 1);
                self.output.push('\n');
                self.print_expr(*rhs, level + 1);
            }

            Expr::Unary { op, operand } => {
                self.indent(level);
                let op_str = match op {
                    UnaryOp::Not => "!",
                    UnaryOp::Neg => "-",
                };
                self.output.push_str(&format!("({op_str}) : {ty}\n"));
                self.print_expr(*operand, level + 1);
            }

            Expr::Call { callee, args } => {
                self.indent(level);
                self.output.push_str(&format!("call : {ty}\n"));
                self.print_expr(*callee, level + 1);
                for arg in args {
                    self.output.push('\n');
                    self.print_expr(*arg, level + 1);
                }
            }

            Expr::Array { elements } => {
                self.indent(level);
                self.output.push_str(&format!("array : {ty}"));
                for elem in elements {
                    self.output.push('\n');
                    self.print_expr(*elem, level + 1);
                }
            }

            Expr::Object { type_name, fields } => {
                self.indent(level);
                let name = type_name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "anon".to_string());
                self.output.push_str(&format!("object {name} : {ty}"));
                for (field_name, value) in fields {
                    self.output.push('\n');
                    self.indent(level + 1);
                    self.output.push_str(&format!("{field_name}:\n"));
                    self.print_expr(*value, level + 2);
                }
            }

            Expr::FieldAccess { base, field } => {
                self.indent(level);
                self.output.push_str(&format!(".{field} : {ty}\n"));
                self.print_expr(*base, level + 1);
            }

            Expr::Index { base, index } => {
                self.indent(level);
                self.output.push_str(&format!("index : {ty}\n"));
                self.print_expr(*base, level + 1);
                self.output.push('\n');
                self.indent(level);
                self.output.push_str("[\n");
                self.print_expr(*index, level + 1);
                self.output.push('\n');
                self.indent(level);
                self.output.push(']');
            }
        }
    }
}

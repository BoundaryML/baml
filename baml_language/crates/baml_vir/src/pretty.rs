//! Pretty printing for VIR expressions.
//!
//! This module provides human-readable output of the VIR tree,
//! useful for debugging and testing.

use std::fmt::Write;

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
                    Literal::Int(n) => write!(self.output, "{n}").unwrap(),
                    Literal::Float(s) => self.output.push_str(s),
                    Literal::String(s) => write!(self.output, "{s:?}").unwrap(),
                    Literal::Bool(b) => write!(self.output, "{b}").unwrap(),
                    Literal::Null => self.output.push_str("null"),
                }
                write!(self.output, " : {ty}").unwrap();
            }

            Expr::Unit => {
                self.indent(level);
                self.output.push_str("()");
            }

            Expr::Var(name) => {
                self.indent(level);
                write!(self.output, "{name} : {ty}").unwrap();
            }

            Expr::Path(segments) => {
                self.indent(level);
                let path: Vec<_> = segments
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                self.output.push_str(&path.join("."));
                write!(self.output, " : {ty}").unwrap();
            }

            Expr::Let {
                pattern,
                ty: let_ty,
                value,
                body,
                is_watched,
            } => {
                self.indent(level);
                let pat_name = self.format_pattern(*pattern);
                let watch_prefix = if *is_watched { "watch " } else { "" };
                writeln!(self.output, "{watch_prefix}let {pat_name}: {let_ty} =").unwrap();
                self.print_expr(*value, level + 1);
                self.output.push('\n');
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
                self.output.push('\n');
                self.indent(level);
                self.output.push_str("then\n");
                self.print_expr(*then_branch, level + 1);
                if let Some(else_b) = else_branch {
                    self.output.push('\n');
                    self.indent(level);
                    self.output.push_str("else\n");
                    self.print_expr(*else_b, level + 1);
                }
            }

            Expr::While { condition, body } => {
                self.indent(level);
                self.output.push_str("while\n");
                self.print_expr(*condition, level + 1);
                self.output.push('\n');
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
                self.output.push('\n');
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
                writeln!(self.output, "assign-op {op_str}").unwrap();
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
                    BinaryOp::Instanceof => "instanceof",
                };
                writeln!(self.output, "({op_str}) : {ty}").unwrap();
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
                writeln!(self.output, "({op_str}) : {ty}").unwrap();
                self.print_expr(*operand, level + 1);
            }

            Expr::Call { callee, args } => {
                self.indent(level);
                writeln!(self.output, "call : {ty}").unwrap();
                self.print_expr(*callee, level + 1);
                for arg in args {
                    self.output.push('\n');
                    self.print_expr(*arg, level + 1);
                }
            }

            Expr::Array { elements } => {
                self.indent(level);
                write!(self.output, "array : {ty}").unwrap();
                for elem in elements {
                    self.output.push('\n');
                    self.print_expr(*elem, level + 1);
                }
            }

            Expr::Object {
                type_name,
                fields,
                spreads,
            } => {
                self.indent(level);
                let name = type_name
                    .as_ref()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_else(|| "anon".to_string());
                write!(self.output, "object {name} : {ty}").unwrap();
                for (field_name, value) in fields {
                    self.output.push('\n');
                    self.indent(level + 1);
                    writeln!(self.output, "{field_name}:").unwrap();
                    self.print_expr(*value, level + 2);
                }
                for spread in spreads {
                    self.output.push('\n');
                    self.indent(level + 1);
                    writeln!(self.output, "...(position {})", spread.position).unwrap();
                    self.print_expr(spread.expr, level + 2);
                }
            }

            Expr::Map { entries } => {
                self.indent(level);
                write!(self.output, "map : {ty}").unwrap();
                for (i, (key, value)) in entries.iter().enumerate() {
                    self.output.push('\n');
                    self.indent(level + 1);
                    writeln!(self.output, "entry[{i}]:").unwrap();
                    self.print_expr(*key, level + 2);
                    self.output.push('\n');
                    self.print_expr(*value, level + 2);
                }
            }

            Expr::FieldAccess { base, field } => {
                self.indent(level);
                writeln!(self.output, ".{field} : {ty}").unwrap();
                self.print_expr(*base, level + 1);
            }

            Expr::Index { base, index } => {
                self.indent(level);
                writeln!(self.output, "index : {ty}").unwrap();
                self.print_expr(*base, level + 1);
                self.output.push('\n');
                self.indent(level);
                self.output.push_str("[\n");
                self.print_expr(*index, level + 1);
                self.output.push('\n');
                self.indent(level);
                self.output.push(']');
            }

            Expr::Match { scrutinee, arms } => {
                self.indent(level);
                writeln!(self.output, "match : {ty}").unwrap();
                self.print_expr(*scrutinee, level + 1);
                for arm in arms {
                    self.output.push('\n');
                    self.indent(level + 1);
                    let pat_str = self.format_pattern(arm.pattern);
                    write!(self.output, "{pat_str}").unwrap();
                    if let Some(guard) = arm.guard {
                        self.output.push_str(" if ");
                        // Print guard inline (simplified)
                        let guard_expr = self.body.expr(guard);
                        write!(self.output, "{guard_expr:?}").unwrap();
                    }
                    self.output.push_str(" =>\n");
                    self.print_expr(arm.body, level + 2);
                }
            }
            Expr::NotifyBlock { name, level: lvl } => {
                self.indent(level);
                self.output.push_str("//");
                for _ in 0..*lvl {
                    self.output.push('#');
                }
                self.output.push(' ');
                self.output.push_str(name.as_ref());
                self.output.push('\n');
            }
        }
    }

    fn format_pattern(&self, pat_id: crate::PatId) -> String {
        let pat = self.body.pattern(pat_id);
        match pat {
            Pattern::Binding(name) => name.to_string(),
            Pattern::TypedBinding { name, ty } => format!("{name}: {ty}"),
            Pattern::Literal(lit) => match lit {
                Literal::Int(n) => n.to_string(),
                Literal::Float(s) => s.clone(),
                Literal::String(s) => format!("{s:?}"),
                Literal::Bool(b) => b.to_string(),
                Literal::Null => "null".to_string(),
            },
            Pattern::EnumVariant { enum_name, variant } => format!("{enum_name}.{variant}"),
            Pattern::Union(pats) => {
                let parts: Vec<_> = pats.iter().map(|p| self.format_pattern(*p)).collect();
                parts.join(" | ")
            }
        }
    }
}

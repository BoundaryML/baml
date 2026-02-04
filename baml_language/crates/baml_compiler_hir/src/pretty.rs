//! Pretty printing HIR as code.
//!
//! This module renders HIR expressions and statements as human-readable code,
//! which is useful for understanding desugaring transformations.

use std::fmt::Write;

use crate::{
    AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, Pattern, Stmt, StmtId, TypeRef, UnaryOp,
};

/// Renders an expression body as code.
pub fn body_to_code(body: &ExprBody) -> String {
    let Some(root) = body.root_expr else {
        return String::new();
    };

    let mut printer = CodePrinter::new(body);
    printer.print_expr(root);
    printer.output
}

/// Renders an expression as a single-line string.
pub fn expr_to_code(expr_id: ExprId, body: &ExprBody) -> String {
    let mut printer = CodePrinter::new(body);
    printer.print_expr(expr_id);
    printer.output
}

/// Renders a statement as code.
pub fn stmt_to_code(stmt_id: StmtId, body: &ExprBody) -> String {
    let mut printer = CodePrinter::new(body);
    printer.print_stmt(stmt_id);
    printer.output
}

struct CodePrinter<'a> {
    body: &'a ExprBody,
    output: String,
    indent: usize,
}

impl<'a> CodePrinter<'a> {
    fn new(body: &'a ExprBody) -> Self {
        Self {
            body,
            output: String::new(),
            indent: 0,
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn print_expr(&mut self, expr_id: ExprId) {
        let expr = &self.body.exprs[expr_id];

        match expr {
            Expr::Literal(lit) => self.print_literal(lit),
            Expr::Path(segments) => {
                let path = segments
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                self.output.push_str(&path);
            }
            Expr::Binary { op, lhs, rhs } => {
                self.print_expr(*lhs);
                write!(self.output, " {} ", binary_op_str(*op)).unwrap();
                self.print_expr(*rhs);
            }
            Expr::Unary { op, expr } => {
                self.output.push_str(unary_op_str(*op));
                self.print_expr(*expr);
            }
            Expr::Call { callee, args } => {
                self.print_expr(*callee);
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.print_expr(*arg);
                }
                self.output.push(')');
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.output.push_str("if (");
                self.print_expr(*condition);
                self.output.push_str(") ");
                self.print_expr(*then_branch);
                if let Some(else_expr) = else_branch {
                    self.output.push_str(" else ");
                    self.print_expr(*else_expr);
                }
            }
            Expr::Block { stmts, tail_expr } => {
                self.output.push_str("{\n");
                self.indent += 1;
                for stmt_id in stmts {
                    self.write_indent();
                    self.print_stmt(*stmt_id);
                    self.output.push('\n');
                }
                if let Some(tail) = tail_expr {
                    self.write_indent();
                    self.print_expr(*tail);
                    self.output.push('\n');
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
            }
            Expr::Array { elements } => {
                self.output.push('[');
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.print_expr(*elem);
                }
                self.output.push(']');
            }
            Expr::Object {
                type_name,
                fields,
                spreads,
            } => {
                if let Some(name) = type_name {
                    self.output.push_str(name.as_ref());
                    self.output.push(' ');
                }
                self.output.push_str("{ ");

                // Build a combined list of elements with their positions
                // for proper ordering in output
                let mut elements: Vec<(usize, bool, usize)> = Vec::new();
                for (i, _) in fields.iter().enumerate() {
                    // We don't have position info for fields in the current struct,
                    // so we'll output fields first, then spreads
                    elements.push((i, false, i));
                }
                for (i, spread) in spreads.iter().enumerate() {
                    elements.push((spread.position, true, i));
                }
                elements.sort_by_key(|(pos, _, _)| *pos);

                let mut first = true;
                for (_, is_spread, idx) in elements {
                    if !first {
                        self.output.push_str(", ");
                    }
                    first = false;

                    if is_spread {
                        self.output.push_str("...");
                        self.print_expr(spreads[idx].expr);
                    } else {
                        let (name, value) = &fields[idx];
                        self.output.push_str(name.as_ref());
                        self.output.push_str(": ");
                        self.print_expr(*value);
                    }
                }
                self.output.push_str(" }");
            }
            Expr::Map { entries } => {
                self.output.push_str("{ ");
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.print_expr(*key);
                    self.output.push_str(": ");
                    self.print_expr(*value);
                }
                self.output.push_str(" }");
            }
            Expr::FieldAccess { base, field } => {
                self.print_expr(*base);
                self.output.push('.');
                self.output.push_str(field.as_ref());
            }
            Expr::Index { base, index } => {
                self.print_expr(*base);
                self.output.push('[');
                self.print_expr(*index);
                self.output.push(']');
            }
            Expr::Missing => {
                self.output.push_str("<missing>");
            }
            Expr::Match { scrutinee, arms } => {
                self.output.push_str("match (");
                self.print_expr(*scrutinee);
                self.output.push_str(") {\n");
                self.indent += 1;
                for arm_id in arms {
                    let arm = &self.body.match_arms[*arm_id];
                    self.write_indent();
                    self.print_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.output.push_str(" if ");
                        self.print_expr(guard);
                    }
                    self.output.push_str(" => ");
                    self.print_expr(arm.body);
                    self.output.push_str(",\n");
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
            }
        }
    }

    fn print_stmt(&mut self, stmt_id: StmtId) {
        let stmt = &self.body.stmts[stmt_id];

        match stmt {
            Stmt::Expr(expr_id) => {
                self.print_expr(*expr_id);
                self.output.push(';');
            }
            Stmt::Let {
                pattern,
                type_annotation,
                initializer,
                is_watched,
            } => {
                if *is_watched {
                    self.output.push_str("watch let ");
                } else {
                    self.output.push_str("let ");
                }
                self.print_pattern(*pattern);
                if let Some(type_id) = type_annotation {
                    let type_ref = &self.body.types[*type_id];
                    write!(self.output, ": {}", type_ref_to_str(type_ref)).unwrap();
                }
                if let Some(init) = initializer {
                    self.output.push_str(" = ");
                    self.print_expr(*init);
                }
                self.output.push(';');
            }
            Stmt::While {
                condition,
                body,
                after,
                origin: _,
            } => {
                self.output.push_str("while (");
                self.print_expr(*condition);
                self.output.push_str(") ");
                // If there's an after statement, inject it at the end of the body block
                if let Some(after_stmt) = after {
                    self.print_block_with_after(*body, *after_stmt);
                } else {
                    self.print_expr(*body);
                }
            }
            Stmt::Return(expr) => {
                self.output.push_str("return");
                if let Some(e) = expr {
                    self.output.push(' ');
                    self.print_expr(*e);
                }
                self.output.push(';');
            }
            Stmt::Break => {
                self.output.push_str("break;");
            }
            Stmt::Continue => {
                self.output.push_str("continue;");
            }
            Stmt::Assign { target, value } => {
                self.print_expr(*target);
                self.output.push_str(" = ");
                self.print_expr(*value);
                self.output.push(';');
            }
            Stmt::AssignOp { target, op, value } => {
                self.print_expr(*target);
                write!(self.output, " {}= ", assign_op_str(*op)).unwrap();
                self.print_expr(*value);
                self.output.push(';');
            }
            Stmt::Assert { condition } => {
                self.output.push_str("assert ");
                self.print_expr(*condition);
                self.output.push(';');
            }
            Stmt::Missing => {
                self.output.push_str("<missing>;");
            }
            Stmt::HeaderComment { name, level } => {
                self.output.push_str("//");
                for _ in 0..*level {
                    self.output.push('#');
                }
                self.output.push(' ');
                self.output.push_str(name.as_ref());
            }
        }
    }

    fn print_pattern(&mut self, pat_id: crate::PatId) {
        let pattern = &self.body.patterns[pat_id];
        match pattern {
            Pattern::Binding(name) => {
                self.output.push_str(name.as_ref());
            }
            Pattern::TypedBinding { name, ty } => {
                self.output.push_str(name.as_ref());
                self.output.push_str(": ");
                self.output.push_str(&type_ref_to_str(ty));
            }
            Pattern::Literal(lit) => {
                self.print_literal(lit);
            }
            Pattern::EnumVariant { enum_name, variant } => {
                self.output.push_str(enum_name.as_ref());
                self.output.push('.');
                self.output.push_str(variant.as_ref());
            }
            Pattern::Union(sub_patterns) => {
                for (i, sub_pat_id) in sub_patterns.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(" | ");
                    }
                    self.print_pattern(*sub_pat_id);
                }
            }
        }
    }

    fn print_literal(&mut self, lit: &Literal) {
        match lit {
            Literal::Int(n) => write!(self.output, "{n}").unwrap(),
            Literal::Float(s) => self.output.push_str(s),
            Literal::String(s) => write!(self.output, "\"{s}\"").unwrap(),
            Literal::Bool(b) => write!(self.output, "{b}").unwrap(),
            Literal::Null => self.output.push_str("null"),
        }
    }

    /// Print a block expression with an additional statement at the end.
    /// Used for C-style for loops where the update statement needs to be
    /// printed at the end of the loop body.
    fn print_block_with_after(&mut self, body_expr: ExprId, after_stmt: StmtId) {
        let expr = &self.body.exprs[body_expr];

        if let Expr::Block { stmts, tail_expr } = expr {
            self.output.push_str("{\n");
            self.indent += 1;

            // Print all statements in the block
            for stmt_id in stmts {
                self.write_indent();
                self.print_stmt(*stmt_id);
                self.output.push('\n');
            }

            // Print the after statement
            self.write_indent();
            self.print_stmt(after_stmt);
            self.output.push('\n');

            // Print tail expression if present
            if let Some(tail) = tail_expr {
                self.write_indent();
                self.print_expr(*tail);
                self.output.push('\n');
            }

            self.indent -= 1;
            self.write_indent();
            self.output.push('}');
        } else {
            // Fallback: just print the body and after separately
            self.print_expr(body_expr);
        }
    }
}

fn binary_op_str(op: BinaryOp) -> &'static str {
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
        BinaryOp::Instanceof => "instanceof",
    }
}

fn unary_op_str(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "!",
        UnaryOp::Neg => "-",
    }
}

fn assign_op_str(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Add => "+",
        AssignOp::Sub => "-",
        AssignOp::Mul => "*",
        AssignOp::Div => "/",
        AssignOp::Mod => "%",
        AssignOp::BitAnd => "&",
        AssignOp::BitOr => "|",
        AssignOp::BitXor => "^",
        AssignOp::Shl => "<<",
        AssignOp::Shr => ">>",
    }
}

/// Formats a `TypeRef` as code.
pub fn type_ref_to_str(ty: &TypeRef) -> String {
    type_ref_to_str_impl(ty, false)
}

/// Formats a `TypeRef` as code, optionally wrapping unions in parentheses.
///
/// The `wrap_union` parameter controls whether union types should be wrapped
/// in parentheses. This is needed when a union appears inside an `Optional`
/// or `List` type to ensure correct parsing (e.g., `(int | string)?` vs `int | string?`).
fn type_ref_to_str_impl(ty: &TypeRef, wrap_union: bool) -> String {
    match ty {
        TypeRef::Path(path) => path
            .segments
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("."),
        TypeRef::Int => "int".to_string(),
        TypeRef::Float => "float".to_string(),
        TypeRef::String => "string".to_string(),
        TypeRef::Bool => "bool".to_string(),
        TypeRef::Null => "null".to_string(),
        TypeRef::Media(kind) => kind.to_string(),
        TypeRef::Optional(inner) => format!("{}?", type_ref_to_str_impl(inner, true)),
        TypeRef::List(inner) => format!("{}[]", type_ref_to_str_impl(inner, true)),
        TypeRef::Map { key, value } => {
            format!(
                "map<{}, {}>",
                type_ref_to_str_impl(key, false),
                type_ref_to_str_impl(value, false)
            )
        }
        TypeRef::Union(types) => {
            let inner = types
                .iter()
                .map(|t| type_ref_to_str_impl(t, false))
                .collect::<Vec<_>>()
                .join(" | ");
            if wrap_union {
                format!("({inner})")
            } else {
                inner
            }
        }
        TypeRef::StringLiteral(s) => format!("\"{s}\""),
        TypeRef::IntLiteral(n) => n.to_string(),
        TypeRef::FloatLiteral(f) => f.clone(),
        TypeRef::BoolLiteral(b) => b.to_string(),
        TypeRef::Generic { base, args } => {
            let args_str = args
                .iter()
                .map(|t| type_ref_to_str_impl(t, false))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", type_ref_to_str_impl(base, false), args_str)
        }
        TypeRef::TypeParam(name) => name.to_string(),
        TypeRef::Function { params, ret } => {
            let params_str = params
                .iter()
                .map(|p| {
                    let ty_str = type_ref_to_str_impl(&p.ty, false);
                    if let Some(name) = &p.name {
                        format!("{name}: {ty_str}")
                    } else {
                        ty_str
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) -> {}", params_str, type_ref_to_str_impl(ret, false))
        }
        TypeRef::Error => "<error>".to_string(),
        TypeRef::Unknown => "<unknown>".to_string(),
        TypeRef::BuiltinUnknown => "unknown".to_string(),
    }
}

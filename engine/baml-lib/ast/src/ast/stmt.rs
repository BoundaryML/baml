use std::fmt;

use super::{Expression, ExpressionBlock, Identifier, Span};

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub identifier: Identifier,
    pub is_mutable: bool,
    pub expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssignStmt {
    pub identifier: Identifier,
    pub expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssignOpStmt {
    pub identifier: Identifier,
    pub assign_op: AssignOp,
    pub expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum AssignOp {
    /// The `+=` operator (addition)
    AddAssign,
    /// The `-=` operator (subtraction)
    SubAssign,
    /// The `*=` operator (multiplication)
    MulAssign,
    /// The `/=` operator (division)
    DivAssign,
    /// The `%=` operator (modulus)
    ModAssign,
    /// The `^=` operator (bitwise xor)
    BitXorAssign,
    /// The `&=` operator (bitwise and)
    BitAndAssign,
    /// The `|=` operator (bitwise or)
    BitOrAssign,
    /// The `<<=` operator (shift left)
    ShlAssign,
    /// The `>>=` operator (shift right)
    ShrAssign,
}

#[derive(Debug, Clone)]
pub struct ForLoopStmt {
    pub identifier: Identifier,
    pub iterator: Expression,
    pub body: ExpressionBlock,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expression,
    pub body: ExpressionBlock,
    pub span: Span,
}

// Stmt(statements) perform actions and not often return values.
#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    ForLoop(ForLoopStmt),
    WhileLoop(WhileStmt),
    /// Expression with trailing semicolon.
    Expression(Expression),
    Assign(AssignStmt),
    AssignOp(AssignOpStmt),
    Break(Span),
    Continue(Span),
}

impl fmt::Display for AssignOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AssignOp::AddAssign => "+=",
            AssignOp::SubAssign => "-=",
            AssignOp::MulAssign => "*=",
            AssignOp::DivAssign => "/=",
            AssignOp::ModAssign => "%=",
            AssignOp::BitAndAssign => "&=",
            AssignOp::BitOrAssign => "|=",
            AssignOp::BitXorAssign => "^=",
            AssignOp::ShlAssign => "<<=",
            AssignOp::ShrAssign => ">>=",
        })
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Let(stmt) => write!(f, "let {} = {}", stmt.identifier, stmt.expr),
            Stmt::ForLoop(stmt) => write!(f, "for {} in {}", stmt.identifier, stmt.iterator),
            Stmt::Expression(expr) => fmt::Display::fmt(expr, f),
            Stmt::Assign(stmt) => write!(f, "{} = {}", stmt.identifier, stmt.expr),
            Stmt::AssignOp(stmt) => {
                write!(f, "{} {} {}", stmt.identifier, stmt.assign_op, stmt.expr)
            }
            Stmt::WhileLoop(stmt) => write!(f, "while {} {}", stmt.condition, stmt.body),
            Stmt::Break(_) => f.write_str("break"),
            Stmt::Continue(_) => f.write_str("continue"),
        }
    }
}

impl Stmt {
    pub fn assert_eq_up_to_span(&self, other: &Stmt) {
        match (self, other) {
            (Stmt::Let(stmt1), Stmt::Let(stmt2)) => {
                stmt1.identifier.assert_eq_up_to_span(&stmt2.identifier);
                stmt1.expr.assert_eq_up_to_span(&stmt2.expr);
            }
            (Stmt::ForLoop(stmt1), Stmt::ForLoop(stmt2)) => {
                stmt1.identifier.assert_eq_up_to_span(&stmt2.identifier);
                stmt1.iterator.assert_eq_up_to_span(&stmt2.iterator);
                stmt1.body.assert_eq_up_to_span(&stmt2.body);
            }
            (Stmt::Expression(expr1), Stmt::Expression(expr2)) => {
                expr1.assert_eq_up_to_span(expr2);
            }

            (Stmt::Assign(stmt1), Stmt::Assign(stmt2)) => {
                stmt1.identifier.assert_eq_up_to_span(&stmt2.identifier);
                stmt1.expr.assert_eq_up_to_span(&stmt2.expr);
            }
            (_, _) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
        }
    }

    pub fn identifier(&self) -> &Identifier {
        match self {
            Stmt::Let(stmt) => &stmt.identifier,
            Stmt::ForLoop(stmt) => &stmt.identifier,
            Stmt::Expression(expr) => panic!("expressions don't have identifiers"),
            Stmt::WhileLoop(expr) => panic!("while loops don't have identifiers"),
            Stmt::Break(_) => panic!("break statements don't have identifiers"),
            Stmt::Continue(_) => panic!("continue statements don't have identifiers"),
            Stmt::Assign(stmt) => &stmt.identifier,
            Stmt::AssignOp(stmt) => &stmt.identifier,
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Stmt::Let(stmt) => &stmt.span,
            Stmt::ForLoop(stmt) => &stmt.span,
            Stmt::Expression(expr) => expr.span(),
            Stmt::Assign(stmt) => &stmt.span,
            Stmt::AssignOp(stmt) => &stmt.span,
            Stmt::WhileLoop(stmt) => &stmt.span,
            Stmt::Break(span) | Stmt::Continue(span) => span,
        }
    }

    // TODO: Get rid of this, just match over the type and grab the body.
    pub fn body(&self) -> &Expression {
        match self {
            Stmt::Let(stmt) => &stmt.expr,
            _ => panic!("body() called on non-let statement"),
        }
    }
}

use std::fmt;

use super::{Expression, ExpressionBlock, Identifier, Span};

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub identifier: Identifier,
    pub expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForLoopStmt {
    pub identifier: Identifier,
    pub iterator: Expression,
    pub body: ExpressionBlock,
    pub span: Span,
}

// Stmt(statements) perform actions and not often return values.
#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    ForLoop(ForLoopStmt),
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Let(stmt) => write!(f, "let {} = {}", stmt.identifier, stmt.expr)?,
            Stmt::ForLoop(stmt) => write!(f, "for {} in {}", stmt.identifier, stmt.iterator)?,
        }
        Ok(())
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
            (Stmt::Let(_), Stmt::ForLoop(_)) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
            (Stmt::ForLoop(_), Stmt::Let(_)) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
        }
    }

    pub fn identifier(&self) -> &Identifier {
        match self {
            Stmt::Let(stmt) => &stmt.identifier,
            Stmt::ForLoop(stmt) => &stmt.identifier,
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Stmt::Let(stmt) => &stmt.span,
            Stmt::ForLoop(stmt) => &stmt.span,
        }
    }

    pub fn body(&self) -> &Expression {
        match self {
            Stmt::Let(stmt) => &stmt.expr,
            Stmt::ForLoop(stmt) => &stmt.body.expr,
        }
    }
}

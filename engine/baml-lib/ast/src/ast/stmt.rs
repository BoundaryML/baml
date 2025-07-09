use std::fmt;

use super::{Expression, ExpressionBlock, Identifier, Span};

// Stmt(statements) perform actions and not often return values.
#[derive(Debug, Clone)]
pub enum Stmt {
    Let(Identifier, Expression, Span),
    ForLoop(Identifier, Expression, ExpressionBlock, Span),
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Let(identifier, expr, span) => write!(f, "let {} = {}", identifier, expr)?,
            Stmt::ForLoop(identifier, expr, block, span) => {
                write!(f, "for {} in {}", identifier, expr)?
            }
        }
        Ok(())
    }
}

impl Stmt {
    pub fn assert_eq_up_to_span(&self, other: &Stmt) {
        match (self, other) {
            (Stmt::Let(identifier1, expr1, span1), Stmt::Let(identifier2, expr2, span2)) => {
                identifier1.assert_eq_up_to_span(identifier2);
                expr1.assert_eq_up_to_span(expr2);
            }
            (
                Stmt::ForLoop(identifier1, expr1, block1, span1),
                Stmt::ForLoop(identifier2, expr2, block2, span2),
            ) => {
                identifier1.assert_eq_up_to_span(identifier2);
                expr1.assert_eq_up_to_span(expr2);
                block1.assert_eq_up_to_span(block2);
            }
            (Stmt::Let(_, _, _), Stmt::ForLoop(_, _, _, _)) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
            (Stmt::ForLoop(_, _, _, _), Stmt::Let(_, _, _)) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
        }
    }

    pub fn identifier(&self) -> &Identifier {
        match self {
            Stmt::Let(identifier, _, _) => identifier,
            Stmt::ForLoop(identifier, _, _, _) => identifier,
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Stmt::Let(_, _, span) => span,
            Stmt::ForLoop(_, _, _, span) => span,
        }
    }

    pub fn body(&self) -> &Expression {
        match self {
            Stmt::Let(_, expr, _) => expr,
            Stmt::ForLoop(_, _, block, _) => &block.expr,
        }
    }
}

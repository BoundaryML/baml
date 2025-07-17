use baml_types::expr::Expr;
use internal_baml_ast::ast::{self, ExprFn, TopLevelAssignment, WithName, WithSpan};
use internal_baml_diagnostics::Span;

use super::{ConfigurationWalker, Walker};

/// Walker for top level assignments.
pub type TopLevelAssignmentWalker<'db> = Walker<'db, ast::TopLevelAssignmentId>;

impl<'db> TopLevelAssignmentWalker<'db> {
    /// Returns the name of the top level assignment.
    pub fn name(&self) -> &str {
        self.db.ast[self.id].stmt.identifier.name()
    }

    /// Return the AST node for the top level assignment.
    pub fn top_level_assignment(&self) -> &ast::TopLevelAssignment {
        &self.db.ast[self.id]
    }

    /// Returns the expression of the top level assignment.
    pub fn expr(&self) -> &ast::Expression {
        &self.db.ast[self.id].stmt.expr
    }
}

/// Walker for expression functions.
pub type ExprFnWalker<'db> = Walker<'db, ast::ExprFnId>;

impl<'db> ExprFnWalker<'db> {
    /// Return the name of the function.
    pub fn name(&self) -> &str {
        self.db.ast[self.id].name.name()
    }

    /// Return the span of the name of the function.
    pub fn name_span(&self) -> &Span {
        self.db.ast[self.id].name.span()
    }

    /// Return the AST node for the function.
    pub fn expr_fn(&self) -> &ast::ExprFn {
        &self.db.ast[self.id]
    }

    /// Return the arguments of the function.
    pub fn args(&self) -> &ast::BlockArgs {
        &self.db.ast[self.id].args
    }

    /// All the test cases for this function.
    pub fn walk_tests(self) -> impl ExactSizeIterator<Item = ConfigurationWalker<'db>> {
        let mut tests = self
            .db
            .walk_test_cases()
            .filter(|w| w.test_case().functions.iter().any(|f| f.0 == self.name()))
            .collect::<Vec<_>>();

        // log::debug!("Found {} tests for function {}", tests.len(), self.name());

        tests.sort_by(|a, b| a.name().cmp(b.name()));

        tests.into_iter()
    }
}

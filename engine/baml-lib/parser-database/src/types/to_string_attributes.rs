use crate::interner::StringId;
use internal_baml_schema_ast::ast::Expression;

///
#[derive(Debug)]
pub enum ToStringAttributes {
    ///
    Static(StaticStringAttributes),
}

///
#[derive(Debug, Default)]
pub struct StaticStringAttributes {
    description: Option<Expression>,
    alias: Option<StringId>, // TODO: This should be an Expression.
    asserts: Vec<(String, Expression)>,
    checks: Vec<(String, Expression)>,
}

// TODO: (Greg) replace getters/setters with public fields,
// because getters/setters aren't adding any logic.
impl StaticStringAttributes {
    /// Set a description.
    pub fn add_description(&mut self, description: Expression) {
        self.description.replace(description);
    }

    /// Get the description.
    pub fn description(&self) -> &Option<Expression> {
        &self.description
    }

    /// Set an alias.
    pub fn add_alias(&mut self, alias: StringId) {
        self.alias.replace(alias);
    }

    /// Get the alias.
    pub fn alias(&self) -> &Option<StringId> {
        &self.alias
    }

    /// Add an assert.
    pub fn add_assert(&mut self, assert_name: String, assert: Expression) {
        let _ = &self.asserts.push((assert_name, assert));
    }

    /// Get the asserts.
    pub fn asserts(&self) -> &[(String, Expression)] {
        self.asserts.as_ref()
    }

    /// Add an assert.
    pub fn add_check(&mut self, check_name: String, check: Expression) {
        let _ = &self.checks.push((check_name, check));
    }

    /// Get the asserts.
    pub fn checks(&self) -> &[(String, Expression)] {
        self.checks.as_ref()
    }
}

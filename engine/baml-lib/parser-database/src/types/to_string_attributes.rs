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
    alias: Option<StringId>, // TODO: This should be a LazyExpression.
    dynamic_type: Option<bool>,
}

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

    /// Get dynamism of type.
    pub fn dynamic_type(&self) -> &Option<bool> {
        &self.dynamic_type
    }

    /// Set dynamism of type.
    pub fn set_dynamic_type(&mut self) {
        self.dynamic_type.replace(true);
    }
}

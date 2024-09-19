use crate::types::{DynamicStringAttributes, StaticStringAttributes, ToStringAttributes};

use super::{ClassWalker, Walker};

use internal_baml_schema_ast::ast::{self, FieldType, Identifier, WithName, WithSpan};

/// A model field, scalar or relation.
pub type FieldWalker<'db> = Walker<'db, (ast::TypeExpId, ast::FieldId, bool)>;

impl<'db> FieldWalker<'db> {
    /// The AST node for the field.
    pub fn ast_field(self) -> &'db ast::Field<FieldType> {
        &self.db.ast[self.id.0][self.id.1]
    }

    /// The field type.
    pub fn r#type(self) -> &'db Option<FieldType> {
        &self.ast_field().expr
    }

    /// Traverse the field's parent model.
    pub fn model(self) -> ClassWalker<'db> {
        self.walk(self.id.0)
    }

    /// Traverse the field's attributes.
    pub fn attributes(self) -> &'db ToStringAttributes {
        &self.db.types.class_attributes[&self.id.0].field_serilizers[&self.id.1]
    }

    /// Whether the field is dynamic.
    pub fn is_dynamic(self) -> bool {
        self.id.2
    }

    /// Attributes for the field.
    pub fn static_attributes(self) -> &'db StaticStringAttributes {
        match self.attributes() {
            ToStringAttributes::Static(d) => d,
            _ => panic!("Expected static attributes"),
        }
    }

    /// Attributes for the field.
    pub fn dynamic_attributes(self) -> &'db DynamicStringAttributes {
        match self.attributes() {
            ToStringAttributes::Dynamic(d) => d,
            _ => panic!("Expected dynamic attributes"),
        }
    }

    /// The field's alias.
    pub fn code_for_language(self, language: &str) -> Option<&'db str> {
        match self.db.interner.lookup(language) {
            Some(language) => self
                .dynamic_attributes()
                .code
                .get(&language)
                .and_then(|&s| self.db.interner.get(s)),
            None => None,
        }
    }

    /// The field's default attributes.
    pub fn get_default_attributes(&self) -> Option<&'db ToStringAttributes> {
        let result = self
            .db
            .types
            .class_attributes
            .get(&self.id.0)
            .and_then(|f| f.field_serilizers.get(&self.id.1));

        result
    }
}

impl<'db> WithName for FieldWalker<'db> {
    /// The field name.
    fn name(&self) -> &'db str {
        self.ast_field().name()
    }
}

impl<'db> WithSpan for FieldWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.ast_field().span()
    }
}

use crate::types::Attributes;

use super::{ClassWalker, Walker};

use internal_baml_schema_ast::ast::{self, FieldType, WithName, WithSpan};

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
    pub fn attributes(self) -> &'db Attributes {
        &self.db.types.class_attributes[&self.id.0].field_serilizers[&self.id.1]
    }

    /// The field's default attributes.
    pub fn get_default_attributes(&self) -> Option<&'db Attributes> {
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

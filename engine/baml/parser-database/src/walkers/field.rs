use crate::types::{ClassAttributes, ToStringAttributes};

use super::{ClassWalker, Walker};
use internal_baml_schema_ast::ast::{self, FieldArity, FieldType};

/// A model field, scalar or relation.
pub type FieldWalker<'db> = Walker<'db, (ast::ClassId, ast::FieldId)>;

impl<'db> FieldWalker<'db> {
    /// The AST node for the field.
    pub fn ast_field(self) -> &'db ast::Field {
        &self.db.ast[self.id.0][self.id.1]
    }

    /// The field name.
    pub fn name(self) -> &'db str {
        self.ast_field().name()
    }

    /// The field type.
    pub fn r#type(self) -> (FieldArity, &'db FieldType) {
        (self.ast_field().arity, &self.ast_field().field_type)
    }

    /// The parsed attributes.
    #[track_caller]
    pub(crate) fn attributes(self) -> &'db ToStringAttributes {
        &self.db.types.class_attributes[&self.id.0].field_serilizers[&self.id.1]
    }

    /// Traverse the field's parent model.
    pub fn model(self) -> ClassWalker<'db> {
        self.walk(self.id.0)
    }
}

use super::{ClassWalker, Walker};
use internal_baml_schema_ast::ast;

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

    /// Traverse the field's parent model.
    pub fn model(self) -> ClassWalker<'db> {
        self.walk(self.id.0)
    }
}

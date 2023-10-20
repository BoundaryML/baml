use crate::{
    ast::{self, WithName},
    types::ClassAttributes,
};

use super::field::FieldWalker;

/// A `class` declaration in the Prisma schema.
pub type ClassWalker<'db> = super::Walker<'db, ast::ClassId>;

impl<'db> ClassWalker<'db> {
    /// The name of the class.
    pub fn name(self) -> &'db str {
        self.ast_class().name()
    }

    /// The ID of the class in the db
    pub fn class_id(self) -> ast::ClassId {
        self.id
    }

    /// The AST node.
    pub fn ast_class(self) -> &'db ast::Class {
        &self.db.ast[self.id]
    }

    /// The parsed attributes.
    #[track_caller]
    pub(crate) fn attributes(self) -> &'db ClassAttributes {
        &self.db.types.class_attributes[&self.id]
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn static_fields(self) -> impl ExactSizeIterator<Item = FieldWalker<'db>> {
        self.ast_class()
            .iter_fields()
            .filter_map(move |(field_id, _)| {
                self.db
                    .types
                    .refine_class_field((self.id, field_id))
                    .left()
                    .map(|_id| self.walk((self.id, field_id)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn dynamic_fields(self) -> impl ExactSizeIterator<Item = FieldWalker<'db>> {
        self.ast_class()
            .iter_fields()
            .filter_map(move |(field_id, _)| {
                self.db
                    .types
                    .refine_class_field((self.id, field_id))
                    .left()
                    .map(|_id| self.walk((self.id, field_id)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}

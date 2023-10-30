use either::Either;

use crate::{
    ast::{self, WithName},
    types::ClassAttributes,
};

use super::{field::FieldWalker, EnumWalker};

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
    pub fn attributes(self) -> &'db ClassAttributes {
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

    /// Find all enums used by this class and any of its fields.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        self.static_fields()
            .flat_map(|f| f.r#type().flat_idns())
            .flat_map(|idn| match self.db.find_type(idn) {
                Some(Either::Right(walker)) => vec![walker],
                Some(Either::Left(walker)) => walker.required_enums().collect(),
                None => vec![],
            })
    }

    /// Find all classes used by this class and any of its fields.
    pub fn required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        self.static_fields()
            .flat_map(|f| f.r#type().flat_idns())
            .flat_map(|idn| match self.db.find_type(idn) {
                Some(Either::Left(walker)) => {
                    let mut classes = walker.required_classes().collect::<Vec<_>>();
                    classes.push(walker);
                    classes
                }
                Some(Either::Right(_)) => vec![],
                None => vec![],
            })
    }
}

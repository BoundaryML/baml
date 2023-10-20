use internal_baml_schema_ast::ast::{IndentationType, NewlineType, WithDocumentation, WithName};

use crate::{
    ast,
    types::{EnumAttributes, ToStringAttributes},
    walkers::Walker,
};

/// An `enum` declaration in the schema.
pub type EnumWalker<'db> = Walker<'db, ast::EnumId>;
/// One value in an `enum` declaration in the schema.
pub type EnumValueWalker<'db> = Walker<'db, (ast::EnumId, ast::EnumValueId)>;

impl<'db> EnumWalker<'db> {
    /// The name of the enum.
    pub fn name(self) -> &'db str {
        &self.ast_enum().name.name
    }

    /// The AST node.
    pub fn ast_enum(self) -> &'db ast::Enum {
        &self.db.ast()[self.id]
    }

    /// The values of the enum.
    pub fn values(self) -> impl ExactSizeIterator<Item = EnumValueWalker<'db>> {
        self.ast_enum()
            .iter_values()
            .filter_map(move |(valid_id, _)| {
                self.db
                    .types
                    .refine_enum_value((self.id, valid_id))
                    .left()
                    .map(|_id| self.walk((self.id, valid_id)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// How fields are indented in the enum.
    pub fn indentation(self) -> IndentationType {
        IndentationType::default()
    }

    /// What kind of newlines the enum uses.
    pub fn newline(self) -> NewlineType {
        NewlineType::Unix
    }

    /// The parsed attributes.
    #[track_caller]
    pub(crate) fn attributes(self) -> &'db EnumAttributes {
        &self.db.types.enum_attributes[&self.id]
    }
}

impl<'db> EnumValueWalker<'db> {
    fn r#enum(self) -> EnumWalker<'db> {
        self.walk(self.id.0)
    }

    /// The enum documentation
    pub fn documentation(self) -> Option<&'db str> {
        self.r#enum().ast_enum()[self.id.1].documentation()
    }

    /// The name of the value.
    pub fn name(self) -> &'db str {
        &self.r#enum().ast_enum()[self.id.1].name()
    }

    /// The parsed attributes.
    #[track_caller]
    pub(crate) fn attributes(self) -> &'db ToStringAttributes {
        &self.db.types.enum_attributes[&self.id.0].value_serilizers[&self.id.1]
    }
}

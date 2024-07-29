use internal_baml_diagnostics::DatamodelError;

use internal_baml_schema_ast::ast::{WithDocumentation, WithIdentifier, WithName, WithSpan};
use serde_json::json;

use crate::{
    ast,
    printer::{serialize_with_printer, WithSerialize, WithSerializeableContent},
    types::ToStringAttributes,
    walkers::Walker,
    ParserDatabase,
};

/// An `enum` declaration in the schema.
pub type EnumWalker<'db> = Walker<'db, ast::TypeExpId>;
/// One value in an `enum` declaration in the schema.
pub type EnumValueWalker<'db> = Walker<'db, (ast::TypeExpId, ast::FieldId)>;

impl<'db> EnumWalker<'db> {
    /// The name of the enum.

    /// The AST node.
    pub fn ast_enum(self) -> &'db ast::TypeExpressionBlock {
        &self.db.ast()[self.id]
    }

    /// The values of the enum.
    pub fn values(self) -> impl ExactSizeIterator<Item = EnumValueWalker<'db>> {
        self.ast_enum()
            .iter_fields()
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

    /// Find a value by name.
    pub fn find_value(&self, name: &str) -> Option<EnumValueWalker<'db>> {
        self.ast_enum()
            .fields
            .iter()
            .enumerate()
            .find_map(|(idx, v)| {
                if v.name() == name {
                    Some(self.walk((self.id, ast::FieldId(idx as u32))))
                } else {
                    None
                }
            })
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
}

impl<'db> WithSpan for EnumValueWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        &self.r#enum().ast_enum()[self.id.1].span()
    }
}

impl<'db> WithName for EnumValueWalker<'db> {
    fn name(&self) -> &str {
        self.r#enum().ast_enum()[self.id.1].name()
    }
}

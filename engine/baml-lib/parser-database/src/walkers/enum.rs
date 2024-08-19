use crate::{ast, types::ToStringAttributes, walkers::Walker};

use internal_baml_schema_ast::ast::{WithDocumentation, WithName, WithSpan};
/// An `enum` declaration in the schema.
pub type EnumWalker<'db> = Walker<'db, ast::TypeExpId>;
/// One value in an `enum` declaration in the schema.
pub type EnumValueWalker<'db> = Walker<'db, (ast::TypeExpId, ast::FieldId)>;

impl<'db> EnumWalker<'db> {
    /// The name of the enum.

    /// The values of the enum.
    pub fn values(self) -> impl ExactSizeIterator<Item = EnumValueWalker<'db>> {
        self.ast_type_block()
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
        self.ast_type_block()
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
        self.r#enum().ast_type_block()[self.id.1].documentation()
    }

    /// The enum value attributes.
    pub fn get_default_attributes(&self) -> Option<&'db ToStringAttributes> {
        let result = self
            .db
            .types
            .enum_attributes
            .get(&self.id.0)
            .and_then(|f| f.value_serilizers.get(&self.id.1));

        result
    }
}

impl<'db> WithSpan for EnumValueWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        &self.r#enum().ast_type_block()[self.id.1].span()
    }
}

impl<'db> WithName for EnumValueWalker<'db> {
    fn name(&self) -> &str {
        self.r#enum().ast_type_block()[self.id.1].name()
    }
}

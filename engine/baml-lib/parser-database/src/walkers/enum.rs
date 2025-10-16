use internal_baml_ast::ast::{WithDocumentation, WithName, WithSpan};

use crate::{ast, types::Attributes, walkers::Walker};
/// An `enum` declaration in the schema.
pub type EnumWalker<'db> = Walker<'db, ast::TypeExpId>;
/// One value in an `enum` declaration in the schema.
pub type EnumValueWalker<'db> = Walker<'db, (ast::TypeExpId, ast::FieldId)>;

impl<'db> EnumWalker<'db> {
    /// The values of the enum.
    pub fn values(self) -> impl ExactSizeIterator<Item = EnumValueWalker<'db>> {
        self.ast_type_block()
            .iter_fields()
            .map(move |(field_id, _)| self.walk((self.id, field_id)))
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

    /// For some reason this has a symbol naming conflict with ClassWalker::add_to_types
    /// so we name it differently here.
    pub fn add_enums_to_types(self, types: &mut internal_baml_jinja_types::PredefinedTypes) {
        let values = self
            .values()
            .map(|v| {
                let alias = v
                    .get_default_attributes()
                    .and_then(|attrs| attrs.alias().as_ref())
                    .and_then(|unresolved| {
                        // For now, we'll extract simple string values
                        // TODO: This needs proper EvaluationContext to resolve template expressions
                        extract_simple_string_value(unresolved)
                    });

                internal_baml_jinja_types::EnumValueDefinition {
                    name: v.name().to_string(),
                    alias,
                }
            })
            .collect();

        types.add_enum_with_metadata(self.name(), values);
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
    pub fn get_default_attributes(&self) -> Option<&'db Attributes> {
        let result = self
            .db
            .types
            .enum_attributes
            .get(&self.id.0)
            .and_then(|f| f.value_serializers.get(&self.id.1));

        result
    }
}

impl<'db> WithSpan for EnumValueWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.r#enum().ast_type_block()[self.id.1].span()
    }
}

impl<'db> WithName for EnumValueWalker<'db> {
    fn name(&self) -> &str {
        self.r#enum().ast_type_block()[self.id.1].name()
    }
}

fn extract_simple_string_value(
    unresolved: &baml_types::UnresolvedValue<internal_baml_diagnostics::Span>,
) -> Option<String> {
    // TODO: This is a temporary solution until we have proper EvaluationContext access
    // For now, we'll only extract simple string literals that don't require template resolution
    match unresolved {
        baml_types::UnresolvedValue::String(baml_types::StringOr::Value(s), _) => Some(s.clone()),
        _ => None, // Skip complex template expressions for now
    }
}

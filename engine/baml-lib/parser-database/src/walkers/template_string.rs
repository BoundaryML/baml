use either::Either;
use internal_baml_jinja::{PredefinedTypes, Type};
use internal_baml_schema_ast::ast::{self, FunctionArgs, Span, WithIdentifier, WithName, WithSpan};

use crate::types::TemplateStringProperties;

use super::Walker;

/// An `enum` declaration in the schema.
pub type TemplateStringWalker<'db> = Walker<'db, ast::TemplateStringId>;

impl<'db> TemplateStringWalker<'db> {
    /// The AST node.
    pub fn ast_node(self) -> &'db ast::TemplateString {
        &self.db.ast()[self.id]
    }

    fn metadata(self) -> &'db TemplateStringProperties {
        &self.db.types.template_strings[&Either::Left(self.id)]
    }

    /// The value of the template string.
    pub fn template_raw(self) -> Option<&'db ast::RawString> {
        self.ast_node().value.as_raw_string_value()
    }

    /// Dedented and trimmed template string.
    pub fn template_string(self) -> &'db str {
        &self.metadata().template
    }

    /// The name of the template string.
    pub fn add_to_types(self, types: &mut PredefinedTypes) {
        let name = self.name();
        let ret_type = Type::String;
        let mut params = vec![];

        if let Some(FunctionArgs::Named(p)) = self.ast_node().input() {
            p.args.iter().for_each(|(name, t)| {
                params.push((
                    name.name().to_string(),
                    self.db.to_jinja_type(&t.field_type),
                ))
            });
        }

        types.add_function(name, ret_type, params);
    }
}

impl WithIdentifier for TemplateStringWalker<'_> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_node().identifier()
    }
}

impl<'a> WithSpan for TemplateStringWalker<'a> {
    fn span(&self) -> &Span {
        self.ast_node().span()
    }
}

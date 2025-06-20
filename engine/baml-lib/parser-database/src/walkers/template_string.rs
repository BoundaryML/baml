use either::Either;
use internal_baml_ast::ast::{
    self, ArgumentId, BlockArgs, Span, WithIdentifier, WithName, WithSpan,
};
use internal_baml_jinja_types::{PredefinedTypes, Type};

use super::Walker;
use crate::types::TemplateStringProperties;

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

    /// Walk the input arguments of the template string.
    pub fn walk_input_args(self) -> impl ExactSizeIterator<Item = ArgWalker<'db>> {
        match self.ast_node().input() {
            Some(input) => {
                let range_end = input.iter_args().len() as u32;
                (0..range_end)
                    .map(move |f| ArgWalker {
                        db: self.db,
                        id: (self.id, ArgumentId(f)),
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
            }
            None => Vec::new().into_iter(),
        }
    }

    /// The name of the template string.
    pub fn add_to_types(self, types: &mut PredefinedTypes) {
        let name = self.name();
        let ret_type = Type::String;
        let mut params = vec![];

        if let Some(p) = self.ast_node().input() {
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

pub type ArgWalker<'db> = super::Walker<'db, (ast::TemplateStringId, ArgumentId)>;

impl<'db> ArgWalker<'db> {
    /// The ID of the function in the db
    pub fn block_id(self) -> ast::TemplateStringId {
        self.id.0
    }

    /// The AST node.
    pub fn ast_type_block(self) -> &'db ast::TemplateString {
        &self.db.ast[self.id.0]
    }

    /// The AST node.
    pub fn ast_arg(self) -> (Option<&'db ast::Identifier>, &'db ast::BlockArg) {
        let args = self.ast_type_block().input();
        let res: &_ = &args.expect("Expected input args")[self.id.1];
        (Some(&res.0), &res.1)
    }

    /// The name of the type.
    pub fn field_type(self) -> &'db ast::FieldType {
        &self.ast_arg().1.field_type
    }

    /// The name of the function.
    pub fn is_optional(self) -> bool {
        self.field_type().is_optional()
    }
}

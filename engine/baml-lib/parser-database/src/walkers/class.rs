use std::collections::HashSet;

use super::{field::FieldWalker, EnumWalker};
use crate::types::ToStringAttributes;
use either::Either;
use internal_baml_schema_ast::ast::Identifier;
use internal_baml_schema_ast::ast::SubType;
use internal_baml_schema_ast::ast::{self, ArgumentId, WithIdentifier, WithName, WithSpan};
use std::collections::HashMap;

/// A `class` declaration in the Prisma schema.
pub type ClassWalker<'db> = super::Walker<'db, ast::TypeExpId>;

impl<'db> ClassWalker<'db> {
    /// The ID of the class in the db
    pub fn class_id(self) -> ast::TypeExpId {
        self.id
    }

    /// The AST node.
    pub fn ast_type_block(self) -> &'db ast::TypeExpressionBlock {
        &self.db.ast[self.id]
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn static_fields(self) -> impl ExactSizeIterator<Item = FieldWalker<'db>> {
        self.ast_type_block()
            .iter_fields()
            .filter_map(move |(field_id, _)| {
                self.db
                    .types
                    .refine_class_field((self.id, field_id))
                    .left()
                    .map(|_id| self.walk((self.id, field_id, false)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn dynamic_fields(self) -> impl ExactSizeIterator<Item = FieldWalker<'db>> {
        self.ast_type_block()
            .iter_fields()
            .filter_map(move |(field_id, _)| {
                self.db
                    .types
                    .refine_class_field((self.id, field_id))
                    .right()
                    .map(|_id| self.walk((self.id, field_id, true)))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Iterate all the scalar fields in a given class in the order they were defined.
    pub fn dependencies(self) -> &'db HashSet<String> {
        {
            let dependencies = &self.db.types.class_dependencies[&self.id];

            dependencies
        }
    }

    /// Find all enums used by this class and any of its fields.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        self.db.types.class_dependencies[&self.class_id()]
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(_cls)) => None,
                Some(Either::Right(walker)) => Some(walker),
                None => None,
            })
    }

    /// Find all classes used by this class and any of its fields.
    pub fn required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        self.db.types.class_dependencies[&self.class_id()]
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(walker)) => Some(walker),
                Some(Either::Right(_enm)) => None,
                None => None,
            })
    }

    /// The name of the template string.
    pub fn add_to_types(self, types: &mut internal_baml_jinja::PredefinedTypes) {
        types.add_class(
            self.name(),
            self.static_fields()
                .filter_map(|f| {
                    f.r#type()
                        .as_ref()
                        .map(|field_type| (f.name().to_string(), self.db.to_jinja_type(field_type)))
                })
                .collect::<HashMap<_, _>>(),
        )
    }
    /// Getter for default attributes
    pub fn get_default_attributes(&self, sub_type: SubType) -> Option<&'db ToStringAttributes> {
        match sub_type {
            SubType::Enum => self
                .db
                .types
                .enum_attributes
                .get(&self.id)
                .and_then(|f| f.serilizer.as_ref()),
            _ => self
                .db
                .types
                .class_attributes
                .get(&self.id)
                .and_then(|f| f.serilizer.as_ref()),
        }
    }

    /// Arguments of the function.
    pub fn find_input_arg_by_name(self, name: &str) -> Option<ArgWalker<'db>> {
        self.ast_type_block().input().and_then(|args| {
            args.iter_args().find_map(|(idx, (idn, _))| {
                if idn.name() == name {
                    Some(ArgWalker {
                        db: self.db,
                        id: (self.id, true, idx),
                    })
                } else {
                    None
                }
            })
        })
    }

    /// Iterates over the input arguments of the function.
    pub fn walk_input_args(self) -> impl ExactSizeIterator<Item = ArgWalker<'db>> {
        match self.ast_type_block().input() {
            Some(input) => {
                let range_end = input.iter_args().len() as u32;
                (0..range_end)
                    .map(move |f| ArgWalker {
                        db: self.db,
                        id: (self.id, true, ArgumentId(f)),
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
            }
            None => Vec::new().into_iter(),
        }
    }
}
pub type ArgWalker<'db> = super::Walker<'db, (ast::TypeExpId, bool, ArgumentId)>;

impl<'db> ArgWalker<'db> {
    /// The ID of the function in the db
    pub fn block_id(self) -> ast::TypeExpId {
        self.id.0
    }

    /// The AST node.
    pub fn ast_type_block(self) -> &'db ast::TypeExpressionBlock {
        &self.db.ast[self.id.0]
    }

    /// The AST node.
    pub fn ast_arg(self) -> (Option<&'db Identifier>, &'db ast::BlockArg) {
        let args = self.ast_type_block().input();
        let res: &_ = &args.expect("Expected input args")[self.id.2];
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

    /// The name of the function.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        let input = &self.db.types.class_dependencies[&self.block_id()];
        input
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(_cls)) => None,
                Some(Either::Right(walker)) => Some(walker),
                None => None,
            })
    }

    /// The name of the function.
    pub fn required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        let input = &self.db.types.class_dependencies[&self.block_id()];
        input
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(walker)) => Some(walker),
                Some(Either::Right(_enm)) => None,
                None => None,
            })
    }
}
impl<'db> WithIdentifier for ClassWalker<'db> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_type_block().identifier()
    }
}

impl<'db> WithSpan for ClassWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.ast_type_block().span()
    }
}

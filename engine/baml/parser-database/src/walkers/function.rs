use internal_baml_schema_ast::ast::{
    ArguementId, FuncArguementId, Identifier, Variant, VariantConfigId,
};
use log::info;

use crate::ast::{self, WithName};

use super::{ClassWalker, EnumWalker, VariantWalker, Walker};

use std::iter::ExactSizeIterator;

/// A `function` declaration in the Prisma schema.
pub type FunctionWalker<'db> = Walker<'db, ast::FunctionId>;

impl<'db> FunctionWalker<'db> {
    /// The name of the function.
    pub fn name(self) -> &'db str {
        self.ast_function().name()
    }

    /// The ID of the function in the db
    pub fn function_id(self) -> ast::FunctionId {
        self.id
    }

    /// The AST node.
    pub fn ast_function(self) -> &'db ast::Function {
        &self.db.ast[self.id]
    }

    /// The name of the function.
    pub fn is_positional_args(self) -> bool {
        match self.ast_function().input() {
            ast::FunctionArgs::Named(_) => false,
            ast::FunctionArgs::Unnamed(_) => true,
        }
    }

    /// Iterates over the input arguments of the function.
    pub fn walk_input_args(self) -> impl ExactSizeIterator<Item = ArgWalker<'db>> {
        let range_end = match self.ast_function().input() {
            ast::FunctionArgs::Named(arg_list) => arg_list.iter_args().len(),
            ast::FunctionArgs::Unnamed(_) => 1,
        } as u32;

        (0..range_end).map(move |f| ArgWalker {
            db: self.db,
            id: (self.id, true, FuncArguementId(f)),
        })
    }

    /// Iterates over the output arguments of the function.
    pub fn walk_output_args(self) -> impl ExactSizeIterator<Item = ArgWalker<'db>> {
        let range_end = match self.ast_function().output() {
            ast::FunctionArgs::Named(arg_list) => arg_list.iter_args().len(),
            ast::FunctionArgs::Unnamed(_) => 1,
        } as u32;

        (0..range_end).map(move |f| ArgWalker {
            db: self.db,
            id: (self.id, false, FuncArguementId(f)),
        })
    }

    /// Iterates over the variants for this function.
    pub fn walk_variants(self) -> impl ExactSizeIterator<Item = VariantWalker<'db>> {
        self.db
            .ast()
            .iter_tops()
            .filter_map(|(id, t)| match (id, t) {
                (ast::TopId::Variant(id), ast::Top::Variant(impl_))
                    if impl_.function_name() == self.name() =>
                {
                    Some(VariantWalker {
                        db: self.db,
                        id: id,
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}

/// A `function` declaration in the Prisma schema.
pub type ArgWalker<'db> = super::Walker<'db, (ast::FunctionId, bool, FuncArguementId)>;

impl<'db> ArgWalker<'db> {
    /// The name of the function.
    pub fn name(self) -> &'db str {
        self.ast_function().name()
    }

    /// The ID of the function in the db
    pub fn function_id(self) -> ast::FunctionId {
        self.id.0
    }

    /// The AST node.
    pub fn ast_function(self) -> &'db ast::Function {
        &self.db.ast[self.id.0]
    }

    /// The AST node.
    pub fn ast_arg(self) -> (Option<&'db Identifier>, &'db ast::FunctionArg) {
        let args = match self.id.1 {
            true => self.ast_function().input(),
            false => self.ast_function().output(),
        };
        match args {
            ast::FunctionArgs::Named(arg_list) => {
                let res = &arg_list[self.id.2];
                (Some(&res.0), &res.1)
            }
            ast::FunctionArgs::Unnamed(arg) => (None, arg),
        }
    }

    /// The name of the function.
    pub fn is_optional(self) -> bool {
        let (_, arg) = self.ast_arg();
        arg.arity.is_optional() || arg.field_type.is_optional()
    }

    /// The name of the function.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        let (_, arg) = self.ast_arg();
        arg.field_type
            .flat_idns()
            .into_iter()
            .filter_map(|idn| self.db.find_enum(&idn.name))
    }

    /// The name of the function.
    pub fn required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        let (_, arg) = self.ast_arg();
        arg.field_type
            .flat_idns()
            .into_iter()
            .filter_map(|idn| self.db.find_class(&idn.name))
    }
}

use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::WithSpan;
use internal_baml_schema_ast::ast::{FuncArguementId, Identifier};
use serde_json::json;

use crate::{
    ast::{self, WithName},
    printer::{serialize_with_printer, WithSerializeableContent},
    WithSerialize,
};

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
                    if impl_.function_name().name() == self.name() =>
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
        arg.field_type.is_nullable()
    }

    /// The name of the function.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        let (input, output) = &self.db.types.function_dependencies[&self.function_id()];
        if self.id.1 { input } else { output }
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(_cls)) => None,
                Some(Either::Right(walker)) => Some(walker),
                None => None,
            })
    }

    /// The name of the function.
    pub fn required_classes(self) -> impl Iterator<Item = ClassWalker<'db>> {
        let (input, output) = &self.db.types.function_dependencies[&self.function_id()];
        if self.id.1 { input } else { output }
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(walker)) => Some(walker),
                Some(Either::Right(_enm)) => None,
                None => None,
            })
    }
}

impl<'db> WithSerializeableContent for ArgWalker<'db> {
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value {
        json!({
            "rtype": "inline",
            "value": (self.db, &self.ast_arg().1.field_type).serialize_data(variant)
        })
    }
}

impl<'db> WithSerializeableContent for FunctionWalker<'db> {
    fn serialize_data(&self, variant: &VariantWalker<'_>) -> serde_json::Value {
        // TODO: We should handle the case of multiple output args
        json!({
            "rtype": "output",
            "value": self.walk_output_args()
                        .map(|f| f.serialize_data(variant))
                        .next()
                        .unwrap_or(serde_json::Value::Null)
        })
    }
}

impl<'db> WithSerialize for FunctionWalker<'db> {
    fn serialize(
        &self,
        variant: &VariantWalker<'_>,
        block: &internal_baml_prompt_parser::ast::PrinterBlock,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        // Eventually we should validate what parameters are in meta.
        match serialize_with_printer(
            false,
            self.db.find_printer(&block.printer),
            self.serialize_data(variant),
        ) {
            Ok(val) => Ok(val),
            Err(e) => Err(DatamodelError::new_validation_error(
                &format!("Error serializing output for {}\n{}", self.name(), e),
                block.span().clone(),
            )),
        }
    }
}

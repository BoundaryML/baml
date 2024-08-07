use super::{ClassWalker, ClientWalker, ConfigurationWalker, EnumWalker, Walker};
use crate::{
    ast::{self, WithName},
    printer::{serialize_with_printer, WithSerializeableContent},
    types::FunctionType,
    ParserDatabase, WithSerialize,
};
use either::Either;
use internal_baml_schema_ast::ast::{ArgumentId, Identifier, ValExpId, WithIdentifier, WithSpan};

pub type ArgWalker<'db> = super::Walker<'db, (ValExpId, bool, ArgumentId)>;

impl<'db> ArgWalker<'db> {
    /// The ID of the function in the db
    pub fn function_id(self) -> ast::ValExpId {
        self.id.0
    }

    /// The AST node.
    pub fn ast_function(self) -> &'db ast::ValueExprBlock {
        &self.db.ast[self.id.0]
    }

    /// The AST node.
    pub fn ast_arg(self) -> (Option<&'db Identifier>, &'db ast::BlockArg) {
        match self.id.1 {
            true => {
                let args = self.ast_function().input();
                let res = &args.expect("Expected input args")[self.id.2];
                (Some(&res.0), &res.1)
            }

            false => {
                let output = self.ast_function().output();
                let res = output.expect("Error: Output is undefined for function ID");
                (None, res)
            }
        }
    }

    /// The name of the type.
    pub fn field_type(self) -> &'db ast::FieldType {
        &self.ast_arg().1.field_type
    }

    /// The name of the function.
    pub fn is_optional(self) -> bool {
        self.field_type().is_nullable()
    }

    /// The name of the function.
    pub fn required_enums(self) -> impl Iterator<Item = EnumWalker<'db>> {
        let (input, output) = &self.db.types.function[&self.function_id()].dependencies;
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
        let (input, output) = &self.db.types.function[&self.function_id()].dependencies;
        if self.id.1 { input } else { output }
            .iter()
            .filter_map(|f| match self.db.find_type_by_str(f) {
                Some(Either::Left(walker)) => Some(walker),
                Some(Either::Right(_enm)) => None,
                None => None,
            })
    }
}

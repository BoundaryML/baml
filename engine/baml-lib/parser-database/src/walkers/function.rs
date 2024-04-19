use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{FuncArguementId, Identifier, WithIdentifier, WithSpan};
use serde_json::json;

use crate::{
    ast::{self, WithName},
    printer::{serialize_with_printer, WithSerializeableContent},
    types::FunctionType,
    ParserDatabase, WithSerialize,
};

use super::{ClassWalker, ClientWalker, ConfigurationWalker, EnumWalker, VariantWalker, Walker};

use std::{collections::HashMap, iter::ExactSizeIterator};

/// A `function` declaration in the Prisma schema.
pub type FunctionWalker<'db> = Walker<'db, (bool, ast::FunctionId)>;

impl<'db> FunctionWalker<'db> {
    /// The name of the function.
    pub fn name(self) -> &'db str {
        self.ast_function().name()
    }

    /// The name of the function.
    pub fn identifier(self) -> &'db Identifier {
        self.ast_function().identifier()
    }

    /// The ID of the function in the db
    pub fn function_id(self) -> ast::FunctionId {
        self.id.1
    }

    /// The AST node.
    pub fn ast_function(self) -> &'db ast::Function {
        &self.db.ast[self.id.1]
    }

    /// The name of the function.
    pub fn is_positional_args(self) -> bool {
        match self.ast_function().input() {
            ast::FunctionArgs::Named(_) => false,
            ast::FunctionArgs::Unnamed(_) => true,
        }
    }

    /// Arguments of the function.
    pub fn find_input_arg_by_name(self, name: &str) -> Option<ArgWalker<'db>> {
        match self.ast_function().input() {
            ast::FunctionArgs::Named(arg_list) => {
                arg_list.iter_args().find_map(|(idx, (idn, _))| {
                    if idn.name() == name {
                        Some(ArgWalker {
                            db: self.db,
                            id: (self.id.1, true, idx),
                        })
                    } else {
                        None
                    }
                })
            }
            ast::FunctionArgs::Unnamed(_) => None,
        }
    }

    /// Arguments of the function.
    pub fn find_input_arg_by_position(self, position: u32) -> Option<ArgWalker<'db>> {
        match self.ast_function().input() {
            ast::FunctionArgs::Named(_) => None,
            ast::FunctionArgs::Unnamed(_) => {
                if position == 0_u32 {
                    Some(ArgWalker {
                        db: self.db,
                        id: (self.id.1, true, FuncArguementId(position)),
                    })
                } else {
                    None
                }
            }
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
            id: (self.id.1, true, FuncArguementId(f)),
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
            id: (self.id.1, false, FuncArguementId(f)),
        })
    }

    /// Iterates over the variants for this function.
    pub fn walk_variants(self) -> impl ExactSizeIterator<Item = VariantWalker<'db>> {
        assert!(!self.id.0, "Only old functions have variants");
        self.db
            .ast()
            .iter_tops()
            .filter_map(|(id, t)| match (id, t) {
                (ast::TopId::Variant(id), ast::Top::Variant(impl_))
                    if impl_.function_name().name() == self.name() =>
                {
                    Some(VariantWalker { db: self.db, id })
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// All the test cases for this function.
    pub fn walk_tests(self) -> impl ExactSizeIterator<Item = ConfigurationWalker<'db>> {
        let mut tests = self
            .db
            .walk_test_cases()
            .filter(|w| w.test_case().function.0 == self.name())
            .collect::<Vec<_>>();

        tests.sort_by(|a, b| a.name().cmp(b.name()));

        tests.into_iter()
    }

    /// Get metadata about the function
    pub fn metadata(self) -> &'db FunctionType {
        &self.db.types.function[&self.function_id()]
    }

    /// Is this function an old version
    pub fn is_old_function(self) -> bool {
        !self.id.0
    }

    /// The prompt for the function
    pub fn jinja_prompt(self) -> &'db str {
        assert!(self.id.0, "Only new functions have prompts");
        self.db.types.template_strings[&Either::Right(self.function_id())]
            .template
            .as_str()
    }

    /// The client for the function
    pub fn client(self) -> Option<ClientWalker<'db>> {
        assert!(self.id.0, "Only new functions have clients");
        let client = self.metadata().client.as_ref()?;
        self.db.find_client(client.0.as_str())
    }
}

/// A `function` declaration in the Prisma schema.
pub type ArgWalker<'db> = super::Walker<'db, (ast::FunctionId, bool, FuncArguementId)>;

impl<'db> ArgWalker<'db> {
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

impl WithSpan for FunctionWalker<'_> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.ast_function().span()
    }
}

impl WithIdentifier for ArgWalker<'_> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_arg().0.unwrap()
    }
}

impl<'db> WithSerializeableContent for ArgWalker<'db> {
    fn serialize_data(
        &self,
        variant: Option<&VariantWalker<'_>>,
        db: &'_ ParserDatabase,
    ) -> serde_json::Value {
        json!({
            "rtype": "inline",
            "value": (self.db, &self.ast_arg().1.field_type).serialize_data(variant, db)
        })
    }
}

impl<'db> WithSerializeableContent for FunctionWalker<'db> {
    fn serialize_data(
        &self,
        variant: Option<&VariantWalker<'_>>,
        db: &'_ ParserDatabase,
    ) -> serde_json::Value {
        if let Some((idx, _)) = variant.and_then(|v| v.properties().output_adapter.as_ref()) {
            let adapter = &variant.unwrap().ast_variant()[*idx];

            return json!({
                "rtype": "output",
                "value": (self.db, &adapter.from).serialize_data(variant, db)
            });
        }

        // TODO: We should handle the case of multiple output args
        json!({
            "rtype": "output",
            "value": self.walk_output_args()
                        .map(|f| f.serialize_data(variant, db))
                        .next()
                        .unwrap_or(serde_json::Value::Null)
        })
    }
}

impl<'db> WithSerialize for FunctionWalker<'db> {
    fn serialize(
        &self,
        db: &'_ ParserDatabase,
        variant: Option<&VariantWalker<'_>>,
        block: Option<&internal_baml_prompt_parser::ast::PrinterBlock>,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        let printer_template = match &block.and_then(|b| b.printer.as_ref()) {
            Some((p, _)) => self
                .db
                .find_printer(p)
                .map(|w| w.printer().template().to_string()),
            _ => None,
        };
        // Eventually we should validate what parameters are in meta.
        match serialize_with_printer(false, printer_template, self.serialize_data(variant, db)) {
            Ok(val) => Ok(val),
            Err(e) => Err(DatamodelError::new_validation_error(
                &format!("Error serializing output for {}\n{}", self.name(), e),
                span.clone(),
            )),
        }
    }

    fn output_format(
        &self,
        db: &'_ ParserDatabase,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        let class_schema = self.serialize(db, None, None, span)?;

        let mut enum_schemas = self
            .walk_output_args()
            .flat_map(|arg| arg.required_enums())
            .map(|e| (e.name().to_string(), e))
            .collect::<HashMap<_, _>>()
            .iter()
            // TODO(sam) - if enum serialization fails, then we do not surface the error to the user.
            // That is bad!!!!!!!
            .filter_map(
                |(_, e)| match e.serialize(db, None, None, e.identifier().span()) {
                    Ok(enum_schema) => Some((e.name().to_string(), enum_schema)),
                    Err(_) => None,
                },
            )
            .collect::<Vec<_>>();

        if enum_schemas.is_empty() {
            Ok(class_schema)
        } else {
            // Enforce a stable order on enum schemas. Without this, the order is actually unstable, and the order can ping-pong
            // when the vscode ext re-renders the live preview
            enum_schemas.sort_by_key(|(name, _)| name.to_string());

            let enum_schemas = enum_schemas
                .into_iter()
                .map(|(_, enum_schema)| enum_schema)
                .collect::<Vec<_>>();
            let enum_schemas = enum_schemas.join("\n---\n\n");
            Ok(format!("{}\n\n{}", class_schema, enum_schemas))
        }
    }
}

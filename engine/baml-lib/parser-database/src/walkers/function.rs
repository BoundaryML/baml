use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{ArgumentId, Identifier, WithIdentifier, WithSpan};
use serde_json::json;

use crate::{
    ast::{self, WithName},
    printer::{serialize_with_printer, WithSerializeableContent},
    types::FunctionType,
    ParserDatabase, WithSerialize,
};

use super::{ClassWalker, ClientWalker, ConfigurationWalker, EnumWalker, Walker};

use std::{collections::HashMap, iter::ExactSizeIterator};

/// A `function` declaration in the Prisma schema.
pub type FunctionWalker<'db> = Walker<'db, (bool, ast::ValExpId)>;

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
    pub fn function_id(self) -> ast::ValExpId {
        self.id.1
    }

    /// The AST node.
    pub fn ast_function(self) -> &'db ast::ValueExprBlock {
        &self.db.ast[self.id.1]
    }

    /// The name of the function.
    pub fn is_positional_args(self) -> bool {
        false
    }

    /// Arguments of the function.
    pub fn find_input_arg_by_name(self, name: &str) -> Option<ArgWalker<'db>> {
        self.ast_function().input().and_then(|args| {
            args.iter_args().find_map(|(idx, (idn, _))| {
                if idn.name() == name {
                    Some(ArgWalker {
                        db: self.db,
                        id: (self.id.1, true, idx),
                    })
                } else {
                    None
                }
            })
        })
    }

    /// Arguments of the function.
    pub fn find_input_arg_by_position(self, position: u32) -> Option<ArgWalker<'db>> {
        None
    }

    /// Iterates over the input arguments of the function.
    pub fn walk_input_args(self) -> impl ExactSizeIterator<Item = ArgWalker<'db>> {
        match self.ast_function().input() {
            Some(input) => {
                let range_end = input.iter_args().len() as u32;
                (0..range_end)
                    .map(move |f| ArgWalker {
                        db: self.db,
                        id: (self.id.1, true, ArgumentId(f)),
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
            }
            None => Vec::new().into_iter(),
        }
    }

    /// Iterates over the output arguments of the function.
    pub fn walk_output_args(self) -> impl ExactSizeIterator<Item = ArgWalker<'db>> {
        let range_end = 1;

        (0..range_end).map(move |f| ArgWalker {
            db: self.db,
            id: (self.id.1, false, ArgumentId(f)),
        })
    }

    /// All the test cases for this function.
    pub fn walk_tests(self) -> impl ExactSizeIterator<Item = ConfigurationWalker<'db>> {
        let mut tests = self
            .db
            .walk_test_cases()
            .filter(|w| w.test_case().functions.iter().any(|f| f.0 == self.name()))
            .collect::<Vec<_>>();

        // log::debug!("Found {} tests for function {}", tests.len(), self.name());

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

        log::info!("Client: {:?}", client.0);
        self.db.find_client(client.0.as_str())
    }
}

/// A `function` declaration in the Prisma schema.
pub type ArgWalker<'db> = super::Walker<'db, (ast::ValExpId, bool, ArgumentId)>;

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
    fn serialize_data(&self, db: &'_ ParserDatabase) -> serde_json::Value {
        json!({
            "rtype": "inline",
            "value": (self.db, &self.ast_arg().1.field_type).serialize_data( db)
        })
    }
}

impl<'db> WithSerializeableContent for FunctionWalker<'db> {
    fn serialize_data(&self, db: &'_ ParserDatabase) -> serde_json::Value {
        // TODO: We should handle the case of multiple output args
        json!({
            "rtype": "output",
            "value": self.walk_output_args()
                        .map(|f| f.serialize_data(db))
                        .next()
                        .unwrap_or(serde_json::Value::Null)
        })
    }
}

impl<'db> WithSerialize for FunctionWalker<'db> {
    fn serialize(
        &self,
        db: &'_ ParserDatabase,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, internal_baml_diagnostics::DatamodelError> {
        // Eventually we should validate what parameters are in meta.
        match serialize_with_printer(false, self.serialize_data(db)) {
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
        let class_schema = self.serialize(db, span)?;

        let mut enum_schemas = self
            .walk_output_args()
            .flat_map(|arg| arg.required_enums())
            .map(|e| (e.name().to_string(), e))
            .collect::<HashMap<_, _>>()
            .iter()
            // TODO(sam) - if enum serialization fails, then we do not surface the error to the user.
            // That is bad!!!!!!!
            .filter_map(|(_, e)| match e.serialize(db, e.identifier().span()) {
                Ok(enum_schema) => Some((e.name().to_string(), enum_schema)),
                Err(_) => None,
            })
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

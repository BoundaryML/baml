use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{ArgumentId, Identifier, WithIdentifier, WithSpan};

use crate::{
    ast::{self, WithName},
    types::FunctionType,
};

use super::{ClassWalker, ConfigurationWalker, EnumWalker, Walker};

use std::iter::ExactSizeIterator;

/// A `function` declaration in the Prisma schema.
pub type FunctionWalker<'db> = Walker<'db, (bool, ast::ValExpId)>;

impl<'db> FunctionWalker<'db> {
    /// The name of the function.
    pub fn name(self) -> &'db str {
        self.ast_function().name()
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
        self.walk_input_args().find(|arg| {
            let span = arg.ast_arg().1.span();
            span.contains(position as usize)
        })
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
}

/// Reference to a client
pub enum ClientSpec {
    /// References a client by name
    Named(String),

    /// Defined inline using shorthand "<provider>/<model>" syntax
    Shorthand(String),
}

impl<'db> FunctionWalker<'db> {
    /// Returns the client spec for the function, if it is well-formed
    pub fn client_spec(self) -> Result<ClientSpec, DatamodelError> {
        assert!(self.id.0, "Only new functions have clients");
        let Some(client) = self.metadata().client.as_ref() else {
            return Err(DatamodelError::new_validation_error(
                "Client metadata is missing.",
                self.span().clone(),
            ));
        };

        match client.0.split_once("/") {
            // TODO: do this in a more robust way
            // actually validate which clients are and aren't allowed
            Some((provider, model)) => Ok(ClientSpec::Shorthand(format!("{provider}/{model}"))),
            None => match self.db.find_client(client.0.as_str()) {
                Some(client) => Ok(ClientSpec::Named(client.name().to_string())),
                None => {
                    let clients = self
                        .db
                        .walk_clients()
                        .map(|c| c.name().to_string())
                        .collect::<Vec<_>>();
                    Err(DatamodelError::not_found_error(
                        "Client",
                        &client.0,
                        client.1.clone(),
                        clients.clone(),
                    ))
                }
            },
        }
    }
}
impl<'db> WithIdentifier for FunctionWalker<'db> {
    /// The name of the function.
    fn identifier(&self) -> &'db Identifier {
        self.ast_function().identifier()
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
        self.field_type().is_optional()
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

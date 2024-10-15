mod v2;

use crate::{configuration::Generator, internal_baml_diagnostics::*};
use internal_baml_parser_database::ast;
use internal_baml_schema_ast::ast::WithSpan;

/// Load and validate Generators defined in an AST.
pub(crate) fn load_generators_from_ast<'i>(
    ast_schema: &'i ast::SchemaAst,
    diagnostics: &'i mut Diagnostics,
) -> Vec<Generator> {
    let mut generators: Vec<Generator> = Vec::new();

    for gen in ast_schema.generators() {
        if let Some(generator) = parse_generator(gen, diagnostics) {
            generators.push(generator)
        }
    }

    generators
}

fn parse_generator(
    ast_generator: &ast::ValueExprBlock,
    diagnostics: &mut Diagnostics,
) -> Option<Generator> {
    let errors = match v2::parse_generator(ast_generator, &diagnostics.root_path) {
        Ok(gen) => {
            return Some(gen);
        }
        Err(errors) => errors,
    };

    for error in errors {
        diagnostics.push_error(error);
    }

    None
}

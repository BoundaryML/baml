mod v0;
mod v1;
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
    ast_generator: &ast::GeneratorConfig,
    diagnostics: &mut Diagnostics,
) -> Option<Generator> {
    let errors = match v2::parse_generator(ast_generator, &diagnostics.root_path) {
        Ok(gen) => {
            return Some(gen);
        }
        Err(errors) => errors,
    };

    log::info!("Failed to parse generator as v2 generator, moving on to v1 and v0.");

    if let Ok(gen) = v1::parse_generator(ast_generator, &diagnostics.root_path) {
        diagnostics.push_warning(DatamodelWarning::new(
            format!(
                "This generator format is deprecated. Please use the new format.\n\n{}",
                gen.as_baml(),
            ),
            ast_generator.span().clone(),
        ));
        return None;
    };

    log::info!("Failed to parse generator as v1 generator, moving on to v0.");

    if let Ok(gen) = v0::parse_generator(ast_generator, &diagnostics.root_path) {
        diagnostics.push_warning(DatamodelWarning::new(
            format!(
                "This generator format is deprecated. Please use the new format.\n{}",
                gen.as_baml(),
            ),
            ast_generator.span().clone(),
        ));
        return None;
    };

    for error in errors {
        diagnostics.push_error(error);
    }

    None
}

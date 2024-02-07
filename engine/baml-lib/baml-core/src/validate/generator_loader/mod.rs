mod v0;
mod v1;

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
    match v1::parse_generator(ast_generator, &diagnostics.root_path) {
        Ok(gen) => Some(gen),
        Err(errors) => match v0::parse_generator(ast_generator, &diagnostics.root_path) {
            Ok(gen) => {
                // Convert the generator to the new format:

                let updated_client = format!(
                    r#"generator {} {{
    language "{}"
    project_root "{}"
    test_command "{}"
    install_command "{}"
    package_version_command "{}"
}}"#,
                    gen.name,
                    gen.language.to_string(),
                    gen.project_root.display(),
                    gen.test_command,
                    gen.install_command,
                    gen.package_version_command,
                );

                diagnostics.push_warning(DatamodelWarning::new(
                    format!(
                        "The generator format is deprecated. Please use the new format.\n{}",
                        updated_client
                    ),
                    ast_generator.span().clone(),
                ));
                Some(gen)
            }
            Err(_) => {
                for error in errors {
                    diagnostics.push_error(error);
                }
                None
            }
        },
    }
}

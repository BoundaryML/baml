use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::ClassId;

use crate::configuration::Generator;

pub(crate) fn generate_py(cls: Walker<'_, ClassId>, gen: &Generator) {
    println!("Generating python code {}", cls.name());
}

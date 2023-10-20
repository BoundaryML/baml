use internal_baml_parser_database::walkers::Walker;
use internal_baml_schema_ast::ast::EnumId;

use crate::configuration::Generator;

pub(crate) fn generate_py(enm: Walker<'_, EnumId>, gen: &Generator) {
    println!("Generating python code {}", enm.name());
}

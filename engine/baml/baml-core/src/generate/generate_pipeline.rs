use internal_baml_parser_database::ParserDatabase;

use crate::configuration::Generator;

use super::generate_python_client::generate_py;

pub(crate) fn generate_pipeline(db: &ParserDatabase, gen: &Generator) {
    match gen.language.as_str() {
        "python" => generate_py(db, gen),
        _ => unreachable!("Unsupported generator language"),
    }
}

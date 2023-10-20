use internal_baml_parser_database::ParserDatabase;

use crate::configuration::Generator;

mod r#class;
mod r#enum;

pub(crate) fn generate_py(db: &ParserDatabase, gen: &Generator) {
    db.walk_enums().for_each(|e| r#enum::generate_py(e, gen));
    db.walk_classes().for_each(|c| r#class::generate_py(c, gen));
}

use internal_baml_parser_database::ParserDatabase;

use crate::{
    configuration::{Generator, GeneratorLanguage},
    lockfile::LockFileWrapper,
};

use super::generate_python_client::generate_py;
use super::lockfile;

pub use super::test_request::TestRequest;

pub(crate) fn generate_pipeline(
    db: &ParserDatabase,
    gen: &Generator,
    lock: &LockFileWrapper,
) -> std::io::Result<()> {
    lockfile::generate(db)?;
    match gen.language {
        GeneratorLanguage::Python => generate_py(db, gen, lock),
        GeneratorLanguage::TypeScript => todo!(),
    }
}

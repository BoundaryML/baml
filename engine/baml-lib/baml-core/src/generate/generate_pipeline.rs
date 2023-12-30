use internal_baml_parser_database::ParserDatabase;

use crate::{configuration::Generator, lockfile::LockFileWrapper};

use super::generate_python_client::generate_py;
use super::ir;

pub use super::test_request::TestRequest;

pub(crate) fn generate_pipeline(
    db: &ParserDatabase,
    gen: &Generator,
    lock: &LockFileWrapper,
) -> std::io::Result<()> {
    ir::generate_lockfile(db)?;
    match gen.language.as_str() {
        "python" => generate_py(db, gen, lock),
        _ => unreachable!("Unsupported generator language"),
    }
}

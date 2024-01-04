use internal_baml_parser_database::ParserDatabase;

use crate::{
    configuration::{Generator, GeneratorLanguage},
    lockfile::LockFileWrapper,
};

use super::generate_python_client::generate_py;
use super::generate_ts_client::generate_ts;
use super::ir;

pub use super::test_request::TestRequest;

pub(crate) fn generate_pipeline(
    db: &ParserDatabase,
    gen: &Generator,
    lock: &LockFileWrapper,
) -> std::io::Result<()> {
    let ir = ir::to_ir(db).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to generate IR: {}", e),
        )
    })?;
    match gen.language {
        GeneratorLanguage::Python => generate_py(db, gen, lock),
        GeneratorLanguage::TypeScript => generate_ts(&ir, gen),
        _ => unreachable!("Unsupported generator language"),
    }
}

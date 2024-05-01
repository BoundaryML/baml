use internal_baml_parser_database::ParserDatabase;

use crate::{
    configuration::{Generator, GeneratorLanguage},
    ir::IntermediateRepr,
    lockfile::LockFileWrapper,
};

use super::generate_python_client_old::generate_py;
use super::generate_ts_client::generate_ts;


pub fn generate_pipeline(
    db: &ParserDatabase,
    gen: &Generator,
    ir: &IntermediateRepr,
    lock: &LockFileWrapper,
) -> anyhow::Result<()> {
    match gen.language {
        GeneratorLanguage::Python => generate_py(db, gen, lock).map_err(anyhow::Error::new),
        GeneratorLanguage::TypeScript => generate_ts(&ir, gen).map_err(anyhow::Error::new),
        _ => anyhow::bail!("generator v1 only supports python and TS"),
    }
}

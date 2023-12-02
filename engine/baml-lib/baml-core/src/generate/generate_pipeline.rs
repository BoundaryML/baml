use std::path::PathBuf;

use internal_baml_parser_database::ParserDatabase;
use log::info;

use crate::{
    configuration::Generator, generate::generate_base::WithFileName, lockfile::LockFileWrapper,
};

use super::generate_base::TargetLanguage;

pub use super::test_request::TestRequest;

fn generate(
    db: &ParserDatabase,
    language: TargetLanguage,
    gen: &Generator,
    lock: &LockFileWrapper,
) -> std::io::Result<()> {
    let mut fc = Default::default();
    db.to_file(&mut fc, language);

    info!("Writing files to {}", gen.output.to_string_lossy());
    let temp_path = PathBuf::from(format!("{}.tmp", &gen.output.to_string_lossy().to_string()));
    match fc.write(&temp_path, gen, lock) {
        Ok(_) => {
            let _ = std::fs::remove_dir_all(&gen.output);
            std::fs::rename(&temp_path, &gen.output)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(e)
        }
    }
}

pub(crate) fn generate_pipeline(
    db: &ParserDatabase,
    gen: &Generator,
    lock: &LockFileWrapper,
) -> std::io::Result<()> {
    let lang = match gen.language.as_str() {
        "python" => TargetLanguage::Python,
        _ => unreachable!("Unsupported generator language"),
    };
    generate(db, lang, gen, lock)
}

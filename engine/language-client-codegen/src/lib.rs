use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

mod dir_writer;
mod ruby;

#[derive(Deserialize)]
struct GeneratorInstructions {
    project_root: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "language")]
enum LanguageClientFactory {
    #[serde(rename = "python/pydantic")]
    Python(GeneratorInstructions),

    #[serde(rename = "typescript")]
    Typescript(GeneratorInstructions),

    #[serde(rename = "ruby/sorbet")]
    Ruby(GeneratorInstructions),
}

impl LanguageClientFactory {
    fn new() -> Self {
        todo!()
    }

    fn generate_client(&self) -> Result<()> {
        todo!()
    }
}

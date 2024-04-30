use std::path::PathBuf;

use anyhow::Result;
use internal_baml_core::ir::repr::IntermediateRepr;
use serde::Deserialize;

mod dir_writer;
mod ruby;

#[derive(Deserialize)]
pub struct GeneratorInstructions {
    project_root: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "language")]
pub enum LanguageClientFactory {
    #[serde(rename = "python/pydantic")]
    Python(GeneratorInstructions),

    #[serde(rename = "typescript")]
    Typescript(GeneratorInstructions),

    #[serde(rename = "ruby/sorbet")]
    Ruby(GeneratorInstructions),
}

impl LanguageClientFactory {
    pub fn new() -> Self {
        todo!()
    }

    pub fn generate_client(&self, ir: &IntermediateRepr) -> Result<()> {
        match self {
            LanguageClientFactory::Python(_) => anyhow::bail!("Python not implemented"),
            LanguageClientFactory::Typescript(_) => anyhow::bail!("Typescript not implemented"),
            LanguageClientFactory::Ruby(gen) => ruby::generate_ruby(ir, gen.project_root.as_path()),
        }
    }
}

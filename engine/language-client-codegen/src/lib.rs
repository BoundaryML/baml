use std::path::PathBuf;

use anyhow::Result;
use internal_baml_core::ir::repr::IntermediateRepr;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Bytes;

mod dir_writer;
mod python;
mod ruby;

#[derive(Deserialize)]
pub struct GeneratorInstructions {
    pub project_root: PathBuf,
    pub encoded_baml_files: Option<String>,
}

//#[derive(Deserialize)]
//#[serde(tag = "language")]
pub enum LanguageClientFactory {
    PythonPydantic(GeneratorInstructions),
    Ruby(GeneratorInstructions),
}

impl LanguageClientFactory {
    pub fn new() -> Self {
        todo!()
    }

    pub fn generate_client(&self, ir: &IntermediateRepr) -> Result<()> {
        match self {
            LanguageClientFactory::Ruby(gen) => ruby::generate(ir, gen),
            LanguageClientFactory::PythonPydantic(gen) => python::generate(ir, gen),
        }
    }
}

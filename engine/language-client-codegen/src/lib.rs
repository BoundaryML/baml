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
pub struct GeneratorArgs {
    pub output_root: PathBuf,
    pub encoded_baml_files: Option<String>,
}

#[derive(Clone, Deserialize)]
pub enum LanguageClientType {
    #[serde(rename = "python/pydantic")]
    PythonPydantic,

    #[serde(rename = "ruby")]
    Ruby,
}

impl LanguageClientType {
    pub fn generate_client(&self, ir: &IntermediateRepr, gen: &GeneratorArgs) -> Result<()> {
        match self {
            LanguageClientType::Ruby => ruby::generate(ir, gen),
            LanguageClientType::PythonPydantic => python::generate(ir, gen),
        }
    }
}

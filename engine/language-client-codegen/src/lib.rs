use std::path::PathBuf;

use anyhow::Result;
use internal_baml_core::ir::repr::IntermediateRepr;
use serde::{Deserialize, Serialize};

mod dir_writer;
mod python;
mod ruby;
mod typescript;

#[derive(Deserialize)]
pub struct GeneratorArgs {
    pub output_root: PathBuf,
    pub encoded_baml_files: Option<String>,
}

pub struct GenerateOutput {
    pub client_type: String,
    pub files: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LanguageClientType {
    PythonPydantic,
    Ruby,
    Typescript,
}

impl LanguageClientType {
    pub fn generate_client(
        &self,
        ir: &IntermediateRepr,
        gen: &GeneratorArgs,
    ) -> Result<GenerateOutput> {
        let files = match self {
            LanguageClientType::Ruby => ruby::generate(ir, gen),
            LanguageClientType::PythonPydantic => python::generate(ir, gen),
            LanguageClientType::Typescript => typescript::generate(ir, gen),
        }?;

        Ok(GenerateOutput {
            client_type: match self {
                LanguageClientType::Ruby => "Ruby",
                LanguageClientType::PythonPydantic => "Python",
                LanguageClientType::Typescript => "Typescript",
            }
            .to_string(),
            files,
        })
    }
}

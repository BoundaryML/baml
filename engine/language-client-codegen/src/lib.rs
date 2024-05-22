use std::path::PathBuf;

use anyhow::Result;
use indexmap::IndexMap;
use internal_baml_core::{configuration::GeneratorOutputType, ir::repr::IntermediateRepr};
use serde::{Deserialize, Serialize};

mod dir_writer;
mod python;
mod ruby;
mod typescript;

#[derive(Deserialize)]
pub struct GeneratorArgs {
    pub output_dir: PathBuf,
    pub encoded_baml_files: Option<String>,
}

pub struct GenerateOutput {
    pub client_type: GeneratorOutputType,
    pub files: IndexMap<PathBuf, String>,
}

pub trait GenerateClient {
    fn generate_client(&self, ir: &IntermediateRepr, gen: &GeneratorArgs)
        -> Result<GenerateOutput>;
}

impl GenerateClient for GeneratorOutputType {
    fn generate_client(
        &self,
        ir: &IntermediateRepr,
        gen: &GeneratorArgs,
    ) -> Result<GenerateOutput> {
        let files = match self {
            GeneratorOutputType::Ruby => ruby::generate(ir, gen),
            GeneratorOutputType::PythonPydantic => python::generate(ir, gen),
            GeneratorOutputType::Typescript => typescript::generate(ir, gen),
        }?;

        Ok(GenerateOutput {
            client_type: self.clone(),
            files,
        })
    }
}

use crate::{BamlRuntime, RuntimeInterface};
use anyhow::Result;
use clap::ValueEnum;
use std::fmt::Display;
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct GenerateArgs {
    #[arg(long, help = "path/to/baml_src")]
    from: String,

    #[arg(long, help = "output path where baml_client will be generated")]
    to: String,

    #[arg(long, help = "type of BAML client to generate")]
    client_type: LanguageClientType,
}

impl GenerateArgs {
    pub fn run(&self) -> Result<()> {
        let runtime = BamlRuntime::from_directory(&self.from.clone().into())?;

        let generate_output = runtime.generate_client(
            &(&self.client_type).into(),
            &internal_baml_codegen::GeneratorArgs {
                output_root: self.to.clone().into(),
                encoded_baml_files: None,
            },
        )?;

        println!(
            "Generated {} BAML client ({} files) in {} from {}",
            generate_output.client_type,
            generate_output.files.len(),
            self.to,
            self.from
        );

        Ok(())
    }
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum LanguageClientType {
    #[clap(name = "python/pydantic")]
    PythonPydantic,

    #[clap(name = "ruby")]
    Ruby,

    #[clap(name = "typescript")]
    Typescript,
}

impl Into<internal_baml_codegen::LanguageClientType> for &LanguageClientType {
    fn into(self) -> internal_baml_codegen::LanguageClientType {
        match self {
            LanguageClientType::PythonPydantic => {
                internal_baml_codegen::LanguageClientType::PythonPydantic
            }
            LanguageClientType::Ruby => internal_baml_codegen::LanguageClientType::Ruby,
            LanguageClientType::Typescript => internal_baml_codegen::LanguageClientType::Typescript,
        }
    }
}

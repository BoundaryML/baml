use crate::{BamlRuntime, RuntimeContext};
use anyhow::Result;
use internal_baml_core::configuration::GeneratorOutputType;
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct GenerateArgs {
    #[arg(long, help = "path/to/baml_src")]
    from: String,

    #[arg(long, help = "output path where baml_client will be generated")]
    to: String,

    #[arg(long, help = "type of BAML client to generate")]
    client_type: Option<LanguageClientType>,
}

impl GenerateArgs {
    pub fn run(&self, caller_type: super::CallerType) -> Result<()> {
        let runtime =
            BamlRuntime::from_directory(&self.from.clone().into(), std::env::vars().collect())?;

        let client_type: GeneratorOutputType = match (self.client_type.as_ref(), caller_type) {
            (Some(explicit_client_type), _) => explicit_client_type.into(),
            (None, super::CallerType::Python) => GeneratorOutputType::PythonPydantic,
            (None, super::CallerType::Ruby) => GeneratorOutputType::Ruby,
            (None, super::CallerType::Typescript) => GeneratorOutputType::Typescript,
        };

        let generate_output = runtime.generate_client(
            &client_type,
            &internal_baml_codegen::GeneratorArgs {
                output_dir: PathBuf::from(&self.to).join("baml_client"),
                rel_baml_src_path: PathBuf::from(&self.from),
            },
        )?;

        println!(
            "Generated {} BAML client ({} files)\n\
            output: {}\n\
            baml_src: {}",
            generate_output.client_type.to_string(),
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

impl Into<GeneratorOutputType> for &LanguageClientType {
    fn into(self) -> GeneratorOutputType {
        match self {
            LanguageClientType::PythonPydantic => GeneratorOutputType::PythonPydantic,
            LanguageClientType::Ruby => GeneratorOutputType::Ruby,
            LanguageClientType::Typescript => GeneratorOutputType::Typescript,
        }
    }
}

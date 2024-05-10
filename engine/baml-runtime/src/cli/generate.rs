use crate::{BamlRuntime, RuntimeContext, RuntimeInterface};
use anyhow::Result;

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
        let ctx = RuntimeContext::from_env();
        let runtime = BamlRuntime::from_directory(&self.from.clone().into(), &ctx)?;

        let client_type: internal_baml_codegen::LanguageClientType =
            match (self.client_type.as_ref(), caller_type) {
                (Some(explicit_client_type), _) => explicit_client_type.into(),
                (None, super::CallerType::Python) => {
                    internal_baml_codegen::LanguageClientType::PythonPydantic
                }
                (None, super::CallerType::Ruby) => internal_baml_codegen::LanguageClientType::Ruby,
                (None, super::CallerType::Typescript) => {
                    internal_baml_codegen::LanguageClientType::Typescript
                }
            };

        let generate_output = runtime.generate_client(
            &client_type,
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

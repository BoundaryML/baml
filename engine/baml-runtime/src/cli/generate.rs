use crate::{runtime::runtime_interface::baml_src_files, BamlRuntime};
use anyhow::Result;
use internal_baml_core::configuration::GeneratorOutputType;
use std::path::PathBuf;

use super::LanguageClientType;

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

        // Safe to unwrap as the files are guaranteed to exist if the runtime was created successfully.
        let src_files = baml_src_files(&self.from.clone().into())?;
        let file_content = src_files
            .iter()
            .map(|k| {
                (
                    k.to_string_lossy().into(),
                    std::fs::read_to_string(&k).unwrap(),
                )
            })
            .collect();

        let client_type = self
            .client_type
            .as_ref()
            .map_or_else(|| caller_type.into(), |x| x.into());

        let generate_output = runtime.generate_client(
            &client_type,
            &internal_baml_codegen::GeneratorArgs::new(
                &self.from,
                PathBuf::from(&self.to).join("baml_client"),
                &file_content,
            ),
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

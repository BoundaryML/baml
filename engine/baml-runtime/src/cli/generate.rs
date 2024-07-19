use crate::{runtime::runtime_interface::baml_src_files, BamlRuntime};
use anyhow::{Context, Result};
use colored::*;
use std::{env, path::PathBuf};

#[derive(clap::Args, Debug)]
pub struct GenerateArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    from: String,
    #[arg(
        long,
        help = "Generate baml_client without checking for version mismatch",
        default_value_t = false
    )]
    no_version_check: bool,
}

impl GenerateArgs {
    pub fn run(&self, caller_type: super::CallerType) -> Result<()> {
        let result = self.generate_clients(caller_type);

        if let Err(e) = result {
            eprintln!("{}", "Error generating clients".red());
            return Err(e);
        }

        Ok(())
    }

    fn generate_clients(&self, caller_type: super::CallerType) -> Result<()> {
        let src_dir = PathBuf::from(&self.from);
        let runtime = BamlRuntime::from_directory(&src_dir, std::env::vars().collect())?;
        let src_files = baml_src_files(&src_dir)?;
        let all_files = src_files
            .iter()
            .map(|k| Ok((k.clone(), std::fs::read_to_string(&k)?)))
            .collect::<Result<_>>()?;
        let generated = runtime.run_generators(&all_files, self.no_version_check)?;

        // give the user a working config to copy-paste (so we need to run it through generator again)
        if generated.is_empty() {
            let client_type = caller_type.into();
            let output_dir_relative_to_baml_src = PathBuf::from("..");
            let version = env!("CARGO_PKG_VERSION");
            let generate_output = runtime.generate_client(
                &client_type,
                &internal_baml_codegen::GeneratorArgs::new(
                    &output_dir_relative_to_baml_src.join("baml_client"),
                    &self.from,
                    all_files.iter(),
                    version.to_string(),
                    false,
                )?,
            )?;

            println!(
                "Generated 1 baml_client at {}",
                generate_output.output_dir.display()
            );
            println!(
                r#"
You can automatically generate a client by adding the following to any one of your BAML files:
generator my_client {{
 output_type "{}"
 output_dir "{}"
 version "{}"
}}"#,
                generate_output.client_type.to_string(),
                output_dir_relative_to_baml_src.join("").display(),
                version
            );
        } else {
            println!("Generated {} baml_client", generated.len());
        }

        Ok(())
    }
}

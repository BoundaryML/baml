use crate::{baml_src_files, BamlRuntime};
use anyhow::{Context, Result};
use internal_baml_core::configuration::GeneratorDefaultClientMode;
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct GenerateArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    pub from: PathBuf,
    #[arg(
        long,
        help = "Generate baml_client without checking for version mismatch",
        default_value_t = false
    )]
    pub(super) no_version_check: bool,
}

impl GenerateArgs {
    pub fn run(&self, defaults: super::RuntimeCliDefaults) -> Result<()> {
        let result = self.generate_clients(defaults);

        if let Err(e) = result {
            log::error!("Error generating clients: {:?}", e);
            return Err(e);
        }

        Ok(())
    }

    fn generate_clients(&self, defaults: super::RuntimeCliDefaults) -> Result<()> {
        let runtime = BamlRuntime::from_directory(&self.from, std::env::vars().collect())
            .context("Failed to build BAML runtime")?;
        let src_files = baml_src_files(&self.from)
            .context("Failed while searching for .baml files in baml_src/")?;
        let all_files = src_files
            .iter()
            .map(|k| Ok((k.clone(), std::fs::read_to_string(&k)?)))
            .collect::<Result<_>>()
            .context("Failed while reading .baml files in baml_src/")?;
        let generated = runtime
            .run_codegen(&all_files, self.no_version_check)
            .context("Client generation failed")?;

        // give the user a working config to copy-paste (so we need to run it through generator again)
        if generated.is_empty() {
            let client_type = defaults.output_type;

            let default_client_mode = match client_type {
                internal_baml_core::configuration::GeneratorOutputType::OpenApi => {
                    // this has no meaning
                    GeneratorDefaultClientMode::Sync
                }
                internal_baml_core::configuration::GeneratorOutputType::PythonPydantic => {
                    // TODO: Consider changing this default to sync
                    GeneratorDefaultClientMode::Async
                }
                internal_baml_core::configuration::GeneratorOutputType::Typescript => {
                    GeneratorDefaultClientMode::Async
                }
                internal_baml_core::configuration::GeneratorOutputType::RubySorbet => {
                    // this has no meaning
                    GeneratorDefaultClientMode::Sync
                }
            };
            // Normally `baml_client` is added via the generator, but since we're not running the generator, we need to add it manually.
            let output_dir_relative_to_baml_src = PathBuf::from("..");
            let version = env!("CARGO_PKG_VERSION");
            let generate_output = runtime
                .generate_client(
                    &client_type,
                    &internal_baml_codegen::GeneratorArgs::new(
                        &output_dir_relative_to_baml_src.join("baml_client"),
                        &self.from,
                        all_files.iter(),
                        version.to_string(),
                        false,
                        default_client_mode,
                        // TODO: this should be set if user is asking for openapi
                        vec![],
                    )
                    .context("Failed while resolving .baml paths in baml_src/")?,
                )
                .context(format!(
                    "Failed to run generator for {client_type} in {}",
                    output_dir_relative_to_baml_src.display()
                ))?;

            log::info!(
                "Generated 1 baml_client: {}",
                generate_output.output_dir_full.display()
            );
            log::info!(
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
            match generated.len() {
                1 => log::info!(
                    "Generated 1 baml_client: {}",
                    generated[0].output_dir_full.display()
                ),
                n => log::info!(
                    "Generated {n} baml_clients: {}",
                    generated
                        .iter()
                        .map(|g| g.output_dir_shorthand.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }

        Ok(())
    }
}

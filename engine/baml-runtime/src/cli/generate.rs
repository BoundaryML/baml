use std::path::PathBuf;

use anyhow::{Context, Result};
use generators_lib::version_check::GeneratorType;
use internal_baml_core::configuration::GeneratorDefaultClientMode;

use crate::{baml_src_files, BamlRuntime, InternalRuntimeInterface};

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
    #[arg(
        long,
        help = "Strip test blocks from inlined BAML to reduce generated file size",
        default_value_t = false
    )]
    pub no_tests: bool,
}

impl GenerateArgs {
    pub fn run(
        &self,
        defaults: super::RuntimeCliDefaults,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<()> {
        let result = self.generate_clients(defaults, feature_flags);

        if let Err(e) = result {
            baml_log::error!("Error generating clients: {:?}", e);
            return Err(e);
        }

        Ok(())
    }

    fn generate_clients(
        &self,
        defaults: super::RuntimeCliDefaults,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<()> {
        // Log enabled features
        if feature_flags.is_beta_enabled() {
            baml_log::info!("Beta features enabled - experimental warnings will be suppressed");
        }
        if feature_flags.should_display_warnings() {
            baml_log::info!("Warning display enabled - all warnings will be shown");
        }

        // Set BAML_GENERATE to prevent starting the tracing publisher
        let mut env_vars: std::collections::HashMap<String, String> = std::env::vars().collect();
        env_vars.insert("BAML_GENERATE".to_string(), "1".to_string());

        baml_log::info!(
            "Generating clients with CLI version: {}",
            env!("CARGO_PKG_VERSION")
        );

        let runtime = BamlRuntime::from_directory(&self.from, env_vars, feature_flags.clone())
            .context("Failed to build BAML runtime")?;

        // Display warnings only if the feature flag is enabled
        let diagnostics = &runtime.diagnostics;
        if feature_flags.should_display_warnings() && diagnostics.has_warnings() {
            eprintln!("{}", diagnostics.warnings_to_pretty_string());
        }

        let src_files = baml_src_files(&self.from)
            .context("Failed while searching for .baml files in baml_src/")?;
        let all_files = src_files
            .iter()
            .map(|k| Ok((k.clone(), std::fs::read_to_string(k)?)))
            .collect::<Result<_>>()
            .context("Failed while reading .baml files in baml_src/")?;
        let generated = runtime
            .run_codegen(
                &all_files,
                self.no_version_check,
                GeneratorType::CLI,
                self.no_tests,
            )
            .context("Client generation failed")?;

        // give the user a working config to copy-paste (so we need to run it through generator again)
        if generated.is_empty() {
            let client_type = defaults.output_type;

            let default_client_mode = match client_type {
                internal_baml_core::configuration::GeneratorOutputType::OpenApi => {
                    // this has no meaning
                    GeneratorDefaultClientMode::Sync
                }
                internal_baml_core::configuration::GeneratorOutputType::PythonPydantic
                | internal_baml_core::configuration::GeneratorOutputType::PythonPydanticV1 => {
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
                internal_baml_core::configuration::GeneratorOutputType::TypescriptReact => {
                    GeneratorDefaultClientMode::Async
                }
                internal_baml_core::configuration::GeneratorOutputType::Go => {
                    GeneratorDefaultClientMode::Sync
                }
                internal_baml_core::configuration::GeneratorOutputType::Rust => {
                    GeneratorDefaultClientMode::Sync
                }
            };
            // Normally `baml_client` is added via the generator, but since we're not running the generator, we need to add it manually.
            let output_dir_relative_to_baml_src = PathBuf::from("..");
            let version = env!("CARGO_PKG_VERSION");
            let generate_output = runtime
                .generate_client(
                    &client_type,
                    &generators_lib::GeneratorArgs::new(
                        output_dir_relative_to_baml_src.join("baml_client"),
                        &self.from,
                        all_files.iter(),
                        version.to_string(),
                        false,
                        default_client_mode,
                        vec![],
                        client_type,
                        if matches!(
                            client_type,
                            internal_baml_core::configuration::GeneratorOutputType::Go
                        ) {
                            todo!("Implement how to get the client package name for go projects")
                        } else {
                            None
                        },
                        None,
                    )
                    .context("Failed while resolving .baml paths in baml_src/")?,
                    GeneratorType::CLI,
                )
                .context(format!(
                    "Failed to run generator for {client_type} in {}",
                    output_dir_relative_to_baml_src.display()
                ))?;

            baml_log::info!(
                "Generated 1 baml_client: {}",
                generate_output.output_dir_full.display()
            );
            baml_log::info!(
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
                1 => baml_log::info!(
                    "Generated 1 baml_client: {}",
                    generated[0].output_dir_full.display()
                ),
                n => baml_log::info!(
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

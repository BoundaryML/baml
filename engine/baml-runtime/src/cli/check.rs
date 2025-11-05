use std::{path::PathBuf, process::exit};

use anyhow::{Context, Result};
use internal_baml_core::configuration::GeneratorDefaultClientMode;

use crate::{baml_src_files, BamlRuntime, InternalRuntimeInterface};

#[derive(clap::Args, Debug)]
pub struct CheckArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    pub from: PathBuf,
    #[arg(
        long,
        help = "Checks for erros and warnings without checking for version mismatch",
        default_value_t = false
    )]
    pub(super) no_version_check: bool,
    // TODO: Implement this flag
    // #[arg(
    //     long,
    //     help = "Only display errors, not warnings",
    //     default_value_t = false
    // )]
    // pub(super) only_errors: bool,
}

impl CheckArgs {
    pub fn run(
        &self,
        defaults: super::RuntimeCliDefaults,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<()> {
        let result = self.check(defaults, feature_flags);

        if let Err(e) = result {
            baml_log::error!("Error checking: {:?}", e);
            return Err(e);
        }

        Ok(())
    }

    #[allow(clippy::print_stdout)]
    fn check(
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

        let env_vars: std::collections::HashMap<String, String> = std::env::vars().collect();
        let runtime = BamlRuntime::from_directory(&self.from, env_vars, feature_flags.clone());

        match runtime {
            Err(e) => {
                println!("{e:?}");

                // We should probably name this exit code more specifically
                exit(1);
            }
            Ok(runtime) => {
                let diagnostics = &runtime.diagnostics;
                if diagnostics.has_warnings() {
                    println!("{}", diagnostics.warnings_to_pretty_string());
                }
            }
        }

        Ok(())
    }
}

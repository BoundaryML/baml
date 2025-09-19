use std::{path::PathBuf, sync::Arc};

pub mod version_check;

pub use dir_writer::GeneratorArgs;
use dir_writer::{IntermediateRepr, LanguageFeatures};
use indexmap::IndexMap;
use internal_baml_core::configuration::GeneratorOutputType;

pub struct GenerateOutput {
    pub client_type: GeneratorOutputType,
    /// Relative path to the output directory (output_dir in the generator)
    pub output_dir_shorthand: PathBuf,
    /// The absolute path that the generated baml client was written to
    pub output_dir_full: PathBuf,
    pub files: IndexMap<PathBuf, String>,
}

pub fn generate_sdk(
    ir: Arc<IntermediateRepr>,
    gen: &GeneratorArgs,
) -> Result<IndexMap<PathBuf, String>, anyhow::Error> {
    let res = match gen.client_type {
        GeneratorOutputType::Go => {
            use generators_go::GoLanguageFeatures;
            let features = GoLanguageFeatures;
            features.generate_sdk(ir, gen)?
        }
        GeneratorOutputType::PythonPydantic | GeneratorOutputType::PythonPydanticV1 => {
            use generators_python::PyLanguageFeatures;
            let features = PyLanguageFeatures;
            features.generate_sdk(ir, gen)?
        }
        GeneratorOutputType::OpenApi => {
            use generators_openapi::OpenApiLanguageFeatures;
            let features = OpenApiLanguageFeatures;
            features.generate_sdk(ir, gen)?
        }
        GeneratorOutputType::Typescript | GeneratorOutputType::TypescriptReact => {
            use generators_typescript::TsLanguageFeatures;
            let features = TsLanguageFeatures;
            features.generate_sdk(ir, gen)?
        }
        GeneratorOutputType::RubySorbet => {
            use generators_ruby::RbLanguageFeatures;
            let features = RbLanguageFeatures::default();
            features.generate_sdk(ir, gen)?
        }
    };

    // Run on_generate commands
    #[cfg(not(target_arch = "wasm32"))]
    {
        if matches!(gen.client_type, GeneratorOutputType::OpenApi) && gen.on_generate.is_empty() {
            // TODO: we should auto-suggest a command for the user to run here
            baml_log::warn!("No on_generate commands were provided for OpenAPI generator - skipping OpenAPI client generation");
        }
        for cmd in gen.on_generate.iter() {
            use anyhow::Context;

            baml_log::info!("Running {:?} in {}", cmd, gen.output_dir().display());

            let output_result = std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(gen.output_dir())
                .output()
                .context(format!("Failed to run on_generate command: {cmd}"));

            let output = match output_result {
                Ok(output) => output,
                Err(e) => {
                    baml_log::error!("Failed to execute on_generate command: {}", e);
                    return Err(e);
                }
            };

            // log::info!("on_generate command finished");
            if !output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let error_msg = format!(
                    "on_generate command finished with {}: {}\nStdout:\n{}\nStderr:\n{}",
                    match output.status.code() {
                        Some(code) => format!("exit code {code}"),
                        None => "no exit code".to_string(),
                    },
                    cmd,
                    stdout,
                    stderr
                );
                return Err(anyhow::anyhow!("{}", error_msg));
            }
        }

        Ok(res)
    }

    #[cfg(target_arch = "wasm32")]
    {
        Ok(res)
    }
}

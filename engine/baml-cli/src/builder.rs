mod dir_utils;

use anyhow::Result;
use std::path::PathBuf;

use baml_lib::{parse_and_validate_schema, Configuration, SourceFile, ValidatedSchema};
use colored::*;

use crate::{builder::dir_utils::get_src_files, errors::CliError, update::version_check};

pub(crate) use crate::builder::dir_utils::{get_baml_src, get_src_dir};

pub fn build(
    baml_dir: &Option<String>,
) -> std::result::Result<(PathBuf, Configuration, ValidatedSchema), CliError> {
    build_anyhow(baml_dir).map_err(|e| CliError::StringError(e.to_string()))
}

pub fn build_anyhow(
    baml_dir: &Option<String>,
) -> Result<(PathBuf, Configuration, ValidatedSchema)> {
    let (baml_dir, (config, diagnostics)) = get_src_dir(baml_dir)?;
    let src_files = get_src_files(&baml_dir)?;
    log::info!(
        "Building baml project: {} {}",
        baml_dir.to_string_lossy().green().bold(),
        format!("({} files)", src_files.len()).dimmed()
    );

    let mut parsed = parse_and_validate_schema(
        &baml_dir,
        src_files
            .iter()
            .map(|path| SourceFile::from((path.clone(), std::fs::read_to_string(path).unwrap())))
            .collect::<Vec<_>>(),
    )?;

    parsed.diagnostics.to_result()?;

    if parsed.diagnostics.has_warnings() {
        log::warn!("{}", parsed.diagnostics.warnings_to_pretty_string());
    }

    if diagnostics.has_warnings() {
        log::warn!("{}", diagnostics.warnings_to_pretty_string());
    }

    let ir = internal_baml_core::ir::to_ir(&parsed.db)
        .map_err(|e| e.context("Failed to build BAML (IR stage)"))?;
    for (gen, lock_file) in config.generators.iter() {
        use internal_baml_core::configuration::GeneratorLanguage;

        match gen.language {
            GeneratorLanguage::Python | GeneratorLanguage::TypeScript => {
                internal_baml_core::generate_pipeline(&parsed.db, gen, &ir, lock_file)?
            }
            GeneratorLanguage::Ruby => {
                internal_baml_codegen::LanguageClientFactory::Ruby(
                    internal_baml_codegen::GeneratorInstructions {
                        project_root: gen.output_path.clone(),
                    },
                )
                .generate_client(&ir)?;
                log::info!("Generated Ruby client");
            }
        }
    }

    config.generators.iter().for_each(|(_, lockfile)| {
        version_check(lockfile);
    });
    Ok((baml_dir, config, parsed))
}

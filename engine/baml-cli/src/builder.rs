mod dir_utils;

use std::path::PathBuf;

use baml_lib::{
    generate_schema, parse_and_validate_schema, Configuration, SourceFile, ValidatedSchema,
};
use colored::*;
use log::info;

use crate::{builder::dir_utils::get_src_files, errors::CliError, update::version_check};

pub(crate) use crate::builder::dir_utils::{get_baml_src, get_src_dir};

pub fn build(
    baml_dir: &Option<String>,
) -> Result<(PathBuf, Configuration, ValidatedSchema), CliError> {
    let (baml_dir, (config, diagnostics)) = get_src_dir(baml_dir)?;
    let src_files = get_src_files(&baml_dir)?;
    info!(
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

    generate_schema(&parsed, &config).map_err(|e| e.to_string())?;

    config.generators.iter().for_each(|(_, lockfile)| {
        version_check(lockfile);
    });
    Ok((baml_dir, config, parsed))
}

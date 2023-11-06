mod dir_utils;

use baml_lib::{generate_schema, parse_and_validate_schema, SourceFile};
use colored::*;
use log::info;

use crate::{
    builder::dir_utils::{get_src_dir, get_src_files},
    errors::CliError,
};

pub fn build(baml_dir: &Option<String>) -> Result<(), CliError> {
    let (baml_dir, (config, diagnostics)) = get_src_dir(baml_dir)?;
    let src_files = get_src_files(&baml_dir)?;

    info!(
        "Building: {} ({} BAML files found)",
        baml_dir.to_string_lossy().bold(),
        src_files.len()
    );

    let mut parsed = parse_and_validate_schema(
        &baml_dir,
        src_files
            .iter()
            .map(|path| SourceFile::from((path.clone(), std::fs::read_to_string(&path).unwrap())))
            .collect::<Vec<_>>(),
    )?;

    parsed.diagnostics.to_result()?;

    if parsed.diagnostics.has_warnings() {
        log::warn!("{}", parsed.diagnostics.warnings_to_pretty_string());
    }

    if diagnostics.has_warnings() {
        log::warn!("{}", diagnostics.warnings_to_pretty_string());
    }

    match generate_schema(&parsed, &config) {
        Ok(_) => Ok(()),
        Err(err) => Err(err.to_string().into()),
    }
}

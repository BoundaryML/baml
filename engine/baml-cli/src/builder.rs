mod dir_utils;

use baml::{generate_schema, parse_configuration, parse_schema, SourceFile};
use colored::*;
use log::{error, info, warn};
use std::path::PathBuf;

use crate::{
    builder::dir_utils::{get_src_dir, get_src_files},
    errors::CliError,
};

// Walk up a directory until you find a directory named: baml_src
fn default_baml_dir() -> Result<PathBuf, &'static str> {
    let mut current_dir = std::env::current_dir().unwrap();
    loop {
        let baml_dir = current_dir.join("baml_src");
        if baml_dir.exists() {
            return Ok(baml_dir);
        }
        if !current_dir.pop() {
            break;
        }
    }
    Err("Failed to find a directory named: baml_src")
}

pub fn build(baml_dir: &Option<String>) -> Result<(), CliError> {
    let (baml_dir, config) = get_src_dir(baml_dir)?;
    let mut src_files = get_src_files(&baml_dir)?;

    info!(
        "Building: {} ({} BAML files found)",
        baml_dir.to_string_lossy().bold(),
        src_files.len()
    );

    let mut parsed = parse_schema(
        &baml_dir,
        src_files
            .iter()
            .map(|path| SourceFile::from((path.clone(), std::fs::read_to_string(&path).unwrap())))
            .collect::<Vec<_>>(),
    )?;

    parsed.diagnostics.to_result()?;

    generate_schema(&parsed, &config);

    Ok(())
}

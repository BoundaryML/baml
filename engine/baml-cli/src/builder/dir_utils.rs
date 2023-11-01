use std::path::PathBuf;

use baml::{parse_configuration, Configuration, Diagnostics};

use crate::errors::CliError;

// Walk up a directory until you find a directory named: baml_src
fn default_baml_dir() -> Result<PathBuf, CliError> {
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
    Err("Failed to find a directory named: baml_src".into())
}

pub(crate) fn get_src_dir(
    baml_dir: &Option<String>,
) -> Result<(PathBuf, (Configuration, Diagnostics)), CliError> {
    let src_dir = match baml_dir {
        Some(dir) => PathBuf::from(dir),
        None => match default_baml_dir() {
            Ok(dir) => dir,
            Err(err) => {
                return Err(err.into());
            }
        },
    };
    let abs_src_dir = src_dir.canonicalize();

    if let Err(_) = abs_src_dir {
        return Err(format!("Directory not found {}", src_dir.to_string_lossy()).into());
    }
    let baml_dir = abs_src_dir.unwrap();

    // Find a main.baml file
    let main_baml = baml_dir.join("main.baml");
    // Read the main.baml file
    let main_baml_contents = match std::fs::read_to_string(&main_baml) {
        Ok(contents) => contents,
        Err(err) => return Err((&main_baml, err).into()),
    };
    let config = parse_configuration(&baml_dir, main_baml, &main_baml_contents)?;

    Ok((baml_dir, config))
}

// Function to yield each found path
pub(crate) fn get_src_files(baml_dir: &PathBuf) -> Result<Vec<PathBuf>, CliError> {
    let glob_pattern = format!("{}/**/*.baml", baml_dir.display());
    let entries = glob::glob(&glob_pattern)?;
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(path) => paths.push(path),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(paths)
}

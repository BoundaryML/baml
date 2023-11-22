use std::path::{Component, PathBuf};

use baml_lib::{parse_configuration, Configuration, Diagnostics};

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

pub(crate) fn get_baml_src(baml_dir: &Option<String>) -> Result<PathBuf, CliError> {
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

    Ok(abs_src_dir.unwrap())
}

pub(crate) fn get_src_dir(
    baml_dir: &Option<String>,
) -> Result<(PathBuf, (Configuration, Diagnostics)), CliError> {
    let baml_dir = get_baml_src(baml_dir)?;

    // Find a main.baml file
    let main_baml = baml_dir.join("main.baml");
    // Read the main.baml file
    let main_baml_contents = match std::fs::read_to_string(&main_baml) {
        Ok(contents) => contents,
        Err(err) => return Err((&main_baml, err).into()),
    };
    let config = parse_configuration(&baml_dir, main_baml, &main_baml_contents)?;

    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();

    Ok((relative_path(cwd, baml_dir), config))
}

// Function to yield each found path
pub(crate) fn get_src_files(baml_dir: &PathBuf) -> Result<Vec<PathBuf>, CliError> {
    let glob_pattern = baml_dir.join("**/*").to_string_lossy().to_string();
    let glob_pattern = if glob_pattern.starts_with(r"\\?\") {
        &glob_pattern[4..]
    } else {
        &glob_pattern
    };
    let entries = glob::glob(&glob_pattern)?;
    let mut paths = Vec::new();

    let valid_extensions = vec!["baml", "json"];
    for entry in entries {
        match entry {
            Ok(path) => {
                if !path.is_file() {
                    continue;
                }
                if let Some(ext) = path.extension() {
                    if !valid_extensions.contains(&ext.to_str().unwrap()) {
                        continue;
                    }
                    paths.push(path)
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(paths)
}

fn relative_path(from: PathBuf, to: PathBuf) -> PathBuf {
    let from_iter = from.components();
    let to_iter = to.components();

    let common_components = from_iter
        .clone()
        .zip(to_iter.clone())
        .take_while(|&(a, b)| a == b)
        .map(|(a, _)| a)
        .collect::<Vec<_>>();

    let from_diff = from_iter.count() - common_components.len();

    let mut components = Vec::new();
    components.extend(std::iter::repeat(Component::ParentDir).take(from_diff));
    components.extend(to.components().skip(common_components.len()));

    components.into_iter().collect::<PathBuf>()
}

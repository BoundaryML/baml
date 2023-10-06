use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DirReadError {
    #[error("Failed to read directory: {0}")]
    ReadDirectoryError(PathBuf),

    #[error("Failed to read file: {0}")]
    ReadFileError(PathBuf),

    #[error("Missing main.gloo: {0}/main.gloo")]
    MissingTopLevelGlooFile(PathBuf),
}

impl From<std::io::Error> for DirReadError {
    fn from(_: std::io::Error) -> Self {
        DirReadError::ReadDirectoryError(PathBuf::new()) // Default to ReadDirectoryError
    }
}

pub fn read_directory(directory: &str) -> Result<Vec<(String, String)>, DirReadError> {
    let dir_path = Path::new(directory);

    if !dir_path.exists() {
        return Err(DirReadError::ReadDirectoryError(dir_path.to_path_buf()));
    }

    // Check if root.gloo exists in the root directory
    if !dir_path.join("main.gloo").exists() {
        return Err(DirReadError::MissingTopLevelGlooFile(
            dir_path.to_path_buf(),
        ));
    }

    let mut map = Vec::new();
    traverse_directory(dir_path, dir_path, &mut map)?;
    Ok(map)
}

fn traverse_directory(
    base_dir: &Path,
    current_dir: &Path,
    map: &mut Vec<(String, String)>,
) -> Result<(), DirReadError> {
    let entries = fs::read_dir(current_dir)
        .map_err(|_| DirReadError::ReadDirectoryError(current_dir.to_path_buf()))?;

    for entry in entries {
        let entry =
            entry.map_err(|_| DirReadError::ReadDirectoryError(current_dir.to_path_buf()))?;
        let path = entry.path();

        if path.is_dir() {
            traverse_directory(base_dir, &path, map)?;
        } else if path.extension().map_or(false, |ext| ext == "gloo") {
            let content =
                fs::read_to_string(&path).map_err(|_| DirReadError::ReadFileError(path.clone()))?;
            let relative_path = path
                .strip_prefix(base_dir)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.display().to_string());
            map.push((relative_path, content));
        }
    }

    Ok(())
}

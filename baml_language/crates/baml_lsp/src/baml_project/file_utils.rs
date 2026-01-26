//! A simple text document structure similar to `vscode-languageserver-textdocument`

use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

use lsp_types::{TextDocumentItem, Url};

/// Walks up from `file_path` until it finds a directory named `baml_src`.
///
/// # Precedence
///
/// 1. `baml_src` - Standard BAML project directory (always takes precedence)
/// 2. `baml_language` - Internal development fallback (see below)
///
/// # Internal Development Support
///
/// If no `baml_src` directory is found but the path contains a directory named
/// `baml_language` anywhere in its ancestry, this function returns the file path
/// itself as the project root. This allows each `.baml` file in the baml_language
/// repository (e.g., test fixtures in `crates/baml_ide_tests/`) to be treated as
/// its own isolated single-file project for development/testing.
///
/// **Note:** This `baml_language` behavior is for internal BAML development only
/// and should not be relied upon by end users. Users should always place their
/// BAML files in a `baml_src/` directory.
///
/// # Arguments
///
/// * `file_path` - A reference to the file path.
///
/// # Returns
///
/// * `Some(PathBuf)` if a directory with basename "baml_src" is found,
///   or if the path is within a "baml_language" directory (returns file path itself),
///   or `None` otherwise.
pub fn find_top_level_parent(file_path: &Path) -> Option<PathBuf> {
    // First, check for baml_src (standard project detection - takes precedence)
    let mut current = file_path;
    if let Some(file_name) = current.file_name() {
        if file_name == "baml_src" {
            return Some(current.to_path_buf());
        }
    }
    while let Some(parent) = current.parent() {
        if let Some(dir_name) = parent.file_name() {
            if dir_name == "baml_src" {
                return Some(parent.to_path_buf());
            }
        }
        current = parent;
    }

    // Fallback: Check for baml_language directory (internal development only).
    // If found, treat each .baml file as its own isolated single-file project.
    // This enables LSP features for test fixtures and development files.
    let mut check_path = file_path;
    while let Some(parent) = check_path.parent() {
        if let Some(dir_name) = parent.file_name() {
            if dir_name == "baml_language" {
                // Return the file path itself as the project root.
                // This ensures each standalone .baml file gets its own project context,
                // rather than sharing a project with other files in the same directory.
                return Some(file_path.to_path_buf());
            }
        }
        check_path = parent;
    }

    None
}

/// Seach for baml_src, either at the current directory, in any parent
/// directory, or in an immediate child directory.
pub fn find_baml_src(file_path: &Path) -> Option<PathBuf> {
    let current = file_path;
    let parent_baml_src = find_top_level_parent(current);
    let child_baml_src = current.join("baml_src");
    if parent_baml_src.is_some() {
        parent_baml_src
    } else if child_baml_src.exists() {
        Some(child_baml_src)
    } else {
        None
    }
}

/// Gathers files with .baml extensions from a given root path.
///
/// If `root_path` is a file with a `.baml` extension, returns just that file.
/// If `root_path` is a directory, performs an iterative search so that each
/// directory is only visited once.
///
/// # Arguments
///
/// * `root_path` - The root file or directory to start searching.
/// * `debug` - When true, errors reading directories are printed to stderr.
///
/// # Returns
///
/// * `Ok(Vec<PathBuf>)` containing the paths of discovered files,
///   or an `io::Error` if an error is encountered.
pub fn gather_files(root_path: &Path, debug: bool) -> io::Result<Vec<PathBuf>> {
    // If root_path is a file (standalone mode), just return it if it's a .baml file.
    if root_path.is_file() {
        if let Some(ext) = root_path.extension().and_then(|s| s.to_str()) {
            if ext.eq_ignore_ascii_case("baml") {
                return Ok(vec![root_path.to_path_buf()]);
            }
        }
        // Not a .baml file, return empty
        return Ok(vec![]);
    }

    let mut visited_dirs = HashSet::new();
    let mut dir_stack = Vec::new();
    let mut file_list = Vec::new();

    // Mark the root directory as visited.
    let root_str = root_path.to_string_lossy().to_string();
    visited_dirs.insert(root_str);
    dir_stack.push(root_path.to_path_buf());

    let max_dirs = 1000;
    let mut iterations = 0;

    while let Some(current_dir) = dir_stack.pop() {
        if iterations > max_dirs {
            if debug {
                tracing::error!("Max directory limit reached ({})", max_dirs);
            }
            return Err(io::Error::other(format!(
                "Directory failed to load after {iterations} iterations"
            )));
        }
        iterations += 1;

        let entries = fs::read_dir(&current_dir);
        match entries {
            Ok(read_dir) => {
                for entry in read_dir {
                    let entry = entry?;
                    let path = entry.path();
                    let metadata = entry.metadata()?;
                    if metadata.is_dir() {
                        let path_str = path.to_string_lossy().to_string();
                        if !visited_dirs.contains(&path_str) {
                            visited_dirs.insert(path_str);
                            dir_stack.push(path);
                        }
                    } else if metadata.is_file() {
                        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                            if ext.eq_ignore_ascii_case("baml") {
                                file_list.push(path);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if debug {
                    tracing::error!("Error reading directory {}: {}", current_dir.display(), e);
                }
                return Err(e);
            }
        }
    }
    Ok(file_list)
}

/// Converts the file at `file_path` into a simple text document.
///
/// The file is read and its extension is checked; if the extension is `.baml`,
/// the language id is set to `"baml"`, otherwise `"json"`. The file path is
/// converted into a file URI.
///
/// # Arguments
///
/// * `file_path` - A reference to the file path to convert.
///
/// # Returns
///
/// * `Ok(TextDocument)` representing the file as a text document,
///   or an `io::Error` in case of failure.
pub fn convert_to_text_document(file_path: &Path) -> io::Result<TextDocumentItem> {
    let content = fs::read_to_string(file_path)?;
    let language_id = if let Some(ext) = file_path.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("baml") {
            "baml"
        } else {
            "json"
        }
    } else {
        "plaintext"
    };

    let url = Url::from_file_path(file_path)
        .map_err(|_| io::Error::other("Invalid file path for URI"))?
        .to_string();

    Ok(TextDocumentItem {
        uri: Url::parse(&url).unwrap(),
        language_id: language_id.to_string(),
        version: 1,
        text: content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_top_level_parent_baml_src() {
        // Standard baml_src detection
        let path = PathBuf::from("/path/to/baml_src/subdir/file.baml");
        let result = find_top_level_parent(&path);
        assert_eq!(result, Some(PathBuf::from("/path/to/baml_src")));
    }

    #[test]
    fn test_find_top_level_parent_baml_src_direct() {
        // Path is the baml_src directory itself
        let path = PathBuf::from("/path/to/baml_src");
        let result = find_top_level_parent(&path);
        assert_eq!(result, Some(PathBuf::from("/path/to/baml_src")));
    }

    #[test]
    fn test_find_top_level_parent_baml_language() {
        // baml_language directory should return the file path itself (internal dev support)
        // Each standalone .baml file gets its own isolated project context
        let path = PathBuf::from("/path/to/baml_language/crates/test/file.baml");
        let result = find_top_level_parent(&path);
        assert_eq!(
            result,
            Some(PathBuf::from(
                "/path/to/baml_language/crates/test/file.baml"
            ))
        );
    }

    #[test]
    fn test_find_top_level_parent_baml_language_nested() {
        // Deeply nested in baml_language - returns the file path itself
        let path = PathBuf::from(
            "/home/user/baml_language/crates/baml_ide_tests/test_files/syntax/class/valid.baml",
        );
        let result = find_top_level_parent(&path);
        assert_eq!(
            result,
            Some(PathBuf::from(
                "/home/user/baml_language/crates/baml_ide_tests/test_files/syntax/class/valid.baml"
            ))
        );
    }

    #[test]
    fn test_find_top_level_parent_baml_src_takes_precedence() {
        // baml_src inside baml_language should prefer baml_src (it takes precedence)
        let path = PathBuf::from("/path/to/baml_language/examples/baml_src/file.baml");
        let result = find_top_level_parent(&path);
        // baml_src is checked first, so it wins
        assert_eq!(
            result,
            Some(PathBuf::from("/path/to/baml_language/examples/baml_src"))
        );
    }

    #[test]
    fn test_find_top_level_parent_no_match() {
        // No baml_src or baml_language in path
        let path = PathBuf::from("/random/path/to/file.baml");
        let result = find_top_level_parent(&path);
        assert_eq!(result, None);
    }
}

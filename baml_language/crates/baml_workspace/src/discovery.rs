//! File discovery utilities.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Discover all BAML files in a project directory.
///
/// Returns paths sorted for deterministic ordering.
pub fn discover_baml_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();

        // Skip hidden directories and common ignore patterns
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
        }

        // Collect .baml files
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("baml") {
            files.push(path.to_path_buf());
        }
    }

    files.sort(); // Ensure deterministic ordering
    files
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use super::*;

    #[test]
    fn test_discovers_baml_files() {
        let temp_dir = std::env::temp_dir().join("baml_workspace_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create test files
        let mut file1 = fs::File::create(temp_dir.join("test1.baml")).unwrap();
        file1.write_all(b"// test").unwrap();

        let mut file2 = fs::File::create(temp_dir.join("test2.baml")).unwrap();
        file2.write_all(b"// test").unwrap();

        let files = discover_baml_files(&temp_dir);

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.file_name().unwrap() == "test1.baml"));
        assert!(files.iter().any(|p| p.file_name().unwrap() == "test2.baml"));

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

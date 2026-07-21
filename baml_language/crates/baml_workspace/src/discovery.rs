//! File discovery utilities.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use ignore::WalkBuilder;

/// Discover all BAML files in a project directory.
///
/// Standard ignore files and hidden entries are respected, directory links are
/// not followed, and traversal errors are returned to the caller. The returned
/// paths are sorted for deterministic ordering.
pub fn discover_baml_files(root: &Path) -> Result<Vec<PathBuf>, ignore::Error> {
    let mut files = Vec::new();

    let mut builder = WalkBuilder::new(root);
    builder.standard_filters(true).follow_links(false);

    for entry in builder.build() {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension() == Some(OsStr::new("baml")) {
            files.push(entry.into_path());
        }
    }

    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::{fs, io, path::Path};

    use super::*;

    fn write(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "// test\n").unwrap();
    }

    #[test]
    fn discovers_baml_files_in_deterministic_order() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let nested = root.join("nested/a.baml");
        let top_level = root.join("z.baml");
        write(&top_level);
        write(&nested);
        write(&root.join("not-baml.txt"));

        assert_eq!(discover_baml_files(root).unwrap(), vec![nested, top_level]);
    }

    #[test]
    fn respects_standard_ignores_and_gitignore_negation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(
            root.join(".gitignore"),
            concat!(
                "node_modules/\n",
                "target/\n",
                "generated/*\n",
                "!generated/keep.baml\n",
                "!main.baml\n",
            ),
        )
        .unwrap();
        fs::write(root.join(".ignore"), "local/\n").unwrap();

        let main = root.join("main.baml");
        let kept = root.join("generated/keep.baml");
        for path in [
            &main,
            &kept,
            &root.join(".hidden/hidden.baml"),
            &root.join("generated/drop.baml"),
            &root.join("local/ignored.baml"),
            &root.join("node_modules/pkg/ignored.baml"),
            &root.join("target/debug/ignored.baml"),
        ] {
            write(path);
        }

        assert_eq!(discover_baml_files(root).unwrap(), vec![kept, main]);
    }

    #[test]
    fn applies_gitignore_in_a_jj_repository() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".jj")).unwrap();
        fs::write(root.join(".gitignore"), "ignored/\n!main.baml\n").unwrap();

        let main = root.join("main.baml");
        write(&main);
        write(&root.join("ignored/ignored.baml"));

        assert_eq!(discover_baml_files(root).unwrap(), vec![main]);
    }

    #[test]
    fn reports_a_missing_root() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");

        assert!(discover_baml_files(&missing).is_err());
    }

    #[cfg(any(unix, windows))]
    fn link_directory(original: &Path, link: &Path) -> io::Result<()> {
        #[cfg(unix)]
        return std::os::unix::fs::symlink(original, link);

        #[cfg(windows)]
        return std::os::windows::fs::symlink_dir(original, link);
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn does_not_follow_linked_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).unwrap();
        write(&root.join("main.baml"));
        write(&outside.join("linked.baml"));

        if let Err(error) = link_directory(&outside, &root.join("linked")) {
            #[cfg(windows)]
            if error.raw_os_error() == Some(1314) {
                // Windows requires Developer Mode or symlink privileges.
                return;
            }
            panic!("failed to create directory link: {error}");
        }

        assert_eq!(
            discover_baml_files(&root).unwrap(),
            vec![root.join("main.baml")]
        );
    }
}

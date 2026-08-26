//! BAML project root discovery.

use std::{
    io,
    path::{Path, PathBuf},
};

pub const BAML_TOML: &str = "baml.toml";
pub const BAML_SRC_DIR: &str = "baml_src";

/// Resolve the path that project discovery should start from.
///
/// When `from` is omitted, discovery starts at the current directory.
pub fn resolve_project_search_start(from: Option<&Path>) -> io::Result<PathBuf> {
    match from {
        Some(from) => std::fs::canonicalize(from),
        None => std::env::current_dir(),
    }
}

/// Convert a search start path into the directory whose ancestors should be
/// inspected.
pub fn project_search_dir(path: &Path) -> PathBuf {
    if path.is_file() {
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    } else {
        path.to_path_buf()
    }
}

/// Walk `start` and its ancestors looking for a BAML project marker, stopping at
/// the filesystem root. Prefer any `baml.toml` owner over manifest-less
/// `baml_src/` markers; if no manifest is found, use the nearest directory that
/// owns a `baml_src/` child.
pub fn find_baml_project_root(start: &Path) -> Option<PathBuf> {
    let start = project_search_dir(start);
    find_baml_project_root_from_ancestors(
        start.ancestors().map(Path::to_path_buf),
        |dir| dir.join(BAML_TOML).is_file(),
        |dir| dir.join(BAML_SRC_DIR).is_dir(),
    )
}

/// Return the directory that should be walked for BAML source files once
/// `project_root` has been resolved.
pub fn project_source_root(project_root: &Path) -> PathBuf {
    let baml_src = project_root.join(BAML_SRC_DIR);
    if baml_src.is_dir() {
        baml_src
    } else {
        project_root.to_path_buf()
    }
}

/// Shared project-root selection rule for callers that use non-`std::path`
/// filesystems, such as LSP VFS paths.
pub fn find_baml_project_root_from_ancestors<P, I, HasToml, HasBamlSrc>(
    ancestors: I,
    mut has_baml_toml: HasToml,
    mut has_baml_src_dir: HasBamlSrc,
) -> Option<P>
where
    P: Clone,
    I: IntoIterator<Item = P>,
    HasToml: FnMut(&P) -> bool,
    HasBamlSrc: FnMut(&P) -> bool,
{
    let mut nearest_baml_src_owner = None;
    for dir in ancestors {
        if has_baml_toml(&dir) {
            return Some(dir);
        }
        if nearest_baml_src_owner.is_none() && has_baml_src_dir(&dir) {
            nearest_baml_src_owner = Some(dir.clone());
        }
    }
    nearest_baml_src_owner
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "baml_db_project_resolution_{name}_{}_{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn valid_manifest() -> &'static str {
        "[package]\nname = \"test-project\"\n"
    }

    #[test]
    fn walks_up_from_baml_src_dir() {
        let tmp = temp_dir("walks_up_from_baml_src_dir");
        fs::write(tmp.join(BAML_TOML), valid_manifest()).unwrap();
        let src = tmp.join(BAML_SRC_DIR);
        fs::create_dir(&src).unwrap();

        assert_eq!(find_baml_project_root(&src), Some(tmp.clone()));

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn walks_up_from_nested_baml_src_dir() {
        let tmp = temp_dir("walks_up_from_nested_baml_src_dir");
        let nested = tmp.join(BAML_SRC_DIR).join("nested").join("deeper");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_baml_project_root(&nested), Some(tmp.clone()));

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn prefers_baml_toml_over_nearer_baml_src_marker() {
        let tmp = temp_dir("prefers_baml_toml");
        fs::write(tmp.join(BAML_TOML), valid_manifest()).unwrap();
        let nested = tmp.join("child").join(BAML_SRC_DIR).join("nested");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_baml_project_root(&nested), Some(tmp.clone()));

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn project_source_root_prefers_baml_src_child() {
        let tmp = temp_dir("project_source_root_prefers_baml_src_child");
        let src = tmp.join(BAML_SRC_DIR);
        fs::create_dir(&src).unwrap();

        assert_eq!(project_source_root(&tmp), src);

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn generic_resolver_prefers_manifest_over_nearer_baml_src() {
        let ancestors = ["child/nested", "child", ""];
        let root = find_baml_project_root_from_ancestors(
            ancestors,
            |dir| dir.is_empty(),
            |dir| *dir == "child",
        );

        assert_eq!(root, Some(""));
    }
}

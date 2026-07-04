//! Native filesystem VFS for `bex_project::BamlVFS`.
//!
//! Wraps `vfs::PhysicalFS` and implements `BulkReadFileSystem` by walking
//! only the glob's base directory (not the entire filesystem).

use std::{borrow::Cow, io::Read};

/// Wrapper around `vfs::PhysicalFS` that implements `BulkReadFileSystem`.
#[derive(Debug)]
pub struct NativeVfs {
    root: String,
}

impl NativeVfs {
    pub fn new() -> Self {
        Self {
            root: "/".to_string(),
        }
    }

    fn inner(&self) -> vfs::PhysicalFS {
        vfs::PhysicalFS::new(&self.root)
    }
}

impl Clone for NativeVfs {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }
}

impl vfs::FileSystem for NativeVfs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        self.inner().read_dir(path)
    }

    fn create_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.inner().create_dir(path)
    }

    fn open_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        self.inner().open_file(path)
    }

    fn create_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.inner().create_file(path)
    }

    fn append_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.inner().append_file(path)
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        self.inner().metadata(path)
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        self.inner().exists(path)
    }

    fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        self.inner().remove_file(path)
    }

    fn remove_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.inner().remove_dir(path)
    }
}

impl bex_project::BulkReadFileSystem for NativeVfs {
    fn read_many(&self, glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
        // Extract the base directory from the glob (everything before the
        // first `*` or `?` wildcard). This lets us walk only the relevant
        // subtree instead of the entire filesystem.
        let native_glob = native_glob_for_fs(glob);
        let base_dir = glob_base_dir(&native_glob);
        let pattern = glob_to_regex(&native_glob);

        let base = std::path::Path::new(&base_dir);
        if !base.is_dir() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        walk_dir_native(base, &pattern, &mut results);
        Ok(results)
    }
}

fn walk_dir_native(
    dir: &std::path::Path,
    pattern: &regex::Regex,
    results: &mut Vec<(String, Vec<u8>)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk_dir_native(&path, pattern, results);
        } else if path.is_file() {
            let path_str = path.to_string_lossy();
            let match_path = native_path_for_glob_match(&path_str);
            if pattern.is_match(&match_path)
                && let Ok(mut file) = std::fs::File::open(&path)
            {
                let mut buf = Vec::new();
                if file.read_to_end(&mut buf).is_ok() {
                    results.push((vfs_path_from_native_path(&match_path), buf));
                }
            }
        }
    }
}

#[cfg(windows)]
fn native_glob_for_fs(glob: &str) -> Cow<'_, str> {
    let glob = normalize_windows_separators(glob);
    let Some(stripped) = glob.strip_prefix('/') else {
        return glob;
    };

    if is_windows_drive_path(stripped) {
        Cow::Owned(stripped.to_string())
    } else {
        glob
    }
}

#[cfg(not(windows))]
fn native_glob_for_fs(glob: &str) -> Cow<'_, str> {
    Cow::Borrowed(glob)
}

#[cfg(windows)]
fn native_path_for_glob_match(path: &str) -> Cow<'_, str> {
    normalize_windows_separators(path)
}

#[cfg(not(windows))]
fn native_path_for_glob_match(path: &str) -> Cow<'_, str> {
    Cow::Borrowed(path)
}

#[cfg(windows)]
fn vfs_path_from_native_path(path: &str) -> String {
    if is_windows_drive_path(path) {
        format!("/{path}")
    } else {
        path.to_string()
    }
}

#[cfg(not(windows))]
fn vfs_path_from_native_path(path: &str) -> String {
    path.to_string()
}

#[cfg(windows)]
fn normalize_windows_separators(path: &str) -> Cow<'_, str> {
    if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    }
}

#[cfg(windows)]
fn is_windows_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

/// Extract the directory prefix from a glob pattern (everything before the
/// first wildcard character).
fn glob_base_dir(glob: &str) -> String {
    let wildcard_pos = glob
        .find('*')
        .unwrap_or(glob.len())
        .min(glob.find('?').unwrap_or(glob.len()));
    let prefix = &glob[..wildcard_pos];
    // Trim back to the last `/` to get a directory path.
    match prefix.rfind('/') {
        Some(pos) => prefix[..=pos].to_string(),
        None => ".".to_string(),
    }
}

fn glob_to_regex(glob: &str) -> regex::Regex {
    let mut re = String::from("^");
    let bytes = glob.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            re.push_str(".*");
            i += 2;
            if i < bytes.len() && bytes[i] == b'/' {
                i += 1;
            }
        } else if bytes[i] == b'*' {
            re.push_str("[^/]*");
            i += 1;
        } else if bytes[i] == b'?' {
            re.push_str("[^/]");
            i += 1;
        } else {
            let ch = bytes[i] as char;
            if ".+^${}()|[]\\".contains(ch) {
                re.push('\\');
            }
            re.push(ch);
            i += 1;
        }
    }
    re.push('$');
    regex::Regex::new(&re).unwrap_or_else(|_| regex::Regex::new("$^").unwrap())
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(windows)]
    fn read_many_accepts_vfs_drive_globs_and_returns_vfs_paths() {
        use bex_project::BulkReadFileSystem;

        let root = std::env::temp_dir().join(format!(
            "baml_native_vfs_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let baml_src = root.join("baml_src");
        std::fs::create_dir_all(&baml_src).unwrap();
        std::fs::write(
            baml_src.join("agent_flow.baml"),
            "function Foo() -> string { \"x\" }",
        )
        .unwrap();
        std::fs::write(baml_src.join("clients.baml"), "client<llm> FooClient {}").unwrap();

        let native_root = root.to_string_lossy().replace('\\', "/");
        let glob = format!("/{native_root}/baml_src/**/*.baml");
        let mut entries = super::NativeVfs::new().read_many(&glob).unwrap();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|(path, _)| path.starts_with('/')));
        assert!(entries.iter().all(|(path, _)| !path.contains('\\')));
        assert!(
            entries
                .iter()
                .any(|(path, _)| path.ends_with("/clients.baml"))
        );
        assert!(
            entries
                .iter()
                .any(|(path, _)| path.ends_with("/agent_flow.baml"))
        );

        let _ = std::fs::remove_dir_all(root);
    }
}

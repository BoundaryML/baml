use std::{borrow::Cow, io::Read};

use crate::LspError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FsPath(String);

impl FsPath {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(path: String) -> Self {
        Self(path)
    }

    pub fn from_vfs(vfs_path: &vfs::VfsPath) -> Self {
        Self(vfs_path.as_str().to_string())
    }

    pub fn as_path(&self) -> &std::path::Path {
        std::path::Path::new(self.0.as_str())
    }
}

/// Extension of `vfs::FileSystem` that supports bulk-reading files matching a
/// glob pattern in a single call. This avoids repeated WASM-JS boundary
/// crossings when loading project sources.
pub trait BulkReadFileSystem: vfs::FileSystem {
    /// Return all files whose absolute paths match `glob`.
    /// Standard glob syntax: `*` (single segment), `**` (recursive), `?` (one char).
    /// We allow this to be overridden by the implementation for performance reasons.
    /// e.g. for WASM, we can prevent repeated WASM-JS boundary crossings by using a single method.
    fn read_many(&self, glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>>;
}

pub trait DefaultBulkReadFileSystem {}

impl<T: DefaultBulkReadFileSystem + vfs::FileSystem + Clone> BulkReadFileSystem for T {
    fn read_many(&self, glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
        let pattern = glob_to_regex(glob);
        let root = vfs::VfsPath::new(self.clone());
        let mut results = Vec::new();
        for entry in root.walk_dir()?.filter_map(Result::ok) {
            let path_str = entry.as_str().to_string();
            if !pattern.is_match(&path_str) {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if meta.file_type == vfs::VfsFileType::File {
                    if let Ok(mut reader) = entry.open_file() {
                        let mut buf = Vec::new();
                        if reader.read_to_end(&mut buf).is_ok() {
                            results.push((path_str, buf));
                        }
                    }
                }
            }
        }
        Ok(results)
    }
}

/// Minimal glob-to-regex converter.
struct GlobPattern {
    re: regex::Regex,
}

impl GlobPattern {
    fn is_match(&self, s: &str) -> bool {
        self.re.is_match(s)
    }
}

fn glob_to_regex(glob: &str) -> GlobPattern {
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
    GlobPattern {
        re: regex::Regex::new(&re).unwrap_or_else(|_| regex::Regex::new("$^").unwrap()),
    }
}

#[derive(Debug, Clone)]
pub struct BamlVFS {
    fs: std::sync::Arc<Box<dyn BulkReadFileSystem>>,
}

impl BamlVFS {
    pub fn new(fs: std::sync::Arc<Box<dyn BulkReadFileSystem>>) -> Self {
        Self { fs }
    }

    pub fn read_many(&self, glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
        self.fs.read_many(glob)
    }

    #[allow(clippy::unused_self)]
    fn get_cwd(&self) -> std::path::PathBuf {
        static CWD: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        CWD.get_or_init(|| {
            #[cfg(target_arch = "wasm32")]
            {
                std::path::PathBuf::from("/")
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("~/.baml"))
            }
        })
        .clone()
    }

    pub(crate) fn get_path_from_str(
        &self,
        raw: &FsPath,
        context: &'static str,
    ) -> Result<vfs::VfsPath, LspError> {
        self.get_path_from_path(raw.as_path(), context)
    }

    pub(crate) fn get_path_from_path(
        &self,
        raw: &std::path::Path,
        context: &'static str,
    ) -> Result<vfs::VfsPath, LspError> {
        let vfs_path = vfs::VfsPath::from(self.clone());
        let raw = normalize_windows_drive_path(raw);
        let raw = raw.as_ref();
        #[cfg(target_arch = "wasm32")]
        let is_absolute = raw.starts_with("/");
        #[cfg(not(target_arch = "wasm32"))]
        let is_absolute = raw.is_absolute();
        #[allow(clippy::implicit_clone)]
        let raw: std::path::PathBuf = if !is_absolute {
            self.get_cwd().join(raw)
        } else {
            raw.to_path_buf()
        };

        let path_as_str = path_for_vfs_join(&raw);
        vfs_path
            .join(path_as_str.as_ref())
            .map_err(|e| LspError::InvalidPath {
                path: raw.clone(),
                message: format!("{context}: {e}"),
            })
    }
}

#[cfg(windows)]
fn normalize_windows_drive_path(path: &std::path::Path) -> Cow<'_, std::path::Path> {
    let path_as_str = path.to_string_lossy();
    let slash_path = path_as_str.replace('\\', "/");
    let Some(stripped) = slash_path.strip_prefix('/') else {
        return Cow::Borrowed(path);
    };

    if is_windows_drive_path(stripped) {
        Cow::Owned(std::path::PathBuf::from(stripped))
    } else {
        Cow::Borrowed(path)
    }
}

#[cfg(not(windows))]
fn normalize_windows_drive_path(path: &std::path::Path) -> Cow<'_, std::path::Path> {
    Cow::Borrowed(path)
}

#[cfg(windows)]
fn is_windows_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn path_for_vfs_join(path: &std::path::Path) -> Cow<'_, str> {
    normalize_path_separators_for_vfs(path.to_string_lossy())
}

#[cfg(windows)]
fn normalize_path_separators_for_vfs(path: Cow<'_, str>) -> Cow<'_, str> {
    if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        path
    }
}

#[cfg(not(windows))]
fn normalize_path_separators_for_vfs(path: Cow<'_, str>) -> Cow<'_, str> {
    path
}

impl vfs::FileSystem for BamlVFS {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        self.fs.read_dir(path)
    }

    fn create_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.fs.create_dir(path)
    }

    fn open_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        self.fs.open_file(path)
    }

    fn create_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.fs.create_file(path)
    }

    fn append_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.fs.append_file(path)
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        self.fs.metadata(path)
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        self.fs.exists(path)
    }

    fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        self.fs.remove_file(path)
    }

    fn remove_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.fs.remove_dir(path)
    }

    /// [`vfs::FileSystem`] trait requires [`std::time::SystemTime`] in the signature.
    #[allow(clippy::disallowed_types)]
    fn set_creation_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.fs.set_creation_time(path, time)
    }

    /// [`vfs::FileSystem`] trait requires [`std::time::SystemTime`] in the signature.
    #[allow(clippy::disallowed_types)]
    fn set_modification_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.fs.set_modification_time(path, time)
    }

    /// [`vfs::FileSystem`] trait requires [`std::time::SystemTime`] in the signature.
    #[allow(clippy::disallowed_types)]
    fn set_access_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.fs.set_access_time(path, time)
    }

    fn copy_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.fs.copy_file(src, dest)
    }

    fn move_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.fs.move_file(src, dest)
    }

    fn move_dir(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.fs.move_dir(src, dest)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(windows)]
    fn windows_paths_use_vfs_separators_for_parent_traversal() {
        let raw = std::path::Path::new(r"d:\ReframeWeb\agent-host\baml_src\agent_flow.baml");
        let path_as_str = super::path_for_vfs_join(raw);
        let vfs_path = vfs::VfsPath::new(vfs::MemoryFS::new())
            .join(path_as_str.as_ref())
            .unwrap();

        assert_eq!(
            vfs_path.as_str(),
            "/d:/ReframeWeb/agent-host/baml_src/agent_flow.baml"
        );
        assert_eq!(
            vfs_path.parent().as_str(),
            "/d:/ReframeWeb/agent-host/baml_src"
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_uri_drive_paths_are_treated_as_absolute() {
        let raw = std::path::Path::new(r"/d:/ReframeWeb/agent-host/baml_src/agent_flow.baml");
        let path = super::normalize_windows_drive_path(raw);

        assert!(path.is_absolute());
        assert_eq!(
            super::path_for_vfs_join(path.as_ref()).as_ref(),
            "d:/ReframeWeb/agent-host/baml_src/agent_flow.baml"
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_backslash_uri_drive_paths_are_treated_as_absolute() {
        let raw = std::path::Path::new(r"\d:\ReframeWeb\agent-host\baml_src\agent_flow.baml");
        let path = super::normalize_windows_drive_path(raw);

        assert!(path.is_absolute());
        assert_eq!(
            super::path_for_vfs_join(path.as_ref()).as_ref(),
            "d:/ReframeWeb/agent-host/baml_src/agent_flow.baml"
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn non_windows_paths_are_unchanged_for_vfs() {
        let raw = std::path::Path::new("/tmp/project/baml_src/main.baml");
        let path_as_str = super::path_for_vfs_join(raw);

        assert_eq!(path_as_str.as_ref(), "/tmp/project/baml_src/main.baml");
    }
}

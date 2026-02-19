use std::io::Read;

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

/// Thin wrapper so we can construct a `VfsPath` from a `&dyn vfs::FileSystem`
/// reference for the fallback `walk_dir` implementation.
#[allow(dead_code)]
struct WrapFs(&'static dyn vfs::FileSystem);

impl std::fmt::Debug for WrapFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WrapFs({:?})", self.0)
    }
}

impl vfs::FileSystem for WrapFs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        self.0.read_dir(path)
    }
    fn create_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.0.create_dir(path)
    }
    fn open_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        self.0.open_file(path)
    }
    fn create_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.0.create_file(path)
    }
    fn append_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        self.0.append_file(path)
    }
    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        self.0.metadata(path)
    }
    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        self.0.exists(path)
    }
    fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        self.0.remove_file(path)
    }
    fn remove_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.0.remove_dir(path)
    }
    fn set_creation_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.0.set_creation_time(path, time)
    }
    fn set_modification_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.0.set_modification_time(path, time)
    }
    fn set_access_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.0.set_access_time(path, time)
    }
    fn copy_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.0.copy_file(src, dest)
    }
    fn move_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.0.move_file(src, dest)
    }
    fn move_dir(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.0.move_dir(src, dest)
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

        let path_as_str = raw.to_string_lossy();
        vfs_path
            .join(path_as_str)
            .map_err(|e| LspError::InvalidPath {
                path: raw.clone(),
                message: format!("{context}: {e}"),
            })
    }
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

    fn set_creation_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.fs.set_creation_time(path, time)
    }

    fn set_modification_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.fs.set_modification_time(path, time)
    }

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

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
pub(crate) struct GlobPattern {
    re: regex::Regex,
}

impl GlobPattern {
    pub(crate) fn is_match(&self, s: &str) -> bool {
        self.re.is_match(s)
    }
}

pub(crate) fn glob_to_regex(glob: &str) -> GlobPattern {
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

/// In-memory `BulkReadFileSystem` backed by a `HashMap<String, Vec<u8>>`.
///
/// Path semantics: keys are absolute (start with `/`). `read_many(glob)`
/// matches keys against the glob with the same syntax used by `glob_to_regex`.
/// `exists`/`metadata` return File for any key present, and Directory for any
/// path that is a prefix of at least one key.
#[derive(Debug, Clone)]
pub struct InMemoryFs {
    inner: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>>,
}

impl InMemoryFs {
    pub fn new(files: std::collections::HashMap<String, Vec<u8>>) -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::RwLock::new(files)),
        }
    }

    /// Replace the file set wholesale.
    pub fn replace(&self, files: std::collections::HashMap<String, Vec<u8>>) {
        *self.inner.write().unwrap() = files;
    }
}

impl vfs::FileSystem for InMemoryFs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let inner = self.inner.read().unwrap();
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };
        let mut names = std::collections::BTreeSet::new();
        for key in inner.keys() {
            if let Some(rest) = key.strip_prefix(&prefix) {
                let component = rest.split('/').next().unwrap_or("");
                if !component.is_empty() {
                    names.insert(component.to_string());
                }
            }
        }
        Ok(Box::new(names.into_iter()))
    }

    fn create_dir(&self, _path: &str) -> vfs::VfsResult<()> {
        Ok(())
    }

    fn open_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        let inner = self.inner.read().unwrap();
        match inner.get(path) {
            Some(bytes) => Ok(Box::new(std::io::Cursor::new(bytes.clone()))),
            None => Err(vfs::VfsError::from(vfs::error::VfsErrorKind::FileNotFound)),
        }
    }

    fn create_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        let mut inner = self.inner.write().unwrap();
        inner.insert(path.to_string(), Vec::new());
        Ok(Box::new(InMemoryWriter {
            path: path.to_string(),
            buffer: Vec::new(),
            inner: self.inner.clone(),
        }))
    }

    fn append_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        let inner = self.inner.read().unwrap();
        let initial = inner.get(path).cloned().unwrap_or_default();
        drop(inner);
        Ok(Box::new(InMemoryWriter {
            path: path.to_string(),
            buffer: initial,
            inner: self.inner.clone(),
        }))
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        let inner = self.inner.read().unwrap();
        if let Some(bytes) = inner.get(path) {
            return Ok(vfs::VfsMetadata {
                file_type: vfs::VfsFileType::File,
                len: bytes.len() as u64,
                created: None,
                modified: None,
                accessed: None,
            });
        }
        let dir_prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };
        // Empty path means root — always considered a directory.
        if path.is_empty() || path == "/" || inner.keys().any(|k| k.starts_with(&dir_prefix)) {
            return Ok(vfs::VfsMetadata {
                file_type: vfs::VfsFileType::Directory,
                len: 0,
                created: None,
                modified: None,
                accessed: None,
            });
        }
        Err(vfs::VfsError::from(vfs::error::VfsErrorKind::FileNotFound))
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        Ok(self.metadata(path).is_ok())
    }

    fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        let mut inner = self.inner.write().unwrap();
        inner.remove(path);
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> vfs::VfsResult<()> {
        let mut inner = self.inner.write().unwrap();
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };
        inner.retain(|k, _| !k.starts_with(&prefix));
        Ok(())
    }

    #[allow(clippy::disallowed_types)]
    fn set_creation_time(&self, _path: &str, _time: std::time::SystemTime) -> vfs::VfsResult<()> {
        Ok(())
    }

    #[allow(clippy::disallowed_types)]
    fn set_modification_time(
        &self,
        _path: &str,
        _time: std::time::SystemTime,
    ) -> vfs::VfsResult<()> {
        Ok(())
    }

    #[allow(clippy::disallowed_types)]
    fn set_access_time(&self, _path: &str, _time: std::time::SystemTime) -> vfs::VfsResult<()> {
        Ok(())
    }

    fn copy_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        let mut inner = self.inner.write().unwrap();
        let data = inner
            .get(src)
            .cloned()
            .ok_or_else(|| vfs::VfsError::from(vfs::error::VfsErrorKind::FileNotFound))?;
        inner.insert(dest.to_string(), data);
        Ok(())
    }

    fn move_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        let mut inner = self.inner.write().unwrap();
        let data = inner
            .remove(src)
            .ok_or_else(|| vfs::VfsError::from(vfs::error::VfsErrorKind::FileNotFound))?;
        inner.insert(dest.to_string(), data);
        Ok(())
    }

    fn move_dir(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        let mut inner = self.inner.write().unwrap();
        let src_prefix = if src.ends_with('/') {
            src.to_string()
        } else {
            format!("{src}/")
        };
        let dest_prefix = if dest.ends_with('/') {
            dest.to_string()
        } else {
            format!("{dest}/")
        };
        let moves: Vec<(String, String, Vec<u8>)> = inner
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix(&src_prefix)
                    .map(|rest| (k.clone(), format!("{dest_prefix}{rest}"), v.clone()))
            })
            .collect();
        for (old, new, data) in moves {
            inner.remove(&old);
            inner.insert(new, data);
        }
        Ok(())
    }
}

impl BulkReadFileSystem for InMemoryFs {
    fn read_many(&self, glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
        let pattern = glob_to_regex(glob);
        let inner = self.inner.read().unwrap();
        let mut out = Vec::new();
        for (path, bytes) in inner.iter() {
            if pattern.is_match(path) {
                out.push((path.clone(), bytes.clone()));
            }
        }
        Ok(out)
    }
}

struct InMemoryWriter {
    path: String,
    buffer: Vec<u8>,
    inner: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>>,
}

impl std::io::Write for InMemoryWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner
            .write()
            .unwrap()
            .insert(self.path.clone(), self.buffer.clone());
        Ok(())
    }
}

impl std::io::Seek for InMemoryWriter {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Start(p) => Ok(p),
            std::io::SeekFrom::Current(_) | std::io::SeekFrom::End(_) => Ok(self.buffer.len() as u64),
        }
    }
}

impl Drop for InMemoryWriter {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.inner.write() {
            guard.insert(self.path.clone(), std::mem::take(&mut self.buffer));
        }
    }
}

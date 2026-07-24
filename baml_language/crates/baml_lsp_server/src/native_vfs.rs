//! Native filesystem VFS for `bex_project::BamlVFS`.
//!
//! Implements `vfs::FileSystem` over `std::fs` and implements
//! `BulkReadFileSystem` by walking only the glob's base directory (not the
//! entire filesystem).

use std::io::Read;

use baml_path::{NativePathBuf, VfsPathBuf};

/// Native filesystem adapter for BAML's absolute, slash-oriented VFS paths.
///
/// This adapter has no configurable base directory: each VFS path identifies
/// an absolute path in the host filesystem. `/workspace/...` remains rooted at
/// `/` on Unix, while `/C:/...` and `/server/share/...` map to drive-rooted and
/// UNC paths on Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeVfs;

impl NativeVfs {
    pub fn new() -> Self {
        Self
    }

    fn native_path(path: &str) -> vfs::VfsResult<NativePathBuf> {
        let path = VfsPathBuf::new(path.to_string())?;
        Ok(NativePathBuf::try_from(&path)?)
    }
}

impl vfs::FileSystem for NativeVfs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let native = Self::native_path(path)?;
        let mut names = Vec::new();
        for entry in std::fs::read_dir(native.as_path())? {
            let name = entry?.file_name().into_string().map_err(|name| {
                vfs::VfsError::from(vfs::error::VfsErrorKind::Other(format!(
                    "path is not valid Unicode: {name:?}"
                )))
            })?;
            names.push(name);
        }
        Ok(Box::new(names.into_iter()))
    }

    fn create_dir(&self, path: &str) -> vfs::VfsResult<()> {
        let native = Self::native_path(path)?;
        std::fs::create_dir(native.as_path()).map_err(|error| {
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return error.into();
            }
            match std::fs::metadata(native.as_path()) {
                Ok(metadata) if metadata.is_dir() => {
                    vfs::VfsError::from(vfs::error::VfsErrorKind::DirectoryExists)
                }
                Ok(_) => vfs::VfsError::from(vfs::error::VfsErrorKind::FileExists),
                Err(error) => error.into(),
            }
        })
    }

    fn open_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        Ok(Box::new(std::fs::File::open(
            Self::native_path(path)?.as_path(),
        )?))
    }

    fn create_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        Ok(Box::new(std::fs::File::create(
            Self::native_path(path)?.as_path(),
        )?))
    }

    fn append_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        Ok(Box::new(
            std::fs::OpenOptions::new()
                .append(true)
                .open(Self::native_path(path)?.as_path())?,
        ))
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        let metadata = std::fs::metadata(Self::native_path(path)?.as_path())?;
        let is_dir = metadata.is_dir();
        Ok(vfs::VfsMetadata {
            file_type: if is_dir {
                vfs::VfsFileType::Directory
            } else {
                vfs::VfsFileType::File
            },
            len: if is_dir { 0 } else { metadata.len() },
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            accessed: metadata.accessed().ok(),
        })
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        Ok(Self::native_path(path)?.as_path().exists())
    }

    fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        Ok(std::fs::remove_file(Self::native_path(path)?.as_path())?)
    }

    fn remove_dir(&self, path: &str) -> vfs::VfsResult<()> {
        Ok(std::fs::remove_dir(Self::native_path(path)?.as_path())?)
    }
}

impl bex_project::BulkReadFileSystem for NativeVfs {
    fn read_many(&self, glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
        // Extract the base directory from the glob (everything before the
        // first `*` or `?` wildcard). This lets us walk only the relevant
        // subtree instead of the entire filesystem.
        let base_dir = glob_base_dir(glob);
        let pattern = glob_to_regex(glob);

        let base_dir = base_dir.trim_end_matches('/');
        let base_dir = if base_dir.is_empty() { "/" } else { base_dir };
        let base = VfsPathBuf::new(base_dir.to_string())?;
        let base = NativePathBuf::try_from(&base)?;
        if !base.as_path().is_dir() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        walk_dir_native(base.as_path(), &pattern, &mut results)?;
        Ok(results)
    }
}

fn walk_dir_native(
    dir: &std::path::Path,
    pattern: &regex::Regex,
    results: &mut Vec<(String, Vec<u8>)>,
) -> vfs::VfsResult<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir_native(&path, pattern, results)?;
        } else if path.is_file() {
            let native_path = NativePathBuf::new(path.clone())?;
            let vfs_path = VfsPathBuf::try_from(&native_path)?;
            if pattern.is_match(vfs_path.as_str()) {
                let mut file = std::fs::File::open(&path)?;
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)?;
                results.push((vfs_path.into_string(), buf));
            }
        }
    }
    Ok(())
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
    use std::{
        io::{Read as _, Write as _},
        sync::Arc,
    };

    use bex_project::{BexLsp as _, BulkReadFileSystem as _};
    use vfs::FileSystem as _;

    use super::*;

    fn vfs_path(path: &std::path::Path) -> String {
        let native = NativePathBuf::new(path.to_path_buf()).unwrap();
        VfsPathBuf::try_from(&native).unwrap().into_string()
    }

    struct NoopSender;

    impl bex_project::LspClientSenderTrait for NoopSender {
        fn send_notification(
            &self,
            _msg: lsp_server::Notification,
        ) -> Result<(), bex_project::LspError> {
            Ok(())
        }

        fn send_response_impl(
            &self,
            _msg: lsp_server::Response,
        ) -> Result<(), bex_project::LspError> {
            Ok(())
        }

        fn make_request(&self, _msg: lsp_server::Request) -> Result<(), bex_project::LspError> {
            Ok(())
        }
    }

    struct NoopPlaygroundSender;

    impl bex_project::PlaygroundSender for NoopPlaygroundSender {
        fn send_playground_notification(&self, _notification: bex_project::PlaygroundNotification) {
        }
    }

    #[test]
    fn native_vfs_loads_discovers_and_saves_encoded_paths() {
        let temp = tempfile::Builder::new()
            .prefix("baml native vfs % ")
            .tempdir()
            .unwrap();
        let source_dir = temp.path().join("baml_src");
        std::fs::create_dir(&source_dir).unwrap();
        let existing = source_dir.join("main.baml");
        std::fs::write(&existing, "function Main() -> string { client Test }").unwrap();
        std::fs::write(source_dir.join("notes.txt"), "not BAML").unwrap();

        let fs = NativeVfs::new();
        let existing_vfs = vfs_path(&existing);
        let mut contents = String::new();
        fs.open_file(&existing_vfs)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert!(contents.contains("function Main"));

        let glob = format!("{}/**/*.baml", vfs_path(&source_dir));
        let discovered = fs.read_many(&glob).unwrap();
        assert_eq!(
            discovered,
            vec![(existing_vfs.clone(), contents.as_bytes().to_vec())]
        );

        let saved = source_dir.join("saved %.baml");
        let saved_vfs = vfs_path(&saved);
        fs.create_file(&saved_vfs)
            .unwrap()
            .write_all(b"// saved")
            .unwrap();
        assert_eq!(std::fs::read_to_string(saved).unwrap(), "// saved");
    }

    #[test]
    fn native_lsp_discovers_and_loads_an_absolute_workspace() {
        let temp = tempfile::Builder::new()
            .prefix("baml native lsp % ")
            .tempdir()
            .unwrap();
        let source = temp.path().join("baml_src/main.baml");
        std::fs::create_dir(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "function Main() -> string { client Test }").unwrap();

        let native_fs: Arc<Box<dyn bex_project::BulkReadFileSystem>> =
            Arc::new(Box::new(NativeVfs::new()));
        let lsp = bex_project::new_lsp(
            Arc::new(|_| Arc::new(sys_ops::SysOpsBuilder::new().build())),
            Arc::new(NoopSender),
            Arc::new(NoopPlaygroundSender),
            bex_project::BamlVFS::new(native_fs),
            bex_project::BackgroundSpawner::new(),
        );

        let expected_root = vfs_path(temp.path());
        assert_eq!(
            lsp.initialize_workspace_roots(vec![temp.path().to_path_buf()])
                .unwrap(),
            vec![expected_root.clone()]
        );
        let files = lsp.playground_source_files(&expected_root).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, vfs_path(&source));
        assert!(files[0].content.contains("function Main"));
        assert!(lsp.project_generation(&expected_root).is_some());
    }
}

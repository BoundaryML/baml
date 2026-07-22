use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativePathBuf;

/// An absolute, normalized path in BAML's slash-oriented VFS domain.
///
/// VFS paths always start with `/` and never contain empty, `.` or `..`
/// components. They are logical paths, not host operating-system paths.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfsPathBuf(String);

impl VfsPathBuf {
    pub fn new(path: String) -> Result<Self, FsPathError> {
        validate_vfs_path(&path)?;
        Ok(Self(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FsPathError {
    #[error("native path is not absolute: {0}")]
    NotAbsolute(PathBuf),

    #[error("native path is not valid Unicode: {0:?}")]
    NonUnicode(PathBuf),

    #[error("unsupported native path prefix: {0}")]
    UnsupportedPrefix(PathBuf),

    #[error("invalid VFS path: {0}")]
    InvalidVfsPath(String),
}

impl From<FsPathError> for vfs::VfsError {
    fn from(error: FsPathError) -> Self {
        vfs::VfsError::from(vfs::error::VfsErrorKind::Other(error.to_string()))
    }
}

fn validate_vfs_path(path: &str) -> Result<(), FsPathError> {
    if path == "/" {
        return Ok(());
    }

    let invalid = !path.starts_with('/')
        || path.ends_with('/')
        || path
            .split('/')
            .skip(1)
            .any(|component| component.is_empty() || matches!(component, "." | ".."));

    if invalid {
        return Err(FsPathError::InvalidVfsPath(path.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_vfs_paths_are_unrepresentable() {
        for path in [
            "relative/path",
            "/workspace/../secret",
            "/workspace/./main.baml",
            "/workspace//main.baml",
            "/workspace/",
        ] {
            assert!(matches!(
                VfsPathBuf::new(path.to_string()),
                Err(FsPathError::InvalidVfsPath(invalid)) if invalid == path
            ));
        }
    }

    #[test]
    fn root_is_a_valid_vfs_path() {
        assert_eq!(VfsPathBuf::new("/".to_string()).unwrap().as_str(), "/");
    }
}

//! Path vocabulary shared with the SDK bridges.

/// A source-file path as the embedding host spelled it (relative or
/// absolute, forward slashes), used to key the source map handed to
/// [`crate::new`].
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

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

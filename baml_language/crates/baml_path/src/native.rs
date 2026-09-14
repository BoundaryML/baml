use std::path::{Path, PathBuf};

use crate::{FsPathError, VfsPathBuf};

#[cfg(not(windows))]
#[path = "native/non_windows.rs"]
mod platform;
#[cfg(windows)]
#[path = "native/windows.rs"]
mod platform;

/// An absolute, normalized path in the current host operating system's native
/// path domain.
///
/// This type deliberately does not exist on WASM targets: browser/WASM paths
/// belong to the VFS domain regardless of the client operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePathBuf(PathBuf);

impl NativePathBuf {
    pub fn new(path: PathBuf) -> Result<Self, FsPathError> {
        if !path.is_absolute() {
            return Err(FsPathError::NotAbsolute(path));
        }

        // Round-trip through the VFS codec so every value has one normalized,
        // reversible native spelling.
        let vfs_path = platform::vfs_path_from_native_path(&path)?;
        Ok(Self(platform::native_path_from_vfs_path(&vfs_path)?))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl TryFrom<&NativePathBuf> for VfsPathBuf {
    type Error = FsPathError;

    fn try_from(path: &NativePathBuf) -> Result<Self, Self::Error> {
        platform::vfs_path_from_native_path(path.as_path())
    }
}

impl TryFrom<&VfsPathBuf> for NativePathBuf {
    type Error = FsPathError;

    fn try_from(path: &VfsPathBuf) -> Result<Self, Self::Error> {
        NativePathBuf::new(platform::native_path_from_vfs_path(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_native_paths_are_unrepresentable() {
        assert!(matches!(
            NativePathBuf::new(PathBuf::from("relative/path")),
            Err(FsPathError::NotAbsolute(_))
        ));
    }

    #[test]
    fn native_paths_round_trip_through_the_vfs_domain() {
        let native = std::env::current_dir()
            .unwrap()
            .join("workspace % 01")
            .join("main.baml");
        let native = NativePathBuf::new(native).unwrap();
        let vfs = VfsPathBuf::try_from(&native).unwrap();
        let decoded = NativePathBuf::try_from(&vfs).unwrap();

        assert_eq!(decoded, native);
    }

    #[test]
    fn native_paths_have_one_normalized_spelling() {
        let base = std::env::current_dir().unwrap();
        let native =
            NativePathBuf::new(base.join("workspace").join("..").join("main.baml")).unwrap();

        assert_eq!(native.as_path(), base.join("main.baml"));
    }
}

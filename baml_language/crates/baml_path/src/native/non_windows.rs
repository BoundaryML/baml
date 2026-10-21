use std::path::{Component, Path, PathBuf};

use crate::{FsPathError, VfsPathBuf};

pub(super) fn vfs_path_from_native_path(path: &Path) -> Result<VfsPathBuf, FsPathError> {
    if !path.is_absolute() {
        return Err(FsPathError::NotAbsolute(path.to_path_buf()));
    }

    let mut tail = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                tail.pop();
            }
            Component::Normal(component) => {
                let component = component
                    .to_str()
                    .ok_or_else(|| FsPathError::NonUnicode(path.to_path_buf()))?;
                tail.push(component);
            }
            Component::Prefix(_) => {
                return Err(FsPathError::UnsupportedPrefix(path.to_path_buf()));
            }
        }
    }

    let encoded = if tail.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", tail.join("/"))
    };
    VfsPathBuf::new(encoded)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "keep the platform conversion contract fallible for Windows"
)]
pub(super) fn native_path_from_vfs_path(path: &VfsPathBuf) -> Result<PathBuf, FsPathError> {
    Ok(PathBuf::from(path.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_paths_keep_their_vfs_spelling() {
        let native = Path::new("/workspace/main.baml");
        let vfs = vfs_path_from_native_path(native).unwrap();

        assert_eq!(vfs.as_str(), "/workspace/main.baml");
        assert_eq!(
            native_path_from_vfs_path(&vfs).unwrap(),
            native.to_path_buf()
        );
    }
}

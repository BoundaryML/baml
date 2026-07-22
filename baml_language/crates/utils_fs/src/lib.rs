use std::path::{Path, PathBuf};

/// An absolute path in the host operating system's native path domain.
///
/// Native paths and VFS path strings are deliberately distinct. On
/// Windows, VFS paths use `/`, prefix drive paths with `/`, and encode UNC
/// roots as `/server/share`. Conversions between the two domains belong here
/// so callers cannot accidentally pass an internal VFS spelling to
/// [`std::path::Path`]. Component casing is preserved; filesystem identity is
/// a separate policy owned by the project registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePathBuf(PathBuf);

#[derive(Debug, thiserror::Error)]
pub enum NativePathError {
    #[error("native path is not absolute: {0}")]
    NotAbsolute(PathBuf),

    #[error("native path is not valid Unicode: {0:?}")]
    NonUnicode(PathBuf),

    #[error("unsupported native path prefix: {0}")]
    UnsupportedPrefix(PathBuf),

    #[error("invalid native VFS path: {0}")]
    InvalidVfsPath(String),
}

impl From<NativePathError> for vfs::VfsError {
    fn from(error: NativePathError) -> Self {
        vfs::VfsError::from(vfs::error::VfsErrorKind::Other(error.to_string()))
    }
}

impl NativePathBuf {
    pub fn new(path: PathBuf) -> Result<Self, NativePathError> {
        if path.is_absolute() {
            Ok(Self(path))
        } else {
            Err(NativePathError::NotAbsolute(path))
        }
    }

    pub fn from_vfs_path(path: &str) -> Result<Self, NativePathError> {
        native_path_from_vfs_path(path).and_then(Self::new)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    pub fn to_vfs_path(&self) -> Result<String, NativePathError> {
        vfs_path_from_native_path(&self.0)
    }
}

fn validate_vfs_path(path: &str) -> Result<(), NativePathError> {
    if !path.starts_with('/')
        || path
            .split('/')
            .any(|component| matches!(component, "." | ".."))
    {
        return Err(NativePathError::InvalidVfsPath(path.to_string()));
    }
    Ok(())
}

#[cfg(not(windows))]
fn vfs_path_from_native_path(path: &Path) -> Result<String, NativePathError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| NativePathError::NonUnicode(path.to_path_buf()))
}

#[cfg(not(windows))]
fn native_path_from_vfs_path(path: &str) -> Result<PathBuf, NativePathError> {
    validate_vfs_path(path)?;
    Ok(PathBuf::from(path))
}

#[cfg(windows)]
fn vfs_path_from_native_path(path: &Path) -> Result<String, NativePathError> {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let prefix = match components.next() {
        Some(Component::Prefix(prefix)) => prefix.kind(),
        _ => return Err(NativePathError::NotAbsolute(path.to_path_buf())),
    };

    let mut root = match prefix {
        Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => {
            format!("/{}:", char::from(drive))
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            let server = server
                .to_str()
                .ok_or_else(|| NativePathError::NonUnicode(path.to_path_buf()))?;
            let share = share
                .to_str()
                .ok_or_else(|| NativePathError::NonUnicode(path.to_path_buf()))?;
            format!("/{server}/{share}")
        }
        Prefix::Verbatim(_) | Prefix::DeviceNS(_) => {
            return Err(NativePathError::UnsupportedPrefix(path.to_path_buf()));
        }
    };

    let mut tail = Vec::new();
    for component in components {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                tail.pop();
            }
            Component::Normal(component) => {
                let component = component
                    .to_str()
                    .ok_or_else(|| NativePathError::NonUnicode(path.to_path_buf()))?;
                tail.push(component);
            }
            Component::Prefix(_) => {
                return Err(NativePathError::UnsupportedPrefix(path.to_path_buf()));
            }
        }
    }

    for component in tail {
        root.push('/');
        root.push_str(component);
    }
    Ok(root)
}

#[cfg(windows)]
fn native_path_from_vfs_path(path: &str) -> Result<PathBuf, NativePathError> {
    validate_vfs_path(path)?;
    let components = path
        .strip_prefix('/')
        .ok_or_else(|| NativePathError::InvalidVfsPath(path.to_string()))?
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();

    let Some(first) = components.first() else {
        return Err(NativePathError::InvalidVfsPath(path.to_string()));
    };

    let (mut native, tail) = if first.len() == 2
        && first.as_bytes()[0].is_ascii_alphabetic()
        && first.as_bytes()[1] == b':'
    {
        (PathBuf::from(format!("{first}\\")), &components[1..])
    } else if components.len() >= 2 {
        (
            PathBuf::from(format!(r"\\{}\{}", components[0], components[1])),
            &components[2..],
        )
    } else {
        return Err(NativePathError::InvalidVfsPath(path.to_string()));
    };

    for component in tail {
        native.push(component);
    }
    Ok(native)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_native_paths_are_rejected() {
        assert!(matches!(
            NativePathBuf::new(PathBuf::from("relative/path")),
            Err(NativePathError::NotAbsolute(_))
        ));
    }

    #[test]
    fn vfs_paths_with_traversal_components_are_rejected() {
        for path in ["/workspace/../secret", "/workspace/./main.baml"] {
            assert!(matches!(
                NativePathBuf::from_vfs_path(path),
                Err(NativePathError::InvalidVfsPath(invalid)) if invalid == path
            ));
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_drive_paths_have_one_stable_vfs_spelling() {
        let native = std::env::current_dir()
            .unwrap()
            .join("workspace % 01")
            .join("main.baml");
        let slash_spelling = native.to_string_lossy().replace('\\', "/");
        let mixed_spelling = slash_spelling.replacen('/', "\\", 1);
        let verbatim_spelling = PathBuf::from(format!(r"\\?\{}", native.display()));

        let expected = NativePathBuf::new(native.clone())
            .unwrap()
            .to_vfs_path()
            .unwrap();
        for spelling in [PathBuf::from(mixed_spelling), verbatim_spelling] {
            assert_eq!(
                NativePathBuf::new(spelling).unwrap().to_vfs_path().unwrap(),
                expected
            );
        }

        assert_eq!(
            NativePathBuf::from_vfs_path(&expected)
                .unwrap()
                .into_path_buf(),
            native
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_unc_paths_round_trip_without_losing_the_share_root() {
        let native = PathBuf::from(r"\\server\share\workspace % 01\main.baml");
        let encoded = NativePathBuf::new(native.clone())
            .unwrap()
            .to_vfs_path()
            .unwrap();

        assert_eq!(encoded, "/server/share/workspace % 01/main.baml");
        assert_eq!(
            NativePathBuf::from_vfs_path(&encoded)
                .unwrap()
                .into_path_buf(),
            native
        );
    }
}

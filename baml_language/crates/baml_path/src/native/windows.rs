use std::path::{Component, Path, PathBuf, Prefix};

use crate::{FsPathError, VfsPathBuf};

pub(super) fn vfs_path_from_native_path(path: &Path) -> Result<VfsPathBuf, FsPathError> {
    let mut components = path.components();
    let prefix = match components.next() {
        Some(Component::Prefix(prefix)) => prefix.kind(),
        _ => return Err(FsPathError::NotAbsolute(path.to_path_buf())),
    };

    let mut root = match prefix {
        Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => {
            format!("/{}:", char::from(drive))
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            let server = server
                .to_str()
                .ok_or_else(|| FsPathError::NonUnicode(path.to_path_buf()))?;
            let share = share
                .to_str()
                .ok_or_else(|| FsPathError::NonUnicode(path.to_path_buf()))?;
            format!("/{server}/{share}")
        }
        Prefix::Verbatim(_) | Prefix::DeviceNS(_) => {
            return Err(FsPathError::UnsupportedPrefix(path.to_path_buf()));
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
                    .ok_or_else(|| FsPathError::NonUnicode(path.to_path_buf()))?;
                tail.push(component);
            }
            Component::Prefix(_) => {
                return Err(FsPathError::UnsupportedPrefix(path.to_path_buf()));
            }
        }
    }

    for component in tail {
        root.push('/');
        root.push_str(component);
    }
    VfsPathBuf::new(root)
}

pub(super) fn native_path_from_vfs_path(path: &VfsPathBuf) -> Result<PathBuf, FsPathError> {
    let components = path
        .as_str()
        .strip_prefix('/')
        .ok_or_else(|| FsPathError::InvalidVfsPath(path.as_str().to_string()))?
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();

    let Some(first) = components.first() else {
        return Err(FsPathError::InvalidVfsPath(path.as_str().to_string()));
    };
    if components.iter().any(|component| component.contains('\\')) {
        return Err(FsPathError::InvalidVfsPath(path.as_str().to_string()));
    }

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
        return Err(FsPathError::InvalidVfsPath(path.as_str().to_string()));
    };

    for component in tail {
        native.push(component);
    }
    Ok(native)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativePathBuf;

    #[test]
    fn windows_drive_paths_have_one_stable_vfs_spelling() {
        let native = PathBuf::from(r"C:\workspace % 01\main.baml");
        let slash_spelling = native.to_string_lossy().replace('\\', "/");
        let mixed_spelling = slash_spelling.replacen('/', "\\", 1);
        let verbatim_spelling = PathBuf::from(format!(r"\\?\{}", native.display()));

        let expected = VfsPathBuf::try_from(&NativePathBuf::new(native.clone()).unwrap()).unwrap();
        assert_eq!(expected.as_str(), "/C:/workspace % 01/main.baml");
        for spelling in [PathBuf::from(mixed_spelling), verbatim_spelling] {
            assert_eq!(
                VfsPathBuf::try_from(&NativePathBuf::new(spelling).unwrap()).unwrap(),
                expected
            );
        }

        let decoded = NativePathBuf::try_from(&expected).unwrap();
        assert_eq!(decoded.as_path(), native.as_path());
    }

    #[test]
    fn windows_unc_paths_round_trip_without_losing_the_share_root() {
        let native = PathBuf::from(r"\\server\share\workspace % 01\main.baml");
        let encoded = VfsPathBuf::try_from(&NativePathBuf::new(native.clone()).unwrap()).unwrap();

        assert_eq!(encoded.as_str(), "/server/share/workspace % 01/main.baml");
        let decoded = NativePathBuf::try_from(&encoded).unwrap();
        assert_eq!(decoded.as_path(), native.as_path());
    }
}

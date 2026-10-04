//! Native filesystem, path, archive and configuration primitives for BAML applications.
use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use bex_heap::{BexExternalValue, BexHeap};
use sys_ops::io::{self, CallId, SysOpContext, SysOpOutput, VmBamlError, VmRustFnError, owned};

use crate::{NativeSysOps, WorkingDir};

fn error(e: impl std::fmt::Display) -> VmBamlError {
    VmBamlError::Io {
        message: e.to_string(),
    }
}
fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, VmRustFnError> + Send + 'static,
) -> SysOpOutput<T> {
    SysOpOutput::async_op(async move { tokio::task::spawn_blocking(f).await.map_err(error)? })
}
fn path_string(path: impl AsRef<Path>) -> Result<String, VmRustFnError> {
    path.as_ref()
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| error("Path is not valid UTF-8").into())
}
pub(crate) fn sys_current_dir(native: &NativeSysOps) -> SysOpOutput<String> {
    let result = match &native.working_dir {
        WorkingDir::Fixed(path) => path_string(path),
        WorkingDir::Process => std::env::current_dir()
            .map_err(error)
            .map_err(Into::into)
            .and_then(path_string),
    };
    SysOpOutput::Ready(result)
}
pub(crate) fn sys_current_exe(_: &NativeSysOps) -> SysOpOutput<String> {
    SysOpOutput::Ready(
        std::env::current_exe()
            .map_err(error)
            .map_err(Into::into)
            .and_then(path_string),
    )
}
pub(crate) fn sys_home_dir(_: &NativeSysOps) -> SysOpOutput<Option<String>> {
    SysOpOutput::Ready(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|p| path_string(PathBuf::from(p)))
            .transpose(),
    )
}
pub(crate) fn sys_platform(_: &NativeSysOps) -> SysOpOutput<String> {
    SysOpOutput::ok(std::env::consts::OS.to_string())
}
pub(crate) fn sys_host_target(_: &NativeSysOps) -> SysOpOutput<String> {
    SysOpOutput::ok(env!("BAML_HOST_TARGET").to_string())
}

impl io::IoNamespacePath for NativeSysOps {
    fn join(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        parts: Vec<String>,
        _: &SysOpContext,
    ) -> SysOpOutput<String> {
        let path: PathBuf = parts.iter().collect();
        SysOpOutput::Ready(path_string(path))
    }
    fn parent(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        path: String,
        _: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::Ready(Path::new(&path).parent().map(path_string).transpose())
    }
    fn is_absolute(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        path: String,
        _: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::ok(Path::new(&path).is_absolute())
    }
    fn _absolute(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        path: String,
        base: Option<String>,
        _: &SysOpContext,
    ) -> SysOpOutput<String> {
        let result = (|| {
            let base = match base {
                Some(base) => self.working_dir.resolve(&base),
                None => match self.working_dir.for_child(None) {
                    Some(base) => base,
                    None => std::env::current_dir().map_err(error)?,
                },
            };
            let mut path = PathBuf::from(&path);
            if path == Path::new("~") || path.starts_with("~/") {
                if let Some(home) =
                    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
                {
                    path =
                        PathBuf::from(home).join(path.strip_prefix("~").unwrap_or(Path::new("")));
                }
            }
            // Keep `..` on Unix: resolving it lexically changes the meaning
            // of symlink paths, including paths to files that do not yet exist.
            // Also make an explicitly relative base absolute in process mode.
            path_string(std::path::absolute(base.join(path)).map_err(error)?)
        })();
        SysOpOutput::Ready(result)
    }
}

impl io::IoNamespaceCrypto for NativeSysOps {
    fn sha256(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        data: Vec<u8>,
        _: &SysOpContext,
    ) -> SysOpOutput<String> {
        use sha2::{Digest, Sha256};
        SysOpOutput::ok(format!("{:x}", Sha256::digest(data)))
    }
}
fn archive_suffix(format: BexExternalValue) -> Result<&'static str, VmRustFnError> {
    match format {
        BexExternalValue::String(s) if s.as_str() == "zip" => Ok("zip"),
        BexExternalValue::String(s) if s.as_str() == "tar.gz" => Ok("tar.gz"),
        BexExternalValue::Union { value, .. } => archive_suffix(*value),
        _ => Err(error("Unsupported archive format").into()),
    }
}

fn archive_path(path: &Path) -> Result<PathBuf, VmRustFnError> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => result.push(part),
            std::path::Component::CurDir => {}
            _ => return Err(error(format!("Unsafe archive path: {}", path.display())).into()),
        }
    }
    Ok(result)
}
fn zip_is_regular(mode: Option<u32>) -> bool {
    matches!(mode.unwrap_or(0) & 0o170_000, 0 | 0o100_000)
}
fn extract_archive(data: &[u8], format: &str, dest: &Path) -> Result<(), VmRustFnError> {
    if format == "zip" {
        let mut archive = zip::ZipArchive::new(Cursor::new(data)).map_err(error)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(error)?;
            let path = archive_path(Path::new(file.name()))?;
            let out = dest.join(path);
            if file.is_dir() {
                std::fs::create_dir_all(out).map_err(error)?;
            } else if zip_is_regular(file.unix_mode()) {
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).map_err(error)?;
                }
                let mut output = std::fs::File::create(&out).map_err(error)?;
                std::io::copy(&mut file, &mut output).map_err(error)?;
                #[cfg(unix)]
                if let Some(mode) = file.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o777))
                        .map_err(error)?;
                }
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(data)));
        for entry in archive.entries().map_err(error)? {
            let mut entry = entry.map_err(error)?;
            let path = archive_path(&entry.path().map_err(error)?)?;
            let out = dest.join(path);
            if entry.header().entry_type().is_dir() {
                std::fs::create_dir_all(out).map_err(error)?;
            } else if entry.header().entry_type().is_file() {
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).map_err(error)?;
                }
                entry.unpack(out).map_err(error)?;
            }
        }
    }
    Ok(())
}
fn read_archive_file(data: &[u8], format: &str, name: &str) -> Result<Vec<u8>, VmRustFnError> {
    let wanted = archive_path(Path::new(name))?;
    let mut bytes = Vec::new();
    if format == "zip" {
        let mut archive = zip::ZipArchive::new(Cursor::new(data)).map_err(error)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(error)?;
            if archive_path(Path::new(file.name()))? == wanted
                && !file.is_dir()
                && zip_is_regular(file.unix_mode())
            {
                file.read_to_end(&mut bytes).map_err(error)?;
                return Ok(bytes);
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(data)));
        for entry in archive.entries().map_err(error)? {
            let mut entry = entry.map_err(error)?;
            if archive_path(&entry.path().map_err(error)?)? == wanted
                && entry.header().entry_type().is_file()
            {
                entry.read_to_end(&mut bytes).map_err(error)?;
                return Ok(bytes);
            }
        }
    }
    Err(error(format!("Archive does not contain file {name}")).into())
}
impl io::IoNamespaceArchive for NativeSysOps {
    fn extract(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        data: Vec<u8>,
        format: BexExternalValue,
        destination: String,
        _: &SysOpContext,
    ) -> SysOpOutput<()> {
        let dest = self.working_dir.resolve(&destination);
        blocking(move || {
            let suffix = archive_suffix(format)?;
            std::fs::create_dir_all(&dest).map_err(error)?;
            if std::fs::symlink_metadata(&dest)
                .map_err(error)?
                .file_type()
                .is_symlink()
                || std::fs::read_dir(&dest).map_err(error)?.next().is_some()
            {
                return Err(
                    error("Archive destination must be an empty directory, not a symlink").into(),
                );
            }
            extract_archive(&data, suffix, &dest)
        })
    }
    fn read_file(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        data: Vec<u8>,
        format: BexExternalValue,
        name: String,
        _: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        blocking(move || read_archive_file(&data, archive_suffix(format)?, &name))
    }
}

pub(crate) fn fs_metadata(native: &NativeSysOps, path: &str) -> SysOpOutput<owned::fs::Metadata> {
    let path = native.working_dir.resolve(path);
    blocking(move || {
        let metadata = std::fs::metadata(path).map_err(error)?;
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = metadata.is_file();
        Ok(owned::fs::Metadata {
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            is_executable: executable,
            modified_ms: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_millis()).ok()),
        })
    })
}
pub(crate) fn fs_rename(native: &NativeSysOps, source: &str, destination: &str) -> SysOpOutput<()> {
    let source = native.working_dir.resolve(source);
    let destination = native.working_dir.resolve(destination);
    blocking(move || std::fs::rename(source, destination).map_err(|e| error(e).into()))
}
pub(crate) fn fs_canonicalize(native: &NativeSysOps, path: &str) -> SysOpOutput<String> {
    let path = native.working_dir.resolve(path);
    blocking(move || path_string(std::fs::canonicalize(path).map_err(error)?))
}
pub(crate) fn fs_write_atomic(
    native: &NativeSysOps,
    path: &str,
    content: Vec<u8>,
) -> SysOpOutput<()> {
    let path = native.working_dir.resolve(path);
    blocking(move || {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(error)?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
        tmp.write_all(&content).map_err(error)?;
        if let Ok(metadata) = std::fs::metadata(&path) {
            tmp.as_file()
                .set_permissions(metadata.permissions())
                .map_err(error)?;
        }
        tmp.as_file().sync_all().map_err(error)?;
        tmp.persist(&path).map_err(error)?;
        // Persist the renamed directory entry as well as the file contents.
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(error)?;
        Ok(())
    })
}
type LockHandle = Mutex<Option<std::fs::File>>;
pub(crate) fn fs_lock(
    native: &NativeSysOps,
    path: &str,
    timeout_ms: i64,
) -> SysOpOutput<owned::fs::Lock> {
    let path = native.working_dir.resolve(path);
    blocking(move || {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(error)?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(error)?;
        let started = Instant::now();
        loop {
            match fs2::FileExt::try_lock_exclusive(&file) {
                Ok(()) => {
                    return Ok(owned::fs::Lock {
                        _handle: Arc::new(Mutex::new(Some(file))),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() >= Duration::from_millis(timeout_ms.max(0).cast_unsigned())
                    {
                        return Err(error("Timed out waiting for filesystem lock").into());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(error(e).into()),
            }
        }
    })
}
impl io::IoClassFsLock for NativeSysOps {
    fn close(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        lock: owned::fs::Lock,
        _: &SysOpContext,
    ) -> SysOpOutput<()> {
        if let Some(handle) = lock._handle.downcast_ref::<LockHandle>() {
            if let Ok(mut file) = handle.lock() {
                file.take();
            }
        }
        SysOpOutput::ok(())
    }
}

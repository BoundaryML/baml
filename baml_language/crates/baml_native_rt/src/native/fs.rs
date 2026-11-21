//! `baml.fs`: every operation is a job on the runtime's own Tokio threads,
//! and the task that asked is parked meanwhile. A `File` and a `Lock` own
//! what the operating system gave them as Rust state the heap drops, which
//! closes it, when the object is dead.

use std::{
    io::{SeekFrom, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant, UNIX_EPOCH},
};

use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _};

use super::{
    Arr, Bytes, Elem, NativeBamlFs, NativeBamlFsBamlIoReadForFile, NativeBamlFsBamlIoWriteForFile,
    NativeBamlFsFile, NativeBamlFsLock, Natives, Obj, ObjRef, Opaque, Root, Scope, Str, Thrown,
    failure::Failure,
};
use crate::layout::{NAMED_FS_DIR_ENTRY, NAMED_FS_FILE, NAMED_FS_LOCK, NAMED_FS_METADATA};

/// The most one `read` returns, whatever limit it is given.
pub(super) const MAX_READ_CHUNK: usize = 64 * 1024;

/// An open file, until it is closed.
struct FileHandle(tokio::sync::Mutex<Option<tokio::fs::File>>);

/// A held lock: the locked file, until the lock is closed.
struct LockHandle(Mutex<Option<std::fs::File>>);

fn closed() -> Failure {
    Failure::Io("File is closed".to_owned())
}

/// The handle of the `File` at `file`, for a job to use.
fn handle(scope: &Scope<'_>, file: Root<'_, Obj>) -> Arc<FileHandle> {
    // SAFETY: a `File` has its handle in its one field.
    unsafe { file.get(scope).field::<Opaque>(0) }.share::<FileHandle>()
}

fn path_of(scope: &Scope<'_>, path: Root<'_, Str>) -> PathBuf {
    PathBuf::from(path.get(scope).as_str())
}

/// Run `work`, which blocks, where the I/O runtime keeps threads for
/// that.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    match tokio::task::spawn_blocking(work).await {
        Ok(done) => done,
        Err(error) => Err(Failure::Io(error.to_string())),
    }
}

/// Make the directories `path` is in, as a write to a path does first.
async fn make_parents(path: &Path) -> Result<(), Failure> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                Failure::io(
                    format_args!(
                        "Failed to create parent directories for '{}'",
                        path.display()
                    ),
                    error,
                )
            })
        }
        _ => Ok(()),
    }
}

/// Write `data` as the whole of the file at `path`, and return how many
/// bytes that was.
async fn write_whole(path: PathBuf, data: &[u8]) -> Result<i64, Failure> {
    make_parents(&path).await?;
    tokio::fs::write(&path, data).await.map_err(|error| {
        Failure::io(
            format_args!("Failed to write file '{}'", path.display()),
            error,
        )
    })?;
    i64::try_from(data.len())
        .map_err(|_| Failure::Io(format!("Write size {} exceeds i64::MAX", data.len())))
}

/// Open the file at `path` as `mode` says.
async fn open(path: &Path, mode: &str) -> Result<tokio::fs::File, Failure> {
    let mut options = tokio::fs::OpenOptions::new();
    match mode {
        "r" => options.read(true),
        "r+" => options.read(true).write(true),
        "w" => options.write(true).create(true).truncate(true),
        "w+" => options.read(true).write(true).create(true).truncate(true),
        "a" => options.append(true).create(true),
        "a+" => options.read(true).append(true).create(true),
        _ => {
            return Err(Failure::InvalidArgument(format!(
                "Unsupported file mode '{mode}': expected \"r\", \"r+\", \"w\", \"w+\", \"a\", \
                 or \"a+\""
            )));
        }
    };
    // A mode that makes the file makes the directories it is in too.
    if mode != "r" && mode != "r+" {
        make_parents(path).await?;
    }
    options.open(path).await.map_err(|error| {
        Failure::io(
            format_args!("Failed to open file '{}'", path.display()),
            error,
        )
    })
}

/// The permission bits `mode` names, when it names nothing else.
fn permission_bits(mode: i64) -> Result<u32, Failure> {
    u32::try_from(mode)
        .ok()
        .filter(|mode| *mode <= 0o7777)
        .ok_or_else(|| {
            // A negative mode in octal would read as its two's complement.
            Failure::InvalidArgument(if mode >= 0 {
                format!("Invalid file mode {mode:#o}: expected a permission mask in 0..=0o7777")
            } else {
                format!("Invalid file mode {mode}: expected a permission mask in 0..=0o7777")
            })
        })
}

#[cfg(unix)]
async fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await
}

/// Windows has one flag, read-only, which the owner's write bit decides.
#[cfg(windows)]
async fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    let mut permissions = tokio::fs::metadata(path).await?.permissions();
    permissions.set_readonly(mode & 0o200 == 0);
    tokio::fs::set_permissions(path, permissions).await
}

#[cfg(unix)]
async fn make_symlink(target: &str, path: &Path) -> std::io::Result<()> {
    tokio::fs::symlink(target, path).await
}

/// Windows fixes whether a link is to a directory when it is made: one is
/// made when the target is a directory now.
#[cfg(windows)]
async fn make_symlink(target: &str, path: &Path) -> std::io::Result<()> {
    let resolved = path.parent().unwrap_or(Path::new("")).join(target);
    if tokio::fs::metadata(&resolved)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
    {
        tokio::fs::symlink_dir(target, path).await
    } else {
        tokio::fs::symlink_file(target, path).await
    }
}

/// What `baml.fs.metadata` says of a path.
struct Stat {
    is_file: bool,
    is_dir: bool,
    is_executable: bool,
    modified_ms: Option<i64>,
}

fn stat(path: &Path) -> std::io::Result<Stat> {
    let metadata = std::fs::metadata(path)?;
    #[cfg(unix)]
    let is_executable = {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let is_executable = metadata.is_file();
    Ok(Stat {
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        is_executable,
        modified_ms: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|since| i64::try_from(since.as_millis()).ok()),
    })
}

/// Write `content` beside `path` and move it there, so that a reader sees
/// the old file or the new one and never part of either.
fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(content)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.as_file().sync_all()?;
    // Closed before it is moved: the next writer does not find it open.
    temporary
        .into_temp_path()
        .persist(path)
        .map_err(|error| error.error)?;
    // The directory entry is written through as the contents were.
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// Lock the file at `path`, made if there is none, waiting up to
/// `timeout_ms` for whoever holds it.
fn lock(path: &Path, timeout_ms: i64) -> Result<std::fs::File, Failure> {
    let io = |error: std::io::Error| Failure::Io(error.to_string());
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(io)?;
    let started = Instant::now();
    let patience = Duration::from_millis(timeout_ms.max(0).cast_unsigned());
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() >= patience => {
                return Err(Failure::Io(
                    "Timed out waiting for filesystem lock".to_owned(),
                ));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(io(error)),
        }
    }
}

/// One entry of a directory.
struct Entry {
    name: String,
    is_dir: bool,
    is_file: bool,
    is_symlink: bool,
}

async fn entries(path: &Path) -> Result<Vec<Entry>, Failure> {
    let mut directory = tokio::fs::read_dir(path).await.map_err(|error| {
        Failure::io(
            format_args!("Failed to read directory '{}'", path.display()),
            error,
        )
    })?;
    let mut found = Vec::new();
    loop {
        let entry = directory.next_entry().await.map_err(|error| {
            Failure::io(
                format_args!("Failed to read directory entry in '{}'", path.display()),
                error,
            )
        })?;
        let Some(entry) = entry else { return Ok(found) };
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type().await.map_err(|error| {
            Failure::io(format_args!("Failed to get file type for '{name}'"), error)
        })?;
        found.push(Entry {
            name,
            is_dir: kind.is_dir(),
            is_file: kind.is_file(),
            is_symlink: kind.is_symlink(),
        });
    }
}

/// What a job made, or the error it failed with, thrown.
fn done<'s, T>(scope: &mut Scope<'s>, made: Result<T, Failure>) -> Result<T, Thrown<'s>> {
    made.map_err(|failure| failure.throw(scope))
}

impl NativeBamlFsBamlIoReadForFile for Natives {
    async fn read<'s>(
        scope: &mut Scope<'s>,
        file: Root<'s, Obj>,
        limit: i64,
    ) -> Result<Option<Root<'s, Bytes>>, Thrown<'s>> {
        if limit <= 0 {
            return Ok(Some(scope.alloc_bytes(&[])));
        }
        let limit = usize::try_from(limit)
            .unwrap_or(MAX_READ_CHUNK)
            .min(MAX_READ_CHUNK);
        let handle = handle(scope, file);
        let read = scope
            .io(async move {
                let mut chunk = vec![0; limit];
                let mut file = handle.0.lock().await;
                let file = file.as_mut().ok_or_else(closed)?;
                let length = file
                    .read(&mut chunk)
                    .await
                    .map_err(|error| Failure::io("Failed to read file", error))?;
                chunk.truncate(length);
                Ok(chunk)
            })
            .await;
        let chunk = done(scope, read)?;
        Ok((!chunk.is_empty()).then(|| scope.alloc_bytes(&chunk)))
    }

    async fn bytes<'s>(
        scope: &mut Scope<'s>,
        file: Root<'s, Obj>,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        let handle = handle(scope, file);
        let read = scope
            .io(async move {
                let mut file = handle.0.lock().await;
                let file = file.as_mut().ok_or_else(closed)?;
                let mut all = Vec::new();
                file.read_to_end(&mut all)
                    .await
                    .map_err(|error| Failure::io("Failed to read file", error))?;
                Ok(all)
            })
            .await;
        let all = done(scope, read)?;
        Ok(scope.alloc_bytes(&all))
    }
}

impl NativeBamlFsBamlIoWriteForFile for Natives {
    async fn write_some<'s>(
        scope: &mut Scope<'s>,
        file: Root<'s, Obj>,
        data: Root<'s, Bytes>,
    ) -> Result<i64, Thrown<'s>> {
        let handle = handle(scope, file);
        let data = data.get(scope).hold();
        let written = scope
            .io(async move {
                let mut file = handle.0.lock().await;
                let file = file.as_mut().ok_or_else(closed)?;
                let failed = |error| Failure::io("Failed to write", error);
                let length = file.write(&data).await.map_err(failed)?;
                // The file buffers: without this a write the system refuses
                // would be reported by a later call, or by none.
                file.flush().await.map_err(failed)?;
                Ok(i64::try_from(length).unwrap_or(i64::MAX))
            })
            .await;
        done(scope, written)
    }

    async fn flush<'s>(scope: &mut Scope<'s>, file: Root<'s, Obj>) -> Result<(), Thrown<'s>> {
        let handle = handle(scope, file);
        let flushed = scope
            .io(async move {
                let mut file = handle.0.lock().await;
                let file = file.as_mut().ok_or_else(closed)?;
                file.flush()
                    .await
                    .map_err(|error| Failure::io("Failed to flush", error))
            })
            .await;
        done(scope, flushed)
    }
}

impl NativeBamlFsFile for Natives {
    async fn close<'s>(scope: &mut Scope<'s>, file: Root<'s, Obj>) -> Result<(), Thrown<'s>> {
        let handle = handle(scope, file);
        scope
            .io(async move {
                handle.0.lock().await.take();
            })
            .await;
        Ok(())
    }

    async fn seek_from<'s>(
        scope: &mut Scope<'s>,
        file: Root<'s, Obj>,
        whence: Elem<'s>,
        offset: i64,
    ) -> Result<i64, Thrown<'s>> {
        let from = match whence.as_str(scope).map(super::StrRef::as_str) {
            Some("start") => u64::try_from(offset).map(SeekFrom::Start).map_err(|_| {
                Failure::InvalidArgument(format!("Negative offset with whence=\"start\": {offset}"))
            }),
            Some("current") => Ok(SeekFrom::Current(offset)),
            Some("end") => Ok(SeekFrom::End(offset)),
            Some(other) => Err(Failure::InvalidArgument(format!(
                "Unsupported whence '{other}': expected \"start\", \"current\", or \"end\""
            ))),
            None => Err(Failure::InvalidArgument("Invalid whence type".to_owned())),
        };
        let from = done(scope, from)?;
        let handle = handle(scope, file);
        let sought = scope
            .io(async move {
                let mut file = handle.0.lock().await;
                let file = file.as_mut().ok_or_else(closed)?;
                let position = file
                    .seek(from)
                    .await
                    .map_err(|error| Failure::io("Failed to seek", error))?;
                i64::try_from(position)
                    .map_err(|_| Failure::Io(format!("Seek position out of range: {position}")))
            })
            .await;
        done(scope, sought)
    }
}

impl NativeBamlFsLock for Natives {
    fn close(lock: ObjRef<'_>) {
        // SAFETY: a `Lock` has its handle in its one field.
        let handle = unsafe { lock.field::<Opaque>(0) }.owned::<LockHandle>();
        // Dropping the file gives the lock back.
        handle
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

impl NativeBamlFs for Natives {
    async fn open<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        mode: Elem<'s>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let path = path_of(scope, path);
        let Some(mode) = mode.as_str(scope).map(|mode| mode.as_str().to_owned()) else {
            return Err(scope.invalid_argument("Invalid mode type"));
        };
        let opened = scope.io(async move { open(&path, &mode).await }).await;
        let file = done(scope, opened)?;
        let handle = scope.alloc_owned(Arc::new(FileHandle(tokio::sync::Mutex::new(Some(file)))));
        let mut built = scope.build(scope.named_class(NAMED_FS_FILE));
        built.set(scope, 0, handle);
        Ok(built.finish())
    }

    async fn exists<'s>(scope: &mut Scope<'s>, path: Root<'s, Str>) -> Result<bool, Thrown<'s>> {
        let path = path_of(scope, path);
        let found = scope
            .io(async move {
                tokio::fs::try_exists(&path).await.map_err(|error| {
                    Failure::io(
                        format_args!("Failed to check existence of '{}'", path.display()),
                        error,
                    )
                })
            })
            .await;
        done(scope, found)
    }

    async fn remove<'s>(scope: &mut Scope<'s>, path: Root<'s, Str>) -> Result<(), Thrown<'s>> {
        let path = path_of(scope, path);
        let removed = scope
            .io(async move {
                let Err(error) = tokio::fs::remove_file(&path).await else {
                    return Ok(());
                };
                // What the system says of a directory differs by system,
                // and none of it names what removes one.
                let is_directory = tokio::fs::symlink_metadata(&path)
                    .await
                    .is_ok_and(|metadata| metadata.is_dir());
                Err(if is_directory {
                    Failure::Io(format!(
                        "Failed to remove '{}': it is a directory; use baml.fs.remove_dir or \
                         baml.fs.remove_dir_all to delete directories",
                        path.display()
                    ))
                } else {
                    Failure::io(
                        format_args!("Failed to remove file '{}'", path.display()),
                        error,
                    )
                })
            })
            .await;
        done(scope, removed)
    }

    async fn size<'s>(scope: &mut Scope<'s>, path: Root<'s, Str>) -> Result<i64, Thrown<'s>> {
        let path = path_of(scope, path);
        let size = scope
            .io(async move {
                let metadata = tokio::fs::metadata(&path).await.map_err(|error| {
                    Failure::io(format_args!("Failed to stat '{}'", path.display()), error)
                })?;
                i64::try_from(metadata.len()).map_err(|_| {
                    Failure::Io(format!("File '{}' size exceeds i64::MAX", path.display()))
                })
            })
            .await;
        done(scope, size)
    }

    async fn read<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let path = path_of(scope, path);
        let asked = path.clone();
        let read = scope
            .io(async move { tokio::fs::read_to_string(asked).await })
            .await;
        match read {
            Ok(text) => Ok(scope.alloc_str(&text)),
            Err(error) => Err(scope.io_error(&format!(
                "Failed to read file '{}': {error}",
                path.display()
            ))),
        }
    }

    async fn write<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        content: Root<'s, Str>,
    ) -> Result<i64, Thrown<'s>> {
        let path = path_of(scope, path);
        let content = content.get(scope).as_str().to_owned();
        let written = scope
            .io(async move { write_whole(path, content.as_bytes()).await })
            .await;
        done(scope, written)
    }

    async fn write_bytes<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        content: Root<'s, Bytes>,
    ) -> Result<i64, Thrown<'s>> {
        let path = path_of(scope, path);
        let content = content.get(scope).hold();
        let written = scope
            .io(async move { write_whole(path, &content).await })
            .await;
        done(scope, written)
    }

    async fn read_dir<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
    ) -> Result<Root<'s, Arr<Obj>>, Thrown<'s>> {
        let path = path_of(scope, path);
        let listed = scope.io(async move { entries(&path).await }).await;
        let listed = done(scope, listed)?;
        let class = scope.named_class(NAMED_FS_DIR_ENTRY);
        let array = scope.alloc_array::<Obj>(listed.len());
        for (index, entry) in listed.iter().enumerate() {
            // An entry is reached through the array once the round is over.
            scope.scoped(|scope, _| {
                let name = scope.alloc_str(&entry.name);
                let mut built = scope.build(class);
                built.set(scope, 0, name);
                built.set(scope, 1, entry.is_dir);
                built.set(scope, 2, entry.is_file);
                built.set(scope, 3, entry.is_symlink);
                let built = built.finish();
                array.set(scope, index, Some(built));
            });
        }
        Ok(array)
    }

    async fn mkdir<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        options: Root<'s, Obj>,
    ) -> Result<(), Thrown<'s>> {
        let path = path_of(scope, path);
        let recursive = scope.field_bits(options.get(scope), 0) == Some(1);
        let made = scope
            .io(async move {
                let made = if recursive {
                    tokio::fs::create_dir_all(&path).await
                } else {
                    tokio::fs::create_dir(&path).await
                };
                made.map_err(|error| {
                    Failure::io(
                        format_args!("Failed to create directory '{}'", path.display()),
                        error,
                    )
                })
            })
            .await;
        done(scope, made)
    }

    async fn remove_dir<'s>(scope: &mut Scope<'s>, path: Root<'s, Str>) -> Result<(), Thrown<'s>> {
        let path = path_of(scope, path);
        let removed = scope
            .io(async move {
                tokio::fs::remove_dir(&path).await.map_err(|error| {
                    Failure::io(
                        format_args!("Failed to remove directory '{}'", path.display()),
                        error,
                    )
                })
            })
            .await;
        done(scope, removed)
    }

    async fn remove_dir_all<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
    ) -> Result<(), Thrown<'s>> {
        let path = path_of(scope, path);
        let removed = scope
            .io(async move {
                match tokio::fs::remove_dir_all(&path).await {
                    // Nothing there is nothing left to remove.
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(Failure::io(
                        format_args!("Failed to remove directory '{}'", path.display()),
                        error,
                    )),
                    _ => Ok(()),
                }
            })
            .await;
        done(scope, removed)
    }

    async fn chmod<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        mode: i64,
    ) -> Result<(), Thrown<'s>> {
        let mode = done(scope, permission_bits(mode))?;
        let path = path_of(scope, path);
        let changed = scope
            .io(async move {
                set_mode(&path, mode).await.map_err(|error| {
                    Failure::io(
                        format_args!("Failed to set permissions on '{}'", path.display()),
                        error,
                    )
                })
            })
            .await;
        done(scope, changed)
    }

    async fn symlink<'s>(
        scope: &mut Scope<'s>,
        target: Root<'s, Str>,
        path: Root<'s, Str>,
    ) -> Result<(), Thrown<'s>> {
        let target = target.get(scope).as_str().to_owned();
        let path = path_of(scope, path);
        let linked = scope
            .io(async move {
                make_symlink(&target, &path).await.map_err(|error| {
                    Failure::io(
                        format_args!(
                            "Failed to create symlink '{}' -> '{target}'",
                            path.display()
                        ),
                        error,
                    )
                })
            })
            .await;
        done(scope, linked)
    }

    async fn metadata<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let path = path_of(scope, path);
        let found = scope
            .io(blocking(move || {
                stat(&path).map_err(|error| Failure::Io(error.to_string()))
            }))
            .await;
        let found = done(scope, found)?;
        let mut built = scope.build(scope.named_class(NAMED_FS_METADATA));
        built.set(scope, 0, found.is_file);
        built.set(scope, 1, found.is_dir);
        built.set(scope, 2, found.is_executable);
        built.set(scope, 3, found.modified_ms);
        Ok(built.finish())
    }

    async fn rename<'s>(
        scope: &mut Scope<'s>,
        source: Root<'s, Str>,
        destination: Root<'s, Str>,
    ) -> Result<(), Thrown<'s>> {
        let (source, destination) = (path_of(scope, source), path_of(scope, destination));
        let renamed = scope
            .io(blocking(move || {
                std::fs::rename(source, destination).map_err(|error| Failure::Io(error.to_string()))
            }))
            .await;
        done(scope, renamed)
    }

    async fn canonicalize<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let path = path_of(scope, path);
        let found = scope
            .io(blocking(move || {
                let found =
                    std::fs::canonicalize(path).map_err(|error| Failure::Io(error.to_string()))?;
                found
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| Failure::Io("Path is not valid UTF-8".to_owned()))
            }))
            .await;
        let found = done(scope, found)?;
        Ok(scope.alloc_str(&found))
    }

    async fn write_atomic<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        content: Root<'s, Bytes>,
    ) -> Result<(), Thrown<'s>> {
        let path = path_of(scope, path);
        let content = content.get(scope).hold();
        let written = scope
            .io(blocking(move || {
                write_atomic(&path, &content).map_err(|error| Failure::Io(error.to_string()))
            }))
            .await;
        done(scope, written)
    }

    async fn lock<'s>(
        scope: &mut Scope<'s>,
        path: Root<'s, Str>,
        timeout_ms: i64,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let path = path_of(scope, path);
        let locked = scope.io(blocking(move || lock(&path, timeout_ms))).await;
        let file = done(scope, locked)?;
        let handle = scope.alloc_owned(Arc::new(LockHandle(Mutex::new(Some(file)))));
        let mut built = scope.build(scope.named_class(NAMED_FS_LOCK));
        built.set(scope, 0, handle);
        Ok(built.finish())
    }
}

//! New IO trait implementations for `NativeSysOps`.
//!
//! These implement the generated `IoClass*` and `IoNamespace*` traits from
//! `sys_types::io`. They coexist with the legacy `SysOp*` trait impls in
//! `lib.rs` during the transition.

use std::sync::{Arc, OnceLock};

use bex_heap::{BexExternalValue, BexHeap};
use sys_ops::io::{self, CallId, OpErrorKind, SysOpContext, SysOpOutput, owned};

// Process-level shared BufReader for stdin, preventing data loss when
// BufReader over-reads into its internal buffer across multiple io.input() calls.
static STDIN_READER: OnceLock<tokio::sync::Mutex<tokio::io::BufReader<tokio::io::Stdin>>> =
    OnceLock::new();

fn shared_stdin() -> &'static tokio::sync::Mutex<tokio::io::BufReader<tokio::io::Stdin>> {
    STDIN_READER
        .get_or_init(|| tokio::sync::Mutex::new(tokio::io::BufReader::new(tokio::io::stdin())))
}

use crate::NativeSysOps;

// ============================================================================
// Environment
// ============================================================================

impl io::IoNamespaceEnv for NativeSysOps {
    fn get(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        match std::env::var(&key) {
            Ok(val) => SysOpOutput::ok(Some(val)),
            Err(std::env::VarError::NotPresent) => SysOpOutput::ok(None),
            Err(std::env::VarError::NotUnicode(_)) => SysOpOutput::err(OpErrorKind::Other(
                format!("Environment variable '{key}' is not valid UTF-8"),
            )),
        }
    }
}

// ============================================================================
// IO (stdin input)
// ============================================================================

impl io::IoNamespaceIo for NativeSysOps {
    fn input(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        prompt: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

            let mut stdin = shared_stdin().lock().await;

            if let Some(p) = prompt {
                let mut stdout = tokio::io::stdout();
                stdout
                    .write_all(p.as_bytes())
                    .await
                    .map_err(|e| OpErrorKind::Other(format!("Failed to write prompt: {e}")))?;
                stdout
                    .flush()
                    .await
                    .map_err(|e| OpErrorKind::Other(format!("Failed to flush stdout: {e}")))?;
            }
            let mut line = String::new();
            stdin
                .read_line(&mut line)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read stdin: {e}")))?;
            // Trim trailing newline (returns "" on EOF when read_line returns 0 bytes)
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            Ok(line)
        })
    }

    fn print(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            use tokio::io::AsyncWriteExt;
            let mut stdout = tokio::io::stdout();
            stdout
                .write_all(s.as_bytes())
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to write stdout: {e}")))?;
            stdout
                .flush()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to flush stdout: {e}")))?;
            Ok(())
        })
    }

    fn println(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            use tokio::io::AsyncWriteExt;
            let mut stdout = tokio::io::stdout();
            // Single write_all so concurrent calls from spawned threads don't
            // interleave on a line boundary.
            let mut buf = s.into_bytes();
            buf.push(b'\n');
            stdout
                .write_all(&buf)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to write stdout: {e}")))?;
            stdout
                .flush()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to flush stdout: {e}")))?;
            Ok(())
        })
    }

    fn eprint(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            use tokio::io::AsyncWriteExt;
            let mut stderr = tokio::io::stderr();
            stderr
                .write_all(s.as_bytes())
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to write stderr: {e}")))?;
            stderr
                .flush()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to flush stderr: {e}")))?;
            Ok(())
        })
    }

    fn eprintln(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            use tokio::io::AsyncWriteExt;
            let mut stderr = tokio::io::stderr();
            let mut buf = s.into_bytes();
            buf.push(b'\n');
            stderr
                .write_all(&buf)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to write stderr: {e}")))?;
            stderr
                .flush()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to flush stderr: {e}")))?;
            Ok(())
        })
    }
}

// ============================================================================
// File System
// ============================================================================

type FsFileHandle = tokio::sync::Mutex<Option<tokio::fs::File>>;

fn downcast_handle(file: &owned::fs::File) -> Result<Arc<FsFileHandle>, OpErrorKind> {
    file._handle
        .clone()
        .downcast::<FsFileHandle>()
        .map_err(|_| OpErrorKind::Other("Invalid file handle type".into()))
}

fn closed_err() -> OpErrorKind {
    OpErrorKind::Other("File is closed".into())
}

impl io::IoClassFsFile for NativeSysOps {
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            let mut contents = String::new();
            f.read_to_string(&mut contents)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;
            Ok(contents)
        })
    }

    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            let mut contents = Vec::new();
            f.read_to_end(&mut contents)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;
            Ok(contents)
        })
    }

    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            let bytes = read_up_to(&file, n).await?;
            String::from_utf8(bytes)
                .map_err(|e| OpErrorKind::Other(format!("Invalid UTF-8 in file: {e}")))
        })
    }

    fn read_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::async_op(async move { read_up_to(&file, n).await })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            let handle = downcast_handle(&file)?;
            handle.lock().await.take();
            Ok(())
        })
    }

    fn seek_from(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        whence: BexExternalValue,
        offset: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        use tokio::io::AsyncSeekExt;

        SysOpOutput::async_op(async move {
            let BexExternalValue::String(whence) = whence else {
                return Err(OpErrorKind::Other("Invalid whence type".into()));
            };
            let from = match whence.as_str() {
                "start" => {
                    let off = u64::try_from(offset).map_err(|_| {
                        OpErrorKind::Other(format!(
                            "Negative offset with whence=\"start\": {offset}"
                        ))
                    })?;
                    std::io::SeekFrom::Start(off)
                }
                "current" => std::io::SeekFrom::Current(offset),
                "end" => std::io::SeekFrom::End(offset),
                _ => {
                    return Err(OpErrorKind::Other(format!(
                        "Unsupported whence '{whence}': expected \"start\", \"current\", or \"end\""
                    )));
                }
            };
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            let pos = f
                .seek(from)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to seek: {e}")))?;
            i64::try_from(pos)
                .map_err(|_| OpErrorKind::Other(format!("Seek position out of range: {pos}")))
        })
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_all_bytes(&file, data.into_bytes()).await })
    }

    fn write_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_all_bytes(&file, data).await })
    }
}

async fn read_up_to(file: &owned::fs::File, n: i64) -> Result<Vec<u8>, OpErrorKind> {
    use tokio::io::AsyncReadExt;

    let cap =
        u64::try_from(n).map_err(|_| OpErrorKind::Other(format!("Negative read length: {n}")))?;
    let handle = downcast_handle(file)?;
    let mut guard = handle.lock().await;
    let f = guard.as_mut().ok_or_else(closed_err)?;
    let mut buf = Vec::new();
    f.take(cap)
        .read_to_end(&mut buf)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;
    Ok(buf)
}

async fn write_all_bytes(file: &owned::fs::File, data: Vec<u8>) -> Result<i64, OpErrorKind> {
    use tokio::io::AsyncWriteExt;

    let handle = downcast_handle(file)?;
    let mut guard = handle.lock().await;
    let f = guard.as_mut().ok_or_else(closed_err)?;
    #[allow(clippy::cast_possible_wrap)]
    let len = data.len() as i64;
    f.write_all(&data)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to write: {e}")))?;
    f.flush()
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to write: {e}")))?;
    Ok(len)
}

impl io::IoNamespaceFs for NativeSysOps {
    fn open(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        mode: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::fs::File> {
        SysOpOutput::async_op(async move {
            let BexExternalValue::String(mode) = mode else {
                return Err(OpErrorKind::Other("Invalid mode type".into()));
            };
            // Modes that create the file also auto-create missing parent dirs,
            // matching Bun's `Bun.write` behavior.
            let creates = matches!(mode.as_str(), "w" | "w+" | "a" | "a+");
            if creates {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    if !parent.as_os_str().is_empty() {
                        tokio::fs::create_dir_all(parent).await.map_err(|e| {
                            OpErrorKind::Other(format!(
                                "Failed to create parent directories for '{path}': {e}"
                            ))
                        })?;
                    }
                }
            }
            let file = match mode.as_str() {
                "r" => tokio::fs::File::open(&path).await,
                "r+" => {
                    tokio::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&path)
                        .await
                }
                "w" => {
                    tokio::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&path)
                        .await
                }
                "w+" => {
                    tokio::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&path)
                        .await
                }
                "a" => {
                    tokio::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&path)
                        .await
                }
                "a+" => {
                    tokio::fs::OpenOptions::new()
                        .read(true)
                        .append(true)
                        .create(true)
                        .open(&path)
                        .await
                }
                _ => {
                    return Err(OpErrorKind::Other(format!(
                        "Unsupported file mode '{mode}': expected \"r\", \"r+\", \"w\", \"w+\", \"a\", or \"a+\""
                    )));
                }
            }
            .map_err(|e| OpErrorKind::Other(format!("Failed to open file '{path}': {e}")))?;
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(Some(file)));
            Ok(owned::fs::File { _handle: handle })
        })
    }

    fn exists(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::async_op(async move {
            tokio::fs::try_exists(&path).await.map_err(|e| {
                OpErrorKind::Other(format!("Failed to check existence of '{path}': {e}"))
            })
        })
    }

    fn remove(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to remove file '{path}': {e}")))
        })
    }

    fn size(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move {
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to stat '{path}': {e}")))?;
            i64::try_from(metadata.len())
                .map_err(|_| OpErrorKind::Other(format!("File '{path}' size exceeds i64::MAX")))
        })
    }

    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read file '{path}': {e}")))
        })
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        content: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_path(&path, content.as_bytes()).await })
    }

    fn write_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_path(&path, &content).await })
    }

    fn read_dir(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<owned::fs::DirEntry>> {
        SysOpOutput::async_op(async move {
            let mut rd = tokio::fs::read_dir(&path).await.map_err(|e| {
                OpErrorKind::Other(format!("Failed to read directory '{path}': {e}"))
            })?;
            let mut entries = Vec::new();
            while let Some(entry) = rd.next_entry().await.map_err(|e| {
                OpErrorKind::Other(format!("Failed to read directory entry in '{path}': {e}"))
            })? {
                let ft = entry.file_type().await.map_err(|e| {
                    OpErrorKind::Other(format!(
                        "Failed to get file type for '{}': {e}",
                        entry.file_name().to_string_lossy()
                    ))
                })?;
                entries.push(owned::fs::DirEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    is_dir: ft.is_dir(),
                    is_file: ft.is_file(),
                    is_symlink: ft.is_symlink(),
                });
            }
            Ok(entries)
        })
    }

    fn mkdir(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        options: owned::fs::MkdirOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            if options.recursive {
                tokio::fs::create_dir_all(&path).await
            } else {
                tokio::fs::create_dir(&path).await
            }
            .map_err(|e| OpErrorKind::Other(format!("Failed to create directory '{path}': {e}")))
        })
    }
}

// Auto-creates missing parent dirs, matching Bun's `Bun.write` behavior.
async fn write_path(path: &str, data: &[u8]) -> Result<i64, OpErrorKind> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                OpErrorKind::Other(format!(
                    "Failed to create parent directories for '{path}': {e}"
                ))
            })?;
        }
    }
    tokio::fs::write(path, data)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to write file '{path}': {e}")))?;
    i64::try_from(data.len())
        .map_err(|_| OpErrorKind::Other(format!("Write size {} exceeds i64::MAX", data.len())))
}

// ============================================================================
// Glob
// ============================================================================

use sys_glob::GlobPattern;

type GlobHandle = GlobPattern;

fn downcast_glob_handle(glob: &owned::glob::Glob) -> Result<Arc<GlobHandle>, OpErrorKind> {
    glob._handle
        .clone()
        .downcast::<GlobHandle>()
        .map_err(|_| OpErrorKind::Other("Invalid glob handle type".into()))
}

impl io::IoNamespaceGlob for NativeSysOps {
    fn new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        pattern: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::glob::Glob> {
        match GlobPattern::new(&pattern) {
            Ok(gp) => {
                let handle: Arc<dyn std::any::Any + Send + Sync> = Arc::new(gp);
                SysOpOutput::ok(owned::glob::Glob { _handle: handle })
            }
            Err(e) => SysOpOutput::err(OpErrorKind::Other(e)),
        }
    }
}

impl io::IoClassGlobGlob for NativeSysOps {
    fn scan(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        glob: owned::glob::Glob,
        root: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<String>> {
        SysOpOutput::async_op(async move {
            let handle = downcast_glob_handle(&glob)?;

            let (cwd, dot, absolute, follow_symlinks, throw_on_broken, only_files) = match &root {
                BexExternalValue::String(s) => (s.clone(), false, false, false, false, true),
                BexExternalValue::Instance { fields, .. } => {
                    let get_string = |key: &str, default: &str| {
                        fields
                            .get(key)
                            .and_then(BexExternalValue::as_string)
                            .unwrap_or_else(|| default.to_string())
                    };
                    let get_bool = |key: &str, default: bool| {
                        fields
                            .get(key)
                            .and_then(BexExternalValue::as_bool)
                            .unwrap_or(default)
                    };
                    let cwd = get_string("cwd", ".");
                    let dot = get_bool("dot", false);
                    let absolute = get_bool("absolute", false);
                    let follow_symlinks = get_bool("follow_symlinks", false);
                    let throw_on_broken = get_bool("throw_error_on_broken_symlink", false);
                    let only_files = get_bool("only_files", true);
                    (
                        cwd,
                        dot,
                        absolute,
                        follow_symlinks,
                        throw_on_broken,
                        only_files,
                    )
                }
                _ => {
                    return Err(OpErrorKind::Other(
                        "scan argument must be a string or ScanOptions".into(),
                    ));
                }
            };

            let cwd_path = std::path::Path::new(&cwd);
            let abs_cwd = if cwd_path.is_absolute() {
                cwd_path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|e| OpErrorKind::Other(format!("Failed to get cwd: {e}")))?
                    .join(cwd_path)
            };

            // Prune dot directories during the walk when `dot=false` so we
            // never descend into trees like `.git/`. The previous post-walk
            // filter still discarded those entries but only after walkdir
            // had paid the I/O to enumerate them. Depth 0 is always kept so
            // the user can scan a dot-prefixed root they explicitly point
            // at (e.g. `Glob.scan(".config")`).
            let walker = walkdir::WalkDir::new(&abs_cwd)
                .follow_links(follow_symlinks)
                .into_iter()
                .filter_entry(move |entry| {
                    dot || entry.depth() == 0
                        || !entry.file_name().to_string_lossy().starts_with('.')
                });

            let mut results = Vec::new();
            for entry in walker {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        // Classify: broken symlink vs. other I/O error. A broken
                        // symlink surfaces as NotFound on a path whose lstat
                        // identifies it as a symlink. The `throw_on_broken`
                        // option is scoped to broken symlinks only — every
                        // other walk error (permission denied, transient I/O,
                        // symlink loop) is a real failure and propagates
                        // regardless. Silently swallowing real errors hides
                        // problems users want to know about.
                        let is_broken_symlink = e
                            .io_error()
                            .is_some_and(|io_err| io_err.kind() == std::io::ErrorKind::NotFound)
                            && e.path()
                                .and_then(|p| std::fs::symlink_metadata(p).ok())
                                .is_some_and(|m| m.file_type().is_symlink());

                        if is_broken_symlink {
                            if throw_on_broken {
                                return Err(OpErrorKind::Other(format!("Broken symlink: {e}")));
                            }
                            continue;
                        }
                        return Err(OpErrorKind::Other(format!("Walk error: {e}")));
                    }
                };

                // Skip the root itself
                if entry.depth() == 0 {
                    continue;
                }

                let ft = entry.file_type();
                if only_files && !ft.is_file() {
                    continue;
                }

                let rel = entry.path().strip_prefix(&abs_cwd).unwrap_or(entry.path());
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let abs_str = entry.path().to_string_lossy().replace('\\', "/");

                // Dot filtering is handled by `filter_entry` above; no need
                // to re-check here.

                if !handle.is_match_entry(&rel_str, &abs_str) {
                    continue;
                }

                if absolute {
                    results.push(abs_str);
                } else {
                    results.push(rel_str);
                }
            }
            Ok(results)
        })
    }

    fn matches(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        glob: owned::glob::Glob,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        match downcast_glob_handle(&glob) {
            Ok(handle) => SysOpOutput::ok(handle.is_match(&path)),
            Err(e) => SysOpOutput::err(e),
        }
    }
}

// ============================================================================
// System
// ============================================================================

/// Shared helper: apply `ProcessOptions` to a `tokio::process::Command`, run
/// it, and collect its output. Both `exec()` and `shell()` use this.
async fn run_process(
    cmd: &mut tokio::process::Command,
    options: Option<owned::sys::ProcessOptions>,
    label: &str,
) -> Result<owned::sys::ShellOutput, OpErrorKind> {
    use std::process::Stdio;

    use tokio::io::AsyncWriteExt as _;

    if let Some(ref opts) = options {
        if let Some(ref cwd) = opts.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(ref env) = opts.env {
            cmd.env_clear();
            cmd.envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
        if opts.stdin.is_some() {
            cmd.stdin(Stdio::piped());
        }
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| OpErrorKind::Io {
        message: format!("Failed to spawn '{label}': {e}"),
    })?;

    // Write stdin if provided
    if let Some(ref opts) = options {
        if let Some(ref stdin_data) = opts.stdin {
            if let Some(mut stdin_pipe) = child.stdin.take() {
                let _ = stdin_pipe.write_all(stdin_data.as_bytes()).await;
                // Drop stdin_pipe to close the pipe (child gets EOF)
            }
        }
    }

    // Apply timeout if specified
    let timeout_ms = options.as_ref().and_then(|o| o.timeout_ms);
    let output = if let Some(ms) = timeout_ms {
        let duration = std::time::Duration::from_millis(ms.max(0).cast_unsigned());
        // Spawn the wait in a separate task so we can kill regardless of ownership
        let task_child = child;
        let mut wait_task = tokio::spawn(async move { task_child.wait_with_output().await });
        match tokio::time::timeout(duration, &mut wait_task).await {
            Ok(Ok(result)) => result.map_err(|e| OpErrorKind::Io {
                message: format!("Failed to wait on '{label}': {e}"),
            })?,
            Ok(Err(join_err)) => {
                return Err(OpErrorKind::Io {
                    message: format!("Task join error waiting on '{label}': {join_err}"),
                });
            }
            Err(_elapsed) => {
                // Abort the task so task_child is dropped, triggering kill_on_drop
                wait_task.abort();
                return Err(OpErrorKind::Timeout {
                    message: format!("Command '{label}' timed out after {ms}ms"),
                    duration,
                });
            }
        }
    } else {
        child
            .wait_with_output()
            .await
            .map_err(|e| OpErrorKind::Io {
                message: format!("Failed to wait on '{label}': {e}"),
            })?
    };

    let exit_code = i64::from(output.status.code().unwrap_or(-1));
    Ok(owned::sys::ShellOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_code,
    })
}

impl io::IoNamespaceSys for NativeSysOps {
    fn exec(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        program: String,
        args: Option<Vec<String>>,
        options: Option<owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sys::ShellOutput> {
        SysOpOutput::async_op(async move {
            let mut cmd = tokio::process::Command::new(&program);
            if let Some(ref a) = args {
                cmd.args(a);
            }
            run_process(&mut cmd, options, &program).await
        })
    }

    fn shell(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        command: String,
        options: Option<owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sys::ShellOutput> {
        SysOpOutput::async_op(async move {
            let resolved = crate::shell::default_shell();
            let mut cmd = tokio::process::Command::new(&resolved.path);
            resolved.apply(&mut cmd, &command);
            run_process(&mut cmd, options, &command).await
        })
    }

    fn sleep(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        ms: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        #[allow(clippy::cast_sign_loss)]
        let millis = ms.max(0) as u64;
        SysOpOutput::async_op(async move {
            tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
            Ok(())
        })
    }
}

// ============================================================================
// Network
// ============================================================================

type NetSocketHandle = tokio::sync::Mutex<tokio::net::TcpStream>;
type NetTcpListenerHandle = tokio::net::TcpListener;

impl io::IoClassNetSocket for NativeSysOps {
    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        socket: owned::net::Socket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let handle: Arc<NetSocketHandle> = socket
                ._handle
                .downcast::<NetSocketHandle>()
                .map_err(|_| OpErrorKind::Other("Invalid socket handle type".into()))?;
            let mut stream = handle.lock().await;
            let mut buffer = vec![0u8; 4096];
            let n = stream
                .read(&mut buffer)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read from socket: {e}")))?;
            Ok(String::from_utf8_lossy(&buffer[..n]).into_owned())
        })
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        socket: owned::net::Socket,
        data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use tokio::io::AsyncWriteExt;

        SysOpOutput::async_op(async move {
            let handle: Arc<NetSocketHandle> = socket
                ._handle
                .downcast::<NetSocketHandle>()
                .map_err(|_| OpErrorKind::Other("Invalid socket handle type".into()))?;
            let mut stream = handle.lock().await;
            stream
                .write_all(data.as_bytes())
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to write to socket: {e}")))?;
            stream
                .flush()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to flush socket: {e}")))?;
            Ok(())
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        socket: owned::net::Socket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use tokio::io::AsyncWriteExt;

        SysOpOutput::async_op(async move {
            let handle: Arc<NetSocketHandle> = socket
                ._handle
                .downcast::<NetSocketHandle>()
                .map_err(|_| OpErrorKind::Other("Invalid socket handle type".into()))?;
            // shutdown() flushes pending writes and closes the write half, so
            // peers waiting on EOF (e.g. curl with "Connection: close") stop
            // blocking. We swallow ENOTCONN / NotConnected since the peer may
            // already have closed.
            let mut stream = handle.lock().await;
            match stream.shutdown().await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotConnected => Ok(()),
                Err(e) => Err(OpErrorKind::Other(format!("Failed to close socket: {e}"))),
            }
        })
    }
}

impl io::IoClassNetTcpListener for NativeSysOps {
    fn accept(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        listener: owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::Socket> {
        SysOpOutput::async_op(async move {
            let handle: Arc<NetTcpListenerHandle> = listener
                ._handle
                .downcast::<NetTcpListenerHandle>()
                .map_err(|_| OpErrorKind::Other("Invalid TcpListener handle type".into()))?;
            let (stream, _peer) = handle
                .accept()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to accept connection: {e}")))?;
            let sock_handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(stream));
            Ok(owned::net::Socket {
                _handle: sock_handle,
            })
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _listener: owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // Dropping the Arc<TcpListener> closes the underlying socket once the
        // last reference goes away. There's no explicit "stop accepting" call
        // for tokio's listener — moving on is enough.
        SysOpOutput::ok(())
    }
}

impl io::IoNamespaceNet for NativeSysOps {
    fn connect(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::Socket> {
        SysOpOutput::async_op(async move {
            let stream = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to connect to '{addr}': {e}")))?;
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(stream));
            Ok(owned::net::Socket { _handle: handle })
        })
    }

    fn listen(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::TcpListener> {
        SysOpOutput::async_op(async move {
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to bind '{addr}': {e}")))?;
            let handle: Arc<dyn std::any::Any + Send + Sync> = Arc::new(listener);
            Ok(owned::net::TcpListener { _handle: handle })
        })
    }
}

// ============================================================================
// HTTP
// ============================================================================

impl io::IoClassHttpResponse for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            let body: Arc<tokio::sync::Mutex<Option<reqwest::Response>>> = response
                ._body
                .downcast::<tokio::sync::Mutex<Option<reqwest::Response>>>()
                .map_err(|_| OpErrorKind::Other("Invalid response body handle".into()))?;
            let mut guard = body.lock().await;
            let resp = guard.take().ok_or_else(|| {
                OpErrorKind::Other("Response body has already been consumed".into())
            })?;
            resp.text()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read response body: {e}")))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    #[cfg(feature = "bundle-http")]
    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::async_op(async move {
            let body: Arc<tokio::sync::Mutex<Option<reqwest::Response>>> = response
                ._body
                .downcast::<tokio::sync::Mutex<Option<reqwest::Response>>>()
                .map_err(|_| OpErrorKind::Other("Invalid response body handle".into()))?;
            let mut guard = body.lock().await;
            let resp = guard.take().ok_or_else(|| {
                OpErrorKind::Other("Response body has already been consumed".into())
            })?;
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| OpErrorKind::Other(format!("Failed to read response body: {e}")))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

#[cfg(feature = "bundle-http")]
fn build_io_http_response(response: reqwest::Response, url: String) -> owned::http::Response {
    let status = i64::from(response.status().as_u16());
    let headers: indexmap::IndexMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body: Arc<dyn std::any::Any + Send + Sync> =
        Arc::new(tokio::sync::Mutex::new(Some(response)));
    owned::http::Response {
        status_code: status,
        headers,
        url,
        _body: body,
    }
}

impl io::IoClassHttpSseStream for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        sse_stream: owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        use std::sync::atomic::Ordering;

        SysOpOutput::async_op(async move {
            let handle = sse_stream
                ._handle
                .downcast::<bex_resource_types::ResourceHandle>()
                .map_err(|_| OpErrorKind::Other("Invalid SSE stream handle type".into()))?;

            let (buffer, notify, closed) =
                crate::registry::REGISTRY
                    .get_sse_stream(handle.key())
                    .ok_or_else(|| OpErrorKind::Other("SSE stream handle is invalid".into()))?;

            loop {
                let notified = notify.notified();
                {
                    let mut buf = buffer.lock().await;
                    if closed.load(Ordering::Acquire) {
                        buf.done = true;
                        buf.error = None;
                        return Ok(None);
                    }
                    if !buf.events.is_empty() {
                        let events: Vec<serde_json::Value> = std::mem::take(&mut buf.events)
                            .into_iter()
                            .map(|e| {
                                serde_json::json!({
                                    "event": e.event,
                                    "data": e.data,
                                    "id": e.id,
                                })
                            })
                            .collect();
                        return Ok(Some(serde_json::to_string(&events).map_err(|e| {
                            OpErrorKind::Other(format!("Failed to serialize SSE events: {e}"))
                        })?));
                    }
                    if let Some(err) = buf.error.take() {
                        return Err(OpErrorKind::Other(err));
                    }
                    if buf.done {
                        return Ok(None);
                    }
                }
                notified.await;
            }
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _sse_stream: owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _sse_stream: owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // Dropping the owned SseStream drops its ResourceHandle, which
        // triggers cleanup via ResourceRegistryRef::remove.
        SysOpOutput::ok(())
    }
}

impl io::IoNamespaceHttp for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn fetch(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        url: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::async_op(async move {
            crate::ensure_rustls_crypto_provider();
            let client = reqwest::Client::new();
            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|e| OpErrorKind::Other(format!("HTTP fetch failed: {e}")))?;
            let final_url = response.url().to_string();
            Ok(build_io_http_response(response, final_url))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn fetch(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _url: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    #[cfg(feature = "bundle-http")]
    fn send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::async_op(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
                OpErrorKind::Other(format!("Invalid HTTP method '{}': {e}", request.method))
            })?;

            crate::ensure_rustls_crypto_provider();
            let client = reqwest::Client::new();
            let mut builder = client.request(method, &request.url);

            for (k, v) in &request.headers {
                builder = builder.header(k.as_str(), v.as_str());
            }

            if !request.body.is_empty() {
                builder = builder.body(request.body);
            }

            let response = builder
                .send()
                .await
                .map_err(|e| OpErrorKind::Other(format!("HTTP send failed: {e}")))?;
            let final_url = response.url().to_string();
            Ok(build_io_http_response(response, final_url))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    #[cfg(feature = "bundle-http")]
    fn fetch_sse(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::SseStream> {
        use std::sync::atomic::{AtomicBool, Ordering};

        use futures::StreamExt;
        use sys_types::sse::SseParser;
        use tokio::sync::{Mutex as TokioMutex, Notify};

        use crate::registry::{REGISTRY, SseBuffer};

        SysOpOutput::async_op(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
                OpErrorKind::Other(format!("Invalid HTTP method '{}': {e}", request.method))
            })?;

            crate::ensure_rustls_crypto_provider();
            let client = reqwest::Client::new();
            let mut builder = client.request(method, &request.url);

            for (key, value) in &request.headers {
                builder = builder.header(key.as_str(), value.as_str());
            }

            if !request.body.is_empty() {
                builder = builder.body(request.body.clone());
            }

            let response = builder
                .send()
                .await
                .map_err(|e| OpErrorKind::Other(format!("SSE connection failed: {e}")))?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<could not read body>".to_string());
                return Err(OpErrorKind::Other(format!(
                    "SSE request failed with status {status}: {body}"
                )));
            }

            let url = response.url().to_string();

            let buffer = Arc::new(TokioMutex::new(SseBuffer {
                events: Vec::new(),
                done: false,
                error: None,
            }));
            let closed = Arc::new(AtomicBool::new(false));
            let notify = Arc::new(Notify::new());

            let buf_clone = buffer.clone();
            let closed_clone = closed.clone();
            let notify_clone = notify.clone();
            let consumer = tokio::spawn(async move {
                struct SseDropGuard {
                    buffer: Arc<TokioMutex<SseBuffer>>,
                    closed: Arc<AtomicBool>,
                    notify: Arc<Notify>,
                    completed: bool,
                }

                impl Drop for SseDropGuard {
                    fn drop(&mut self) {
                        if !self.completed {
                            if let Ok(mut buf) = self.buffer.try_lock() {
                                if !buf.done {
                                    if !self.closed.load(Ordering::Acquire) {
                                        buf.error = Some("SSE stream task was cancelled".into());
                                    }
                                    buf.done = true;
                                }
                            }
                            self.notify.notify_waiters();
                        }
                    }
                }

                let mut guard = SseDropGuard {
                    buffer: buf_clone.clone(),
                    closed: closed_clone.clone(),
                    notify: notify_clone.clone(),
                    completed: false,
                };

                let mut parser = SseParser::new();
                let mut byte_stream = response.bytes_stream();

                while let Some(chunk_result) = byte_stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            let events = parser.feed(&bytes);
                            if !events.is_empty() {
                                let mut buf = buf_clone.lock().await;
                                buf.events.extend(events);
                                notify_clone.notify_waiters();
                            }
                        }
                        Err(e) => {
                            let mut buf = buf_clone.lock().await;
                            buf.error = Some(format!("SSE stream error: {e}"));
                            buf.done = true;
                            notify_clone.notify_waiters();
                            guard.completed = true;
                            return;
                        }
                    }
                }

                // Stream ended cleanly — flush any event buffered without a
                // trailing blank line (some servers omit the final delimiter).
                let final_events = parser.finish();
                let mut buf = buf_clone.lock().await;
                if !final_events.is_empty() {
                    buf.events.extend(final_events);
                }
                buf.done = true;
                notify_clone.notify_waiters();
                guard.completed = true;
            });

            let handle = REGISTRY.register_sse_stream(
                buffer,
                closed,
                notify,
                consumer.abort_handle(),
                url.clone(),
            );
            let handle: Arc<dyn std::any::Any + Send + Sync> = Arc::new(handle);
            Ok(owned::http::SseStream {
                url,
                _handle: handle,
            })
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn fetch_sse(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::SseStream> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

// BEP-034 Future methods live on the heap `Object::Future` itself (atomic
// state + SetOnce + cancel token) and are dispatched via the native-call
// path (`$rust_function` in `ns_future/future.baml`), not through sys-ops.
// See `bex_vm::package_baml` for the trait impl.

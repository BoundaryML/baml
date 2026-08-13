//! New IO trait implementations for `NativeSysOps`.
//!
//! These implement the generated `IoClass*` and `IoNamespace*` traits from
//! `sys_types::io`. They coexist with the legacy `SysOp*` trait impls in
//! `lib.rs` during the transition.

use std::sync::{Arc, OnceLock};

use bex_heap::{BexExternalValue, BexHeap};
use sys_ops::io::{
    self, CallId, SysOpContext, SysOpOutput, VmBamlError, VmPanic, VmRustFnError, owned,
};

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
            Err(std::env::VarError::NotUnicode(_)) => SysOpOutput::err(VmBamlError::ParseError {
                message: format!("Environment variable '{key}' is not valid UTF-8"),
            }),
        }
    }
}

// ============================================================================
// Time
// ============================================================================

impl io::IoClassTimeInstant for NativeSysOps {
    fn now(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::time::Instant> {
        // Wall-clock time as nanoseconds since the UNIX epoch. Per the
        // `Instant.now()` contract, an unavailable/pre-epoch clock panics.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| unreachable!("system clock is set before the UNIX epoch"))
            .as_nanos();
        SysOpOutput::ok(owned::time::Instant {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(nanos)),
        })
    }
}

/// Converts an absolute time (nanoseconds since the Unix epoch, as an
/// arbitrary-precision integer) into a `jiff::Timestamp`, saturating at
/// jiff's representable range. Saturation is fine for offset queries: a
/// timezone's offset is constant beyond the last tzdb transition, so the
/// boundary instant resolves to the same offset as any farther one.
fn saturating_timestamp(ns: &num_bigint::BigInt) -> jiff::Timestamp {
    use num_bigint::Sign;
    match i128::try_from(ns) {
        Ok(ns) => jiff::Timestamp::from_nanosecond(ns).unwrap_or(if ns < 0 {
            jiff::Timestamp::MIN
        } else {
            jiff::Timestamp::MAX
        }),
        Err(_) if ns.sign() == Sign::Minus => jiff::Timestamp::MIN,
        Err(_) => jiff::Timestamp::MAX,
    }
}

impl io::IoNamespaceTime for NativeSysOps {
    fn system_timezone(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // The host's timezone database / system configuration, per BEP-021
        // (Deno-style: no tzdata is bundled on platforms that provide one).
        match jiff::tz::TimeZone::system().iana_name() {
            Some(name) => SysOpOutput::ok(name.to_string()),
            None => SysOpOutput::err(VmBamlError::Io {
                message: "could not determine the system's IANA timezone".to_string(),
            }),
        }
    }

    fn _tz_offset_at(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        timezone: String,
        at_ns: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<i64>> {
        // Unknown identifier → None (the BAML layer turns this into
        // `UnknownTimezoneError`).
        let Ok(tz) = jiff::tz::TimeZone::get(&timezone) else {
            return SysOpOutput::ok(None);
        };
        let offset = tz.to_offset(saturating_timestamp(&at_ns));
        SysOpOutput::ok(Some(i64::from(offset.seconds()) * 1_000_000_000))
    }

    fn _tz_to_instant(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        timezone: String,
        civil_ns: Arc<num_bigint::BigInt>,
        disambiguation: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<Arc<num_bigint::BigInt>>> {
        let Ok(tz) = jiff::tz::TimeZone::get(&timezone) else {
            return SysOpOutput::ok(None);
        };
        // The civil reading is encoded as nanoseconds since
        // 1970-01-01T00:00:00 as if it were UTC; decode it through UTC.
        let civil = match i128::try_from(&*civil_ns)
            .ok()
            .and_then(|ns| jiff::Timestamp::from_nanosecond(ns).ok())
        {
            Some(ts) => ts.to_zoned(jiff::tz::TimeZone::UTC).datetime(),
            None => {
                return SysOpOutput::err(VmBamlError::Io {
                    message: "civil time is outside the supported year range".to_string(),
                });
            }
        };
        let ambiguous = tz.to_ambiguous_timestamp(civil);
        // TC39 disambiguation semantics, via jiff. "reject" is implemented in
        // the BAML layer (earlier+later agreement), so it never reaches here.
        let resolved = match disambiguation.as_str() {
            "earlier" => ambiguous.earlier(),
            "later" => ambiguous.later(),
            _ => ambiguous.compatible(),
        };
        match resolved {
            Ok(ts) => SysOpOutput::ok(Some(Arc::new(num_bigint::BigInt::from(ts.as_nanosecond())))),
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("cannot resolve civil time in timezone '{timezone}': {e}"),
            }),
        }
    }
}

// ============================================================================
// Random (operating-system entropy)
// ============================================================================

impl io::IoClassRandomSystemRandom for NativeSysOps {
    fn random(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        bytes: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        let Ok(n) = usize::try_from(bytes) else {
            return SysOpOutput::err(VmPanic::UserPanic {
                message: format!("Rng.random: byte count must be non-negative, got {bytes}"),
            });
        };
        // Allocate fallibly so an unsatisfiable request is a catchable
        // `AllocFailure` panic, not a host-process abort.
        let mut buf = Vec::new();
        if buf.try_reserve(n).is_err() {
            return SysOpOutput::err(VmPanic::AllocFailure {
                message: format!("Rng.random: allocation of {n} bytes failed"),
            });
        }
        buf.resize(n, 0u8);
        match getrandom::getrandom(&mut buf) {
            Ok(()) => SysOpOutput::ok(buf),
            Err(e) => SysOpOutput::err(VmPanic::HostUnavailable {
                resource: "randomness".to_string(),
                message: format!("SystemRandom.random: system entropy unavailable: {e}"),
            }),
        }
    }

    fn random_int(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        let mut buf = [0u8; 8];
        match getrandom::getrandom(&mut buf) {
            // Arithmetic shift right by one maps the uniform 64-bit draw onto
            // the BAML i63 range `[INT_MIN, INT_MAX]`.
            Ok(()) => SysOpOutput::ok(i64::from_le_bytes(buf) >> 1),
            Err(e) => SysOpOutput::err(VmPanic::HostUnavailable {
                resource: "randomness".to_string(),
                message: format!("SystemRandom.random_int: system entropy unavailable: {e}"),
            }),
        }
    }
}

impl io::IoNamespaceRandom for NativeSysOps {}

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
                    .map_err(|e| VmBamlError::Io {
                        message: format!("Failed to write prompt: {e}"),
                    })?;
                stdout.flush().await.map_err(|e| VmBamlError::Io {
                    message: format!("Failed to flush stdout: {e}"),
                })?;
            }
            let mut line = String::new();
            stdin
                .read_line(&mut line)
                .await
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to read stdin: {e}"),
                })?;
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
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to write stdout: {e}"),
                })?;
            stdout.flush().await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to flush stdout: {e}"),
            })?;
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
            stdout.write_all(&buf).await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to write stdout: {e}"),
            })?;
            stdout.flush().await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to flush stdout: {e}"),
            })?;
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
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to write stderr: {e}"),
                })?;
            stderr.flush().await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to flush stderr: {e}"),
            })?;
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
            stderr.write_all(&buf).await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to write stderr: {e}"),
            })?;
            stderr.flush().await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to flush stderr: {e}"),
            })?;
            Ok(())
        })
    }
}

// ============================================================================
// File System
// ============================================================================

type FsFileHandle = tokio::sync::Mutex<Option<tokio::fs::File>>;

fn downcast_handle(file: &owned::fs::File) -> Result<Arc<FsFileHandle>, VmBamlError> {
    file._handle
        .clone()
        .downcast::<FsFileHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid file handle type".into(),
        })
}

fn closed_err() -> VmBamlError {
    VmBamlError::InvalidArgument {
        message: "File is closed".into(),
    }
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
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to read file: {e}"),
                })?;
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
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to read file: {e}"),
                })?;
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
                .map_err(|e| VmBamlError::ParseError {
                    message: format!("Invalid UTF-8 in file: {e}"),
                })
                .map_err(VmRustFnError::from)
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
        SysOpOutput::async_op(
            async move { read_up_to(&file, n).await.map_err(VmRustFnError::from) },
        )
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
                return Err(VmRustFnError::from(VmBamlError::InvalidArgument {
                    message: "Invalid whence type".into(),
                }));
            };
            let from = match whence.as_str() {
                "start" => {
                    let off = u64::try_from(offset).map_err(|_| VmBamlError::InvalidArgument {
                        message: format!("Negative offset with whence=\"start\": {offset}"),
                    })?;
                    std::io::SeekFrom::Start(off)
                }
                "current" => std::io::SeekFrom::Current(offset),
                "end" => std::io::SeekFrom::End(offset),
                _ => {
                    return Err(VmRustFnError::from(VmBamlError::InvalidArgument {
                        message: format!(
                            "Unsupported whence '{whence}': expected \"start\", \"current\", or \"end\""
                        ),
                    }));
                }
            };
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            let pos = f.seek(from).await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to seek: {e}"),
            })?;
            i64::try_from(pos)
                .map_err(|_| VmBamlError::Io {
                    message: format!("Seek position out of range: {pos}"),
                })
                .map_err(VmRustFnError::from)
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
        SysOpOutput::async_op(async move {
            write_all_bytes(&file, data.into_bytes())
                .await
                .map_err(VmRustFnError::from)
        })
    }

    fn write_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move {
            write_all_bytes(&file, data)
                .await
                .map_err(VmRustFnError::from)
        })
    }
}

async fn read_up_to(file: &owned::fs::File, n: i64) -> Result<Vec<u8>, VmBamlError> {
    use tokio::io::AsyncReadExt;

    let cap = u64::try_from(n).map_err(|_| VmBamlError::InvalidArgument {
        message: format!("Negative read length: {n}"),
    })?;
    let handle = downcast_handle(file)?;
    let mut guard = handle.lock().await;
    let f = guard.as_mut().ok_or_else(closed_err)?;
    let mut buf = Vec::new();
    f.take(cap)
        .read_to_end(&mut buf)
        .await
        .map_err(|e| VmBamlError::Io {
            message: format!("Failed to read file: {e}"),
        })?;
    Ok(buf)
}

async fn write_all_bytes(file: &owned::fs::File, data: Vec<u8>) -> Result<i64, VmBamlError> {
    use tokio::io::AsyncWriteExt;

    let handle = downcast_handle(file)?;
    let mut guard = handle.lock().await;
    let f = guard.as_mut().ok_or_else(closed_err)?;
    #[allow(clippy::cast_possible_wrap)]
    let len = data.len() as i64;
    f.write_all(&data).await.map_err(|e| VmBamlError::Io {
        message: format!("Failed to write: {e}"),
    })?;
    f.flush().await.map_err(|e| VmBamlError::Io {
        message: format!("Failed to write: {e}"),
    })?;
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
                return Err(VmRustFnError::from(VmBamlError::InvalidArgument {
                    message: "Invalid mode type".into(),
                }));
            };
            // Modes that create the file also auto-create missing parent dirs,
            // matching Bun's `Bun.write` behavior.
            let creates = matches!(mode.as_str(), "w" | "w+" | "a" | "a+");
            if creates {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    if !parent.as_os_str().is_empty() {
                        tokio::fs::create_dir_all(parent)
                            .await
                            .map_err(|e| VmBamlError::Io {
                                message: format!(
                                    "Failed to create parent directories for '{path}': {e}"
                                ),
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
                    return Err(VmRustFnError::from(VmBamlError::InvalidArgument {
                        message: format!(
                            "Unsupported file mode '{mode}': expected \"r\", \"r+\", \"w\", \"w+\", \"a\", or \"a+\""
                        ),
                    }));
                }
            }
            .map_err(|e| VmBamlError::Io {
                message: format!("Failed to open file '{path}': {e}"),
            })?;
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
            tokio::fs::try_exists(&path)
                .await
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to check existence of '{path}': {e}"),
                })
                .map_err(VmRustFnError::from)
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
            match tokio::fs::remove_file(&path).await {
                Ok(()) => Ok(()),
                Err(e) => {
                    // `remove_file` rejects directories, but the raw OS error is
                    // opaque and platform-specific (EISDIR / "Is a directory" on
                    // Linux, EPERM / "Operation not permitted" on macOS). When
                    // the target is actually a directory, surface a message that
                    // points at the directory-removal API instead. `symlink_metadata`
                    // does not follow the final component, so a symlink (which
                    // `remove_file` deletes fine) is never misreported here.
                    if tokio::fs::symlink_metadata(&path)
                        .await
                        .is_ok_and(|m| m.is_dir())
                    {
                        return Err(VmBamlError::Io {
                            message: format!(
                                "Failed to remove '{path}': it is a directory; use baml.fs.remove_dir or baml.fs.remove_dir_all to delete directories"
                            ),
                        }
                        .into());
                    }
                    Err(VmBamlError::Io {
                        message: format!("Failed to remove file '{path}': {e}"),
                    }
                    .into())
                }
            }
        })
    }

    fn remove_dir(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            tokio::fs::remove_dir(&path)
                .await
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to remove directory '{path}': {e}"),
                })
                .map_err(VmRustFnError::from)
        })
    }

    fn remove_dir_all(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            match tokio::fs::remove_dir_all(&path).await {
                Ok(()) => Ok(()),
                // `force: true` semantics: a missing path is not an error.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(VmBamlError::Io {
                    message: format!("Failed to remove directory '{path}': {e}"),
                }
                .into()),
            }
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
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to stat '{path}': {e}"),
                })?;
            i64::try_from(metadata.len())
                .map_err(|_| VmBamlError::Io {
                    message: format!("File '{path}' size exceeds i64::MAX"),
                })
                .map_err(VmRustFnError::from)
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
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to read file '{path}': {e}"),
                })
                .map_err(VmRustFnError::from)
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
        SysOpOutput::async_op(async move {
            write_path(&path, content.as_bytes())
                .await
                .map_err(VmRustFnError::from)
        })
    }

    fn write_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move {
            write_path(&path, &content)
                .await
                .map_err(VmRustFnError::from)
        })
    }

    fn read_dir(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<owned::fs::DirEntry>> {
        SysOpOutput::async_op(async move {
            let mut rd = tokio::fs::read_dir(&path)
                .await
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to read directory '{path}': {e}"),
                })?;
            let mut entries = Vec::new();
            while let Some(entry) = rd.next_entry().await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to read directory entry in '{path}': {e}"),
            })? {
                let ft = entry.file_type().await.map_err(|e| VmBamlError::Io {
                    message: format!(
                        "Failed to get file type for '{}': {e}",
                        entry.file_name().to_string_lossy()
                    ),
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
            .map_err(|e| VmBamlError::Io {
                message: format!("Failed to create directory '{path}': {e}"),
            })
            .map_err(VmRustFnError::from)
        })
    }
}

// Auto-creates missing parent dirs, matching Bun's `Bun.write` behavior.
async fn write_path(path: &str, data: &[u8]) -> Result<i64, VmBamlError> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to create parent directories for '{path}': {e}"),
                })?;
        }
    }
    tokio::fs::write(path, data)
        .await
        .map_err(|e| VmBamlError::Io {
            message: format!("Failed to write file '{path}': {e}"),
        })?;
    i64::try_from(data.len()).map_err(|_| VmBamlError::Io {
        message: format!("Write size {} exceeds i64::MAX", data.len()),
    })
}

// ============================================================================
// Glob
// ============================================================================

use sys_glob::GlobPattern;

type GlobHandle = GlobPattern;

fn downcast_glob_handle(glob: &owned::glob::Glob) -> Result<Arc<GlobHandle>, VmBamlError> {
    glob._handle
        .clone()
        .downcast::<GlobHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid glob handle type".into(),
        })
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
            Err(e) => SysOpOutput::err(VmBamlError::ParseError { message: e }),
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
                BexExternalValue::String(s) => (s.to_string(), false, false, false, false, true),
                BexExternalValue::Instance { fields, .. } => {
                    let get_string = |key: &str, default: &str| {
                        fields
                            .get(key)
                            .and_then(BexExternalValue::as_string)
                            .map(|s| s.to_string())
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
                    return Err(VmRustFnError::from(VmBamlError::InvalidArgument {
                        message: "scan argument must be a string or ScanOptions".into(),
                    }));
                }
            };

            let cwd_path = std::path::Path::new(&cwd);
            let abs_cwd = if cwd_path.is_absolute() {
                cwd_path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|e| VmBamlError::Io {
                        message: format!("Failed to get cwd: {e}"),
                    })?
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
                                return Err(VmRustFnError::from(VmBamlError::Io {
                                    message: format!("Broken symlink: {e}"),
                                }));
                            }
                            continue;
                        }
                        return Err(VmRustFnError::from(VmBamlError::Io {
                            message: format!("Walk error: {e}"),
                        }));
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

type NativeProcessResult = Result<owned::sys::ProcessExit, String>;
type NativeProcessLineResult = Result<String, String>;

struct LiveProcessHandle {
    kill_tx: tokio::sync::watch::Sender<bool>,
    exit_rx: tokio::sync::watch::Receiver<Option<NativeProcessResult>>,
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    deadline: Option<tokio::time::Instant>,
    timeout_ms: Option<i64>,
    label: String,
}

struct ProcessLineStreamHandle {
    lines: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<NativeProcessLineResult>>,
    close_tx: tokio::sync::watch::Sender<bool>,
    closed: std::sync::atomic::AtomicBool,
    deadline: Option<tokio::time::Instant>,
    timeout_ms: Option<i64>,
    label: String,
}

fn downcast_process_handle(
    process: &owned::sys::Process,
) -> Result<Arc<LiveProcessHandle>, VmBamlError> {
    process
        ._handle
        .clone()
        .downcast::<LiveProcessHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid process handle type".into(),
        })
}

fn downcast_process_line_stream_handle(
    stream: &owned::sys::ProcessLineStream,
) -> Result<Arc<ProcessLineStreamHandle>, VmBamlError> {
    stream
        ._handle
        .clone()
        .downcast::<ProcessLineStreamHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid process stdout stream handle type".into(),
        })
}

fn process_exit_from_status(status: std::process::ExitStatus) -> owned::sys::ProcessExit {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt as _;
        status.signal().map(|signal| signal.to_string())
    };
    #[cfg(not(unix))]
    let signal = None;

    owned::sys::ProcessExit {
        exit_code: i64::from(status.code().unwrap_or(-1)),
        signal,
    }
}

fn process_timeout_error(label: &str, timeout_ms: Option<i64>) -> VmBamlError {
    let duration_ms = timeout_ms.unwrap_or(0);
    VmBamlError::Timeout {
        message: format!("Command '{label}' timed out after {duration_ms}ms"),
        duration_ms: Some(duration_ms),
    }
}

async fn receive_process_exit(
    mut exit_rx: tokio::sync::watch::Receiver<Option<NativeProcessResult>>,
) -> Result<owned::sys::ProcessExit, VmRustFnError> {
    loop {
        if let Some(result) = exit_rx.borrow().clone() {
            return result.map_err(|message| VmBamlError::Io { message }.into());
        }
        exit_rx.changed().await.map_err(|_| VmBamlError::Io {
            message: "Process monitor stopped before reporting an exit status".into(),
        })?;
    }
}

fn start_stdout_line_reader(
    stdout: tokio::process::ChildStdout,
    deadline: Option<tokio::time::Instant>,
    timeout_ms: Option<i64>,
    label: String,
) -> owned::sys::ProcessLineStream {
    let (lines_tx, lines_rx) = tokio::sync::mpsc::channel(32);
    let (close_tx, mut close_rx) = tokio::sync::watch::channel(false);
    let reader_label = label.clone();

    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt as _;

        let mut reader = tokio::io::BufReader::new(stdout);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            let read_result = tokio::select! {
                biased;
                _ = close_rx.changed() => break,
                result = reader.read_until(b'\n', &mut bytes) => result,
            };

            match read_result {
                Ok(0) => break,
                Ok(_) => {
                    if bytes.last() == Some(&b'\n') {
                        bytes.pop();
                    }
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    let line = String::from_utf8_lossy(&bytes).into_owned();
                    tokio::select! {
                        biased;
                        _ = close_rx.changed() => break,
                        result = lines_tx.send(Ok(line)) => {
                            if result.is_err() {
                                break;
                            }
                        }
                    }
                }
                Err(error) => {
                    let _ = lines_tx
                        .send(Err(format!(
                            "Failed to read stdout from '{reader_label}': {error}"
                        )))
                        .await;
                    break;
                }
            }
        }
    });

    owned::sys::ProcessLineStream {
        _handle: Arc::new(ProcessLineStreamHandle {
            lines: tokio::sync::Mutex::new(lines_rx),
            close_tx,
            closed: std::sync::atomic::AtomicBool::new(false),
            deadline,
            timeout_ms,
            label,
        }),
    }
}

impl io::IoClassSysProcessLineStream for NativeSysOps {
    fn _next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        processlinestream: owned::sys::ProcessLineStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::async_op(async move {
            let handle = downcast_process_line_stream_handle(&processlinestream)?;
            if handle.closed.load(std::sync::atomic::Ordering::Acquire) {
                return Ok(None);
            }
            let mut lines = handle.lines.lock().await;
            let next_line = if let Some(deadline) = handle.deadline {
                match tokio::time::timeout_at(deadline, lines.recv()).await {
                    Ok(line) => line,
                    Err(_) => {
                        return Err(process_timeout_error(&handle.label, handle.timeout_ms).into());
                    }
                }
            } else {
                lines.recv().await
            };

            match next_line {
                Some(Ok(line)) => Ok(Some(line)),
                Some(Err(message)) => Err(VmBamlError::Io { message }.into()),
                None => Ok(None),
            }
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        processlinestream: owned::sys::ProcessLineStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        if let Ok(handle) = downcast_process_line_stream_handle(&processlinestream) {
            handle
                .closed
                .store(true, std::sync::atomic::Ordering::Release);
            let _ = handle.close_tx.send(true);
        }
        SysOpOutput::ok(())
    }
}

impl io::IoClassSysProcess for NativeSysOps {
    fn write_stdin(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        process: owned::sys::Process,
        data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            use tokio::io::AsyncWriteExt as _;

            let handle = downcast_process_handle(&process)?;
            let mut stdin = handle.stdin.lock().await;
            let pipe = stdin.as_mut().ok_or_else(|| VmBamlError::Io {
                message: format!("Stdin for '{}' is not open", handle.label),
            })?;
            pipe.write_all(data.as_bytes())
                .await
                .map_err(|error| VmBamlError::Io {
                    message: format!("Failed to write stdin to '{}': {error}", handle.label),
                })?;
            pipe.flush().await.map_err(|error| VmBamlError::Io {
                message: format!("Failed to flush stdin for '{}': {error}", handle.label),
            })?;
            Ok(())
        })
    }

    fn close_stdin(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        process: owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            let handle = downcast_process_handle(&process)?;
            handle.stdin.lock().await.take();
            Ok(())
        })
    }

    fn wait(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        process: owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sys::ProcessExit> {
        SysOpOutput::async_op(async move {
            let handle = downcast_process_handle(&process)?;
            if let Some(deadline) = handle.deadline {
                match tokio::time::timeout_at(
                    deadline,
                    receive_process_exit(handle.exit_rx.clone()),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => {
                        let _ = handle.kill_tx.send(true);
                        Err(process_timeout_error(&handle.label, handle.timeout_ms).into())
                    }
                }
            } else {
                receive_process_exit(handle.exit_rx.clone()).await
            }
        })
    }

    fn kill(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        process: owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        match downcast_process_handle(&process) {
            Ok(handle) => {
                let _ = handle.kill_tx.send(true);
                SysOpOutput::ok(())
            }
            Err(error) => SysOpOutput::err(error),
        }
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        process: owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        if let Ok(handle) = downcast_process_handle(&process) {
            let _ = handle.kill_tx.send(true);
        }
        if let Ok(handle) = downcast_process_line_stream_handle(&process.stdout) {
            handle
                .closed
                .store(true, std::sync::atomic::Ordering::Release);
            let _ = handle.close_tx.send(true);
        }
        SysOpOutput::ok(())
    }
}

/// Shared helper: apply `ProcessOptions` to a `tokio::process::Command`, run
/// it, and collect its output. Both `exec()` and `shell()` use this.
async fn run_process(
    cmd: &mut tokio::process::Command,
    options: Option<owned::sys::ProcessOptions>,
    label: &str,
) -> Result<owned::sys::ShellOutput, VmRustFnError> {
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

    let mut child = cmd.spawn().map_err(|e| VmBamlError::Io {
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
            Ok(Ok(result)) => result.map_err(|e| VmBamlError::Io {
                message: format!("Failed to wait on '{label}': {e}"),
            })?,
            Ok(Err(join_err)) => {
                return Err(VmRustFnError::from(VmBamlError::Io {
                    message: format!("Task join error waiting on '{label}': {join_err}"),
                }));
            }
            Err(_elapsed) => {
                // Abort the task so task_child is dropped, triggering kill_on_drop
                wait_task.abort();
                return Err(VmRustFnError::from(VmBamlError::Timeout {
                    message: format!("Command '{label}' timed out after {ms}ms"),
                    duration_ms: i64::try_from(duration.as_millis()).ok(),
                }));
            }
        }
    } else {
        child
            .wait_with_output()
            .await
            .map_err(|e| VmBamlError::Io {
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
    fn collect_garbage(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // BexEngine intercepts this operation before ordinary sys-op dispatch
        // so it can release the calling VM's heap permit and coordinate a
        // stop-the-world collection. This fallback keeps the generated IO
        // contract complete for alternate dispatchers.
        SysOpOutput::ok(())
    }

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

    fn start_process(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        program: String,
        args: Option<Vec<String>>,
        options: Option<owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::sys::Process> {
        SysOpOutput::async_op(async move {
            use std::process::Stdio;

            use tokio::io::AsyncWriteExt as _;

            let mut cmd = tokio::process::Command::new(&program);
            if let Some(ref args) = args {
                cmd.args(args);
            }
            if let Some(ref options) = options {
                if let Some(ref cwd) = options.cwd {
                    cmd.current_dir(cwd);
                }
                if let Some(ref env) = options.env {
                    cmd.env_clear();
                    cmd.envs(
                        env.iter()
                            .map(|(key, value)| (key.as_str(), value.as_str())),
                    );
                }
                if options.stdin.is_some() || options.keep_stdin_open == Some(true) {
                    cmd.stdin(Stdio::piped());
                }
            }

            cmd.stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true);

            let mut child = cmd.spawn().map_err(|error| VmBamlError::Io {
                message: format!("Failed to spawn '{program}': {error}"),
            })?;
            let stdout = child.stdout.take().ok_or_else(|| VmBamlError::Io {
                message: format!("Failed to capture stdout from '{program}'"),
            })?;

            let keep_stdin_open = options
                .as_ref()
                .and_then(|options| options.keep_stdin_open)
                .unwrap_or(false);
            let mut stdin = child.stdin.take();
            if let Some(stdin_data) = options.as_ref().and_then(|options| options.stdin.as_ref()) {
                if let Some(stdin) = stdin.as_mut() {
                    stdin
                        .write_all(stdin_data.as_bytes())
                        .await
                        .map_err(|error| VmBamlError::Io {
                            message: format!("Failed to write stdin to '{program}': {error}"),
                        })?;
                }
            }
            if !keep_stdin_open {
                stdin = None;
            }

            let timeout_ms = options
                .as_ref()
                .and_then(|options| options.timeout_ms)
                .map(|milliseconds| milliseconds.max(0));
            let deadline = timeout_ms.and_then(|milliseconds| {
                tokio::time::Instant::now().checked_add(std::time::Duration::from_millis(
                    milliseconds.cast_unsigned(),
                ))
            });
            let stdout = start_stdout_line_reader(stdout, deadline, timeout_ms, program.clone());

            let (kill_tx, mut kill_rx) = tokio::sync::watch::channel(false);
            let (exit_tx, exit_rx) = tokio::sync::watch::channel(None);
            let monitor_label = program.clone();
            tokio::spawn(async move {
                let exit = tokio::select! {
                    biased;
                    _ = kill_rx.changed() => {
                        let _ = child.start_kill();
                        child.wait().await
                    }
                    result = child.wait() => result,
                }
                .map(process_exit_from_status)
                .map_err(|error| format!("Failed to wait on '{monitor_label}': {error}"));
                let _ = exit_tx.send(Some(exit));
            });

            Ok(owned::sys::Process {
                stdout,
                _handle: Arc::new(LiveProcessHandle {
                    kill_tx,
                    exit_rx,
                    stdin: tokio::sync::Mutex::new(stdin),
                    deadline,
                    timeout_ms,
                    label: program,
                }),
            })
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
        delay: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        let nanos = match sleep_nanos_from_delay(delay) {
            Ok(nanos) => nanos,
            Err(err) => return SysOpOutput::err(err),
        };
        SysOpOutput::async_op(async move {
            tokio::time::sleep(std::time::Duration::from_nanos(nanos)).await;
            Ok(())
        })
    }

    fn pid(&self, _heap: &Arc<BexHeap>, _call_id: CallId, _ctx: &SysOpContext) -> SysOpOutput<i64> {
        // `std::process::id` is a `u32` on every platform this crate builds
        // for, so the widening into BAML's i63 `int` is always exact.
        SysOpOutput::ok(i64::from(std::process::id()))
    }
}

fn sleep_nanos_from_delay(delay: BexExternalValue) -> Result<u64, VmRustFnError> {
    match delay {
        BexExternalValue::Instance {
            class_name,
            mut fields,
            ..
        } if class_name == "baml.time.Duration" => {
            let Some(nanos) = fields.swap_remove("_nanoseconds") else {
                return Err(VmRustFnError::from(VmBamlError::Io {
                    message: "sleep delay is missing Duration._nanoseconds".to_string(),
                }));
            };
            let BexExternalValue::Bigint(nanos) = nanos else {
                return Err(VmRustFnError::from(VmBamlError::Io {
                    message: "sleep delay Duration._nanoseconds is not a bigint".to_string(),
                }));
            };
            if nanos.sign() == num_bigint::Sign::Plus {
                Ok(u64::try_from(&nanos).unwrap_or(u64::MAX))
            } else {
                Ok(0)
            }
        }
        BexExternalValue::Union { value, .. } => sleep_nanos_from_delay(*value),
        other => Err(VmRustFnError::from(VmBamlError::Io {
            message: format!(
                "sleep delay must be baml.time.Duration, got {}",
                other.type_name()
            ),
        })),
    }
}

// ============================================================================
// Network
// ============================================================================

// Network handles mirror `FsFileHandle`: the socket lives inside a
// `Mutex<Option<_>>` so `close()` can `take()` it, after which every other
// reference to the same handle deterministically observes a closed socket.
//
// `TcpStream` ops need `&mut` access, so (like `fs::File`) we hold the guard
// across the await. `TcpListener`/`UdpSocket` ops only need `&self`, so we keep
// each socket behind an inner `Arc` and clone it out under a brief lock — that
// way `close()` stays deterministic without serializing concurrent
// `accept`/`recv_from`/`send_to` on the same socket.
type NetTcpStreamHandle = tokio::sync::Mutex<Option<tokio::net::TcpStream>>;
type NetTcpListenerHandle = tokio::sync::Mutex<Option<Arc<tokio::net::TcpListener>>>;
type NetUdpSocketHandle = tokio::sync::Mutex<Option<Arc<tokio::net::UdpSocket>>>;

/// Convert a `Duration._nanoseconds` value (carried as a bigint across the
/// sys-op boundary) into an operation timeout. Zero or negative disables it
/// (`None` — block indefinitely, matching Rust's `Option<Duration>` socket
/// timeouts); a value too large for `u64` nanoseconds (~584 years) clamps to the
/// maximum. Shared by the net sys-ops and the HTTP server, so it lives here
/// (always compiled) rather than behind the `bundle-http` feature.
pub(crate) fn timeout_from_nanos(nanos: &num_bigint::BigInt) -> Option<std::time::Duration> {
    if nanos.sign() != num_bigint::Sign::Plus {
        return None;
    }
    Some(std::time::Duration::from_nanos(
        u64::try_from(nanos).unwrap_or(u64::MAX),
    ))
}

fn downcast_tcpstream(
    stream: &owned::net::TcpStream,
) -> Result<Arc<NetTcpStreamHandle>, VmBamlError> {
    stream
        ._handle
        .clone()
        .downcast::<NetTcpStreamHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid TcpStream handle type".to_string(),
        })
}

fn downcast_tcplistener(
    listener: &owned::net::TcpListener,
) -> Result<Arc<NetTcpListenerHandle>, VmBamlError> {
    listener
        ._handle
        .clone()
        .downcast::<NetTcpListenerHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid TcpListener handle type".to_string(),
        })
}

fn downcast_udpsocket(
    socket: &owned::net::UdpSocket,
) -> Result<Arc<NetUdpSocketHandle>, VmBamlError> {
    socket
        ._handle
        .clone()
        .downcast::<NetUdpSocketHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid UdpSocket handle type".to_string(),
        })
}

impl io::IoClassNetTcpStream for NativeSysOps {
    fn _connect(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        addr: String,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::TcpStream> {
        // `timeout = null` in BAML arrives as `0n`; `timeout_from_nanos` maps it
        // to `None`, leaving the connect bounded only by the OS default (matching
        // Rust's plain `TcpStream::connect`).
        let timeout = timeout_from_nanos(&timeout_nanos);
        SysOpOutput::async_op(async move {
            let connect = tokio::net::TcpStream::connect(&addr);
            let stream = match timeout {
                Some(dur) => match tokio::time::timeout(dur, connect).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        return Err(VmBamlError::Timeout {
                            message: format!("Connecting to '{addr}' timed out"),
                            duration_ms: i64::try_from(dur.as_millis()).ok(),
                        }
                        .into());
                    }
                },
                None => connect.await,
            }
            .map_err(|e| VmBamlError::Io {
                message: format!("Failed to connect to '{addr}': {e}"),
            })?;
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(Some(stream)));
            Ok(owned::net::TcpStream { _handle: handle })
        })
    }

    fn _read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        stream: owned::net::TcpStream,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        use tokio::io::AsyncReadExt;

        let timeout = timeout_from_nanos(&timeout_nanos);
        SysOpOutput::async_op(async move {
            let handle = downcast_tcpstream(&stream)?;
            let mut guard = handle.lock().await;
            let stream = guard.as_mut().ok_or_else(|| VmBamlError::Io {
                message: "TcpStream is closed".to_string(),
            })?;
            let mut buffer = vec![0u8; 4096];
            let read = stream.read(&mut buffer);
            let n = match timeout {
                Some(dur) => match tokio::time::timeout(dur, read).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        return Err(VmBamlError::Timeout {
                            message: "Reading from socket timed out".to_string(),
                            duration_ms: i64::try_from(dur.as_millis()).ok(),
                        }
                        .into());
                    }
                },
                None => read.await,
            }
            .map_err(|e| VmBamlError::Io {
                message: format!("Failed to read from socket: {e}"),
            })?;
            buffer.truncate(n);
            Ok(buffer)
        })
    }

    fn _write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        stream: owned::net::TcpStream,
        data: Vec<u8>,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use tokio::io::AsyncWriteExt;

        let timeout = timeout_from_nanos(&timeout_nanos);
        SysOpOutput::async_op(async move {
            let handle = downcast_tcpstream(&stream)?;
            let mut guard = handle.lock().await;
            let stream = guard.as_mut().ok_or_else(|| VmBamlError::Io {
                message: "TcpStream is closed".to_string(),
            })?;
            // The whole write (every byte flushed) shares one deadline.
            let write = async {
                stream.write_all(&data).await.map_err(|e| VmBamlError::Io {
                    message: format!("Failed to write to socket: {e}"),
                })?;
                stream.flush().await.map_err(|e| VmBamlError::Io {
                    message: format!("Failed to flush socket: {e}"),
                })?;
                Ok::<(), VmBamlError>(())
            };
            match timeout {
                Some(dur) => match tokio::time::timeout(dur, write).await {
                    Ok(result) => result?,
                    Err(_elapsed) => {
                        return Err(VmBamlError::Timeout {
                            message: "Writing to socket timed out".to_string(),
                            duration_ms: i64::try_from(dur.as_millis()).ok(),
                        }
                        .into());
                    }
                },
                None => write.await?,
            }
            Ok(())
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        stream: owned::net::TcpStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use tokio::io::AsyncWriteExt;

        SysOpOutput::async_op(async move {
            let handle = downcast_tcpstream(&stream)?;
            // Take the stream out of the shared handle so any other reference
            // observes a closed socket on its next op. Already-closed is a no-op.
            let Some(mut stream) = handle.lock().await.take() else {
                return Ok(());
            };
            // shutdown() flushes pending writes and closes the write half, so
            // peers waiting on EOF (e.g. curl with "Connection: close") stop
            // blocking. We swallow ENOTCONN / NotConnected since the peer may
            // already have closed. Dropping `stream` afterwards fully closes it.
            match stream.shutdown().await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotConnected => Ok(()),
                Err(e) => Err(VmBamlError::Io {
                    message: format!("Failed to close socket: {e}"),
                }),
            }
            .map_err(VmRustFnError::from)
        })
    }
}

impl io::IoClassNetTcpListener for NativeSysOps {
    fn bind(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::TcpListener> {
        SysOpOutput::async_op(async move {
            let listener =
                tokio::net::TcpListener::bind(&addr)
                    .await
                    .map_err(|e| VmBamlError::Io {
                        message: format!("Failed to bind '{addr}': {e}"),
                    })?;
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(Some(Arc::new(listener))));
            Ok(owned::net::TcpListener { _handle: handle })
        })
    }

    fn accept(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        listener: owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::TcpStream> {
        SysOpOutput::async_op(async move {
            let handle = downcast_tcplistener(&listener)?;
            // Clone the inner socket out under a brief lock so concurrent
            // accepts aren't serialized and close() stays deterministic.
            let inner = handle
                .lock()
                .await
                .as_ref()
                .cloned()
                .ok_or_else(|| VmBamlError::Io {
                    message: "TcpListener is closed".to_string(),
                })?;
            let (stream, _peer) = inner.accept().await.map_err(|e| VmBamlError::Io {
                message: format!("Failed to accept connection: {e}"),
            })?;
            let sock_handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(Some(stream)));
            Ok(owned::net::TcpStream {
                _handle: sock_handle,
            })
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        listener: owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            // Drop the listener out of the shared handle; subsequent accepts on
            // any reference return a closed error. Dropping the last Arc closes
            // the OS socket.
            let handle = downcast_tcplistener(&listener)?;
            handle.lock().await.take();
            Ok(())
        })
    }
}

impl io::IoClassNetUdpSocket for NativeSysOps {
    fn bind(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::UdpSocket> {
        SysOpOutput::async_op(async move {
            let socket = tokio::net::UdpSocket::bind(&addr)
                .await
                .map_err(|e| VmBamlError::Io {
                    message: format!("Failed to bind UDP '{addr}': {e}"),
                })?;
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(Some(Arc::new(socket))));
            Ok(owned::net::UdpSocket { _handle: handle })
        })
    }

    fn _send_to(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        socket: owned::net::UdpSocket,
        data: Vec<u8>,
        addr: String,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        let timeout = timeout_from_nanos(&timeout_nanos);
        SysOpOutput::async_op(async move {
            let handle = downcast_udpsocket(&socket)?;
            let inner = handle
                .lock()
                .await
                .as_ref()
                .cloned()
                .ok_or_else(|| VmBamlError::Io {
                    message: "UdpSocket is closed".to_string(),
                })?;
            let send = inner.send_to(&data, &addr);
            let n = match timeout {
                Some(dur) => match tokio::time::timeout(dur, send).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        return Err(VmBamlError::Timeout {
                            message: format!("Sending to '{addr}' timed out"),
                            duration_ms: i64::try_from(dur.as_millis()).ok(),
                        }
                        .into());
                    }
                },
                None => send.await,
            }
            .map_err(|e| VmBamlError::Io {
                message: format!("Failed to send to '{addr}': {e}"),
            })?;
            Ok(i64::try_from(n).unwrap_or(i64::MAX))
        })
    }

    fn _recv_from(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        socket: owned::net::UdpSocket,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::Datagram> {
        let timeout = timeout_from_nanos(&timeout_nanos);
        SysOpOutput::async_op(async move {
            let handle = downcast_udpsocket(&socket)?;
            let inner = handle
                .lock()
                .await
                .as_ref()
                .cloned()
                .ok_or_else(|| VmBamlError::Io {
                    message: "UdpSocket is closed".to_string(),
                })?;
            // A single datagram can be up to 65507 bytes for IPv4; size the
            // buffer to the max so we never silently truncate a packet.
            let mut buffer = vec![0u8; 65_536];
            let recv = inner.recv_from(&mut buffer);
            let (n, peer) = match timeout {
                Some(dur) => match tokio::time::timeout(dur, recv).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        return Err(VmBamlError::Timeout {
                            message: "Receiving datagram timed out".to_string(),
                            duration_ms: i64::try_from(dur.as_millis()).ok(),
                        }
                        .into());
                    }
                },
                None => recv.await,
            }
            .map_err(|e| VmBamlError::Io {
                message: format!("Failed to receive datagram: {e}"),
            })?;
            buffer.truncate(n);
            Ok(owned::net::Datagram {
                data: buffer,
                addr: peer.to_string(),
            })
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        socket: owned::net::UdpSocket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            // Drop the socket out of the shared handle; subsequent send_to /
            // recv_from on any reference return a closed error.
            let handle = downcast_udpsocket(&socket)?;
            handle.lock().await.take();
            Ok(())
        })
    }
}

impl io::IoNamespaceNet for NativeSysOps {}

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
            crate::http_server::downcast_body(&response._body)?
                .read_text()
                .await
                .map_err(VmRustFnError::from)
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
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
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
            crate::http_server::downcast_body(&response._body)?
                .read_bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(VmRustFnError::from)
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
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    #[cfg(feature = "bundle-http")]
    fn new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        status_code: i64,
        headers: indexmap::IndexMap<String, String>,
        body: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::ok(crate::http_server::build_response(
            status_code,
            headers,
            body,
        ))
    }

    #[cfg(not(feature = "bundle-http"))]
    fn new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _body: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    #[cfg(feature = "bundle-http")]
    fn new_streaming(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        status_code: i64,
        headers: indexmap::IndexMap<String, String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::ok(crate::http_server::build_streaming_response(
            status_code,
            headers,
        ))
    }

    #[cfg(not(feature = "bundle-http"))]
    fn new_streaming(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    #[cfg(feature = "bundle-http")]
    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: owned::http::Response,
        data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            crate::http_server::downcast_body(&response._body)?
                .write_chunk(data)
                .await
                .map_err(VmRustFnError::from)
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    #[cfg(feature = "bundle-http")]
    fn end(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            crate::http_server::downcast_body(&response._body)?
                .end_stream()
                .await
                .map_err(VmRustFnError::from)
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn end(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

/// Map a `reqwest` transport error to a category the HTTP ops declare they can
/// throw (`baml.errors.Io | baml.errors.Timeout`), so BAML `catch` clauses can
/// handle network failures instead of them surfacing as an uncatchable host
/// error.
#[cfg(feature = "bundle-http")]
fn http_transport_error(context: &str, e: &reqwest::Error) -> VmBamlError {
    if e.is_timeout() {
        VmBamlError::Timeout {
            message: format!("{context}: {e}"),
            // reqwest doesn't expose the configured timeout on the error, so
            // the elapsed duration is unknown.
            duration_ms: None,
        }
    } else {
        VmBamlError::Io {
            message: format!("{context}: {e}"),
        }
    }
}

/// Applies the optional total-request timeout (carried as bigint nanos across
/// the sys-op boundary) to a `reqwest` request builder. `0n`/negative means no
/// deadline (`timeout_from_nanos` returns `None`); a configured deadline that
/// elapses surfaces as `reqwest::Error::is_timeout`, which `http_transport_error`
/// maps to `baml.errors.Timeout`.
#[cfg(feature = "bundle-http")]
fn apply_http_timeout(
    builder: reqwest::RequestBuilder,
    timeout_nanos: &num_bigint::BigInt,
) -> reqwest::RequestBuilder {
    match timeout_from_nanos(timeout_nanos) {
        Some(dur) => builder.timeout(dur),
        None => builder,
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
    owned::http::Response {
        status_code: status,
        headers,
        url,
        _body: crate::http_server::HttpBody::client(response),
    }
}

#[cfg(feature = "bundle-http")]
impl io::IoClassHttpTlsConfig for NativeSysOps {
    fn _new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        cert_pem: Vec<u8>,
        key_pem: Vec<u8>,
        allow_tls1_2: bool,
        handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::TlsConfig> {
        crate::http_server::tls_config_new(cert_pem, key_pem, allow_tls1_2, handshake_timeout_nanos)
    }
}

#[cfg(not(feature = "bundle-http"))]
impl io::IoClassHttpTlsConfig for NativeSysOps {
    fn _new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _cert_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _allow_tls1_2: bool,
        _handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::TlsConfig> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

#[cfg(feature = "bundle-http")]
impl io::IoClassHttpServer for NativeSysOps {
    fn bind(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Server> {
        crate::http_server::bind(addr)
    }

    fn _serve(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        server: owned::http::Server,
        handler: bex_external_types::Handle,
        tls_config: Option<owned::http::TlsConfig>,
        allow_http1: bool,
        allow_http2: bool,
        max_body_size: i64,
        max_connections: i64,
        header_read_timeout_nanos: Arc<num_bigint::BigInt>,
        ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        crate::http_server::serve(
            server,
            handler,
            tls_config,
            allow_http1,
            allow_http2,
            max_body_size,
            max_connections,
            header_read_timeout_nanos,
            ctx.spawner.clone(),
            ctx.cancel.clone(),
        )
    }
}

#[cfg(not(feature = "bundle-http"))]
impl io::IoClassHttpServer for NativeSysOps {
    fn bind(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Server> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _serve(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _server: owned::http::Server,
        _handler: bex_external_types::Handle,
        _tls_config: Option<owned::http::TlsConfig>,
        _allow_http1: bool,
        _allow_http2: bool,
        _max_body_size: i64,
        _max_connections: i64,
        _header_read_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
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
                .map_err(|_| VmBamlError::DevOther {
                    message: "Invalid SSE stream handle type".into(),
                })?;

            let (buffer, notify, closed) = crate::registry::REGISTRY
                .get_sse_stream(handle.key())
                .ok_or_else(|| VmBamlError::DevOther {
                message: "SSE stream handle is invalid".into(),
            })?;

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
                            VmBamlError::DevOther {
                                message: format!("Failed to serialize SSE events: {e}"),
                            }
                        })?));
                    }
                    if let Some(err) = buf.error.take() {
                        return Err(VmRustFnError::from(VmBamlError::Io { message: err }));
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
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
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
    fn _fetch(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        url: String,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::async_op(async move {
            crate::ensure_rustls_crypto_provider();
            let client = reqwest::Client::new();
            let response = apply_http_timeout(client.get(&url), &timeout_nanos)
                .send()
                .await
                .map_err(|e| http_transport_error("HTTP fetch failed", &e))?;
            let final_url = response.url().to_string();
            Ok(build_io_http_response(response, final_url))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn _fetch(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _url: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    #[cfg(feature = "bundle-http")]
    fn _send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        request: owned::http::Request,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::async_op(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
                VmBamlError::InvalidArgument {
                    message: format!("Invalid HTTP method '{}': {e}", request.method),
                }
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

            let response = apply_http_timeout(builder, &timeout_nanos)
                .send()
                .await
                .map_err(|e| http_transport_error("HTTP send failed", &e))?;
            let final_url = response.url().to_string();
            Ok(build_io_http_response(response, final_url))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn _send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _request: owned::http::Request,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
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
                VmBamlError::InvalidArgument {
                    message: format!("Invalid HTTP method '{}': {e}", request.method),
                }
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
                .map_err(|e| http_transport_error("SSE connection failed", &e))?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<could not read body>".to_string());
                return Err(VmRustFnError::from(VmBamlError::Io {
                    message: format!("SSE request failed with status {status}: {body}"),
                }));
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
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassWsWsStream for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        stream: owned::ws::WsStream,
        text: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use futures::SinkExt;
        use tokio_tungstenite::tungstenite::Message;

        SysOpOutput::async_op(async move {
            let handle = stream
                ._handle
                .downcast::<bex_resource_types::ResourceHandle>()
                .map_err(|_| VmBamlError::DevOther {
                    message: "Invalid WebSocket stream handle type".into(),
                })?;
            let (sink, _) = crate::registry::REGISTRY
                .get_ws_stream(handle.key())
                .ok_or_else(|| VmBamlError::DevOther {
                    message: "WebSocket stream handle is invalid".into(),
                })?;
            sink.lock()
                .await
                .send(Message::text(text))
                .await
                .map_err(|error| VmBamlError::Io {
                    message: format!("WebSocket send failed: {error}"),
                })?;
            Ok(())
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _stream: owned::ws::WsStream,
        _text: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "ws".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    #[cfg(feature = "bundle-http")]
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        stream: owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        SysOpOutput::async_op(async move {
            let handle = stream
                ._handle
                .downcast::<bex_resource_types::ResourceHandle>()
                .map_err(|_| VmBamlError::DevOther {
                    message: "Invalid WebSocket stream handle type".into(),
                })?;
            let (sink, source) = crate::registry::REGISTRY
                .get_ws_stream(handle.key())
                .ok_or_else(|| VmBamlError::DevOther {
                    message: "WebSocket stream handle is invalid".into(),
                })?;
            let mut source = source.lock().await;
            loop {
                match source.next().await {
                    Some(Ok(Message::Text(text))) => {
                        return Ok(Some(text.as_str().to_string()));
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        return Err(VmBamlError::Io {
                            message: format!(
                                "received unexpected binary WebSocket frame ({} bytes) on a text-oriented stream",
                                bytes.len()
                            ),
                        }
                        .into());
                    }
                    Some(Ok(Message::Close(_))) | None => return Ok(None),
                    Some(Ok(Message::Ping(payload))) => {
                        sink.lock()
                            .await
                            .send(Message::Pong(payload))
                            .await
                            .map_err(|error| VmBamlError::Io {
                                message: format!("WebSocket pong failed: {error}"),
                            })?;
                    }
                    Some(Ok(Message::Pong(_) | Message::Frame(_))) => {}
                    Some(Err(error)) => {
                        return Err(VmBamlError::Io {
                            message: format!("WebSocket receive failed: {error}"),
                        }
                        .into());
                    }
                }
            }
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _stream: owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "ws".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    #[cfg(feature = "bundle-http")]
    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        stream: owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use bex_resource_types::ResourceRegistryRef;
        use futures::SinkExt;
        use tokio_tungstenite::tungstenite::Message;

        SysOpOutput::async_op(async move {
            if let Ok(handle) = stream
                ._handle
                .downcast::<bex_resource_types::ResourceHandle>()
            {
                if let Some((sink, _)) = crate::registry::REGISTRY.get_ws_stream(handle.key()) {
                    let _ = sink.lock().await.send(Message::Close(None)).await;
                }
                crate::registry::REGISTRY.remove(handle.key());
            }
            Ok(())
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        stream: owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use bex_resource_types::ResourceRegistryRef;

        if let Ok(handle) = stream
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        {
            crate::registry::REGISTRY.remove(handle.key());
        }
        SysOpOutput::ok(())
    }
}

impl io::IoNamespaceWs for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn _connect(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        url: String,
        headers: indexmap::IndexMap<String, String>,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::ws::WsStream> {
        use futures::StreamExt;
        use tokio::sync::Mutex;
        use tokio_tungstenite::tungstenite::{
            client::IntoClientRequest,
            http::{HeaderName, HeaderValue},
        };

        let timeout = timeout_from_nanos(&timeout_nanos);
        SysOpOutput::async_op(async move {
            crate::ensure_rustls_crypto_provider();

            let mut request =
                url.as_str()
                    .into_client_request()
                    .map_err(|error| VmBamlError::Io {
                        message: format!("invalid WebSocket URL '{url}': {error}"),
                    })?;
            for (name, value) in &headers {
                let name =
                    HeaderName::from_bytes(name.as_bytes()).map_err(|error| VmBamlError::Io {
                        message: format!("invalid WebSocket header name '{name}': {error}"),
                    })?;
                let value = HeaderValue::from_str(value).map_err(|error| VmBamlError::Io {
                    message: format!("invalid WebSocket header value for '{name}': {error}"),
                })?;
                request.headers_mut().insert(name, value);
            }

            let connect = tokio_tungstenite::connect_async(request);
            let connected = match timeout {
                Some(duration) => match tokio::time::timeout(duration, connect).await {
                    Ok(result) => result,
                    Err(_) => {
                        return Err(VmBamlError::Timeout {
                            message: format!("Connecting WebSocket to '{url}' timed out"),
                            duration_ms: i64::try_from(duration.as_millis()).ok(),
                        }
                        .into());
                    }
                },
                None => connect.await,
            };
            let (transport, _) = connected.map_err(|error| VmBamlError::Io {
                message: format!("WebSocket connect failed: {error}"),
            })?;
            let (sink, source) = transport.split();
            let handle = crate::registry::REGISTRY.register_ws_stream(
                Arc::new(Mutex::new(sink)),
                Arc::new(Mutex::new(source)),
                url,
            );
            Ok(owned::ws::WsStream {
                _handle: Arc::new(handle),
            })
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn _connect(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _url: String,
        _headers: indexmap::IndexMap<String, String>,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::ws::WsStream> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "ws".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

// BEP-034 Future methods live on the heap `Object::Future` itself (atomic
// state + SetOnce + cancel token) and are dispatched via the native-call
// path (`$rust_function` in `ns_future/future.baml`), not through sys-ops.
// See `bex_vm::package_baml` for the trait impl.

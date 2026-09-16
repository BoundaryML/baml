//! Native Tokio-based system operations provider.
//!
//! This crate provides `SysOps::native()` via an extension trait, which returns
//! a function pointer table with Tokio-based implementations for all system operations.
//!
//! # Usage
//!
//! ```ignore
//! use sys_native::SysOpsExt;
//! use bex_engine::BexEngine;
//!
//! let engine = BexEngine::new(program, SysOps::native())?;
//! ```

use std::path::{Path, PathBuf};

pub mod host_dispatch;
mod host_impls;
#[cfg(feature = "bundle-http")]
mod http_server;
mod io_impls;
pub mod registry;
pub mod shell;

pub use sys_ops::{SysOps, io};
pub use sys_types::{CallId, CompletionHandle, OpError, SysOp, SysOpContext, VmInternalError};

// HTTPS — both the client (`fetch`/`send`) and the server (`build_acceptor`) —
// needs a rustls crypto provider. Require one at compile time so `bundle-http`
// can't silently ship without one, which would panic when building a rustls
// `ServerConfig`/`ClientConfig` at runtime.
#[cfg(all(
    feature = "bundle-http",
    not(feature = "aws-crypto"),
    not(feature = "ring-crypto")
))]
compile_error!(
    "feature `bundle-http` requires a rustls crypto provider: enable `aws-crypto` (the default) or `ring-crypto`"
);

#[cfg(all(feature = "bundle-http", feature = "ring-crypto"))]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(all(
    feature = "bundle-http",
    not(feature = "ring-crypto"),
    feature = "aws-crypto"
))]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(not(feature = "bundle-http"))]
#[expect(dead_code)]
pub(crate) fn ensure_rustls_crypto_provider() {}

/// Where a program's relative paths resolve.
///
/// Every `baml.fs`, `baml.glob` and `baml.sys` operation that takes a path
/// resolves a relative one here before touching the host. The process's own
/// directory is the right answer for a command-line run; a process hosting
/// several projects' engines at once (the playground) gives each engine its
/// project root instead, and never changes directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingDir {
    /// The process's current directory, as any command-line tool reads a
    /// relative path.
    Process,
    /// This directory, whatever the process's current directory is.
    Fixed(PathBuf),
}

impl WorkingDir {
    /// `path` as the host sees it: an absolute path is its own answer, a
    /// relative one hangs off the working directory. Under
    /// [`Process`](Self::Process) a relative path is returned as written —
    /// the OS resolves it against the process's directory at the call.
    pub fn resolve(&self, path: &str) -> PathBuf {
        match self {
            Self::Process => PathBuf::from(path),
            Self::Fixed(base) => {
                let path = Path::new(path);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    base.join(path)
                }
            }
        }
    }

    /// The directory a child process starts in: `requested` (a
    /// `ProcessOptions.cwd`, resolved like any path) when given, else the
    /// working directory itself — `None` under [`Process`](Self::Process),
    /// where the child simply inherits the process's own.
    pub fn for_child(&self, requested: Option<&str>) -> Option<PathBuf> {
        match (requested, self) {
            (Some(dir), _) => Some(self.resolve(dir)),
            (None, Self::Fixed(base)) => Some(base.clone()),
            (None, Self::Process) => None,
        }
    }

    /// The program a child process runs, the way a shell reads it: a name
    /// with a directory component (`./build.sh`, `tools/gen`) is a path and
    /// resolves like one; a bare name (`git`) is left to the `PATH` lookup.
    pub fn resolve_program(&self, program: &str) -> PathBuf {
        let has_directory = Path::new(program)
            .parent()
            .is_some_and(|dir| !dir.as_os_str().is_empty());
        if has_directory {
            self.resolve(program)
        } else {
            PathBuf::from(program)
        }
    }
}

/// The native Tokio-based `sys_op` provider.
///
/// Implements IO traits (`IoNamespaceFs`, `IoNamespaceHttp`, etc.) with clean
/// typed signatures. The generated glue handles arg extraction and error wrapping.
pub struct NativeSysOps {
    /// Where the program's relative paths resolve.
    pub working_dir: WorkingDir,
}

impl NativeSysOps {
    /// A provider resolving relative paths in `working_dir`. A fixed
    /// directory must be absolute: a relative one would itself resolve
    /// against the process's directory, which is exactly what it exists to
    /// avoid.
    pub fn new(working_dir: WorkingDir) -> Self {
        debug_assert!(
            match &working_dir {
                WorkingDir::Process => true,
                WorkingDir::Fixed(base) => base.is_absolute(),
            },
            "a fixed working directory must be absolute: {working_dir:?}"
        );
        Self { working_dir }
    }
}

impl Default for NativeSysOps {
    /// The process's current directory: a command-line run.
    fn default() -> Self {
        Self::new(WorkingDir::Process)
    }
}

impl io::IoPackageBaml for NativeSysOps {}

// ============================================================================
// Extension trait
// ============================================================================

/// Extension trait to add the native constructors to `SysOps`.
pub trait SysOpsExt {
    /// The native platform, resolving relative paths against the process's
    /// current directory.
    fn native() -> Self;
    /// The native platform, resolving relative paths against `working_dir`
    /// (absolute) — for an engine whose host must not change the process's
    /// directory because other engines share it.
    fn native_in(working_dir: PathBuf) -> Self;
}

impl SysOpsExt for SysOps {
    fn native() -> Self {
        SysOps::from_impl(NativeSysOps::default())
    }

    fn native_in(working_dir: PathBuf) -> Self {
        SysOps::from_impl(NativeSysOps::new(WorkingDir::Fixed(working_dir)))
    }
}

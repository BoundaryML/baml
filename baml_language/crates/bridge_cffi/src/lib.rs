//! bridge_cffi - C FFI bindings for BAML using bex_engine.
//!
//! This crate provides the same FFI interface as `engine/language_client_cffi/`
//! but powered by `bex_engine` instead of `baml-runtime`.
//!
//! `lib.rs` defines the platform-neutral bridge surface. Native implementation
//! details live in `lib_native.rs`, while Wasm implementation details live
//! in `lib_wasm.rs`.

#[cfg(not(target_arch = "wasm32"))]
#[path = "lib_native.rs"]
mod platform;
#[cfg(target_arch = "wasm32")]
#[path = "lib_wasm.rs"]
mod platform;

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, Weak},
};

use bex_project::Bex;
#[cfg(target_arch = "wasm32")]
use vfs::error::VfsErrorKind;

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
struct WasmSourceRootFs;

#[cfg(target_arch = "wasm32")]
impl vfs::FileSystem for WasmSourceRootFs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        if path.is_empty() {
            Ok(Box::new(std::iter::empty()))
        } else {
            Err(VfsErrorKind::FileNotFound.into())
        }
    }

    fn create_dir(&self, _path: &str) -> vfs::VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn open_file(&self, _path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        Err(VfsErrorKind::FileNotFound.into())
    }

    fn create_file(&self, _path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn append_file(&self, _path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        if path.is_empty() {
            Ok(vfs::VfsMetadata {
                file_type: vfs::VfsFileType::Directory,
                len: 0,
                created: None,
                modified: None,
                accessed: None,
            })
        } else {
            Err(VfsErrorKind::FileNotFound.into())
        }
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        Ok(path.is_empty())
    }

    fn remove_file(&self, _path: &str) -> vfs::VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn remove_dir(&self, _path: &str) -> vfs::VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
}

#[cfg(target_arch = "wasm32")]
fn source_vfs_root() -> vfs::VfsPath {
    vfs::VfsPath::new(WasmSourceRootFs)
}

#[cfg(not(target_arch = "wasm32"))]
fn source_vfs_root() -> vfs::VfsPath {
    vfs::VfsPath::new(vfs::MemoryFS::new())
}

struct ActiveCallRoute {
    runtime: Weak<dyn Bex>,
}

static ACTIVE_CALL_RUNTIMES: LazyLock<Mutex<HashMap<u64, Arc<ActiveCallRoute>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn active_call_runtimes() -> MutexGuard<'static, HashMap<u64, Arc<ActiveCallRoute>>> {
    ACTIVE_CALL_RUNTIMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub(crate) struct ActiveCallRouteGuard {
    call_id: u64,
    route: Arc<ActiveCallRoute>,
}

impl Drop for ActiveCallRouteGuard {
    fn drop(&mut self) {
        let mut routes = active_call_runtimes();
        if routes
            .get(&self.call_id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.route))
        {
            routes.remove(&self.call_id);
        }
    }
}

pub(crate) fn register_active_call_runtime(
    call_id: u64,
    runtime: &Arc<dyn Bex>,
) -> ActiveCallRouteGuard {
    let route = Arc::new(ActiveCallRoute {
        runtime: Arc::downgrade(runtime),
    });
    active_call_runtimes().insert(call_id, Arc::clone(&route));
    ActiveCallRouteGuard { call_id, route }
}

pub mod baml_to_host;
pub mod buffer;
pub mod error;
pub mod handle;
mod identity;
mod sdk_logs;

pub use baml_to_host::{
    call_and_encode, call_handle_and_encode, error_to_outbound, result_to_outbound,
    unhandled_spawn_error_to_outbound,
};
pub use bridge_ctypes::baml_bridge;
pub use buffer::{Buffer, free_buffer};
pub use error::BridgeError;
pub use identity::{
    BridgeInfo, BridgeLanguage, ensure_version_compatible, register_bridge, registered_bridge,
};
pub use platform::*;

/// Get a clone of the target's global runtime, or error if not initialized.
pub fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    platform::get_runtime()
}

/// Initialize the global runtime from serialized BAML bytecode.
///
/// The payload is the same borsh-encoded `bex_vm_types::Program` that
/// `baml pack` embeds in its pack envelope. Decoding and engine construction
/// live behind `bex_project::new_from_bytecode` so the bridge stays on the
/// `bex_project` surface rather than reaching into bex internals.
pub fn initialize_runtime_from_bytecode_with_sys_ops(
    bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
    sys_ops: sys_ops::SysOps,
) -> Result<Arc<dyn Bex>, BridgeError> {
    let bridge = validate_bytecode_startup_preconditions(bytecode.is_empty())?;

    let generated_toolchain_version = embedded_baml_toml
        .map(|manifest| validate_generated_metadata(manifest, bridge))
        .transpose()?;
    let runtime: Arc<dyn Bex> = bex_project::new_from_bytecode(bytecode, sys_ops).map_err(|error| {
        let generated = generated_toolchain_version
            .as_deref()
            .map(|version| format!(" generated using BAML toolchain {version},"))
            .unwrap_or_default();
        BridgeError::Startup(format!(
            "BAML startup failed: generated SDK bytecode could not be loaded.\n\n`baml_sdk`{generated} could not be loaded by {} {}: {error}",
            bridge.bridge_runtime_name, bridge.bridge_runtime_version,
        ))
    })?;
    install_unhandled_spawn_error_handler(&runtime);
    platform::replace_runtime(runtime.clone())?;
    Ok(runtime)
}

pub(crate) fn validate_bytecode_startup_preconditions(
    bytecode_is_empty: bool,
) -> Result<&'static BridgeInfo, BridgeError> {
    let bridge = registered_bridge().ok_or_else(|| {
        BridgeError::Startup(
            "BAML startup failed: bridge identity is not registered.\n\nThe installed bridge must register its package name, package version, and required BAML toolchain before loading generated bytecode.".to_string(),
        )
    })?;
    if bytecode_is_empty {
        return Err(BridgeError::Startup(format!(
            "BAML startup failed: generated SDK contains no BAML bytecode.\n\n{} {} is installed, but the generated SDK does not contain a loadable bytecode payload.",
            bridge.bridge_runtime_name, bridge.bridge_runtime_version,
        )));
    }
    Ok(bridge)
}

fn validate_generated_metadata(
    embedded_baml_toml: &str,
    bridge: &BridgeInfo,
) -> Result<String, BridgeError> {
    let manifest: toml::Value = toml::from_str(embedded_baml_toml).map_err(|error| {
        BridgeError::Startup(format!(
            "BAML startup failed: generation metadata is invalid.\n\nThe embedded `baml.toml` could not be parsed as TOML: {error}"
        ))
    })?;
    let root = manifest.as_table().ok_or_else(|| {
        BridgeError::Startup(
            "BAML startup failed: generation metadata is invalid.\n\nThe embedded `baml.toml` root must be a TOML table.".to_string(),
        )
    })?;
    let codegen_value = root.get("__baml_codegen").ok_or_else(|| {
        BridgeError::Startup(format!(
            "BAML startup failed: generation metadata is missing.\n\nThe provided embedded `baml.toml` is missing `[__baml_codegen]`; {} {} is installed.",
            bridge.bridge_runtime_name, bridge.bridge_runtime_version,
        ))
    })?;
    let codegen = codegen_value.as_table().ok_or_else(|| {
        BridgeError::Startup(format!(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen` must be a TOML table, but found {codegen_value}."
        ))
    })?;
    let metadata_version = codegen.get("metadata_version").ok_or_else(|| {
        BridgeError::Startup(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.metadata_version` is missing; expected integer `1`.".to_string(),
        )
    })?;
    let Some(metadata_version) = metadata_version.as_integer() else {
        return Err(BridgeError::Startup(format!(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.metadata_version` must be integer `1`, but found {metadata_version}."
        )));
    };
    if metadata_version != 1 {
        return Err(BridgeError::Startup(format!(
            "BAML startup failed: unsupported generation metadata version.\n\n`baml_sdk` uses generation metadata version {metadata_version}, but {} {} supports metadata version 1.",
            bridge.bridge_runtime_name, bridge.bridge_runtime_version,
        )));
    }
    let toolchain_value = codegen.get("toolchain").ok_or_else(|| {
        BridgeError::Startup(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.toolchain` is missing; expected a TOML table.".to_string(),
        )
    })?;
    let toolchain = toolchain_value.as_table().ok_or_else(|| {
        BridgeError::Startup(format!(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.toolchain` must be a TOML table, but found {toolchain_value}."
        ))
    })?;
    let generated_value = toolchain.get("version").ok_or_else(|| {
        BridgeError::Startup(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.toolchain.version` is missing; expected a non-empty string containing canonical BAML SemVer.".to_string(),
        )
    })?;
    let generated = generated_value.as_str().ok_or_else(|| {
        BridgeError::Startup(format!(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.toolchain.version` must be a string containing canonical BAML SemVer, but found {generated_value}."
        ))
    })?;
    if generated.is_empty() {
        return Err(BridgeError::Startup(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.toolchain.version` must be non-empty, but found an empty string.".to_string(),
        ));
    }
    if semver::Version::parse(generated).is_err() {
        return Err(BridgeError::Startup(format!(
            "BAML startup failed: generation metadata is invalid.\n\n`__baml_codegen.toolchain.version` must contain canonical BAML SemVer, but found {generated:?}."
        )));
    }
    if generated != bridge.toolchain_version {
        return Err(BridgeError::Startup(format_version_skew(generated, bridge)));
    }
    Ok(generated.to_string())
}

fn format_version_skew(generated: &str, bridge: &BridgeInfo) -> String {
    format!(
        "BAML startup failed: version skew error.\n\n`baml_sdk` was generated using BAML toolchain {generated}, but {} is installed at {} and expects baml_sdk to be generated using BAML toolchain {}.\n\nTo fix this, either:\n\n  1. run `baml toolchain pin {}` to change the BAML version pinned in `baml.toml`, then re-run `baml generate`; or\n  2. install `{}` ({}) at a version that works with BAML toolchain {generated}, then re-run `baml generate`.",
        bridge.bridge_runtime_name,
        bridge.bridge_runtime_version,
        bridge.toolchain_version,
        bridge.toolchain_version,
        bridge.bridge_runtime_name,
        bridge.language.package_kind(),
    )
}

/// Initialize the global runtime from an in-memory BAML source map.
///
/// The candidate runtime is fully compiled before the process-global slot is
/// replaced, so a path or compilation failure leaves the previous runtime
/// usable. The memory filesystem supplies a platform-neutral project root;
/// source contents continue to enter through `bex_project::new`'s source map.
pub fn initialize_runtime_from_files_with_sys_ops(
    root_path: &str,
    src_files: HashMap<String, String>,
    sys_ops: sys_ops::SysOps,
) -> Result<Arc<dyn Bex>, BridgeError> {
    // `vfs::MemoryFS` timestamps its root with `SystemTime::now()`, which is
    // unavailable on wasm32-unknown-unknown. WASM therefore uses an equivalent
    // timestamp-free root marker; source contents are supplied separately.
    let vfs_root = source_vfs_root();
    let project_root =
        vfs_root
            .join(root_path)
            .map_err(|error| bex_project::RuntimeError::InvalidArgument {
                name: format!("root_path: {error}"),
            })?;

    let mut files = HashMap::with_capacity(src_files.len());
    for (name, contents) in src_files {
        if name.is_empty() {
            return Err(bex_project::RuntimeError::InvalidArgument {
                name: "source filename must not be empty".to_string(),
            }
            .into());
        }
        let validated_path = project_root.join(&name).map_err(|error| {
            bex_project::RuntimeError::InvalidArgument {
                name: format!("source filename {name:?}: {error}"),
            }
        })?;
        let root = project_root.as_str();
        let inside_root = if root.is_empty() {
            validated_path.as_str().starts_with('/')
        } else {
            validated_path
                .as_str()
                .strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
        };
        if !inside_root {
            return Err(bex_project::RuntimeError::InvalidArgument {
                name: format!("source filename {name:?} resolves outside the project root"),
            }
            .into());
        }
        files.insert(bex_project::FsPath::from_str(name), contents);
    }

    let runtime: Arc<dyn Bex> = bex_project::new(project_root, sys_ops, files)?;
    install_unhandled_spawn_error_handler(&runtime);
    platform::replace_runtime(runtime.clone())?;
    Ok(runtime)
}

#[cfg(not(target_arch = "wasm32"))]
fn install_unhandled_spawn_error_handler(runtime: &Arc<dyn Bex>) {
    runtime.set_unhandled_spawn_error_handler(Some(Arc::new(|error| {
        let cancelled = error.cancelled;
        platform::dispatch_unhandled_spawn_error(
            unhandled_spawn_error_to_outbound(error),
            cancelled,
        );
    })));
}

#[cfg(target_arch = "wasm32")]
fn install_unhandled_spawn_error_handler(_: &Arc<dyn Bex>) {}

pub async fn shutdown_runtime() -> Result<(), BridgeError> {
    if let Some(runtime) = platform::take_runtime()? {
        runtime.shutdown().await;
    }
    Ok(())
}

/// Initialize the native process-global runtime from serialized BAML bytecode.
#[cfg(not(target_arch = "wasm32"))]
pub fn initialize_runtime_from_bytecode(
    bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
) -> Result<Arc<dyn Bex>, BridgeError> {
    use sys_native::SysOpsExt as _;

    initialize_runtime_from_bytecode_with_sys_ops(
        bytecode,
        embedded_baml_toml,
        sys_ops::SysOps::native(),
    )
}

/// Allocate a new process-unique function-call ID.
pub fn new_function_call_id() -> u64 {
    bex_project::CallId::next().0
}

/// Build a function-call context builder for a CFFI-owned call id.
pub fn function_call_context_builder(
    call_id: bex_project::CallId,
) -> bex_project::FunctionCallContextBuilder {
    sdk_logs::configure_call_context(bex_project::FunctionCallContextBuilder::new(call_id))
}

/// Cancel an in-flight function call by ID.
///
/// Returns true on success, false if the runtime is not initialized.
pub fn cancel_function_call_by_id(id: u64) -> bool {
    if id == 0 {
        return false;
    }
    let originating_runtime = active_call_runtimes()
        .get(&id)
        .and_then(|route| route.runtime.upgrade());
    originating_runtime
        .map(Ok)
        .unwrap_or_else(get_runtime)
        .and_then(|runtime| {
            runtime
                .cancel_function_call(bex_project::CallId(id))
                .map_err(BridgeError::from)
        })
        .is_ok()
}

/// Allocate a new process-unique function-call ID.
#[unsafe(no_mangle)]
pub extern "C" fn new_function_call() -> u64 {
    new_function_call_id()
}

/// Cancel an in-flight function call.
///
/// Returns 0 on success, 1 if the call ID is unknown or already completed.
#[unsafe(no_mangle)]
pub extern "C" fn cancel_function_call(id: u64) -> i32 {
    if cancel_function_call_by_id(id) { 0 } else { 1 }
}

#[cfg(test)]
mod generated_metadata_tests {
    use super::*;

    fn bridge(required: &str) -> BridgeInfo {
        BridgeInfo {
            language: BridgeLanguage::Python,
            bridge_runtime_name: "baml-bridge".to_string(),
            bridge_runtime_version: "1.2.3.dev4".to_string(),
            toolchain_version: required.to_string(),
        }
    }

    fn manifest(version: &str) -> String {
        format!(
            "[package]\nname = \"test\"\n\n[__baml_codegen]\nmetadata_version = 1\n\n[__baml_codegen.toolchain]\nversion = {version:?}\n"
        )
    }

    #[test]
    fn matching_generated_toolchain_is_accepted() {
        let bridge = bridge("1.2.3");
        assert_eq!(
            validate_generated_metadata(&manifest("1.2.3"), &bridge).unwrap(),
            "1.2.3"
        );
    }

    #[test]
    fn newer_generated_toolchain_reports_complete_repair_guidance() {
        let bridge = bridge("1.2.3");
        let error = validate_generated_metadata(&manifest("1.2.4"), &bridge).unwrap_err();
        let message = error.to_string();
        assert_eq!(
            message,
            "BAML startup failed: version skew error.\n\n`baml_sdk` was generated using BAML toolchain 1.2.4, but baml-bridge is installed at 1.2.3.dev4 and expects baml_sdk to be generated using BAML toolchain 1.2.3.\n\nTo fix this, either:\n\n  1. run `baml toolchain pin 1.2.3` to change the BAML version pinned in `baml.toml`, then re-run `baml generate`; or\n  2. install `baml-bridge` (the Python package) at a version that works with BAML toolchain 1.2.4, then re-run `baml generate`."
        );
    }

    #[test]
    fn older_generated_toolchain_reports_complete_repair_guidance() {
        let bridge = bridge("1.2.3");
        let error = validate_generated_metadata(&manifest("1.2.2"), &bridge).unwrap_err();
        let message = error.to_string();
        assert!(message.starts_with("BAML startup failed: version skew error."));
        assert!(message.contains("baml-bridge is installed at 1.2.3.dev4"));
        assert!(message.contains("`baml toolchain pin 1.2.3`"));
        assert!(message.contains("install `baml-bridge` (the Python package)"));
        assert!(message.contains("BAML toolchain 1.2.2, then re-run `baml generate`"));
    }

    #[test]
    fn missing_and_unsupported_metadata_are_distinct() {
        let bridge = bridge("1.2.3");
        let missing = validate_generated_metadata("[package]\nname = \"test\"\n", &bridge)
            .unwrap_err()
            .to_string();
        assert!(missing.starts_with("BAML startup failed: generation metadata is missing."));

        let unsupported =
            validate_generated_metadata("[__baml_codegen]\nmetadata_version = 2\n", &bridge)
                .unwrap_err()
                .to_string();
        assert!(
            unsupported
                .starts_with("BAML startup failed: unsupported generation metadata version.")
        );
    }

    #[test]
    fn malformed_and_incorrectly_typed_metadata_report_precise_details() {
        let bridge = bridge("1.2.3");
        let malformed = validate_generated_metadata("[__baml_codegen\n", &bridge)
            .unwrap_err()
            .to_string();
        assert!(malformed.starts_with("BAML startup failed: generation metadata is invalid.\n\n"));
        assert!(malformed.contains("could not be parsed as TOML"));

        let incorrectly_typed =
            validate_generated_metadata("[__baml_codegen]\nmetadata_version = \"1\"\n", &bridge)
                .unwrap_err()
                .to_string();
        assert!(
            incorrectly_typed
                .starts_with("BAML startup failed: generation metadata is invalid.\n\n")
        );
        assert!(incorrectly_typed.contains("metadata_version` must be integer `1`"));
        assert!(incorrectly_typed.contains("\"1\""));

        let wrong_toolchain_type = validate_generated_metadata(
            "[__baml_codegen]\nmetadata_version = 1\n\n[__baml_codegen.toolchain]\nversion = 123\n",
            &bridge,
        )
        .unwrap_err()
        .to_string();
        assert!(wrong_toolchain_type.contains("toolchain.version` must be a string"));
        assert!(wrong_toolchain_type.contains("123"));
    }
}

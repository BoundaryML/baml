//! Shared support for this crate's unit tests: locating the
//! workspace-built engine cdylib. (Integration tests carry their own copy
//! in `tests/common/mod.rs` — `#[cfg(test)]` items are not visible to
//! them.)

use std::sync::Once;

/// Point the loader at the engine built by `cargo build -p bridge_cffi`,
/// unless the caller already provided one via `BAML_LIBRARY_PATH`.
///
/// The test binary lives in `<target>/<profile>/deps/`, so the sibling
/// `<target>/<profile>/` directory is where cargo put the cdylib —
/// regardless of `CARGO_TARGET_DIR` or profile.
pub(crate) fn locate_dev_engine() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("BAML_LIBRARY_PATH").is_some() {
            return;
        }
        let exe = std::env::current_exe().expect("current test binary path");
        let profile_dir = exe
            .parent()
            .and_then(std::path::Path::parent)
            .expect("test binary not under <target>/<profile>/deps");
        let name = if cfg!(target_os = "windows") {
            "bridge_cffi.dll"
        } else if cfg!(target_os = "macos") {
            "libbridge_cffi.dylib"
        } else {
            "libbridge_cffi.so"
        };
        let path = profile_dir.join(name);
        assert!(
            path.is_file(),
            "engine library not found at {} — run `cargo build -p bridge_cffi` first \
             (the nextest setup script does this automatically)",
            path.display()
        );
        crate::loader::set_shared_library_path(path);
    });
}

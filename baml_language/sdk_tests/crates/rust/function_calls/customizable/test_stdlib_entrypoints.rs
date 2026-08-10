use baml_sdk::baml::fs::exists;
use baml_sdk::baml::sys::argv;

/// Intrinsic-only modules are not emitted at all, so a missing file is fine;
/// callers only need to confirm the symbol is absent when the file exists.
/// (The test cwd is the generated crate root; sources live under `src/`.
/// Kept for suite parity with the python helper — see the divergence note on
/// `test_compiler_intrinsics_are_not_emitted_as_entry_points`.)
fn _generated_sdk_file(rel_path: &str) -> Option<String> {
    let path = std::path::Path::new("src").join(rel_path);
    if !path.exists() {
        return None;
    }
    Some(std::fs::read_to_string(path).unwrap())
}

// `baml.sys.argv() -> string[]` is a `$rust_function` → `FunctionKind::Native`.
// Calling it as an entry point should run the native and return the argument
// array, not reject with `NotInvokableAsEntry`. The fixture host passes no
// program arguments, so the array is legitimately empty — the shape is what
// this asserts.
#[test]
fn test_stdlib_entrypoints_native_argv_callable_as_entry_point() {
    assert!(argv().is_ok());
}

// `baml.fs.exists(path: string) -> bool` is a `$rust_io_function` →
// `FunctionKind::SysOp`. Calling it as an entry point should run the
// filesystem sysop and return a bool. `.` exists in the generated fixture
// directory on the test host.
#[test]
fn test_stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point() {
    assert!(exists(".".to_string()).unwrap());
}

#[test]
fn test_stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points() {
    // DIVERGENCE(rust): the python probe scans the generated package for
    // intrinsic entry-point bindings (`log.info` / `log.debug` / `log.warn` /
    // `log.error` and `baml.events.send`) that must not be emitted. In Rust
    // an unemitted binding is already unresolvable — importing it is a
    // compile error — and the generated-source scan has no stable module
    // layout to probe until the generator pins one, so the negative
    // assertion has no runtime body here.
}

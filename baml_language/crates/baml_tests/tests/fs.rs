//! Unified tests for filesystem operations.
//!
//! These tests change the process CWD so that BAML source can use plain
//! relative filenames, keeping bytecode snapshots free of platform-specific
//! temp-dir paths.  Because CWD is process-global we serialise the tests
//! with a mutex.

use baml_tests::baml_test;
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

/// Create a temp dir with the given files, change CWD into it, and return a
/// guard that restores the original CWD on drop.  The `TempDir` is kept alive
/// inside the guard so the directory isn't deleted early.
struct TmpGuard {
    _tmp: tempfile::TempDir,
    prev: std::path::PathBuf,
}

fn tmp_cd(files: IndexMap<&str, &str>) -> TmpGuard {
    let tmp = tempfile::TempDir::new().unwrap();
    for (name, contents) in files {
        std::fs::write(tmp.path().join(name), contents).unwrap();
    }
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    TmpGuard { _tmp: tmp, prev }
}

impl Drop for TmpGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev);
    }
}

#[tokio::test]
async fn fs_open_only() {
    let _guard = tmp_cd(indexmap::indexmap! { "hello.txt" => "Hello from BAML!" });

    let output = baml_test!(
        r#"
            function main() -> int {
                let file = baml.fs.open("hello.txt");
                42
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "hello.txt"
        dispatch_future baml.fs.open
        await
        store_var file
        load_const 42
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn fs_open_and_read() {
    let _guard = tmp_cd(indexmap::indexmap! { "hello.txt" => "Hello from BAML!" });

    let output = baml_test!(
        r#"
            function main() -> string {
                let file = baml.fs.open("hello.txt");
                file.read()
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "hello.txt"
        dispatch_future baml.fs.open
        await
        dispatch_future baml.fs.File.read
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello from BAML!".to_string()))
    );
}

#[tokio::test]
async fn fs_open_nonexistent_file() {
    let _guard = tmp_cd(indexmap::indexmap! {});

    let output = baml_test!(
        r#"
            function main() -> string {
                let file = baml.fs.open("nonexistent.txt");
                file.read()
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "nonexistent.txt"
        dispatch_future baml.fs.open
        await
        dispatch_future baml.fs.File.read
        await
        return
    }
    "#);
    // Error message contains OS error text which may differ across platforms.
    assert!(output.result.is_err());
}

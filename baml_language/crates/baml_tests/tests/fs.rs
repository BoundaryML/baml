//! Unified tests for filesystem operations.

use baml_tests::baml_test;
use bex_external_types::BexExternalValue;

/// Replace the temp dir path with a stable placeholder.
fn stabilize(s: &str, root: &str) -> String {
    s.replace(root, "<TMPDIR>")
}

#[tokio::test]
async fn fs_open_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("hello.txt"), "Hello from BAML!").unwrap();
    let root = tmp.path().display().to_string();

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let file = baml.fs.open("{root}/hello.txt");
                42
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "<TMPDIR>/hello.txt"
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
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("hello.txt"), "Hello from BAML!").unwrap();
    let root = tmp.path().display().to_string();

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.open("{root}/hello.txt");
                file.read()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "<TMPDIR>/hello.txt"
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
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().display().to_string();

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.open("{root}/nonexistent.txt");
                file.read()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "<TMPDIR>/nonexistent.txt"
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

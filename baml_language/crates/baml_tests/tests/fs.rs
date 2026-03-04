//! Unified tests for filesystem operations.

use baml_tests::baml_test;
use bex_external_types::BexExternalValue;
use indexmap::{IndexMap, indexmap};

/// Create a temp dir with the given files, return (TempDir, root path string).
/// The root always uses forward slashes so paths and snapshots are consistent
/// across platforms (Windows accepts `/` just fine).
fn tmp(files: IndexMap<&str, &str>) -> (tempfile::TempDir, String) {
    let tmp = tempfile::TempDir::new().unwrap();
    for (name, contents) in files {
        std::fs::write(tmp.path().join(name), contents).unwrap();
    }
    let root = tmp.path().display().to_string().replace('\\', "/");
    (tmp, root)
}

/// Replace the temp dir path with a stable placeholder.
fn stabilize(s: &str, root: &str) -> String {
    s.replace(root, "{TMPDIR}")
}

#[tokio::test]
async fn fs_open_only() {
    let (_tmp, root) = tmp(indexmap! { "hello.txt" => "Hello from BAML!" });

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
        load_const "{TMPDIR}/hello.txt"
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
    let (_tmp, root) = tmp(indexmap! { "hello.txt" => "Hello from BAML!" });

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
        load_const "{TMPDIR}/hello.txt"
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
    let (_tmp, root) = tmp(indexmap! {});

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
        load_const "{TMPDIR}/nonexistent.txt"
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

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
                let file = baml.fs.file("{root}/hello.txt", "r");
                42
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/hello.txt"
        load_const "r"
        dispatch_future baml.fs.file
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
                let file = baml.fs.file("{root}/hello.txt", "r");
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "{TMPDIR}/hello.txt"
        load_const "r"
        dispatch_future baml.fs.file
        await
        dispatch_future baml.fs.File.text
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
async fn fs_open_and_read_bytes() {
    let (_tmp, root) = tmp(indexmap! { "hello.txt" => "Hello from BAML!" });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let file = baml.fs.file("{root}/hello.txt", "r");
                file.bytes()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> uint8array {
        load_const "{TMPDIR}/hello.txt"
        load_const "r"
        dispatch_future baml.fs.file
        await
        dispatch_future baml.fs.File.bytes
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"Hello from BAML!".to_vec()))
    );
}

#[tokio::test]
async fn fs_open_nonexistent_file() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.file("{root}/nonexistent.txt", "r");
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "{TMPDIR}/nonexistent.txt"
        load_const "r"
        dispatch_future baml.fs.file
        await
        dispatch_future baml.fs.File.text
        await
        return
    }
    "#);
    // Error message contains OS error text which may differ across platforms.
    assert!(output.result.is_err());
}

#[tokio::test]
async fn fs_write_string() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                baml.fs.write("{root}/output.txt", "Hello, world!")
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(output.result, Ok(BexExternalValue::Int(13)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/output.txt")).unwrap(),
        "Hello, world!"
    );
}

#[tokio::test]
async fn fs_write_bytes() {
    let (_tmp, root) = tmp(indexmap! { "source.bin" => "binary data" });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let data = baml.fs.file("{root}/source.bin", "r").bytes();
                baml.fs.write_bytes("{root}/copy.bin", data)
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
    assert_eq!(
        std::fs::read(format!("{root}/copy.bin")).unwrap(),
        b"binary data"
    );
}

#[tokio::test]
async fn fs_write_creates_parent_dirs() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                baml.fs.write("{root}/nested/dir/file.txt", "nested content")
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(output.result, Ok(BexExternalValue::Int(14)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/nested/dir/file.txt")).unwrap(),
        "nested content"
    );
}

#[tokio::test]
async fn fs_write_overwrites_existing() {
    let (_tmp, root) = tmp(indexmap! { "existing.txt" => "old content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                baml.fs.write("{root}/existing.txt", "new content")
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/existing.txt")).unwrap(),
        "new content"
    );
}

#[tokio::test]
async fn fs_roundtrip_write_and_read() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                baml.fs.write("{root}/roundtrip.txt", "roundtrip data");
                let f = baml.fs.file("{root}/roundtrip.txt", "r");
                f.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("roundtrip data".to_string()))
    );
}

#[tokio::test]
async fn fs_file_rw_seek_and_read() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "Hello from BAML!" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.file("{root}/data.txt", "r+");
                file.seek(6);
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("from BAML!".to_string()))
    );
}

#[tokio::test]
async fn fs_file_rw_write_and_read_back() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "Hello from BAML!" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.file("{root}/data.txt", "r+");
                file.seek(6);
                file.write("to Rust!!");
                file.seek(0);
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello to Rust!!!".to_string()))
    );
}

#[tokio::test]
async fn fs_file_rw_write_bytes() {
    let (_tmp, root) = tmp(indexmap! {
        "data.bin" => "\x00\x00\x00\x00",
        "source.bin" => "AB",
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let bytes = baml.fs.file("{root}/source.bin", "r").bytes();
                let file = baml.fs.file("{root}/data.bin", "r+");
                file.seek(0);
                file.write_bytes(bytes)
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
    assert_eq!(
        &std::fs::read(format!("{root}/data.bin")).unwrap()[..2],
        b"AB"
    );
}

#[tokio::test]
async fn fs_file_write_on_readonly_errors() {
    let (_tmp, root) = tmp(indexmap! { "readonly.txt" => "content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let file = baml.fs.file("{root}/readonly.txt", "r");
                file.write("should fail")
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root));
    assert!(
        output.result.is_err(),
        "Expected error but got: {:?}",
        output.result
    );
}

#[tokio::test]
#[should_panic(expected = "type mismatch")]
async fn fs_file_invalid_mode() {
    let (_tmp, root) = tmp(indexmap! { "file.txt" => "content" });

    // The mode parameter is typed as "r" | "r+", so invalid modes like "w"
    // are caught at compile time as a type mismatch.
    let _output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.file("{root}/file.txt", "w");
                file.text()
            }}
        "#
    ));
}

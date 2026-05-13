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
async fn fs_open_and_read() {
    let (_tmp, root) = tmp(indexmap! { "hello.txt" => "Hello from BAML!" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.open("{root}/hello.txt", "r");
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "{TMPDIR}/hello.txt"
        load_const "r"
        sys_op baml.fs.open
        sys_op baml.fs.File.text
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
                let file = baml.fs.open("{root}/hello.txt", "r");
                file.bytes()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> uint8array {
        load_const "{TMPDIR}/hello.txt"
        load_const "r"
        sys_op baml.fs.open
        sys_op baml.fs.File.bytes
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
                let file = baml.fs.open("{root}/nonexistent.txt", "r");
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "{TMPDIR}/nonexistent.txt"
        load_const "r"
        sys_op baml.fs.open
        sys_op baml.fs.File.text
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

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/output.txt"
        load_const "Hello, world!"
        sys_op baml.fs.write
        return
    }
    "#);
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
                let data = baml.fs.open("{root}/source.bin", "r").bytes();
                baml.fs.write_bytes("{root}/copy.bin", data)
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/source.bin"
        load_const "r"
        sys_op baml.fs.open
        sys_op baml.fs.File.bytes
        store_var data
        load_const "{TMPDIR}/copy.bin"
        load_var data
        sys_op baml.fs.write_bytes
        return
    }
    "#);
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

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/nested/dir/file.txt"
        load_const "nested content"
        sys_op baml.fs.write
        return
    }
    "#);
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

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/existing.txt"
        load_const "new content"
        sys_op baml.fs.write
        return
    }
    "#);
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
                let f = baml.fs.open("{root}/roundtrip.txt", "r");
                f.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "{TMPDIR}/roundtrip.txt"
        load_const "roundtrip data"
        sys_op baml.fs.write
        pop 1
        load_const "{TMPDIR}/roundtrip.txt"
        load_const "r"
        sys_op baml.fs.open
        sys_op baml.fs.File.text
        return
    }
    "#);
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
                let file = baml.fs.open("{root}/data.txt", "r+");
                file.seek_from("start", 6);
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "{TMPDIR}/data.txt"
        load_const "r+"
        sys_op baml.fs.open
        store_var file
        load_var file
        load_const "start"
        load_const 6
        sys_op baml.fs.File.seek_from
        pop 1
        load_var file
        sys_op baml.fs.File.text
        return
    }
    "#);
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
                let file = baml.fs.open("{root}/data.txt", "r+");
                file.seek_from("start", 6);
                file.write("to Rust!!");
                file.seek_from("start", 0);
                file.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> string {
        load_const "{TMPDIR}/data.txt"
        load_const "r+"
        sys_op baml.fs.open
        store_var file
        load_var file
        load_const "start"
        load_const 6
        sys_op baml.fs.File.seek_from
        pop 1
        load_var file
        load_const "to Rust!!"
        sys_op baml.fs.File.write
        pop 1
        load_var file
        load_const "start"
        load_const 0
        sys_op baml.fs.File.seek_from
        pop 1
        load_var file
        sys_op baml.fs.File.text
        return
    }
    "#);
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
                let bytes = baml.fs.open("{root}/source.bin", "r").bytes();
                let file = baml.fs.open("{root}/data.bin", "r+");
                file.seek_from("start", 0);
                file.write_bytes(bytes)
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/source.bin"
        load_const "r"
        sys_op baml.fs.open
        sys_op baml.fs.File.bytes
        store_var bytes
        load_const "{TMPDIR}/data.bin"
        load_const "r+"
        sys_op baml.fs.open
        store_var file
        load_var file
        load_const "start"
        load_const 0
        sys_op baml.fs.File.seek_from
        pop 1
        load_var file
        load_var bytes
        sys_op baml.fs.File.write_bytes
        return
    }
    "#);
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
                let file = baml.fs.open("{root}/readonly.txt", "r");
                file.write("should fail")
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/readonly.txt"
        load_const "r"
        sys_op baml.fs.open
        load_const "should fail"
        sys_op baml.fs.File.write
        return
    }
    "#);
    let Err(bex_engine::EngineError::ExternalOpFailed(op_err)) = &output.result else {
        panic!("expected ExternalOpFailed, got: {:?}", output.result);
    };
    assert_eq!(op_err.fn_name, sys_types::SysOp::BamlFsFileWrite);
    let sys_types::OpErrorKind::Other(msg) = &op_err.kind else {
        panic!("expected OpErrorKind::Other, got: {:?}", op_err.kind);
    };
    assert!(
        msg.starts_with("Failed to write:"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
#[should_panic(expected = "type mismatch")]
async fn fs_file_invalid_mode() {
    let (_tmp, root) = tmp(indexmap! { "file.txt" => "content" });

    // The mode parameter is a string-literal union, so invalid modes like "x"
    // are caught at compile time as a type mismatch.
    let _output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let file = baml.fs.open("{root}/file.txt", "x");
                file.text()
            }}
        "#
    ));
}

#[tokio::test]
async fn fs_exists_returns_true() {
    let (_tmp, root) = tmp(indexmap! { "here.txt" => "x" });

    let output = baml_test!(&format!(
        r#"
            function main() -> bool {{
                baml.fs.exists("{root}/here.txt")
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> bool {
        load_const "{TMPDIR}/here.txt"
        sys_op baml.fs.exists
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn fs_exists_returns_false() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> bool {{
                baml.fs.exists("{root}/missing.txt")
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn fs_remove_deletes_file() {
    let (_tmp, root) = tmp(indexmap! { "doomed.txt" => "bye" });
    let path = format!("{root}/doomed.txt");
    assert!(std::path::Path::new(&path).exists());

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.remove("{path}")
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Null));
    assert!(!std::path::Path::new(&path).exists());
}

#[tokio::test]
async fn fs_remove_nonexistent_errors() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.remove("{root}/nope.txt")
            }}
        "#
    ));

    assert!(output.result.is_err());
}

#[tokio::test]
async fn fs_size_returns_length() {
    let (_tmp, root) = tmp(indexmap! { "data.bin" => "0123456789" });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                baml.fs.size("{root}/data.bin")
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize(&output.bytecode, &root), @r#"
    function main() -> int {
        load_const "{TMPDIR}/data.bin"
        sys_op baml.fs.size
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn fs_file_read_n_bytes() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "Hello, world!" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/data.txt", "r");
                f.read(5)
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello".to_string()))
    );
}

#[tokio::test]
async fn fs_file_read_truncates_at_eof() {
    let (_tmp, root) = tmp(indexmap! { "short.txt" => "abc" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/short.txt", "r");
                f.read(100)
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("abc".to_string()))
    );
}

#[tokio::test]
async fn fs_file_read_bytes_n() {
    let (_tmp, root) = tmp(indexmap! { "bin.dat" => "\x01\x02\x03\x04\x05" });

    let output = baml_test!(&format!(
        r#"
            function main() -> uint8array {{
                let f = baml.fs.open("{root}/bin.dat", "r");
                f.read_bytes(3)
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(vec![1, 2, 3]))
    );
}

#[tokio::test]
async fn fs_file_read_advances_cursor() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "ABCDEFGH" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/data.txt", "r");
                f.read(3);
                f.read(3)
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("DEF".to_string()))
    );
}

#[tokio::test]
async fn fs_file_read_negative_n_errors() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "abc" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/data.txt", "r");
                f.read(-1)
            }}
        "#
    ));

    assert!(output.result.is_err());
}

#[tokio::test]
async fn fs_open_append_creates_and_appends() {
    let (_tmp, root) = tmp(indexmap! { "log.txt" => "start!" });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let f = baml.fs.open("{root}/log.txt", "a");
                f.write("more!")
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/log.txt")).unwrap(),
        "start!more!"
    );
}

#[tokio::test]
async fn fs_open_append_creates_missing_file() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let f = baml.fs.open("{root}/new.txt", "a");
                f.write("hi")
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/new.txt")).unwrap(),
        "hi"
    );
}

#[tokio::test]
async fn fs_open_append_plus_can_read() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "existing" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/data.txt", "a+");
                f.write(" more");
                f.seek_from("start", 0);
                f.text()
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("existing more".to_string()))
    );
}

#[tokio::test]
async fn fs_file_close_invalidates_handle() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/data.txt", "r");
                f.close();
                f.text()
            }}
        "#
    ));

    assert!(
        output.result.is_err(),
        "Expected error reading after close, got: {:?}",
        output.result
    );
}

#[tokio::test]
async fn fs_file_close_is_idempotent() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "x" });

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                let f = baml.fs.open("{root}/data.txt", "r");
                f.close();
                f.close()
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn fs_write_wrapper_closes_on_error() {
    // Writing to a directory errors out; the wrapper must still release the
    // underlying handle so the directory remains usable afterward.
    let (_tmp, root) = tmp(indexmap! {});
    std::fs::create_dir(format!("{root}/is_a_dir")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                baml.fs.write("{root}/is_a_dir", "oops")
            }}
        "#
    ));

    assert!(
        output.result.is_err(),
        "Expected write-to-dir to fail: {:?}",
        output.result
    );
    // If the wrapper leaked the handle, removing the dir on Windows would
    // fail with a sharing violation; on Unix it's a soft check.
    std::fs::remove_dir(format!("{root}/is_a_dir")).unwrap();
}

#[tokio::test]
async fn fs_read_returns_contents() {
    let (_tmp, root) = tmp(indexmap! { "hello.txt" => "Hello from fs.read!" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                baml.fs.read("{root}/hello.txt")
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello from fs.read!".to_string()))
    );
}

#[tokio::test]
async fn fs_read_nonexistent_errors() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                baml.fs.read("{root}/missing.txt")
            }}
        "#
    ));

    assert!(output.result.is_err());
}

#[tokio::test]
async fn fs_file_seek_negative_errors() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/data.txt", "r+");
                f.seek_from("start", -1);
                f.text()
            }}
        "#
    ));

    assert!(output.result.is_err());
}

#[tokio::test]
async fn fs_open_w_truncates_existing() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "original content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let f = baml.fs.open("{root}/data.txt", "w");
                f.write("new")
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/data.txt")).unwrap(),
        "new"
    );
}

#[tokio::test]
async fn fs_open_w_creates_missing_file() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let f = baml.fs.open("{root}/new.txt", "w");
                f.write("hi")
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/new.txt")).unwrap(),
        "hi"
    );
}

#[tokio::test]
async fn fs_open_w_plus_reads_after_write() {
    let (_tmp, root) = tmp(indexmap! { "data.txt" => "original" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let f = baml.fs.open("{root}/data.txt", "w+");
                f.write("fresh");
                f.seek_from("start", 0);
                f.text()
            }}
        "#
    ));

    // "w+" truncates on open, so only the new write is visible.
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("fresh".to_string()))
    );
}

#[tokio::test]
async fn fs_open_w_creates_parent_dirs() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let f = baml.fs.open("{root}/nested/dir/new.txt", "w");
                f.write("nested")
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/nested/dir/new.txt")).unwrap(),
        "nested"
    );
}

// ============================================================================
// read_dir tests
// ============================================================================

#[tokio::test]
async fn fs_read_dir_returns_entries() {
    let (_tmp, root) = tmp(indexmap! {
        "a.txt" => "aaa",
        "b.txt" => "bbb",
    });
    std::fs::create_dir(format!("{root}/subdir")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> baml.fs.DirEntry[] {{
                baml.fs.read_dir("{root}")
            }}
        "#
    ));

    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array, got: {:?}", output.result);
    };
    assert_eq!(items.len(), 3);
    let mut names: Vec<String> = items
        .iter()
        .map(|item| {
            let BexExternalValue::Instance { fields, .. } = item else {
                panic!("expected instance, got: {item:?}");
            };
            let BexExternalValue::String(name) = &fields["name"] else {
                panic!("expected string name");
            };
            name.clone()
        })
        .collect();
    names.sort();
    assert_eq!(names, vec!["a.txt", "b.txt", "subdir"]);
}

#[tokio::test]
async fn fs_read_dir_type_flags() {
    let (_tmp, root) = tmp(indexmap! { "file.txt" => "content" });
    std::fs::create_dir(format!("{root}/dir")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> baml.fs.DirEntry[] {{
                baml.fs.read_dir("{root}")
            }}
        "#
    ));

    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array, got: {:?}", output.result);
    };
    assert_eq!(items.len(), 2);
    for item in items {
        let BexExternalValue::Instance { fields, .. } = item else {
            panic!("expected instance");
        };
        let BexExternalValue::String(name) = &fields["name"] else {
            panic!("expected string name");
        };
        let BexExternalValue::Bool(is_dir) = &fields["is_dir"] else {
            panic!("expected bool is_dir");
        };
        let BexExternalValue::Bool(is_file) = &fields["is_file"] else {
            panic!("expected bool is_file");
        };
        let BexExternalValue::Bool(is_symlink) = &fields["is_symlink"] else {
            panic!("expected bool is_symlink");
        };
        match name.as_str() {
            "file.txt" => {
                assert!(!is_dir, "file.txt should not be a dir");
                assert!(is_file, "file.txt should be a file");
                assert!(!is_symlink, "file.txt should not be a symlink");
            }
            "dir" => {
                assert!(is_dir, "dir should be a dir");
                assert!(!is_file, "dir should not be a file");
                assert!(!is_symlink, "dir should not be a symlink");
            }
            _ => panic!("unexpected entry: {name}"),
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn fs_read_dir_reports_symlink_flag() {
    let (_tmp, root) = tmp(indexmap! { "target.txt" => "content" });
    std::os::unix::fs::symlink(format!("{root}/target.txt"), format!("{root}/link.txt")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> baml.fs.DirEntry[] {{
                baml.fs.read_dir("{root}")
            }}
        "#
    ));

    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array, got: {:?}", output.result);
    };
    let link = items
        .iter()
        .find_map(|item| {
            let BexExternalValue::Instance { fields, .. } = item else {
                return None;
            };
            match &fields["name"] {
                BexExternalValue::String(name) if name == "link.txt" => Some(fields),
                _ => None,
            }
        })
        .expect("expected link.txt entry");

    assert_eq!(link["is_symlink"], BexExternalValue::Bool(true));
}

#[tokio::test]
async fn fs_read_dir_nonexistent_errors() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> baml.fs.DirEntry[] {{
                baml.fs.read_dir("{root}/no_such_dir")
            }}
        "#
    ));

    assert!(output.result.is_err());
}

#[tokio::test]
async fn fs_read_dir_empty_dir() {
    let (_tmp, root) = tmp(indexmap! {});
    std::fs::create_dir(format!("{root}/empty")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> baml.fs.DirEntry[] {{
                baml.fs.read_dir("{root}/empty")
            }}
        "#
    ));

    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array, got: {:?}", output.result);
    };
    assert_eq!(items.len(), 0);
}

// ============================================================================
// mkdir tests
// ============================================================================

#[tokio::test]
async fn fs_mkdir_creates_directory() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.mkdir("{root}/newdir", baml.fs.MkdirOptions {{ recursive: false }})
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Null));
    assert!(
        std::path::Path::new(&format!("{root}/newdir")).is_dir(),
        "expected {root}/newdir to be a directory"
    );
}

#[tokio::test]
async fn fs_mkdir_recursive_creates_parents() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.mkdir("{root}/a/b/c", baml.fs.MkdirOptions {{ recursive: true }})
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Null));
    assert!(
        std::path::Path::new(&format!("{root}/a/b/c")).is_dir(),
        "expected {root}/a/b/c to be a directory"
    );
}

#[tokio::test]
async fn fs_mkdir_non_recursive_errors_when_parent_missing() {
    let (_tmp, root) = tmp(indexmap! {});

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.mkdir("{root}/no/parent", baml.fs.MkdirOptions {{ recursive: false }})
            }}
        "#
    ));

    assert!(output.result.is_err());
}

#[tokio::test]
async fn fs_mkdir_recursive_is_idempotent() {
    let (_tmp, root) = tmp(indexmap! {});
    std::fs::create_dir(format!("{root}/existing")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.mkdir("{root}/existing", baml.fs.MkdirOptions {{ recursive: true }})
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn fs_mkdir_non_recursive_errors_when_dir_exists() {
    // BEP-037: non-recursive mkdir on a path that already exists must error.
    let (_tmp, root) = tmp(indexmap! {});
    std::fs::create_dir(format!("{root}/existing")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.mkdir("{root}/existing", baml.fs.MkdirOptions {{ recursive: false }})
            }}
        "#
    ));

    assert!(
        output.result.is_err(),
        "expected error creating an already-existing dir, got: {:?}",
        output.result
    );
}

#[tokio::test]
async fn fs_mkdir_recursive_errors_when_leaf_is_file() {
    // BEP-037: even with recursive=true, mkdir must error if the leaf path
    // exists as a regular file. Idempotency only applies when the leaf is
    // already a directory.
    let (_tmp, root) = tmp(indexmap! { "leaf.txt" => "im a file" });

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.mkdir("{root}/leaf.txt", baml.fs.MkdirOptions {{ recursive: true }})
            }}
        "#
    ));

    assert!(
        output.result.is_err(),
        "expected error: leaf path exists as a file, got: {:?}",
        output.result
    );
}

#[tokio::test]
async fn fs_read_dir_on_file_errors() {
    // BEP-037: read_dir on a path that exists but isn't a directory must
    // throw Io. Without this, callers couldn't distinguish "empty dir" from
    // "you pointed at a file".
    let (_tmp, root) = tmp(indexmap! { "not_a_dir.txt" => "x" });

    let output = baml_test!(&format!(
        r#"
            function main() -> baml.fs.DirEntry[] {{
                baml.fs.read_dir("{root}/not_a_dir.txt")
            }}
        "#
    ));

    assert!(
        output.result.is_err(),
        "expected error reading_dir on a regular file, got: {:?}",
        output.result
    );
}

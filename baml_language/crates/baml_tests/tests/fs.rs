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
    // Sysop errors unwind through the VM exception machinery (same path a
    // `throw` opcode takes), so an uncaught one surfaces as `UnhandledThrow`
    // carrying the `baml.errors.*` instance the kind maps to — the write
    // failure is a `VmBamlError::Io`, rendering as `baml.errors.Io`.
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
    let bex_external_types::BexExternalValue::Instance { class_name, fields } = value.as_ref()
    else {
        panic!("expected exception Instance, got: {value:?}");
    };
    assert_eq!(class_name, "baml.errors.Io");
    let Some(BexExternalValue::String(message)) = fields.get("message") else {
        panic!("expected `message` String field, got: {fields:?}");
    };
    assert!(
        message.starts_with("Failed to write:"),
        "unexpected error message: {message}"
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

// ============================================================================
// read_dir tests
// ============================================================================

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

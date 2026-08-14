//! Filesystem operation tests requiring host-side capabilities.
//!
//! Tests here need things BAML cannot express: asserting a compile-time type
//! mismatch via `#[should_panic]`, and inspecting host state that `baml.fs`
//! exposes no reader for — a file's mode bits, and whether a path is a link
//! rather than what it resolves to.
//!
//! The `chmod` and `symlink` tests are Unix-only by design. Windows has no mode
//! bits (only a read-only attribute), and creating a symlink there needs
//! Developer Mode or `SeCreateSymbolicLinkPrivilege`, which CI does not grant.
//! The platform-independent parts of both — argument validation and the errors
//! for a missing or already-occupied path — are covered by `baml_src/ns_fs`.

use baml_tests::baml_test;
#[cfg(unix)]
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

#[tokio::test]
#[should_panic(expected = "mismatched types")]
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

#[cfg(unix)]
#[tokio::test]
async fn fs_remove_on_symlink_to_dir_removes_link() {
    // remove() detects directories via symlink_metadata, which does NOT follow
    // the final component — so a symlink pointing at a directory is removed as a
    // link (it does not trigger the "it is a directory" guidance), and the real
    // directory it targets survives.
    let (_tmp, root) = tmp(indexmap! {});
    std::fs::create_dir(format!("{root}/real_dir")).unwrap();
    std::fs::write(format!("{root}/real_dir/keep.txt"), "x").unwrap();
    std::os::unix::fs::symlink(format!("{root}/real_dir"), format!("{root}/link")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.remove("{root}/link")
            }}
        "#
    ));

    assert!(output.result.is_ok(), "got: {:?}", output.result);
    assert!(!std::path::Path::new(&format!("{root}/link")).exists());
    // The symlink's target and its contents are untouched.
    assert!(std::path::Path::new(&format!("{root}/real_dir/keep.txt")).exists());
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

/// The mode is applied verbatim, not or'd into what the file already had:
/// `0o600` then `0o644` must land on exactly those bits.
#[cfg(unix)]
#[tokio::test]
async fn fs_chmod_sets_the_exact_mode() {
    use std::os::unix::fs::PermissionsExt as _;

    let (_tmp, root) = tmp(indexmap! { "file.txt" => "content" });
    let path = format!("{root}/file.txt");

    let output = baml_test!(&format!(
        r#"
            function main() -> null {{
                baml.fs.chmod("{path}", 0o600);
                baml.fs.chmod("{path}", 0o644);
                null
            }}
        "#
    ));

    assert!(output.result.is_ok(), "got: {:?}", output.result);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o7777, 0o644, "mode was {mode:#o}");
}

/// A mode the kernel would silently mask (here the `S_IFREG` bits of a `stat`
/// result) is rejected before any syscall, so the file keeps its old mode.
#[cfg(unix)]
#[tokio::test]
async fn fs_chmod_rejects_out_of_range_mode() {
    use std::os::unix::fs::PermissionsExt as _;

    let (_tmp, root) = tmp(indexmap! { "file.txt" => "content" });
    let path = format!("{root}/file.txt");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                {{
                    baml.fs.chmod("{path}", 0o100644);
                    "no error thrown"
                }} catch (e) {{
                    baml.errors.InvalidArgument => e.message
                }}
            }}
        "#
    ));

    let Ok(BexExternalValue::String(message)) = &output.result else {
        panic!("expected string, got: {:?}", output.result);
    };
    assert!(
        message.contains("0o100644"),
        "message should name the rejected mode, got: {message}"
    );
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o7777, 0o600, "mode was {mode:#o}");
}

/// The link is a real symlink (not a copy) and resolves to the target's bytes.
#[cfg(unix)]
#[tokio::test]
async fn fs_symlink_creates_a_link_to_the_target() {
    let (_tmp, root) = tmp(indexmap! { "target.txt" => "content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                baml.fs.symlink("{root}/target.txt", "{root}/link.txt");
                baml.fs.read("{root}/link.txt")
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("content".to_string().into()))
    );
    let link = format!("{root}/link.txt");
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        std::path::Path::new(&format!("{root}/target.txt"))
    );
}

/// A relative target is stored verbatim — the OS resolves it against the link's
/// own directory, so the link keeps working regardless of the working directory.
#[cfg(unix)]
#[tokio::test]
async fn fs_symlink_stores_a_relative_target_verbatim() {
    let (_tmp, root) = tmp(indexmap! { "target.txt" => "content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                baml.fs.symlink("target.txt", "{root}/link.txt");
                baml.fs.read("{root}/link.txt")
            }}
        "#
    ));

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("content".to_string().into()))
    );
    assert_eq!(
        std::fs::read_link(format!("{root}/link.txt")).unwrap(),
        std::path::Path::new("target.txt")
    );
}

/// Creating a link never clobbers what is already there.
#[cfg(unix)]
#[tokio::test]
async fn fs_symlink_onto_an_existing_path_errors() {
    let (_tmp, root) = tmp(indexmap! { "target.txt" => "target", "taken.txt" => "keep me" });

    let output = baml_test!(&format!(
        r#"
            function main() -> bool {{
                {{
                    baml.fs.symlink("{root}/target.txt", "{root}/taken.txt");
                    false
                }} catch (e) {{
                    baml.errors.Io => true
                }}
            }}
        "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
    assert_eq!(
        std::fs::read_to_string(format!("{root}/taken.txt")).unwrap(),
        "keep me"
    );
}

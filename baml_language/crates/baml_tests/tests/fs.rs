//! Filesystem operation tests requiring host-side capabilities.
//!
//! Tests here need features BAML doesn't support: creating symlinks and
//! asserting compile-time type mismatches via `#[should_panic]`.

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

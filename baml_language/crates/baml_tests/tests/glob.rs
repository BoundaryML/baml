//! Tests for baml.glob namespace.
//!
//! These tests require host-created symlinks and permission manipulation,
//! which are not available from BAML. The assertion `output.result.is_err()`
//! is a host-level observation with no BAML-side equivalent.

use baml_tests::baml_test;
use indexmap::indexmap;

fn tmp(files: indexmap::IndexMap<&str, &str>) -> (tempfile::TempDir, String) {
    let tmp = tempfile::TempDir::new().unwrap();
    for (name, contents) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }
    let root = tmp.path().display().to_string().replace('\\', "/");
    (tmp, root)
}

#[cfg(unix)]
#[tokio::test]
async fn glob_scan_throws_on_broken_symlink_when_opted_in() {
    let (_tmp, root) = tmp(indexmap! { "real.txt" => "content" });
    std::os::unix::fs::symlink(
        format!("{root}/missing.txt"),
        format!("{root}/dangling.txt"),
    )
    .unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("*.txt");
                g.scan(baml.glob.ScanOptions {{
                    cwd: "{root}",
                    follow_symlinks: true,
                    throw_error_on_broken_symlink: true,
                }})
            }}
        "#
    ));

    assert!(
        output.result.is_err(),
        "expected error for broken symlink with throw_error_on_broken_symlink=true, got: {:?}",
        output.result
    );
}

#[cfg(unix)]
#[tokio::test]
async fn glob_scan_propagates_permission_errors() {
    // A directory the walker can enter the parent of but not read should
    // surface as an Io error, not be silently swallowed. throw_on_broken is
    // scoped to broken symlinks only — real I/O errors propagate by default.
    use std::os::unix::fs::PermissionsExt;

    let (_tmp, root) = tmp(indexmap! { "ok.txt" => "ok" });
    let unreadable = format!("{root}/locked");
    std::fs::create_dir(&unreadable).unwrap();
    std::fs::write(format!("{unreadable}/inner.txt"), "x").unwrap();
    // Mode 0 → no permissions. Root can still read, so probe first and skip
    // under root (CI containers); the assertion isn't meaningful there.
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    let still_readable = std::fs::read_dir(&unreadable).is_ok();
    if still_readable {
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("**/*.txt");
                g.scan("{root}")
            }}
        "#
    ));

    // Restore permissions so the temp dir can be cleaned up.
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        output.result.is_err(),
        "expected permission error to propagate, got: {:?}",
        output.result
    );
}

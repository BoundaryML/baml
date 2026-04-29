//! Tests for baml.glob namespace.

use baml_tests::baml_test;
use bex_external_types::BexExternalValue;
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

fn string_items(value: &BexExternalValue) -> Vec<String> {
    let BexExternalValue::Array { items, .. } = value else {
        panic!("expected array, got: {value:?}");
    };
    let mut strings: Vec<String> = items
        .iter()
        .map(|v| {
            let BexExternalValue::String(s) = v else {
                panic!("expected string, got: {v:?}");
            };
            s.clone()
        })
        .collect();
    strings.sort();
    strings
}

// ============================================================================
// baml.glob.new + Glob.matches
// ============================================================================

#[tokio::test]
async fn glob_matches_basic() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let g = baml.glob.new("*.txt");
                g.matches("hello.txt")
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn glob_matches_no_match() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let g = baml.glob.new("*.txt");
                g.matches("hello.rs")
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn glob_matches_recursive_wildcard() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let g = baml.glob.new("**/*.ts");
                g.matches("src/index.ts")
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn glob_matches_question_mark() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let g = baml.glob.new("file?.txt");
                g.matches("fileA.txt")
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn glob_matches_question_mark_no_match() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let g = baml.glob.new("file?.txt");
                g.matches("file.txt")
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn glob_matches_bracket_character_class() {
    // [abc] matches a single character from the set. Exercised through the
    // BAML surface to catch any wiring regression between the parser's
    // string-literal handling, codegen for Glob.matches, and GlobPattern.
    let output = baml_test!(
        r#"
            function main() -> bool[] {
                let g = baml.glob.new("[abc].txt");
                [g.matches("a.txt"), g.matches("b.txt"), g.matches("c.txt"), g.matches("d.txt")]
            }
        "#
    );
    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array, got: {:?}", output.result);
    };
    let bools: Vec<bool> = items
        .iter()
        .map(|v| match v {
            BexExternalValue::Bool(b) => *b,
            _ => panic!("expected bool"),
        })
        .collect();
    assert_eq!(bools, vec![true, true, true, false]);
}

#[tokio::test]
async fn glob_matches_bracket_range() {
    let output = baml_test!(
        r#"
            function main() -> bool[] {
                let g = baml.glob.new("[a-c].txt");
                [g.matches("a.txt"), g.matches("c.txt"), g.matches("d.txt")]
            }
        "#
    );
    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array");
    };
    let bools: Vec<bool> = items
        .iter()
        .map(|v| match v {
            BexExternalValue::Bool(b) => *b,
            _ => panic!(),
        })
        .collect();
    assert_eq!(bools, vec![true, true, false]);
}

#[tokio::test]
async fn glob_matches_negated_bracket() {
    // [^abc] matches a single character NOT in the set.
    let output = baml_test!(
        r#"
            function main() -> bool[] {
                let g = baml.glob.new("[^abc].txt");
                [g.matches("a.txt"), g.matches("d.txt")]
            }
        "#
    );
    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array");
    };
    let bools: Vec<bool> = items
        .iter()
        .map(|v| match v {
            BexExternalValue::Bool(b) => *b,
            _ => panic!(),
        })
        .collect();
    assert_eq!(bools, vec![false, true]);
}

#[tokio::test]
async fn glob_matches_alternation() {
    let output = baml_test!(
        r#"
            function main() -> bool[] {
                let g = baml.glob.new("*.{ts,tsx}");
                [g.matches("app.ts"), g.matches("app.tsx"), g.matches("app.js")]
            }
        "#
    );
    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array");
    };
    let bools: Vec<bool> = items
        .iter()
        .map(|v| match v {
            BexExternalValue::Bool(b) => *b,
            _ => panic!(),
        })
        .collect();
    assert_eq!(bools, vec![true, true, false]);
}

#[tokio::test]
async fn glob_matches_negated_pattern() {
    // `!index.ts` — match everything except index.ts.
    let output = baml_test!(
        r#"
            function main() -> bool[] {
                let g = baml.glob.new("!index.ts");
                [g.matches("main.ts"), g.matches("index.ts")]
            }
        "#
    );
    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array");
    };
    let bools: Vec<bool> = items
        .iter()
        .map(|v| match v {
            BexExternalValue::Bool(b) => *b,
            _ => panic!(),
        })
        .collect();
    assert_eq!(bools, vec![true, false]);
}

// ============================================================================
// baml.glob.new + Glob.scan
// ============================================================================

#[tokio::test]
async fn glob_scan_finds_txt_files() {
    let (_tmp, root) = tmp(indexmap! {
        "a.txt" => "a",
        "b.txt" => "b",
        "c.rs" => "c",
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("*.txt");
                g.scan("{root}")
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec!["a.txt", "b.txt"]
    );
}

#[tokio::test]
async fn glob_scan_returns_empty_for_no_match() {
    let (_tmp, root) = tmp(indexmap! {
        "a.rs" => "rust",
        "b.rs" => "rust",
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("*.txt");
                g.scan("{root}")
            }}
        "#
    ));

    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array, got: {:?}", output.result);
    };
    assert_eq!(items.len(), 0);
}

#[tokio::test]
async fn glob_scan_only_files_by_default() {
    // With only_files=true (default), scan should return only files, not dirs.
    // We create a directory that also matches *.txt pattern name-wise,
    // but it should be excluded.
    let (_tmp, root) = tmp(indexmap! {
        "file.txt" => "content",
    });
    // Create a directory named "dir.txt" to ensure it's excluded
    std::fs::create_dir(format!("{root}/dir.txt")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("*.txt");
                g.scan("{root}")
            }}
        "#
    ));

    let Ok(BexExternalValue::Array { items, .. }) = &output.result else {
        panic!("expected array, got: {:?}", output.result);
    };
    // By default only_files=true, so "dir.txt/" directory should not appear
    for item in items {
        let BexExternalValue::String(s) = item else {
            panic!("expected string")
        };
        // only files should be returned
        assert_eq!(s.as_str(), "file.txt", "unexpected entry: {s}");
    }
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn glob_scan_options_include_dot_absolute_and_directories() {
    let (_tmp, root) = tmp(indexmap! {
        "file.txt" => "content",
        ".hidden.txt" => "hidden",
    });
    std::fs::create_dir(format!("{root}/dir.txt")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("**/*.txt");
                g.scan(baml.glob.ScanOptions {{ cwd: "{root}", dot: true, absolute: true, only_files: false }})
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec![
            format!("{root}/.hidden.txt"),
            format!("{root}/dir.txt"),
            format!("{root}/file.txt"),
        ]
    );
}

#[tokio::test]
async fn glob_scan_matches_dot_slash_pattern() {
    let (_tmp, root) = tmp(indexmap! { "file.txt" => "content" });

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("./*.txt");
                g.scan("{root}")
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec!["file.txt"]
    );
}

#[tokio::test]
async fn glob_scan_matches_absolute_pattern() {
    let (_tmp, root) = tmp(indexmap! { "file.txt" => "content" });
    let pattern = format!("{root}/**/*.txt");

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("{pattern}");
                g.scan(baml.glob.ScanOptions {{ cwd: "{root}", absolute: true }})
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec![format!("{root}/file.txt")]
    );
}

#[tokio::test]
async fn glob_scan_dot_pattern_matches_only_dot_files() {
    // Regression for the OR-against-three-forms strategy: a plain pattern
    // like `.*` would match every file because the `./<name>` form always
    // starts with `.`. The fix makes a plain pattern match only the relative
    // form, so `.*` matches `.hidden` but not `regular.txt`.
    let (_tmp, root) = tmp(indexmap! {
        ".hidden" => "h",
        "regular.txt" => "r",
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new(".*");
                g.scan(baml.glob.ScanOptions {{ cwd: "{root}", dot: true }})
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec![".hidden"]
    );
}

#[tokio::test]
async fn glob_scan_default_prunes_dot_directories() {
    // dot=false (default) must skip files inside dot directories. Beyond
    // correctness, this also locks in the walker-side pruning so we don't
    // descend into trees like `.git/`.
    let (_tmp, root) = tmp(indexmap! {
        "visible.txt" => "visible",
    });
    std::fs::create_dir_all(format!("{root}/.git/objects")).unwrap();
    std::fs::write(format!("{root}/.git/objects/secret.txt"), "x").unwrap();
    std::fs::write(format!("{root}/.git/HEAD"), "x").unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("**/*.txt");
                g.scan("{root}")
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec!["visible.txt"]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn glob_scan_follows_symlinked_directory_when_enabled() {
    // follow_symlinks=true: a symlinked directory under the scan root should
    // be traversed and its contents matched, just like a real directory.
    let outer = tempfile::TempDir::new().unwrap();
    let target = tempfile::TempDir::new().unwrap();
    std::fs::write(target.path().join("hidden_via_link.txt"), "x").unwrap();
    let outer_root = outer.path().display().to_string().replace('\\', "/");
    std::os::unix::fs::symlink(target.path(), outer.path().join("link_dir")).unwrap();

    let output = baml_test!(&format!(
        r#"
            function main() -> string[] {{
                let g = baml.glob.new("**/*.txt");
                g.scan(baml.glob.ScanOptions {{ cwd: "{outer_root}", follow_symlinks: true }})
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec!["link_dir/hidden_via_link.txt"]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn glob_scan_skips_broken_symlinks_by_default() {
    // follow_symlinks=true makes walkdir try to stat the symlink target. With a
    // broken link present, the default (throw_error_on_broken_symlink=false)
    // must silently skip it and still return the real files.
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
                g.scan(baml.glob.ScanOptions {{ cwd: "{root}", follow_symlinks: true }})
            }}
        "#
    ));

    assert_eq!(
        string_items(output.result.as_ref().unwrap()),
        vec!["real.txt"]
    );
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

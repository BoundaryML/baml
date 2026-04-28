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

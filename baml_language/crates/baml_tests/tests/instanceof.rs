//! Tests that the `instanceof` operator produces a compile error
//! suggesting `match` instead.

use std::path::Path;

use baml_compiler_diagnostics::Severity;
use baml_project::ProjectDatabase;

fn get_errors(source: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.add_file("test.baml", source);

    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let diagnostics = baml_project::collect_diagnostics(&db, project, &all_files);
    diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn instanceof_produces_error() {
    let errors = get_errors(
        "
        class StopTool {
            action string
        }

        function main() -> bool {
            let t = StopTool { action: \"stop\" };
            t instanceof StopTool
        }
    ",
    );

    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("instanceof"));
    assert!(errors[0].contains("match"));
}

#[test]
fn instanceof_in_if_produces_error() {
    let errors = get_errors(
        "
        class Foo {
            field string
        }

        class Bar {
            other int
        }

        function main() -> string {
            let x = Foo { field: \"test\" };
            if (x instanceof Foo) {
                return x.field;
            } else {
                return \"not foo\";
            }
        }
    ",
    );

    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("instanceof")));
}

//! Integration tests for stream annotation normalization.
//!
//! These tests verify that the PPIR normalization pass correctly computes
//! D (during-streaming type), S (starts-as value), and typeof(S) for every
//! field in every class.

use std::fmt::Write;

use baml_compiler_hir::type_ref_to_str as format_type_ref;
use baml_project::ProjectDatabase;
use insta::{assert_snapshot, with_settings};

const SNAPSHOT_PATH: &str = "snapshots/stream_normalize";

/// Format the normalized stream annotations for all classes in a project.
fn format_normalized_stream(db: &ProjectDatabase) -> String {
    let project = db.project().expect("no project loaded");
    let items = baml_compiler_hir::project_items(db, project);
    let mut output = String::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Class(class_loc) = item {
            let file = class_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let class = &item_tree[class_loc.id(db)];

            // Skip generated stream_* classes
            if class.name.starts_with("stream_") {
                continue;
            }

            // Only show classes that have normalized stream data
            let has_stream = class.fields.iter().any(|f| f.stream.is_some());
            if !has_stream {
                continue;
            }

            writeln!(output, "class {} {{", class.name).unwrap();
            for field in &class.fields {
                if let Some(stream) = &field.stream {
                    let typeof_s_str = match &stream.typeof_s {
                        Some(t) => format_type_ref(t),
                        None => "deferred".to_string(),
                    };
                    write!(
                        output,
                        "  {}: {}  D={} S={} typeof_s={}",
                        field.name,
                        format_type_ref(&field.type_ref),
                        format_type_ref(&stream.stream_type),
                        stream.starts_as,
                        typeof_s_str,
                    )
                    .unwrap();
                    if stream.in_progress_never {
                        write!(output, " in_progress_never").unwrap();
                    }
                    writeln!(output).unwrap();
                } else {
                    writeln!(
                        output,
                        "  {}: {}  (no stream data)",
                        field.name,
                        format_type_ref(&field.type_ref)
                    )
                    .unwrap();
                }
            }
            writeln!(output, "}}").unwrap();
            writeln!(output).unwrap();
        }
    }

    output
}

#[test]
fn stream_annotation_defaults() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    let content = include_str!("../projects/stream_annotation_defaults/main.baml");
    let content = content.replace("\r\n", "\n");
    db.add_file("main.baml", &content);

    let output = format_normalized_stream(&db);

    with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
        assert_snapshot!("defaults", output);
    });
}

#[test]
fn stream_annotation_explicit() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    let content = include_str!("../projects/stream_annotation_explicit/main.baml");
    let content = content.replace("\r\n", "\n");
    db.add_file("main.baml", &content);

    let output = format_normalized_stream(&db);

    with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
        assert_snapshot!("explicit", output);
    });
}

#[test]
fn stream_annotation_legacy_sugar() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    let content = include_str!("../projects/stream_annotation_legacy_sugar/main.baml");
    let content = content.replace("\r\n", "\n");
    db.add_file("main.baml", &content);

    let output = format_normalized_stream(&db);

    with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
        assert_snapshot!("legacy_sugar", output);
    });
}

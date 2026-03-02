//! Integration tests for Phase 3: stream type generation.
//!
//! These tests verify that generated `stream_*` classes have:
//! - Correctly simplified field types (`simplify(typeof(S) | D)`)
//! - Correct SAP attributes (field_attr, ty_attr)
//! - Fields omitted when simplified type is never

use std::fmt::Write;

use baml_base::sap::SapAttrValue;
use baml_compiler_hir::type_ref_to_str as format_type_ref;
use baml_project::ProjectDatabase;
use insta::{assert_snapshot, with_settings};

const SNAPSHOT_PATH: &str = "snapshots/stream_generate";

/// Format generated stream_* classes and their SAP attributes for snapshot testing.
fn format_stream_classes(db: &ProjectDatabase) -> String {
    let project = db.project().expect("no project loaded");
    let items = baml_compiler_hir::project_items(db, project);
    let mut output = String::new();

    for item in items.items(db) {
        match item {
            baml_compiler_hir::ItemId::Class(class_loc) => {
                let file = class_loc.file(db);
                let item_tree = baml_compiler_hir::file_item_tree(db, file);
                let class = &item_tree[class_loc.id(db)];

                // Only show generated stream_* classes
                if !class.name.starts_with("stream_") {
                    continue;
                }

                writeln!(output, "class {} {{", class.name).unwrap();
                for field in &class.fields {
                    // Field type
                    write!(output, "  {}: {}", field.name, format_type_ref(&field.type_ref))
                        .unwrap();

                    // SAP annotations
                    let mut attrs = Vec::new();

                    // ty_attr: sap_in_progress
                    if !field.ty_attr.is_default() {
                        match field.ty_attr.sap_in_progress() {
                            SapAttrValue::Never => attrs.push("@sap.in_progress(never)".into()),
                            SapAttrValue::ConstValueExpr(v) => {
                                attrs.push(format!("@sap.in_progress({v:?})"))
                            }
                            SapAttrValue::DefaultForType => {}
                        }
                    }

                    // field_attr: sap_missing
                    if !field.field_attr.is_default() {
                        match field.field_attr.sap_missing() {
                            SapAttrValue::Never => attrs.push("@sap.missing(never)".into()),
                            SapAttrValue::ConstValueExpr(v) => {
                                attrs.push(format!("@sap.missing({v:?})"))
                            }
                            SapAttrValue::DefaultForType => {}
                        }
                    }

                    if !attrs.is_empty() {
                        write!(output, "  {}", attrs.join(" ")).unwrap();
                    }
                    writeln!(output).unwrap();
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();
            }
            baml_compiler_hir::ItemId::TypeAlias(alias_loc) => {
                let file = alias_loc.file(db);
                let item_tree = baml_compiler_hir::file_item_tree(db, file);
                let alias = &item_tree[alias_loc.id(db)];

                // Only show generated stream_* aliases
                if !alias.name.starts_with("stream_") {
                    continue;
                }

                writeln!(
                    output,
                    "type {} = {}",
                    alias.name,
                    format_type_ref(&alias.type_ref)
                )
                .unwrap();
                writeln!(output).unwrap();
            }
            _ => {}
        }
    }

    output
}

#[test]
fn stream_type_generation_basic() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    let content = include_str!("../projects/stream_type_generation_basic/main.baml");
    let content = content.replace("\r\n", "\n");
    db.add_file("main.baml", &content);

    let output = format_stream_classes(&db);

    with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
        assert_snapshot!("basic", output);
    });
}

#[test]
fn stream_type_generation_annotations() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    let content = include_str!("../projects/stream_type_generation_annotations/main.baml");
    let content = content.replace("\r\n", "\n");
    db.add_file("main.baml", &content);

    let output = format_stream_classes(&db);

    with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
        assert_snapshot!("annotations", output);
    });
}

#[test]
fn stream_type_generation_complex() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    let content = include_str!("../projects/stream_type_generation_complex/main.baml");
    let content = content.replace("\r\n", "\n");
    db.add_file("main.baml", &content);

    let output = format_stream_classes(&db);

    with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
        assert_snapshot!("complex", output);
    });
}

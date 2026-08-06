use std::path::Path;

use baml_lsp2_actions::{RenameError, definition_at, prepare_rename, rename, type_at, usages_at};
use baml_project::ProjectDatabase;
use text_size::TextSize;

#[test]
fn navigation_and_rename_spans_are_compiler_owned() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    let source =
        "/// Person docs\nclass Person { name string }\nfunction Use(p: Person) -> Person { p }\n";
    let file = db.add_or_update_file(Path::new("rename_spans.baml"), source);
    let offset = TextSize::from(u32::try_from(source.find("Person {").unwrap()).unwrap());

    let prepared = prepare_rename(&db, file, offset).unwrap();
    assert_eq!(prepared.name, "Person");
    let definition = definition_at(&db, file, offset).unwrap();
    let references = usages_at(&db, file, offset);
    let edits = rename(&db, file, offset, "Human").unwrap();
    let hover = type_at(&db, file, offset).unwrap().to_hover_markdown();

    let render = |locations: &[baml_lsp2_actions::Location]| {
        locations
            .iter()
            .map(|location| {
                let range = std::ops::Range::<usize>::from(location.range);
                let start = range.start;
                let end = range.end;
                format!("{start}..{end} {:?}", &source[range])
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    insta::assert_snapshot!(format!(
        "prepare: {}..{}\ndefinition: {}..{}\nreferences:\n{}\nrename edits:\n{}\nhover:\n{}",
        u32::from(prepared.definition.range.start()),
        u32::from(prepared.definition.range.end()),
        u32::from(definition.range.start()),
        u32::from(definition.range.end()),
        render(&references),
        render(&edits),
        hover,
    ));
}

#[test]
fn rename_rejects_invalid_names_and_project_wide_collisions() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    let source = "class Person {}\nclass Human {}\n";
    let file = db.add_or_update_file(Path::new("rename_collision.baml"), source);
    let offset = TextSize::from(u32::try_from(source.find("Person").unwrap()).unwrap());
    assert!(matches!(
        rename(&db, file, offset, "not-valid"),
        Err(RenameError::InvalidIdentifier(_))
    ));
    assert!(matches!(
        rename(&db, file, offset, "Human"),
        Err(RenameError::Collision(_))
    ));
}

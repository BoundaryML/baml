use internal_baml_parser_database::ParserDatabase;
use internal_baml_schema_ast::ast::{
    self, Attribute, Span, WithAttributes, WithIdentifier, WithName, WithSpan,
};
use serde_json::{json, Value};

use super::repr::WithRepr;

// should have a serde struct with a special serialize/deserialize

/*

requirements

- need to store enough AST information to reconstruct all the generated code
  - use cases:
    - local- dump out enough AST information that we don't have to write to disk again (of i/o, we're already paying the i, just don't pay the o)
    - cloud- dump out enough AST information to be able to analyze the types
      - efficient updates? not important

only thing i need to care about right now is the local part


 */

fn debug1(span: &Span) -> String {
    return format!("{}:[{},{})", span.file.path(), span.start, span.end);
}

fn debug2(attributes: &[Attribute]) -> String {
    return attributes
        .iter()
        .map(|attr| format!("{}:{}", attr.name.name(), attr.arguments.arguments.len()))
        .collect::<Vec<String>>()
        .join(",");
}

pub fn generate(db: &ParserDatabase) -> std::io::Result<()> {
    let mut lockfile_contents: Vec<String> = Vec::new();

    for e in db.walk_enums() {
        lockfile_contents.push(format!(
            "enum:{} {} {} {}",
            e.name(),
            e.identifier().name(),
            debug1(e.identifier().span()),
            serde_json::to_string_pretty(&e.repr())?
        ));
    }
    for e in db.walk_classes() {
        lockfile_contents.push(format!(
            "class:{} {}",
            e.name(),
            serde_json::to_string_pretty(&e.repr())?
        ));
    }
    for e in db.walk_functions() {
        lockfile_contents.push(format!(
            "function2:{} {}",
            e.name(),
            serde_json::to_string_pretty(&e.repr())?
        ));
    }

    std::fs::write(
        "/home/sam/baml-ast.lock",
        lockfile_contents
            .iter()
            .map(|s| format!("{}\n", s))
            .collect::<Vec<String>>()
            .join(""),
    )?;
    Ok(())
}

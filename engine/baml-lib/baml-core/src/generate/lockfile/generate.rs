use internal_baml_parser_database::ParserDatabase;
use internal_baml_schema_ast::ast::{
    self, Attribute, Span, WithAttributes, WithIdentifier, WithName, WithSpan,
};
use serde_json::{json, Value};

use super::repr::{AllElements, WithRepr};

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

pub fn generate(db: &ParserDatabase) -> std::io::Result<()> {
    let all_elements = AllElements {
        enums: db.walk_enums().map(|e| e.repr()).collect(),
        classes: db.walk_classes().map(|e| e.repr()).collect(),
        functions: db.walk_functions().map(|e| e.repr()).collect(),
    };

    std::fs::write(
        "/home/sam/baml-ast.lock",
        serde_json::to_string_pretty(&all_elements)? + "\n",
    )?;
    Ok(())
}

use internal_baml_parser_database::ParserDatabase;

use super::repr::{AllElements, RetryPolicy, WithRepr};

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

pub fn generate_lockfile(db: &ParserDatabase) -> std::io::Result<()> {
    let all_elements = AllElements {
        enums: db.walk_enums().map(|e| e.node(db)).collect(),
        classes: db.walk_classes().map(|e| e.node(db)).collect(),
        functions: db.walk_functions().map(|e| e.node(db)).collect(),
        clients: db.walk_clients().map(|e| e.node(db)).collect(),
        retry_policies: db
            .walk_retry_policies()
            .map(|e| WithRepr::<RetryPolicy>::node(&e, db))
            .collect(),
    };

    std::fs::write(
        "/home/sam/baml-ast.lock",
        serde_json::to_string_pretty(&all_elements)? + "\n",
    )?;
    Ok(())
}

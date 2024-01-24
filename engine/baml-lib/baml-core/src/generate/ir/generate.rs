use anyhow::Result;

use internal_baml_parser_database::ParserDatabase;

use super::repr::IntermediateRepr;

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

pub fn to_ir(db: &ParserDatabase) -> Result<IntermediateRepr> {
    IntermediateRepr::from_parser_database(db)
}

use anyhow::Result;
use baml_types::FieldType;

use crate::{schema::Type, BamlValueWithFlags};

fn coerce_schema(target: &FieldType, schema: Type) -> Result<BamlValueWithFlags> {
    // check if the target matches the schema, and if so, return the schema.

    todo!()
}

//! Native `to_json` implementations for BAML primitive companion classes:
//! `Int`, `Float`, `Bool`, `Null`.
//!
//! Each `to_json` is a pure pass-through: the BAML runtime already stores
//! these as the exact `Value` variants that the `json` union accepts
//! (`Value::Int`, `Value::Float`, `Value::Bool`, `Value::Null`), so no
//! conversion is needed.

use bex_vm_types::types::Value;

use super::{BamlClassBool, BamlClassNull, PackageBamlImpl};

impl BamlClassBool for PackageBamlImpl {
    fn to_json(bool: &Value) -> Value {
        *bool
    }
}

impl BamlClassNull for PackageBamlImpl {
    fn to_json(null: &Value) -> Value {
        *null
    }
}

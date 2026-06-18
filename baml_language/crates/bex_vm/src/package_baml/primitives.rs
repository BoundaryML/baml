//! Native `to_json` implementations for BAML primitive companion classes:
//! `Int`, `Float`, `Bool`, `Null`.
//!
//! Each `to_json` is a pure pass-through: the BAML runtime already stores
//! these as the exact `Value` kinds the `json` union accepts (tagged Int /
//! Bool / `NULL`, plus heap-boxed `Object::Float`), so no conversion is
//! needed.

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

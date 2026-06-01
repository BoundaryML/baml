//! Native `to_json` implementations for BAML primitive companion classes:
//! `Int`, `Float`, `Bool`, `Null`.
//!
//! Each `to_json` is a pure pass-through: the BAML runtime already stores
//! these as the exact `Value` kinds the `json` union accepts (tagged Int /
//! Bool / `NULL`, plus heap-boxed `Object::Float`), so no conversion is
//! needed.

use bex_str::BexStr;
use bex_vm_types::types::Value;

use super::{BamlClassBool, BamlClassNull, PackageBamlImpl};

impl BamlClassBool for PackageBamlImpl {
    fn to_json(bool: &Value) -> Value {
        *bool
    }

    fn to_string(bool: &Value) -> BexStr {
        // BAML `Bool` is represented as a tagged `Value`. Either it's the
        // canonical TRUE / FALSE bit-pattern, in which case `as_bool()`
        // returns the underlying Rust bool, or it's malformed and we
        // surface that as `"<invalid bool>"`. The latter shouldn't happen
        // in practice — the VM only dispatches `Bool::to_string` on
        // boolean values — but the impl is defensive either way.
        match bool.as_bool() {
            Some(true) => BexStr::from("true"),
            Some(false) => BexStr::from("false"),
            None => BexStr::from("<invalid bool>"),
        }
    }
}

impl BamlClassNull for PackageBamlImpl {
    fn to_json(null: &Value) -> Value {
        *null
    }
}

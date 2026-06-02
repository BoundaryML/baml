//! Native handler for `baml.toml`: `Table.parse`.
//!
//! Only string → TOML parsing is native (via the `toml` crate); the
//! TOML ↔ JSON conversions live in BAML (`ns_toml/toml.baml`) on top of
//! the stdlib `to_json` / `from_json` machinery.

use std::str::FromStr;

use bex_heap::TlabHolder;
use bex_vm_types::Value;
use indexmap::IndexMap;

use crate::{BexVm, errors::VmRustFnError, package_baml::PackageBamlImpl};

impl super::BamlNamespaceToml for PackageBamlImpl {}

impl super::BamlClassTomlTable for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        match ::toml::Table::from_str(s.as_str()) {
            Ok(table) => convert_toml_value(vm, ::toml::Value::Table(table)),
            Err(e) => Err(VmRustFnError::Thrown(make_toml_parse_error(
                vm,
                e.to_string(),
            ))),
        }
    }
}

/// Convert a parsed `toml::Value` into a VM `Value`.
///
/// Scalars map onto VM primitives; arrays and tables recurse, with tables
/// wrapped in a `baml.toml.Table` instance and datetimes in a
/// `baml.toml.Datetime` instance (string-backed; see the BEP-021 `Plain*` /
/// `ZonedDateTime` types for where this is headed).
fn convert_toml_value(vm: &mut BexVm, value: ::toml::Value) -> Result<Value, VmRustFnError> {
    match value {
        toml::Value::String(s) => Ok(Value::object(vm.alloc_string(s))),
        toml::Value::Integer(i) => Value::try_int(i).ok_or_else(|| {
            VmRustFnError::Thrown(make_toml_parse_error(
                vm,
                "BAML `int` is 63 bits. It is unable to parse 64-bit TOML integers.".to_string(),
            ))
        }),
        toml::Value::Float(f) => Ok(Value::object(vm.alloc_float(f))),
        toml::Value::Boolean(b) => Ok(Value::bool(b)),
        toml::Value::Datetime(datetime) => {
            let datetime = Value::object(vm.alloc_string(datetime.to_string()));
            let class = vm.resolve_class("baml.toml.Datetime");
            Ok(Value::object(vm.alloc_instance(class, vec![datetime])))
        }
        toml::Value::Array(values) => {
            let array = values
                .into_iter()
                .map(|v| convert_toml_value(vm, v))
                .collect::<Result<Vec<Value>, VmRustFnError>>()?;
            Ok(Value::object(vm.alloc_array(array)))
        }
        toml::Value::Table(map) => {
            let map = map
                .into_iter()
                .map(|(k, v)| {
                    let v = convert_toml_value(vm, v)?;
                    Ok((bex_str::BexStr::from(k), v))
                })
                .collect::<Result<IndexMap<bex_str::BexStr, Value>, VmRustFnError>>()?;
            let map = Value::object(vm.alloc_map(map));
            let class = vm.resolve_class("baml.toml.Table");
            Ok(Value::object(vm.alloc_instance(class, vec![map])))
        }
    }
}

fn make_toml_parse_error(vm: &mut BexVm, message: String) -> Value {
    let err_msg = Value::object(vm.alloc_string(message));
    let class = vm.resolve_class("baml.toml.TomlParseError");
    Value::object(vm.alloc_instance(class, vec![err_msg]))
}

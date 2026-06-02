use bex_vm_types::Value;
use std::str::FromStr;

use crate::{BexVm, errors::VmRustFnError, package_baml::PackageBamlImpl};

impl super::BamlNamespaceToml for PackageBamlImpl {}

impl super::BamlClassTomlTable for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &str) -> Result<Value, VmRustFnError> {
        match ::toml::Table::from_str(s) {
            Ok(table) => convert_toml_value(vm, ::toml::Value::Table(table)),
            Err(e) => Err(VmRustFnError::Thrown(make_toml_parse_error(
                vm,
                e.to_string(),
            ))),
        }
    }
}

fn convert_toml_value(vm: &mut BexVm, value: ::toml::Value) -> Result<Value, VmRustFnError> {
    match value {
        toml::Value::String(s) => Ok(vm.alloc_string(s)),
        toml::Value::Integer(i) => Value::try_int(i).ok_or_else(|| {
            VmRustFnError::Thrown(make_toml_parse_error(
                vm,
                "BAML `int` is 63 bits. It is unable to parse 64-bit TOML integers.".to_string(),
            ))
        }),
        toml::Value::Float(f) => Ok(vm.alloc_float(f)),
        toml::Value::Boolean(b) => Ok(Value::bool(b)),
        toml::Value::Datetime(datetime) => {
            let datetime = vm.alloc_string(datetime.to_string());
            Ok(vm.alloc_instance(vm.resolve_class("baml.toml.Datetime"), vec![datetime]))
        }
        toml::Value::Array(values) => {
            let array = values
                .into_iter()
                .map(|v| convert_toml_value(vm, v))
                .collect::<Result<_, VmRustFnError>>()?;
            Ok(vm.alloc_array(array))
        }
        toml::Value::Table(map) => {
            let map = map
                .into_iter()
                .map(|(k, v)| {
                    let v = convert_toml_value(vm, v)?;
                    Ok((k, v))
                })
                .collect::<Result<_, VmRustFnError>>()?;
            let map = vm.alloc_map(map);
            Ok(vm.alloc_instance(vm.resolve_class("baml.toml.Table"), vec![map]))
        }
    }
}

fn make_toml_parse_error(vm: &mut BexVm, message: String) -> Value {
    let err_msg = vm.alloc_string(message);
    vm.alloc_instance(vm.resolve_class("baml.toml.TomlParseError"), vec![err_msg])
}

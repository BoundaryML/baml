//! Native handler for `baml.yaml.parse`.
//!
//! YAML is parsed into the existing `baml.json.json` value algebra. Values that
//! cannot be represented there are rejected rather than converted lossily.

use bex_heap::TlabHolder;
use bex_vm_types::Value;
use indexmap::IndexMap;
use serde::Deserialize;

use crate::{BexVm, errors::VmRustFnError, package_baml::PackageBamlImpl};

impl super::BamlNamespaceYaml for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        parse_yaml(vm, s.as_str())
    }
}

fn parse_yaml(vm: &mut BexVm, s: &str) -> Result<Value, VmRustFnError> {
    let mut docs = serde_yaml::Deserializer::from_str(s);
    let Some(first_doc) = docs.next() else {
        return Ok(Value::NULL);
    };

    let parsed = serde_yaml::Value::deserialize(first_doc)
        .map_err(|e| VmRustFnError::thrown_fresh(make_yaml_parse_error(vm, e.to_string())))?;

    if docs.next().is_some() {
        return Err(VmRustFnError::thrown_fresh(make_yaml_parse_error(
            vm,
            "YAML stream contains multiple documents; baml.yaml.parse accepts exactly one document"
                .to_string(),
        )));
    }

    convert_yaml_value(vm, parsed)
}

fn convert_yaml_value(vm: &mut BexVm, value: serde_yaml::Value) -> Result<Value, VmRustFnError> {
    match value {
        serde_yaml::Value::Null => Ok(Value::NULL),
        serde_yaml::Value::Bool(b) => Ok(Value::bool(b)),
        serde_yaml::Value::Number(n) => convert_yaml_number(vm, &n),
        serde_yaml::Value::String(s) => Ok(Value::object(vm.alloc_string(s))),
        serde_yaml::Value::Sequence(values) => {
            let values = values
                .into_iter()
                .map(|v| convert_yaml_value(vm, v))
                .collect::<Result<Vec<Value>, VmRustFnError>>()?;
            // YAML is parsed into the `baml.json.json` value algebra.
            Ok(Value::object(
                vm.alloc_array(super::json::json_alias_ty(vm), values),
            ))
        }
        serde_yaml::Value::Mapping(map) => {
            let mut entries = IndexMap::with_capacity(map.len());
            for (key, value) in map {
                let serde_yaml::Value::String(key) = key else {
                    return Err(VmRustFnError::thrown_fresh(make_yaml_parse_error(
                        vm,
                        "YAML mappings must use string keys to fit baml.json.json".to_string(),
                    )));
                };
                let value = convert_yaml_value(vm, value)?;
                entries.insert(bex_str::BexStr::from(key), value);
            }
            // `baml.json.json` maps: string keys, `json` values.
            Ok(Value::object(vm.alloc_map(
                bex_vm_types::RealizedTy::string(),
                super::json::json_alias_ty(vm),
                entries,
            )))
        }
        serde_yaml::Value::Tagged(_) => Err(VmRustFnError::thrown_fresh(make_yaml_parse_error(
            vm,
            "YAML tags are not supported by baml.yaml.parse".to_string(),
        ))),
    }
}

fn convert_yaml_number(vm: &mut BexVm, n: &serde_yaml::Number) -> Result<Value, VmRustFnError> {
    if let Some(i) = n.as_i64() {
        return Value::try_int(i).ok_or_else(|| {
            VmRustFnError::thrown_fresh(make_yaml_parse_error(
                vm,
                "YAML integer is outside BAML int range".to_string(),
            ))
        });
    }

    if n.is_u64() {
        return Err(VmRustFnError::thrown_fresh(make_yaml_parse_error(
            vm,
            "YAML integer is outside BAML int range".to_string(),
        )));
    }

    let Some(f) = n.as_f64() else {
        return Err(VmRustFnError::thrown_fresh(make_yaml_parse_error(
            vm,
            "YAML number cannot be represented as a BAML int or float".to_string(),
        )));
    };

    if !f.is_finite() {
        return Err(VmRustFnError::thrown_fresh(make_yaml_parse_error(
            vm,
            "YAML non-finite floats are not supported by baml.yaml.parse".to_string(),
        )));
    }

    Ok(Value::object(vm.alloc_float(f)))
}

fn make_yaml_parse_error(vm: &mut BexVm, message: String) -> Value {
    let err_msg = Value::object(vm.alloc_string(message));
    let class = vm.resolve_class("baml.yaml.ParseError");
    Value::object(vm.alloc_instance(class, vec![err_msg]))
}

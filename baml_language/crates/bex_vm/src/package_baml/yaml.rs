//! Native handler for `baml.yaml.parse`.
//!
//! YAML is parsed into the existing `baml.json.json` value algebra. Values that
//! cannot be represented there are rejected rather than converted lossily.

use bex_heap::TlabHolder;
use bex_vm_types::Value;
use indexmap::IndexMap;
use yaml_rust2::{
    Yaml, YamlLoader,
    parser::{Event, Parser},
};

use crate::{BexVm, errors::VmRustFnError, package_baml::PackageBamlImpl};

impl super::BamlNamespaceYaml for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        parse_yaml(vm, s.as_str())
    }
}

fn parse_error(vm: &mut BexVm, message: impl Into<String>) -> VmRustFnError {
    VmRustFnError::thrown_fresh(make_yaml_parse_error(vm, message.into()))
}

/// Whether the document uses a tag outside the core schema (`!!str`, ...).
/// The loader drops those silently; they have no `baml.json.json` meaning.
fn has_custom_tag(s: &str) -> bool {
    let mut parser = Parser::new_from_str(s);
    loop {
        match parser.next_token() {
            // Malformed input is reported by the loader.
            Ok((Event::StreamEnd, _)) | Err(_) => return false,
            Ok((
                Event::Scalar(.., Some(tag))
                | Event::SequenceStart(_, Some(tag))
                | Event::MappingStart(_, Some(tag)),
                _,
            )) if tag.handle != "!!" => return true,
            Ok(_) => {}
        }
    }
}

/// Whether a `Real` is really an integer literal the loader couldn't fit in
/// an `i64` (it falls back to a float rather than failing).
fn is_integer_literal(text: &str) -> bool {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn parse_yaml(vm: &mut BexVm, s: &str) -> Result<Value, VmRustFnError> {
    let docs = YamlLoader::load_from_str(s).map_err(|e| parse_error(vm, e.to_string()))?;
    if has_custom_tag(s) {
        return Err(parse_error(
            vm,
            "YAML tags are not supported by baml.yaml.parse",
        ));
    }
    let mut docs = docs.into_iter();
    let Some(first_doc) = docs.next() else {
        return Ok(Value::NULL);
    };
    if docs.next().is_some() {
        return Err(parse_error(
            vm,
            "YAML stream contains multiple documents; baml.yaml.parse accepts exactly one document",
        ));
    }
    convert_yaml_value(vm, first_doc)
}

fn convert_yaml_value(vm: &mut BexVm, value: Yaml) -> Result<Value, VmRustFnError> {
    match value {
        Yaml::Null => Ok(Value::NULL),
        Yaml::Boolean(b) => Ok(Value::bool(b)),
        Yaml::Integer(i) => Value::try_int(i)
            .ok_or_else(|| parse_error(vm, "YAML integer is outside BAML int range")),
        Yaml::Real(ref text) if is_integer_literal(text) => {
            Err(parse_error(vm, "YAML integer is outside BAML int range"))
        }
        Yaml::Real(_) => {
            let Some(f) = value.as_f64() else {
                return Err(parse_error(
                    vm,
                    "YAML number cannot be represented as a BAML int or float",
                ));
            };
            if !f.is_finite() {
                return Err(parse_error(
                    vm,
                    "YAML non-finite floats are not supported by baml.yaml.parse",
                ));
            }
            Ok(Value::object(vm.alloc_float(f)))
        }
        Yaml::String(s) => Ok(Value::object(vm.alloc_string(s))),
        Yaml::Array(values) => {
            let values = values
                .into_iter()
                .map(|v| convert_yaml_value(vm, v))
                .collect::<Result<Vec<Value>, VmRustFnError>>()?;
            // YAML is parsed into the `baml.json.json` value algebra.
            Ok(Value::object(
                vm.alloc_array(super::json::json_alias_ty(vm), values),
            ))
        }
        Yaml::Hash(map) => {
            let mut entries = IndexMap::with_capacity(map.len());
            for (key, value) in map {
                let Yaml::String(key) = key else {
                    return Err(parse_error(
                        vm,
                        "YAML mappings must use string keys to fit baml.json.json",
                    ));
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
        Yaml::Alias(_) => Err(parse_error(
            vm,
            "YAML aliases are not supported by baml.yaml.parse",
        )),
        Yaml::BadValue => Err(parse_error(
            vm,
            "YAML value is not supported by baml.yaml.parse",
        )),
    }
}

fn make_yaml_parse_error(vm: &mut BexVm, message: String) -> Value {
    let err_msg = Value::object(vm.alloc_string(message));
    let class = vm.resolve_class("baml.yaml.ParseError");
    Value::object(vm.alloc_instance(class, vec![err_msg]))
}

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
            Err(e) => Err(VmRustFnError::thrown_fresh(make_toml_parse_error(
                vm,
                e.to_string(),
            ))),
        }
    }
}

/// Convert a parsed `toml::Value` into a VM `Value`.
///
/// Scalars map onto VM primitives; arrays and tables recurse, with tables
/// wrapped in a `baml.toml.Table` instance. TOML's four datetime kinds map
/// losslessly onto the `baml.time` types per BEP-021: offset datetime →
/// `ZonedDateTime` (fixed offset), local datetime → `PlainDateTime`, local
/// date → `PlainDate`, local time → `PlainTime`.
fn convert_toml_value(vm: &mut BexVm, value: ::toml::Value) -> Result<Value, VmRustFnError> {
    match value {
        toml::Value::String(s) => Ok(Value::object(vm.alloc_string(s))),
        toml::Value::Integer(i) => Value::try_int(i).ok_or_else(|| {
            VmRustFnError::thrown_fresh(make_toml_parse_error(
                vm,
                "BAML `int` is 63 bits. It is unable to parse 64-bit TOML integers.".to_string(),
            ))
        }),
        toml::Value::Float(f) => Ok(Value::object(vm.alloc_float(f))),
        toml::Value::Boolean(b) => Ok(Value::bool(b)),
        toml::Value::Datetime(datetime) => convert_toml_datetime(vm, datetime),
        toml::Value::Array(values) => {
            let array = values
                .into_iter()
                .map(|v| convert_toml_value(vm, v))
                .collect::<Result<Vec<Value>, VmRustFnError>>()?;
            // TOML values decode into the same dynamic `json` algebra as JSON
            // (null/bool/number/string/array/table), so the element type is the
            // recursive `json` union.
            Ok(Value::object(
                vm.alloc_array(super::json::json_alias_ty(vm), array),
            ))
        }
        toml::Value::Table(map) => {
            let map = map
                .into_iter()
                .map(|(k, v)| {
                    let v = convert_toml_value(vm, v)?;
                    Ok((bex_str::BexStr::from(k), v))
                })
                .collect::<Result<IndexMap<bex_str::BexStr, Value>, VmRustFnError>>()?;
            // TOML table values decode into the `json` algebra (string keys),
            // so the value type is the recursive `json` union.
            let map = Value::object(vm.alloc_map(
                bex_vm_types::RealizedTy::string(),
                super::json::json_alias_ty(vm),
                map,
            ));
            let class = vm.resolve_class("baml.toml.Table");
            Ok(Value::object(vm.alloc_instance(class, vec![map])))
        }
    }
}

/// Map a TOML datetime onto the `baml.time` types (BEP-021's interop table):
///
/// | TOML kind                                    | BAML type                      |
/// | -------------------------------------------- | ------------------------------ |
/// | Offset Date-Time `1979-05-27T07:32:00-07:00` | `ZonedDateTime` (fixed offset) |
/// | Local Date-Time `1979-05-27T07:32:00`        | `PlainDateTime`                |
/// | Local Date `1979-05-27`                      | `PlainDate`                    |
/// | Local Time `07:32:00`                        | `PlainTime`                    |
fn convert_toml_datetime(
    vm: &mut BexVm,
    datetime: ::toml::value::Datetime,
) -> Result<Value, VmRustFnError> {
    use std::sync::Arc;

    use super::time::{NANOS_PER_DAY, NANOS_PER_HOUR, NANOS_PER_MINUTE, NANOS_PER_SECOND};

    let days = datetime
        .date
        .map(|d| {
            super::time::date_from_components(
                i64::from(d.year),
                i64::from(d.month),
                i64::from(d.day),
            )
            .map(super::time::days_since_epoch)
            .map_err(|_| {
                VmRustFnError::thrown_fresh(make_toml_parse_error(
                    vm,
                    format!("invalid TOML date: {datetime}"),
                ))
            })
        })
        .transpose()?;
    let time_ns = datetime.time.map(|t| {
        i64::from(t.hour) * NANOS_PER_HOUR
            + i64::from(t.minute) * NANOS_PER_MINUTE
            + i64::from(t.second) * NANOS_PER_SECOND
            + i64::from(t.nanosecond)
    });

    match (days, time_ns, datetime.offset) {
        // Offset Date-Time → ZonedDateTime with a fixed offset.
        (Some(days), Some(time_ns), Some(offset)) => {
            let offset_ns = match offset {
                ::toml::value::Offset::Z => 0,
                ::toml::value::Offset::Custom { minutes } => i64::from(minutes) * NANOS_PER_MINUTE,
            };
            let civil = i128::from(days) * NANOS_PER_DAY + i128::from(time_ns);
            let epoch = civil - i128::from(offset_ns);
            let zoned = super::copy::time::ZonedDateTime {
                _nanoseconds: Arc::new(num_bigint::BigInt::from(epoch)),
                _offset_ns: Value::try_int(offset_ns).expect("offset fits in BAML int"),
                _iana: Value::NULL,
            };
            Ok(zoned.to_value(vm))
        }
        // Local Date-Time → PlainDateTime.
        (Some(days), Some(time_ns), None) => {
            let civil = i128::from(days) * NANOS_PER_DAY + i128::from(time_ns);
            let plain = super::copy::time::PlainDateTime {
                _nanoseconds: Arc::new(num_bigint::BigInt::from(civil)),
            };
            Ok(plain.to_value(vm))
        }
        // Local Date → PlainDate.
        (Some(days), None, None) => Ok(super::copy::time::PlainDate { _days: days }.to_value(vm)),
        // Local Time → PlainTime.
        (None, Some(time_ns), None) => Ok(super::copy::time::PlainTime {
            _nanoseconds: time_ns,
        }
        .to_value(vm)),
        // The `toml` crate never produces other combinations (an offset
        // requires both a date and a time).
        _ => Err(VmRustFnError::thrown_fresh(make_toml_parse_error(
            vm,
            format!("invalid TOML datetime: {datetime}"),
        ))),
    }
}

fn make_toml_parse_error(vm: &mut BexVm, message: String) -> Value {
    let err_msg = Value::object(vm.alloc_string(message));
    let class = vm.resolve_class("baml.toml.ParseError");
    Value::object(vm.alloc_instance(class, vec![err_msg]))
}

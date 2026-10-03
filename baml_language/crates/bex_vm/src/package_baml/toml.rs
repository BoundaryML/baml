//! TOML parsing and format-preserving serialization. Table mutation and
//! TOML ↔ JSON conversions are ordinary BAML code.

use std::str::FromStr;

use bex_heap::TlabHolder;
use bex_vm_types::{RealizedTy, Value, ValueKind, types::Object};
use indexmap::IndexMap;
use toml_edit::{DocumentMut, Item, TableLike};

use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
    package_baml::PackageBamlImpl,
};

impl super::BamlNamespaceToml for PackageBamlImpl {}

impl super::BamlClassTomlTable for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        match ::toml::Table::from_str(s.as_str()) {
            Ok(table) => {
                let value = convert_toml_value(vm, ::toml::Value::Table(table))?;
                let source = Value::object(vm.alloc_string(s.clone()));
                vm.as_instance(&value)?.store_field(1, source);
                Ok(value)
            }
            Err(e) => Err(VmRustFnError::thrown_fresh(make_toml_parse_error(
                vm,
                e.to_string(),
            ))),
        }
    }

    fn _to_string_impl(vm: &mut BexVm, table: &Value) -> Result<bex_str::BexStr, VmRustFnError> {
        let source = vm.as_instance(table)?.load_field(1);
        let mut document = if source.is_null() {
            DocumentMut::new()
        } else {
            vm.as_string(&source)?
                .as_str()
                .parse::<DocumentMut>()
                .map_err(|e| invalid_argument(e.to_string()))?
        };
        let node = snapshot(vm, *table, &mut Vec::new())?;
        reconcile(document.as_item_mut(), &node);
        Ok(document.to_string().into())
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
            Ok(Value::object(vm.alloc_array(item_ty(vm), array)))
        }
        toml::Value::Table(map) => {
            let map = map
                .into_iter()
                .map(|(k, v)| {
                    let v = convert_toml_value(vm, v)?;
                    Ok((bex_str::BexStr::from(k), v))
                })
                .collect::<Result<IndexMap<bex_str::BexStr, Value>, VmRustFnError>>()?;
            let map = Value::object(vm.alloc_map(RealizedTy::string(), item_ty(vm), map));
            let class = vm.resolve_class("baml.toml.Table");
            Ok(Value::object(vm.alloc_instance(
                class,
                vec![map, Value::NULL, Value::NULL],
            )))
        }
    }
}

fn item_ty(vm: &BexVm) -> RealizedTy {
    RealizedTy::TypeAlias(
        vm.declaration_head(&baml_type::TypeName::from_dotted_path("baml.toml.Item"))
            .expect("baml.toml.Item is declared by the stdlib"),
    )
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

fn invalid_argument(message: impl Into<String>) -> VmRustFnError {
    VmBamlError::InvalidArgument {
        message: message.into(),
    }
    .into()
}

/// A snapshot of the BAML values. Edits are made to those values, not to a
/// second native document. Only the serializer reconciles them with the source.
enum Node {
    Scalar(toml_edit::Value),
    Array(Vec<Node>),
    Table {
        items: IndexMap<String, Node>,
        renamed: IndexMap<String, String>,
    },
}

#[expect(
    clippy::used_underscore_items,
    reason = "TOML datetime encoding uses the generated private field accessors and existing time codecs"
)]
fn snapshot(vm: &BexVm, value: Value, active: &mut Vec<Value>) -> Result<Node, VmRustFnError> {
    use super::{
        BamlClassTimePlainDate, BamlClassTimePlainDateTime, BamlClassTimePlainTime,
        BamlNamespaceTime, view,
    };
    match value.kind() {
        ValueKind::Bool(b) => return Ok(Node::Scalar(b.into())),
        ValueKind::Int(i) => return Ok(Node::Scalar(i.into())),
        ValueKind::Object(_) => {}
        _ => return Err(invalid_argument("Value is not a TOML item")),
    }
    if active.contains(&value) || active.len() >= 128 {
        return Err(invalid_argument(
            "TOML values must be acyclic and at most 128 levels deep",
        ));
    }
    active.push(value);
    let ptr = value.as_object_ptr().expect("object kind has a pointer");
    let result = match vm.get_object(ptr) {
        Object::String(s) => Ok(Node::Scalar(s.as_str().into())),
        Object::Float(f) => Ok(Node::Scalar((*f).into())),
        Object::Array(values) => values
            .to_vec()
            .into_iter()
            .map(|v| snapshot(vm, v, active))
            .collect::<Result<Vec<_>, _>>()
            .map(Node::Array),
        Object::Instance(instance) => {
            let class = instance.class;
            if class == vm.resolve_class("baml.toml.Table") {
                let items = vm
                    .as_map(&instance.load_field(0))?
                    .iter()
                    .map(|(k, v)| Ok((k.to_string(), snapshot(vm, *v, active)?)))
                    .collect::<Result<IndexMap<_, _>, VmRustFnError>>()?;
                let names = instance.load_field(2);
                let renamed = if names.is_null() {
                    IndexMap::new()
                } else {
                    vm.as_map(&names)?
                        .iter()
                        .map(|(k, v)| Ok((k.to_string(), vm.as_string(v)?.to_string())))
                        .collect::<Result<IndexMap<_, _>, VmRustFnError>>()?
                };
                Ok(Node::Table { items, renamed })
            } else {
                let text = if class == vm.resolve_class("baml.time.PlainDate") {
                    <PackageBamlImpl as BamlClassTimePlainDate>::_to_string_impl(
                        &view::time::PlainDate { instance },
                    )?
                } else if class == vm.resolve_class("baml.time.PlainDateTime") {
                    <PackageBamlImpl as BamlClassTimePlainDateTime>::_to_string_impl(
                        &view::time::PlainDateTime { instance },
                    )?
                } else if class == vm.resolve_class("baml.time.PlainTime") {
                    <PackageBamlImpl as BamlClassTimePlainTime>::_to_string_impl(
                        &view::time::PlainTime { instance },
                    )
                } else if class == vm.resolve_class("baml.time.ZonedDateTime") {
                    let zoned = view::time::ZonedDateTime { instance };
                    // TOML has offsets, not IANA zone identifiers. Named zones
                    // serialize as UTC while preserving the absolute instant.
                    let offset = zoned._offset_ns(vm).unwrap_or(0);
                    if offset % super::time::NANOS_PER_MINUTE != 0 {
                        return Err(invalid_argument("TOML offsets must be whole minutes"));
                    }
                    PackageBamlImpl::_format_zoned(zoned._nanoseconds(), offset, None)?
                } else {
                    return Err(invalid_argument("Value is not a TOML item"));
                };
                let datetime = text
                    .as_str()
                    .parse::<toml_edit::Datetime>()
                    .map_err(|e| invalid_argument(e.to_string()))?;
                Ok(Node::Scalar(datetime.into()))
            }
        }
        _ => Err(invalid_argument("Value is not a TOML item")),
    };
    active.pop();
    result
}

fn as_value(node: &Node) -> toml_edit::Value {
    match node {
        Node::Scalar(value) => value.clone(),
        Node::Array(values) => {
            let mut array = toml_edit::Array::new();
            for value in values {
                array.push(as_value(value));
            }
            toml_edit::Value::Array(array)
        }
        Node::Table { items, .. } => {
            let mut table = toml_edit::InlineTable::new();
            for (key, value) in items {
                table.insert(key, as_value(value));
            }
            toml_edit::Value::InlineTable(table)
        }
    }
}

fn scalar_eq(left: &toml_edit::Value, right: &toml_edit::Value) -> bool {
    use toml_edit::Value as V;
    match (left, right) {
        (V::String(a), V::String(b)) => a.value() == b.value(),
        (V::Integer(a), V::Integer(b)) => a.value() == b.value(),
        (V::Float(a), V::Float(b)) => a.value().to_bits() == b.value().to_bits(),
        (V::Boolean(a), V::Boolean(b)) => a.value() == b.value(),
        (V::Datetime(a), V::Datetime(b)) => a.value() == b.value(),
        _ => false,
    }
}

fn reconcile_table(
    table: &mut dyn TableLike,
    items: &IndexMap<String, Node>,
    renamed: &IndexMap<String, String>,
    inline: bool,
) {
    for (to, from) in renamed {
        if let Some(key) = table.key(from).cloned() {
            if let Some(item) = table.remove(from) {
                let mut new_key = toml_edit::Key::new(to);
                *new_key.leaf_decor_mut() = key.leaf_decor().clone();
                table.remove(to);
                table.entry_format(&new_key).or_insert(item);
            }
        }
    }
    let removed: Vec<_> = table
        .iter()
        .filter(|(k, _)| !items.contains_key(*k))
        .map(|(k, _)| k.to_string())
        .collect();
    for key in removed {
        table.remove(&key);
    }
    for (key, value) in items {
        let entry = table.entry(key).or_insert(Item::None);
        reconcile(entry, value);
        if inline && !entry.is_value() {
            *entry = Item::Value(
                std::mem::take(entry)
                    .into_value()
                    .expect("TOML array and table items convert to values"),
            );
        }
    }
}

fn reconcile(item: &mut Item, node: &Node) {
    match node {
        Node::Table { items, renamed } => {
            if item.as_table_like().is_none() {
                let mut table = toml_edit::Table::new();
                if let Some(old) = item.as_value() {
                    *table.decor_mut() = old.decor().clone();
                }
                *item = Item::Table(table);
            }
            let inline = item.is_inline_table();
            reconcile_table(
                item.as_table_like_mut().expect("table initialized"),
                items,
                renamed,
                inline,
            );
        }
        Node::Array(values) => {
            if let Some(tables) = item.as_array_of_tables_mut() {
                if values.iter().all(|v| matches!(v, Node::Table { .. })) {
                    while tables.len() > values.len() {
                        tables.remove(tables.len() - 1);
                    }
                    for (index, value) in values.iter().enumerate() {
                        if index >= tables.len() {
                            tables.push(toml_edit::Table::new());
                        }
                        if let Node::Table { items, renamed } = value {
                            reconcile_table(
                                tables.get_mut(index).expect("table initialized"),
                                items,
                                renamed,
                                false,
                            );
                        }
                    }
                    return;
                }
            }
            if let Some(array) = item.as_array_mut() {
                while array.len() > values.len() {
                    array.remove(array.len() - 1);
                }
                for (index, node) in values.iter().enumerate() {
                    if let Some(value) = array.get_mut(index) {
                        let mut entry = Item::Value(value.clone());
                        reconcile(&mut entry, node);
                        // Array entries are values, so a newly inserted table
                        // must be inline rather than a document section.
                        *value = entry.into_value().unwrap_or_else(|_| as_value(node));
                    } else {
                        array.push(as_value(node));
                    }
                }
            } else {
                replace_value(item, as_value(node));
            }
        }
        Node::Scalar(value) => {
            if !item.as_value().is_some_and(|old| scalar_eq(old, value)) {
                replace_value(item, value.clone());
            }
        }
    }
}

fn replace_value(item: &mut Item, mut value: toml_edit::Value) {
    if let Some(old) = item.as_value() {
        *value.decor_mut() = old.decor().clone();
    }
    *item = Item::Value(value);
}

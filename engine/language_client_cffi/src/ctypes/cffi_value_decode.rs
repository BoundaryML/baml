use crate::{ctypes::utils::Decode, ffi::Value};

impl Decode for Value {
    type From = crate::baml::cffi::CffiValueHolder;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        use crate::baml::cffi::cffi_value_holder::Value as cValue;
        Ok(match from.value {
            Some(value) => match value {
                cValue::NullValue(_) => Value::Null(()),
                cValue::StringValue(cffi_value_string) => Value::String(cffi_value_string, ()),
                cValue::IntValue(cffi_value_int) => Value::Int(cffi_value_int, ()),
                cValue::FloatValue(cffi_value_float) => Value::Float(cffi_value_float, ()),
                cValue::BoolValue(cffi_value_bool) => Value::Bool(cffi_value_bool, ()),
                cValue::ListValue(cffi_value_list) => Value::List(
                    cffi_value_list
                        .values
                        .into_iter()
                        .map(Value::decode)
                        .collect::<Result<_, _>>()?,
                    (),
                ),
                cValue::MapValue(cffi_value_map) => Value::Map(
                    cffi_value_map
                        .entries
                        .into_iter()
                        .map(from_cffi_map_entry)
                        .collect::<Result<_, _>>()?,
                    (),
                ),
                cValue::ClassValue(cffi_value_class) => {
                    let class_name = cffi_value_class
                        .name
                        .ok_or(anyhow::anyhow!("Class value missing name"))?
                        .name;

                    let fields = cffi_value_class
                        .fields
                        .into_iter()
                        .map(from_cffi_map_entry)
                        .collect::<Result<_, _>>()?;

                    Value::Class(class_name, fields, ())
                }
                cValue::EnumValue(cffi_value_enum) => {
                    let enum_name = cffi_value_enum
                        .name
                        .ok_or(anyhow::anyhow!("Enum value missing name"))?
                        .name;

                    let enum_value = cffi_value_enum.value;

                    Value::Enum(enum_name, enum_value, ())
                }
                cValue::ObjectValue(cffi_value_object) => {
                    let inner = cffi_value_object.object.unwrap();
                    match inner {
                        crate::baml::cffi::cffi_value_raw_object::Object::Media(
                            cffi_raw_object,
                        )
                        | crate::baml::cffi::cffi_value_raw_object::Object::Type(cffi_raw_object) =>
                        {
                            let raw_object =
                                crate::raw_ptr_wrapper::RawPtrType::decode(cffi_raw_object)?;
                            Value::RawPtr(raw_object, ())
                        }
                    }
                }
                cValue::TupleValue(cffi_value_tuple) => {
                    let values = cffi_value_tuple
                        .values
                        .into_iter()
                        .map(Value::decode)
                        .collect::<Result<_, _>>()?;

                    // Convert tuple to list since Value doesn't have a Tuple variant
                    Value::List(values, ())
                }
                cValue::UnionVariantValue(cffi_value_union_variant) => {
                    // For union variants, we just extract the inner value
                    let value = cffi_value_union_variant
                        .value
                        .ok_or(anyhow::anyhow!("Union variant missing value"))?;

                    Value::decode(*value)?
                }
                cValue::CheckedValue(_cffi_value_checked) => {
                    anyhow::bail!("Checked value is not supported in Value::decode")
                }
                cValue::StreamingStateValue(_cffi_value_streaming_state) => {
                    anyhow::bail!("Streaming state value is not supported in Value::decode")
                }
            },
            None => Value::Null(()),
        })
    }
}

pub(super) fn from_cffi_map_entry(
    item: crate::baml::cffi::CffiMapEntry,
) -> Result<(String, Value), anyhow::Error> {
    let value = item
        .value
        .ok_or(anyhow::anyhow!("Value is null for key {}", item.key))?;
    Ok((item.key, Value::decode(value)?))
}

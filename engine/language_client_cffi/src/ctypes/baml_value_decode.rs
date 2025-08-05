use baml_types::BamlValue;

use crate::ctypes::utils::Decode;

impl Decode for BamlValue {
    type From = crate::baml::cffi::CffiValueHolder;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        use crate::baml::cffi::cffi_value_holder::Value;
        Ok(match from.value {
            Some(value) => match value {
                Value::NullValue(_) => BamlValue::Null,
                Value::StringValue(cffi_value_string) => BamlValue::String(cffi_value_string),
                Value::IntValue(cffi_value_int) => BamlValue::Int(cffi_value_int),
                Value::FloatValue(cffi_value_float) => BamlValue::Float(cffi_value_float),
                Value::BoolValue(cffi_value_bool) => BamlValue::Bool(cffi_value_bool),
                Value::ListValue(cffi_value_list) => BamlValue::List(
                    cffi_value_list
                        .values
                        .into_iter()
                        .map(BamlValue::decode)
                        .collect::<Result<_, _>>()?,
                ),
                Value::MapValue(cffi_value_map) => BamlValue::Map(
                    cffi_value_map
                        .entries
                        .into_iter()
                        .map(from_cffi_map_entry)
                        .collect::<Result<_, _>>()?,
                ),
                Value::ClassValue(cffi_value_class) => {
                    let class_name = cffi_value_class
                        .name
                        .ok_or(anyhow::anyhow!("Class value missing name"))?
                        .name;

                    let fields = cffi_value_class
                        .fields
                        .into_iter()
                        .map(from_cffi_map_entry)
                        .collect::<Result<_, _>>()?;

                    BamlValue::Class(class_name, fields)
                }
                Value::EnumValue(cffi_value_enum) => {
                    let enum_name = cffi_value_enum
                        .name
                        .ok_or(anyhow::anyhow!("Enum value missing name"))?
                        .name;

                    let enum_value = cffi_value_enum.value;

                    BamlValue::Enum(enum_name, enum_value)
                }
                Value::ObjectValue(cffi_value_object) => {
                    let inner = cffi_value_object.object.unwrap();
                    match inner {
                        crate::baml::cffi::cffi_value_raw_object::Object::Media(
                            cffi_raw_object,
                        ) => {
                            let media_object =
                                crate::raw_ptr_wrapper::RawPtrType::decode(cffi_raw_object)?;
                            let baml_media = match media_object {
                                crate::raw_ptr_wrapper::RawPtrType::Media(media) => {
                                    media.as_ref().clone()
                                }
                                other => {
                                    anyhow::bail!("Expected media object, got: {:?}", other.name());
                                }
                            };
                            BamlValue::Media(baml_media)
                        }
                        other => {
                            anyhow::bail!("Unexpected object type: {:?}", other)
                        }
                    }
                }
                Value::TupleValue(cffi_value_tuple) => {
                    let values = cffi_value_tuple
                        .values
                        .into_iter()
                        .map(BamlValue::decode)
                        .collect::<Result<_, _>>()?;

                    // Convert tuple to list since BamlValue doesn't have a Tuple variant
                    BamlValue::List(values)
                }
                Value::UnionVariantValue(cffi_value_union_variant) => {
                    // For union variants, we just extract the inner value
                    let value = cffi_value_union_variant
                        .value
                        .ok_or(anyhow::anyhow!("Union variant missing value"))?;

                    BamlValue::decode(*value)?
                }
                Value::CheckedValue(_cffi_value_checked) => {
                    anyhow::bail!("Checked value is not supported in BamlValue::decode")
                }
                Value::StreamingStateValue(_cffi_value_streaming_state) => {
                    anyhow::bail!("Streaming state value is not supported in BamlValue::decode")
                }
            },
            None => BamlValue::Null,
        })
    }
}

pub(super) fn from_cffi_map_entry(
    item: crate::baml::cffi::CffiMapEntry,
) -> Result<(String, BamlValue), anyhow::Error> {
    let value = item
        .value
        .ok_or(anyhow::anyhow!("Value is null for key {}", item.key))?;
    Ok((item.key, BamlValue::decode(value)?))
}

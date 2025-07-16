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
                Value::MediaValue(cffi_value_media) => {
                    let media_type = cffi_value_media
                        .media_type
                        .ok_or(anyhow::anyhow!("Media value missing media_type"))?;

                    let media_value = cffi_value_media
                        .media_value
                        .ok_or(anyhow::anyhow!("Media value missing media_value"))?;

                    let baml_media_type = match media_type.media() {
                        crate::baml::cffi::MediaTypeEnum::Image => baml_types::BamlMediaType::Image,
                        crate::baml::cffi::MediaTypeEnum::Audio => baml_types::BamlMediaType::Audio,
                        crate::baml::cffi::MediaTypeEnum::Pdf => baml_types::BamlMediaType::Pdf,
                        crate::baml::cffi::MediaTypeEnum::Video => baml_types::BamlMediaType::Video,
                    };

                    let mime_type = media_value.mime_type;

                    let baml_media = match media_value.content {
                        Some(crate::baml::cffi::cffi_media_value::Content::UrlContent(
                            url_content,
                        )) => {
                            let url = url_content.url;
                            baml_types::BamlMedia::url(baml_media_type, url, mime_type)
                        }
                        Some(crate::baml::cffi::cffi_media_value::Content::Base64Content(
                            base64_content,
                        )) => {
                            let data = base64_content.data;
                            baml_types::BamlMedia::base64(baml_media_type, data, mime_type)
                        }
                        Some(crate::baml::cffi::cffi_media_value::Content::FileContent(
                            file_content,
                        )) => {
                            let relpath = file_content.path;
                            let baml_path = std::path::PathBuf::from(&relpath);
                            baml_types::BamlMedia::file(
                                baml_media_type,
                                baml_path,
                                relpath,
                                mime_type,
                            )
                        }
                        None => {
                            return Err(anyhow::anyhow!("Media value missing content"));
                        }
                    };

                    BamlValue::Media(baml_media)
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

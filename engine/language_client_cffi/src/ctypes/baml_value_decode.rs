use baml_types::BamlValue;

use crate::ctypes::utils::Decode;

impl Decode for BamlValue {
    type From = crate::baml::cffi::HostValue;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        let value = crate::ffi::Value::decode(from)?;
        from_ffi_value_to_baml_value(value)
    }
}

fn from_ffi_value_to_baml_value(value: crate::ffi::Value) -> Result<BamlValue, anyhow::Error> {
    match value {
        crate::ffi::Value::Null(_) => Ok(BamlValue::Null),
        crate::ffi::Value::String(s, _) => Ok(BamlValue::String(s)),
        crate::ffi::Value::Int(i, _) => Ok(BamlValue::Int(i)),
        crate::ffi::Value::Float(f, _) => Ok(BamlValue::Float(f)),
        crate::ffi::Value::Bool(b, _) => Ok(BamlValue::Bool(b)),
        crate::ffi::Value::Map(m, _) => Ok(BamlValue::Map(
            m.into_iter()
                .map(|(k, v)| from_ffi_value_to_baml_value(v).map(|v| (k, v)))
                .collect::<Result<_, _>>()?,
        )),
        crate::ffi::Value::List(l, _) => Ok(BamlValue::List(
            l.into_iter()
                .map(from_ffi_value_to_baml_value)
                .collect::<Result<_, _>>()?,
        )),
        crate::ffi::Value::RawPtr(r, _) => match r {
            crate::raw_ptr_wrapper::RawPtrType::Media(raw_ptr_wrapper) => {
                Ok(BamlValue::Media(raw_ptr_wrapper.as_ref().clone()))
            }
            _ => anyhow::bail!("unsupported raw pointer type"),
        },
        crate::ffi::Value::Class(c, fields, _) => Ok(BamlValue::Class(
            c,
            fields
                .into_iter()
                .map(|(k, v)| from_ffi_value_to_baml_value(v).map(|v| (k, v)))
                .collect::<Result<_, _>>()?,
        )),
        crate::ffi::Value::Enum(e, value, _) => Ok(BamlValue::Enum(e, value)),
    }
}

pub(super) fn from_host_kv_to_baml_kv(
    item: crate::baml::cffi::HostMapEntry,
) -> Result<(String, BamlValue), anyhow::Error> {
    use crate::baml::cffi::host_map_entry::Key;
    let key = match item.key {
        Some(Key::StringKey(key)) => key,
        Some(Key::EnumKey(key)) => key.value,
        Some(Key::IntKey(_)) | Some(Key::BoolKey(_)) => {
            anyhow::bail!("only string keys are supported")
        }
        None => anyhow::bail!("Key is missing"),
    };

    let value = item
        .value
        .ok_or(anyhow::anyhow!("Value is null for key {}", key))?;

    Ok((key, BamlValue::decode(value)?))
}

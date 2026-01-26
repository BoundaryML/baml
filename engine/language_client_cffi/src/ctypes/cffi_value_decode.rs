use crate::{ctypes::utils::Decode, ffi::Value, raw_ptr_wrapper::RawPtrType};

impl Decode for Value {
    type From = crate::baml::cffi::HostValue;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        use crate::baml::cffi::host_value::Value as HostVal;

        let Some(value) = from.value else {
            return Ok(Value::Null(()));
        };

        Ok(match value {
            HostVal::StringValue(s) => Value::String(s, ()),
            HostVal::IntValue(i) => Value::Int(i, ()),
            HostVal::FloatValue(f) => Value::Float(f, ()),
            HostVal::BoolValue(b) => Value::Bool(b, ()),
            HostVal::ListValue(l) => Value::List(
                l.values
                    .into_iter()
                    .map(Value::decode)
                    .collect::<Result<_, _>>()?,
                (),
            ),
            HostVal::MapValue(m) => Value::Map(
                m.entries
                    .into_iter()
                    .map(from_host_map_entry)
                    .collect::<Result<_, _>>()?,
                (),
            ),
            HostVal::ClassValue(c) => {
                let fields = c
                    .fields
                    .into_iter()
                    .map(from_host_map_entry)
                    .collect::<Result<_, _>>()?;
                Value::Class(c.name, fields, ())
            }
            HostVal::EnumValue(e) => Value::Enum(e.name, e.value, ()),
            HostVal::Handle(handle) => {
                let raw_ptr = RawPtrType::decode(handle)?;
                Value::RawPtr(raw_ptr, ())
            }
        })
    }
}

pub(super) fn from_host_map_entry(
    item: crate::baml::cffi::HostMapEntry,
) -> Result<(String, Value), anyhow::Error> {
    let key = match item.key {
        Some(crate::baml::cffi::host_map_entry::Key::StringKey(k)) => k,
        _ => return Err(anyhow::anyhow!("Key must be a string")),
    };
    let value = item
        .value
        .ok_or(anyhow::anyhow!("Value is null for key {}", key))?;
    Ok((key, Value::decode(value)?))
}

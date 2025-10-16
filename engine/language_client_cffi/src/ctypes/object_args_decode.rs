use crate::{
    baml::cffi::CffiObjectType, ctypes::utils::Decode, ffi::Value, raw_ptr_wrapper::RawPtrType,
};

pub struct BamlMethodArguments {
    pub object: RawPtrType,
    pub method_name: String,
    pub kwargs: baml_types::BamlMap<String, crate::ffi::Value>,
}

pub struct BamlObjectConstructorArgs {
    pub object_type: CffiObjectType,
    pub kwargs: baml_types::BamlMap<String, crate::ffi::Value>,
}

impl Decode for BamlMethodArguments {
    type From = crate::baml::cffi::CffiObjectMethodArguments;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        Ok(BamlMethodArguments {
            object: match from.object.map(RawPtrType::decode).transpose()? {
                Some(object) => object,
                None => {
                    return Err(anyhow::anyhow!("Failed to decode RawPtrType for object"));
                }
            },
            method_name: from.method_name,
            kwargs: from
                .kwargs
                .into_iter()
                .map(|v| match v.value {
                    Some(value) => Ok((v.key, Value::decode(value)?)),
                    None => Err(anyhow::anyhow!("Failed to decode BamlValue")),
                })
                .collect::<Result<_, _>>()?,
        })
    }
}

impl Decode for BamlObjectConstructorArgs {
    type From = crate::baml::cffi::CffiObjectConstructorArgs;

    fn decode(from: Self::From) -> Result<Self, anyhow::Error> {
        Ok(BamlObjectConstructorArgs {
            object_type: CffiObjectType::try_from(from.r#type)?,
            kwargs: from
                .kwargs
                .into_iter()
                .map(|v| match v.value {
                    Some(value) => Ok((v.key, Value::decode(value)?)),
                    None => Err(anyhow::anyhow!("Failed to decode Value")),
                })
                .collect::<Result<_, _>>()?,
        })
    }
}

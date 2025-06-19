use baml_types::BamlValue;

use crate::ctypes::{cffi_generated::cffi::{CFFIValueHolder, CFFIValueUnion}, traits::Decode};

impl Decode for BamlValue {
    type From<'a> = CFFIValueHolder<'a>;

    fn decode(from: Self::From<'_>) -> Result<Self, anyhow::Error> {
        Ok(BamlValue::from(from))
    }
}

impl From<CFFIValueHolder<'_>> for BamlValue {
    fn from(value: CFFIValueHolder) -> Self {
        let value_type = value.value_type();
        match value_type {
            CFFIValueUnion::NONE => BamlValue::Null,
            CFFIValueUnion::CFFIValueString => value
                .value_as_cffivalue_string()
                .and_then(|s| s.value().map(|s| BamlValue::String(s.to_string())))
                .expect("Failed to convert CFFIValueString to BamlValue"),
            CFFIValueUnion::CFFIValueInt => value
                .value_as_cffivalue_int()
                .map(|i| BamlValue::Int(i.value()))
                .expect("Failed to convert CFFIValueInt to BamlValue"),
            CFFIValueUnion::CFFIValueFloat => value
                .value_as_cffivalue_float()
                .map(|f| BamlValue::Float(f.value()))
                .expect("Failed to convert CFFIValueFloat to BamlValue"),
            CFFIValueUnion::CFFIValueBool => value
                .value_as_cffivalue_bool()
                .map(|b| BamlValue::Bool(b.value()))
                .expect("Failed to convert CFFIValueBool to BamlValue"),
            CFFIValueUnion::CFFIValueList => value
                .value_as_cffivalue_list()
                .and_then(|l| l.values())
                .map(|v| v.into_iter().map(|v| v.into()))
                .map(|l| BamlValue::List(l.collect()))
                .expect("Failed to convert CFFIValueList to BamlValue"),
            CFFIValueUnion::CFFIValueMap => value
                .value_as_cffivalue_map()
                .and_then(|m| m.entries())
                .map(|v| v.into_iter().map(|v| v.into()).collect())
                .map(|kv| BamlValue::Map(kv))
                .expect("Failed to convert CFFIValueMap to BamlValue"),
            CFFIValueUnion::CFFIValueClass => value
                .value_as_cffivalue_class()
                .expect("Failed to convert CFFIValueClass to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueEnum => value
                .value_as_cffivalue_enum()
                .expect("Failed to convert CFFIValueEnum to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueMedia => value
                .value_as_cffivalue_media()
                .map(|m| BamlValue::Media(m.into()))
                .expect("Failed to convert CFFIValueMedia to BamlValue"),
            CFFIValueUnion::CFFIValueTuple => value
                .value_as_cffivalue_tuple()
                .expect("Failed to convert CFFIValueTuple to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueUnionVariant => value
                .value_as_cffivalue_union_variant()
                .expect("Failed to convert CFFIValueUnionVariant to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueChecked => value
                .value_as_cffivalue_checked()
                .expect("Failed to convert CFFIValueChecked to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueStreamingState => value
                .value_as_cffivalue_streaming_state()
                .expect("Failed to convert CFFIValueStreamingState to BamlValue")
                .into(),
            CFFIValueUnion::CFFIFunctionArguments => {
                panic!("CFFIFunctionArguments is not supported in BamlValue");
            }
            other => {
                panic!("Unsupported value type: {:?}", other);
            }
        }
    }
}

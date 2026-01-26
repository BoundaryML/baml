use baml_types::{ir_type::UnionConstructor, type_meta, BamlValue};

use crate::{
    baml::cffi::{InvocationResponse, InvocationResponseSuccess},
    ctypes::utils::{Encode, WithIr},
    raw_ptr_wrapper::RawPtrType,
};

pub type BamlObjectResponse = Result<BamlObjectResponseSuccess, String>;

pub struct BamlObjectResponseWrapper(pub BamlObjectResponse);

#[derive(Debug)]
pub enum BamlObjectResponseSuccess {
    Object(RawPtrType),
    Objects(Vec<RawPtrType>),
    Value(BamlValue),
}

impl BamlObjectResponseSuccess {
    pub fn new_object(object: RawPtrType) -> Self {
        Self::Object(object)
    }

    pub fn new_objects(objects: Vec<RawPtrType>) -> Self {
        Self::Objects(objects)
    }

    pub fn new_value(value: BamlValue) -> Self {
        Self::Value(value)
    }
}

// BamlValue uses type_meta::IR for its types when encoding
impl<'a, TypeLookups> Encode<InvocationResponse>
    for WithIr<'a, BamlObjectResponse, TypeLookups, type_meta::IR>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    fn encode(self) -> InvocationResponse {
        use crate::baml::cffi::{
            invocation_response::Response as cResponse,
            invocation_response_success::Result as cResult,
        };

        // For BamlValue, we use to_type_ir() to get the type
        // For Object/Objects, the type is implicit/opaque in RawPtrType encoding

        match self.value {
            Ok(success) => InvocationResponse {
                response: Some(cResponse::Success(InvocationResponseSuccess {
                    result: Some(match success {
                        BamlObjectResponseSuccess::Object(object) => {
                            cResult::Object(object.clone().encode())
                        }
                        BamlObjectResponseSuccess::Objects(objects) => {
                            cResult::Objects(crate::baml::cffi::RepeatedBamlObjectHandle {
                                objects: objects.iter().map(|ptr| ptr.clone().encode()).collect(),
                            })
                        }
                        BamlObjectResponseSuccess::Value(value) => {
                            // Helper to get type IR
                            use baml_runtime::TypeIR;
                            use baml_types::BamlMediaType;

                            fn get_type_ir(val: &BamlValue) -> TypeIR {
                                match val {
                                    BamlValue::Null => TypeIR::null(),
                                    BamlValue::Bool(_) => TypeIR::bool(),
                                    BamlValue::Int(_) => TypeIR::int(),
                                    BamlValue::Float(_) => TypeIR::float(),
                                    BamlValue::String(_) => TypeIR::string(),
                                    BamlValue::Map(index_map) => TypeIR::map(
                                        TypeIR::string(),
                                        TypeIR::union(
                                            index_map.values().map(get_type_ir).collect(),
                                        ),
                                    ),
                                    BamlValue::List(baml_values) => TypeIR::list(TypeIR::union(
                                        baml_values.iter().map(get_type_ir).collect(),
                                    )),
                                    BamlValue::Media(baml_media) => match baml_media.media_type {
                                        BamlMediaType::Image => TypeIR::image(),
                                        BamlMediaType::Audio => TypeIR::audio(),
                                        BamlMediaType::Pdf => TypeIR::pdf(),
                                        BamlMediaType::Video => TypeIR::video(),
                                    },
                                    BamlValue::Enum(name, _) => TypeIR::r#enum(name),
                                    BamlValue::Class(name, _) => TypeIR::class(name),
                                }
                            }

                            let curr_type = get_type_ir(value);

                            cResult::Value(
                                WithIr {
                                    value,
                                    lookup: self.lookup,
                                    mode: self.mode,
                                    curr_type,
                                }
                                .encode(),
                            )
                        }
                    }),
                })),
            },
            Err(error) => InvocationResponse {
                response: Some(cResponse::Error(error.clone())),
            },
        }
    }
}

// Implement EncodeToBuffer for BamlObjectResponseWrapper manually because it doesn't satisfy the blanket impl
// (it doesn't implement HasType)
use crate::ctypes::utils::EncodeToBuffer;

impl<TypeLookups> EncodeToBuffer<InvocationResponse, TypeLookups, type_meta::IR>
    for BamlObjectResponseWrapper
where
    TypeLookups: baml_types::baml_value::TypeLookups,
{
    fn encode_to_c_buffer(&self, lookup: &TypeLookups, mode: baml_types::StreamingMode) -> Vec<u8> {
        // We use type_meta::IR because that's what BamlValue uses
        // 1. Build the IR & convert to the prost message --------------------
        let msg: InvocationResponse = WithIr {
            value: &self.0,
            lookup,
            mode,
            curr_type: baml_types::ir_type::TypeGeneric::top(), // Dummy type, not used for top level Result
        }
        .encode();

        // 2. Prost-encode into a Vec<u8> ------------------------------------
        use prost::Message;
        msg.encode_to_vec()
    }
}

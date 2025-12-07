use baml_types::BamlValue;

use crate::{
    baml::cffi::{InvocationResponse, InvocationResponseSuccess},
    ctypes::utils::{Encode, WithIr},
    raw_ptr_wrapper::RawPtrType,
};

pub type BamlObjectResponse = Result<BamlObjectResponseSuccess, String>;

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

impl<'a, TypeLookups> Encode<InvocationResponse> for WithIr<'a, BamlObjectResponse, TypeLookups>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    fn encode(self) -> InvocationResponse {
        use crate::baml::cffi::{
            invocation_response::Response as cResponse,
            invocation_response_success::Result as cResult,
        };

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
                        BamlObjectResponseSuccess::Value(value) => cResult::Value(
                            WithIr {
                                value,
                                lookup: self.lookup,
                                mode: self.mode,
                            }
                            .encode(),
                        ),
                    }),
                })),
            },
            Err(error) => InvocationResponse {
                response: Some(cResponse::Error(error.clone())),
            },
        }
    }
}

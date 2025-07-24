use baml_types::BamlValue;

use crate::{
    baml::cffi::{CffiObjectResponse, CffiObjectResponseError, CffiObjectResponseSuccess},
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

impl<'a, TypeLookups> Encode<CffiObjectResponse> for WithIr<'a, BamlObjectResponse, TypeLookups>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    fn encode(self) -> CffiObjectResponse {
        use crate::baml::cffi::{
            cffi_object_response::Response as cResponse,
            cffi_object_response_success::Result as cResult,
        };

        match self.value {
            Ok(success) => CffiObjectResponse {
                response: Some(cResponse::Success(CffiObjectResponseSuccess {
                    result: Some(match success {
                        BamlObjectResponseSuccess::Object(object) => {
                            cResult::Object(object.clone().encode())
                        }
                        BamlObjectResponseSuccess::Objects(objects) => {
                            cResult::Objects(crate::baml::cffi::MultipleRawObjectResponse {
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
            Err(error) => CffiObjectResponse {
                response: Some(cResponse::Error(CffiObjectResponseError {
                    error: error.clone(),
                })),
            },
        }
    }
}

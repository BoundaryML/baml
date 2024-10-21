use anyhow::Result;
use baml_types::{BamlMedia, BamlMediaContent, BamlMediaType, MediaBase64, MediaUrl};
use pyo3::{
    types::{PyAnyMethods, PyModule, PyType},
    Bound, Py, PyAny, PyObject, PyResult, Python, ToPyObject,
};
use serde::{Deserialize, Serialize};

/// We rely on the serialization and deserialization of this struct for:
///
/// - pydantic serialization (JSON->FastAPI->Pydantic->baml_py), so that
///   users can include BAML types directly in their user-facing requests
#[derive(Debug, Serialize, Deserialize)]
pub struct UserFacingBamlMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "media_type")]
    pub mime_type: Option<String>,
    #[serde(flatten)]
    pub content: UserFacingBamlMediaContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserFacingBamlMediaContent {
    Url { url: String },
    Base64 { base64: String },
}

impl UserFacingBamlMedia {
    pub fn to_baml_media(self, media_type: BamlMediaType) -> BamlMedia {
        BamlMedia {
            media_type,
            mime_type: self.mime_type,
            content: match self.content {
                UserFacingBamlMediaContent::Url { url } => BamlMediaContent::Url(MediaUrl { url }),
                UserFacingBamlMediaContent::Base64 { base64 } => {
                    BamlMediaContent::Base64(MediaBase64 { base64 })
                }
            },
        }
    }
}

impl TryInto<UserFacingBamlMedia> for &BamlMedia {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<UserFacingBamlMedia> {
        Ok(UserFacingBamlMedia {
            mime_type: self.mime_type.clone(),
            content: match &self.content {
                BamlMediaContent::Url(url) => UserFacingBamlMediaContent::Url {
                    url: url.url.clone(),
                },
                BamlMediaContent::Base64(base64) => UserFacingBamlMediaContent::Base64 {
                    base64: base64.base64.clone(),
                },
                BamlMediaContent::File(_) => {
                    anyhow::bail!("Cannot convert file media to user facing media")
                }
            },
        })
    }
}

/// This function is used for Pydantic compatibility in three ways:
///
///   - allows constructing Pydantic models containing a BamlImagePy instance
///   - allows FastAPI requests to deserialize BamlImagePy instances in JSON format
///   - allows serializing BamlImagePy instances in JSON format
pub fn __get_pydantic_core_schema__(
    _cls: Bound<'_, PyType>,
    _source_type: Bound<'_, PyAny>,
    _handler: Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    Python::with_gil(|py| {
        let code = r#"
from pydantic_core import core_schema, SchemaValidator

def deserialize(data):
    from baml_py.baml_py import BamlImagePy
    if isinstance(data, BamlImagePy):
        return data
    else:
        SchemaValidator(
            core_schema.union_schema([
                core_schema.model_fields_schema({
                    'url': core_schema.model_field(core_schema.str_schema()),
                    'media_type': core_schema.model_field(
                        core_schema.with_default_schema(
                            core_schema.union_schema([
                                core_schema.str_schema(),
                                core_schema.none_schema(),
                            ]),
                            default=None,
                        ),
                    ),
                }),
                core_schema.model_fields_schema({
                    'base64': core_schema.model_field(core_schema.str_schema()),
                    'media_type': core_schema.model_field(
                        core_schema.with_default_schema(
                            core_schema.union_schema([
                                core_schema.str_schema(),
                                core_schema.none_schema(),
                            ]),
                            default=None,
                        ),
                    ),
                }),
            ])
        ).validate_python(data)
        return BamlImagePy.baml_deserialize(data)

def get_schema():
    return core_schema.no_info_after_validator_function(
        deserialize,
        core_schema.any_schema(),
        serialization=core_schema.plain_serializer_function_ser_schema(
            lambda v: v.baml_serialize(),
        )
    )

ret = get_schema()
    "#;
        // py.run(code, None, Some(ret_dict));
        let fun: Py<PyAny> = PyModule::from_code_bound(py, code, "", "")?
            .getattr("ret")?
            .into();
        Ok(fun.to_object(py))
    })
}

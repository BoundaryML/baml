use baml_cffi_macros::export_baml_fn;
use baml_types::BamlValue;

use super::{BamlObjectResponse, BamlObjectResponseSuccess, CallMethod};
use crate::raw_ptr_wrapper::MediaWrapper;

#[export_baml_fn]
impl MediaWrapper {
    #[export_baml_fn]
    fn media_type(&self) -> Result<BamlValue, String> {
        Ok(BamlValue::Enum(
            "MediaType".into(),
            self.inner.media_type.to_string(),
        ))
    }

    #[export_baml_fn]
    fn mime_type(&self) -> Result<Option<BamlValue>, String> {
        match self.inner.mime_type.as_ref() {
            Some(mime_type) => Ok(Some(BamlValue::String(mime_type.to_string()))),
            None => Ok(None),
        }
    }

    #[export_baml_fn]
    fn is_url(&self) -> bool {
        matches!(self.inner.content, baml_types::BamlMediaContent::Url(_))
    }

    #[export_baml_fn]
    fn is_base64(&self) -> bool {
        matches!(self.inner.content, baml_types::BamlMediaContent::Base64(_))
    }

    #[export_baml_fn]
    fn as_url(&self) -> Result<Option<BamlValue>, String> {
        match &self.inner.content {
            baml_types::BamlMediaContent::Url(media_url) => {
                Ok(Some(BamlValue::String(media_url.url.clone())))
            }
            baml_types::BamlMediaContent::File(_) | baml_types::BamlMediaContent::Base64(_) => {
                Ok(None)
            }
        }
    }

    #[export_baml_fn]
    fn as_base64(&self) -> Result<Option<BamlValue>, String> {
        match &self.inner.content {
            baml_types::BamlMediaContent::Base64(media_base64) => {
                Ok(Some(BamlValue::String(media_base64.base64.clone())))
            }
            baml_types::BamlMediaContent::File(_) | baml_types::BamlMediaContent::Url(_) => {
                Ok(None)
            }
        }
    }
}

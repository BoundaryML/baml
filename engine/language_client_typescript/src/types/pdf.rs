use napi::bindgen_prelude::External;
use napi_derive::napi;
use serde_json::json;

use crate::errors::invalid_argument_error;

crate::lang_wrapper!(BamlPdf, baml_types::BamlMedia);

#[napi]
impl BamlPdf {
    #[napi(ts_return_type = "BamlPdf")]
    pub fn from_url(url: String) -> External<BamlPdf> {
        let pdf = BamlPdf {
            inner: baml_types::BamlMedia::url(
                baml_types::BamlMediaType::Pdf,
                url,
                Some("application/pdf".to_string()),
            ),
        };
        External::new(pdf)
    }

    #[napi(ts_return_type = "BamlPdf")]
    pub fn from_base64(base64: String) -> External<BamlPdf> {
        let pdf = BamlPdf {
            inner: baml_types::BamlMedia::base64(
                baml_types::BamlMediaType::Pdf,
                base64,
                Some("application/pdf".to_string()),
            ),
        };
        External::new(pdf)
    }

    #[napi(getter)]
    pub fn url(&self) -> napi::Result<Option<String>> {
        match &self.inner.content {
            baml_types::BamlMediaContent::Url(url) => Ok(Some(url.url.clone())),
            _ => Ok(None),
        }
    }

    #[napi]
    pub fn as_url(&self) -> napi::Result<String> {
        match &self.inner.content {
            baml_types::BamlMediaContent::Url(url) => Ok(url.url.clone()),
            _ => Err(invalid_argument_error("Pdf is not a URL")),
        }
    }

    #[napi(js_name = "isUrl")]
    pub fn is_url(&self) -> bool {
        matches!(&self.inner.content, baml_types::BamlMediaContent::Url(_))
    }

    #[napi(ts_return_type = "[string, string]")]
    pub fn as_base64(&self) -> napi::Result<Vec<String>> {
        match &self.inner.content {
            baml_types::BamlMediaContent::Base64(base64) => Ok(vec![
                base64.base64.clone(),
                self.inner
                    .mime_type
                    .clone()
                    .unwrap_or("application/pdf".to_string()),
            ]),
            _ => Err(invalid_argument_error("Pdf is not base64")),
        }
    }

    #[napi(js_name = "toJSON")]
    pub fn to_json(&self) -> napi::Result<serde_json::Value> {
        Ok(match &self.inner.content {
            baml_types::BamlMediaContent::Url(url) => json!({
                "url": url.url
            }),
            baml_types::BamlMediaContent::Base64(base64) => json!({
                "base64": base64.base64,
                "media_type": self.inner.mime_type.clone().unwrap_or("".to_string())
            }),
            _ => "Unknown BamlPdf variant".into(),
        })
    }
}

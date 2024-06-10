use napi::bindgen_prelude::External;
use napi_derive::napi;
use serde_json::json;

crate::lang_wrapper!(BamlImage, baml_types::BamlImage);

#[napi]
impl BamlImage {
    #[napi(ts_return_type = "BamlImage")]
    pub fn from_url(url: String) -> External<BamlImage> {
        External::new(Self {
            inner: baml_types::BamlImage::from_url(url),
        })
    }

    #[napi(ts_return_type = "BamlImage")]
    pub fn from_base64(media_type: String, base64: String) -> External<BamlImage> {
        External::new(Self {
            inner: baml_types::BamlImage::from_base64(media_type, base64),
        })
    }

    pub fn is_url(&self) -> bool {
        self.inner.is_url()
    }

    pub fn is_base64(&self) -> bool {
        self.inner.is_base64()
    }

    pub fn as_url(&self) -> napi::Result<String> {
        self.inner
            .as_url()
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
    }

    #[napi(ts_return_type = "[string, string]")]
    pub fn as_base64(&self) -> napi::Result<Vec<String>> {
        self.inner
            .as_base64()
            .map(|baml_types::ImageBase64 { media_type, base64 }| vec![base64, media_type])
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
    }

    #[napi(js_name = "toJSON")]
    pub fn to_json(&self) -> napi::Result<serde_json::Value> {
        Ok(match &self.inner {
            baml_types::BamlImage::Url(url) => json!({
                "url": url.url
            }),
            baml_types::BamlImage::Base64(base64) => json!({
                "base64": base64.base64,
                "media_type": base64.media_type
            }),
        })
    }
}

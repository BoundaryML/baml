use napi_derive::napi;
use serde_json::json;

crate::lang_wrapper!(BamlImagePy, baml_types::BamlImage);

#[napi]
impl BamlImagePy {
    #[napi(factory)]
    pub fn from_url(url: String) -> Self {
        BamlImagePy {
            inner: baml_types::BamlImage::Url(baml_types::ImageUrl::new(url)),
        }
    }

    #[napi(factory)]
    pub fn from_base64(media_type: String, base64: String) -> Self {
        BamlImagePy {
            inner: baml_types::BamlImage::Base64(baml_types::ImageBase64::new(base64, media_type)),
        }
    }

    #[napi(js_name = "isUrl")]
    pub fn is_url(&self) -> bool {
        matches!(&self.inner, baml_types::BamlImage::Url(_))
    }

    #[napi(getter)]
    pub fn url(&self) -> napi::Result<String> {
        match &self.inner {
            baml_types::BamlImage::Url(url) => Ok(url.url.clone()),
            _ => Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Image is not a URL".to_string(),
            )),
        }
    }

    #[napi(getter)]
    pub fn base64(&self) -> napi::Result<Vec<String>> {
        match &self.inner {
            baml_types::BamlImage::Base64(base64) => {
                Ok(vec![base64.base64.clone(), base64.media_type.clone()])
            }
            _ => Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Image is not base64".to_string(),
            )),
        }
    }

    #[napi(js_name = "toJson")]
    pub fn to_json(&self) -> serde_json::Value {
        match &self.inner {
            baml_types::BamlImage::Url(url) => json!({
                "url": url.url
            }),
            baml_types::BamlImage::Base64(base64) => json!({
                "base64": base64.base64,
                "media_type": base64.media_type
            }),
        }
    }
}

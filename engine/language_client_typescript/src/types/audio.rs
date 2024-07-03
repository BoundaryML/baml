use baml_types::BamlMediaType;
use napi::bindgen_prelude::External;
use napi_derive::napi;
use serde_json::json;

crate::lang_wrapper!(BamlAudio, baml_types::BamlMedia);

#[napi]
impl BamlAudio {
    #[napi(ts_return_type = "BamlAudio")]
    pub fn from_url(url: String) -> External<BamlAudio> {
        let aud = BamlAudio {
            inner: baml_types::BamlMedia::Url(
                BamlMediaType::Audio,
                baml_types::MediaUrl::new(url, None),
            ),
        };
        External::new(aud)
    }

    #[napi(ts_return_type = "BamlAudio")]
    pub fn from_base64(media_type: String, base64: String) -> External<BamlAudio> {
        let aud = BamlAudio {
            inner: baml_types::BamlMedia::Base64(
                BamlMediaType::Audio,
                baml_types::MediaBase64::new(base64, media_type),
            ),
        };
        External::new(aud)
    }

    #[napi(js_name = "isUrl")]
    pub fn is_url(&self) -> bool {
        matches!(&self.inner, baml_types::BamlMedia::Url(_, _))
    }

    #[napi]
    pub fn as_url(&self) -> napi::Result<String> {
        match &self.inner {
            baml_types::BamlMedia::Url(BamlMediaType::Audio, url) => Ok(url.url.clone()),
            _ => Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Audio is not a URL".to_string(),
            )),
        }
    }

    #[napi(ts_return_type = "[string, string]")]
    pub fn as_base64(&self) -> napi::Result<Vec<String>> {
        match &self.inner {
            baml_types::BamlMedia::Base64(BamlMediaType::Audio, base64) => {
                Ok(vec![base64.base64.clone(), base64.media_type.clone()])
            }
            _ => Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Audio is not base64".to_string(),
            )),
        }
    }

    #[napi(js_name = "toJSON")]
    pub fn to_json(&self) -> napi::Result<serde_json::Value> {
        Ok(match &self.inner {
            baml_types::BamlMedia::Url(BamlMediaType::Audio, url) => json!({
                "url": url.url
            }),
            baml_types::BamlMedia::Base64(BamlMediaType::Audio, base64) => json!({
                "base64": base64.base64,
                "media_type": base64.media_type
            }),
            _ => format!("Unknown BamlAudioPy variant").into(),
        })
    }
}

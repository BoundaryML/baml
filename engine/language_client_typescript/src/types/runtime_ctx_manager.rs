use baml_types::BamlValue;
use napi_derive::napi;

crate::lang_wrapper!(RuntimeContextManager, baml_runtime::RuntimeContextManager);

#[napi]
impl RuntimeContextManager {
    #[napi]
    pub fn upsert_tags(&self, tags: serde_json::Value) -> napi::Result<()> {
        let tags: Result<BamlValue, serde_json::Error> = serde_json::from_value(tags);

        let Ok(tags) = tags else {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Invalid tags",
            ));
        };

        let Some(tags) = tags.as_map_owned() else {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Invalid tags",
            ));
        };

        self.inner.upsert_tags(tags);
        Ok(())
    }

    #[napi]
    pub fn deep_clone(&self) -> Self {
        RuntimeContextManager {
            inner: self.inner.deep_clone(),
        }
    }
}

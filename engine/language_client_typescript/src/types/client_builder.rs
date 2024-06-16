use std::borrow::BorrowMut;

use baml_runtime::client_builder;
use baml_types::BamlValue;
use napi::Env;
use napi::JsObject;
use napi_derive::napi;

use crate::parse_ts_types;

crate::lang_wrapper!(ClientBuilder, client_builder::ClientBuilder);

#[napi]
impl ClientBuilder {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: client_builder::ClientBuilder::new().into(),
        }
    }

    #[napi]
    pub fn add_client(
        &mut self,
        env: Env,
        name: String,
        provider: String,
        #[napi(ts_arg_type = "{ [string]: any }")] options: JsObject,
        retry_policy: Option<String>,
    ) -> napi::Result<()> {
        let args = parse_ts_types::js_object_to_baml_value(env, options)?;
        if !args.is_map() {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                format!(
                    "Invalid options: Expected a map of arguments, got: {}",
                    args.r#type()
                ),
            ));
        }
        let args_map = args.as_map_owned().unwrap();

        let client_property = baml_runtime::client_builder::ClientProperty {
            name,
            provider,
            retry_policy,
            options: args_map,
        };

        self.inner.add_client(client_property);
        Ok(())
    }

    #[napi]
    pub fn set_primary(&mut self, primary: String) {
        self.inner.set_primary(primary);
    }
}

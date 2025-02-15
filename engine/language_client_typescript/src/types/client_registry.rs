use std::str::FromStr;

use baml_runtime::client_registry;
use client_registry::ClientProvider;
use napi::Env;
use napi::JsObject;
use napi_derive::napi;

use crate::errors::invalid_argument_error;
use crate::parse_ts_types;

crate::lang_wrapper!(ClientRegistry, client_registry::ClientRegistry);

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl ClientRegistry {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: client_registry::ClientRegistry::new(),
        }
    }

    #[napi]
    pub fn add_llm_client(
        &mut self,
        env: Env,
        name: String,
        provider: String,
        #[napi(ts_arg_type = "{ [key: string]: any }")] options: JsObject,
        retry_policy: Option<String>,
    ) -> napi::Result<()> {
        let args = parse_ts_types::js_object_to_baml_value(env, options)?;
        if !args.is_map() {
            return Err(invalid_argument_error(&format!(
                "Invalid options: Expected a map of arguments, got: {}",
                args.r#type()
            )));
        }
        let args_map = args.as_map_owned().unwrap();

        let provider = match ClientProvider::from_str(&provider) {
            Ok(provider) => provider,
            Err(e) => {
                return Err(invalid_argument_error(&format!(
                    "Invalid provider: {:?}",
                    e
                )));
            }
        };

        let client_property =
            client_registry::ClientProperty::new(name, provider, retry_policy, args_map);

        self.inner.add_client(client_property);
        Ok(())
    }

    #[napi]
    pub fn set_primary(&mut self, primary: String) {
        self.inner.set_primary(primary);
    }
}

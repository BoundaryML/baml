use baml_runtime::client_registry;
use magnus::{class, method, Error, Module, RHash, Ruby};
use std::sync::{Arc, Mutex};

use crate::ruby_to_json;
use crate::Result;

#[magnus::wrap(class = "Baml::Ffi::ClientRegistry", free_immediately, size)]
struct ClientRegistry {
    // TODO(sam): this shouldn't need Ar
    inner: Arc<Mutex<client_registry::ClientRegistry>>,
}

impl ClientRegistry {
    // pub fn new() -> Self {
    //     Self {
    //         inner: client_registry::ClientRegistry::new(),
    //     }
    // }

    pub fn add_llm_client(
        ruby: &Ruby,
        rb_self: &ClientRegistry,
        name: String,
        provider: String,
        options: RHash,
        retry_policy: Option<String>,
    ) -> Result<()> {
        let options = match ruby_to_json::RubyToJson::convert_hash_to_json(options) {
            Ok(options) => options,
            Err(e) => {
                return Err(Error::new(
                    ruby.exception_syntax_error(),
                    format!("error while parsing call_function args:\n{}", e),
                ));
            }
        };

        let client_property = baml_runtime::client_registry::ClientProperty {
            name,
            provider,
            retry_policy,
            options,
        };

        rb_self.inner.lock().unwrap().add_client(client_property);
        Ok(())
    }

    pub fn set_primary(&self, primary: String) {
        self.inner.lock().unwrap().set_primary(primary);
    }

    pub fn define_in_ruby(module: &magnus::RModule) -> Result<()> {
        let cls = module.define_class("ClientRegistry", class::object())?;

        cls.define_method("add_llm_client", method!(ClientRegistry::add_llm_client, 4))?;
        cls.define_method("set_primary", method!(ClientRegistry::set_primary, 1))?;

        Ok(())
    }
}

use baml_runtime::client_registry;
use magnus::typed_data::Obj;
use magnus::{class, function, method, Error, Module, Object, RHash, Ruby};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use crate::ruby_to_json;
use crate::Result;

#[magnus::wrap(class = "Baml::Ffi::ClientRegistry", free_immediately, size)]
pub(crate) struct ClientRegistry {
    // This is the pattern suggeested in https://github.com/matsadler/magnus/blob/main/examples/mut_point.rs
    pub(crate) inner: RefCell<client_registry::ClientRegistry>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(client_registry::ClientRegistry::new()),
        }
    }

    pub fn add_llm_client(
        ruby: &Ruby,
        rb_self: &Self,
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

        rb_self.inner.borrow_mut().add_client(client_property);
        Ok(())
    }

    pub fn set_primary(&self, primary: String) {
        self.inner.borrow_mut().set_primary(primary);
    }

    pub fn define_in_ruby(module: &magnus::RModule) -> Result<()> {
        log::info!("Defining ClientRegistry in Ruby");
        let cls = module.define_class("ClientRegistry", class::object())?;

        cls.define_singleton_method("new", function!(ClientRegistry::new, 0))?;
        cls.define_method("add_llm_client", method!(ClientRegistry::add_llm_client, 4))?;
        cls.define_method("set_primary", method!(ClientRegistry::set_primary, 1))?;

        Ok(())
    }
}

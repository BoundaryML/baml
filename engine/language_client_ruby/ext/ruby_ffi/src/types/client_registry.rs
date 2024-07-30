use baml_runtime::client_registry;
use magnus::{
    class, function, method, scan_args::scan_args, Error, Module, Object, RHash, Ruby, Value,
};
use std::cell::RefCell;

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

    pub fn add_llm_client(ruby: &Ruby, rb_self: &Self, args: &[Value]) -> Result<()> {
        log::info!("add_llm_client called");
        let args = scan_args::<_, _, (), (), (), ()>(args)?;
        let (name, provider, options): (String, String, RHash) = args.required;
        let (retry_policy,): (Option<String>,) = args.optional;

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
        let cls = module.define_class("ClientRegistry", class::object())?;

        cls.define_singleton_method("new", function!(ClientRegistry::new, 0))?;
        cls.define_method(
            "add_llm_client",
            method!(ClientRegistry::add_llm_client, -1),
        )?;
        cls.define_method("set_primary", method!(ClientRegistry::set_primary, 1))?;

        Ok(())
    }
}

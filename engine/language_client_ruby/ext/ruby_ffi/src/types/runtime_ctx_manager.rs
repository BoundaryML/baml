use magnus::{class, prelude::*, RModule};

use crate::Result;

#[magnus::wrap(class = "Baml::Ffi::RuntimeContextManager", free_immediately, size)]
pub struct RuntimeContextManager {
    pub inner: baml_runtime::RuntimeContextManager,
}
impl RuntimeContextManager {
    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        module.define_class("RuntimeContextManager", class::object())?;

        //cls.define_method("upsert_tags", method!(RuntimeContextManager::upsert_tags, 1))?;
        //cls.define_method("deep_clone", method!(RuntimeContextManager::deep_clone, 0))?;

        Ok(())
    }
}

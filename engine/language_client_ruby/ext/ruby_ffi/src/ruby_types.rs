use magnus::{class, exception::runtime_error, method, prelude::*, value::Value, Error, RModule};

use crate::Result;

#[magnus::wrap(class = "Baml::Ffi::FunctionResult", free_immediately, size)]
pub struct FunctionResult {
    inner: baml_runtime::FunctionResult,
}

impl FunctionResult {
    pub fn new(inner: baml_runtime::FunctionResult) -> Self {
        Self { inner }
    }

    pub fn to_s(&self) -> String {
        format!("{:#?}", self.inner)
    }

    pub fn raw(&self) -> Result<String> {
        let Some(content) = self.inner.content() else {
            return Err(Error::new(
                runtime_error(),
                "never received a response from the LLM",
            ));
        };
        Ok(content.to_string())
    }

    pub fn parsed(&self) -> Result<Value> {
        let Some(value) = self.inner.parsed() else {
            return Err(Error::new(
                runtime_error(),
                "Failed to parse the LLM response",
            ));
        };
        serde_magnus::serialize(value)
    }

    /// For usage in magnus::init
    ///
    /// TODO: use traits and macros to implement this
    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("FunctionResult", class::object())?;

        cls.define_method("to_s", method!(FunctionResult::to_s, 0))?;
        cls.define_method("raw", method!(FunctionResult::raw, 0))?;
        cls.define_method("parsed", method!(FunctionResult::parsed, 0))?;

        Ok(())
    }
}

pub fn define_types(rmod: &RModule) -> Result<()> {
    FunctionResult::define_in_ruby(rmod)?;

    Ok(())
}

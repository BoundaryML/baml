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
        format!("{}", self.inner)
    }

    pub fn raw(&self) -> Result<String> {
        match self.inner.content() {
            Ok(content) => Ok(content.to_string()),
            Err(e) => Err(Error::new(
                runtime_error(),
                format!("No LLM response: {}", self.inner),
            )),
        }
    }

    pub fn parsed(&self) -> Result<Value> {
        match self.inner.parsed_content() {
            Ok(parsed) => serde_magnus::serialize(parsed),
            Err(e) => Err(Error::new(
                runtime_error(),
                format!("Failed to parse LLM response: {}", self.inner),
            )),
        }
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

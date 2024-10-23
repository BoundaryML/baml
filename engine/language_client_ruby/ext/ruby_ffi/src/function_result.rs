use magnus::{
    class, exception::runtime_error, method, prelude::*, value::Value, Error, RModule, Ruby,
};

use crate::ruby_to_json;
use crate::Result;

#[magnus::wrap(class = "Baml::Ffi::FunctionResult", free_immediately, size)]
pub struct FunctionResult {
    inner: baml_runtime::FunctionResult,
}

impl FunctionResult {
    pub fn new(inner: baml_runtime::FunctionResult) -> Self {
        Self { inner }
    }

    #[allow(dead_code)]
    fn to_s(&self) -> String {
        format!("{}", self.inner)
    }

    #[allow(dead_code)]
    fn raw(&self) -> Result<String> {
        match self.inner.content() {
            Ok(content) => Ok(content.to_string()),
            Err(_) => Err(Error::new(
                runtime_error(),
                format!("No LLM response: {}", self.inner),
            )),
        }
    }

    pub fn parsed_using_types(
        ruby: &Ruby,
        rb_self: &FunctionResult,
        types: RModule,
    ) -> Result<Value> {
        match rb_self.inner.result_with_constraints_content() {
            Ok(parsed) => {
                ruby_to_json::RubyToJson::serialize_baml(ruby, types, parsed.clone())
                    .map_err(|e| {
                        magnus::Error::new(
                            ruby.exception_type_error(),
                            format!("failing inside parsed_using_types: {:?}", e),
                        )
                    })
            }
            Err(_) => Err(Error::new(
                ruby.exception_runtime_error(),
                format!("Failed to parse LLM response: {}", rb_self.inner),
            )),
        }
    }

    /// For usage in magnus::init
    ///
    /// TODO: use traits and macros to implement this
    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("FunctionResult", class::object())?;

        cls.define_method(
            "parsed_using_types",
            method!(FunctionResult::parsed_using_types, 1),
        )?;

        Ok(())
    }
}

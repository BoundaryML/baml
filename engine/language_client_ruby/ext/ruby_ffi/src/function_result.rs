use baml_types::BamlValue;
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
    fn raw(ruby: &Ruby, rb_self: &FunctionResult) -> Result<String> {
        match rb_self.inner.content() {
            Ok(content) => Ok(content.to_string()),
            Err(e) => Err(crate::baml_error(
                ruby,
                e,
                format!("No LLM response: {}", rb_self.inner),
            )),
        }
    }

    pub fn parsed_using_types(
        ruby: &Ruby,
        rb_self: &FunctionResult,
        types: RModule,
    ) -> Result<Value> {
        match rb_self.inner.parsed_content() {
            Ok(parsed) => {
                ruby_to_json::RubyToJson::serialize_baml(ruby, types, &BamlValue::from(parsed))
                    .map_err(|e| {
                        crate::baml_error(
                            ruby,
                            anyhow::Error::msg(format!("{:?}", e)),
                            "Failed to convert parsed content to Ruby type",
                        )
                    })
            }
            Err(e) => Err(crate::baml_error(
                ruby,
                e,
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

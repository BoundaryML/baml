use magnus::{class, exception::runtime_error, method, prelude::*, value::Value, Error, RModule};

type Result<T> = std::result::Result<T, magnus::Error>;

#[magnus::wrap(class = "Baml::FunctionResult", free_immediately, size)]
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

    pub fn parsed(&self) -> Result<Value> {
        let Some(value) = self.inner.parsed() else {
            return Err(Error::new(runtime_error(), "Failed to parse"));
        };
        serde_magnus::serialize(value)
    }

    /// For usage in magnus::init
    ///
    /// This should really be implemented using a combination of traits and macros but this will do
    pub fn ruby_define_self(rmod: &RModule) -> Result<()> {
        let cls = rmod.define_class("FunctionResult", class::object())?;

        cls.define_method("to_s", method!(FunctionResult::to_s, 0))?;
        cls.define_method("parsed", method!(FunctionResult::parsed, 0))?;

        Ok(())
    }
}

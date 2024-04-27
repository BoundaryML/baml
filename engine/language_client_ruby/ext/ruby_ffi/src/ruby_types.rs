use magnus::{
    class, error::RubyUnavailableError, exception::runtime_error, function, method, prelude::*,
    scan_args::get_kwargs, value::Value, Error, RHash, RModule,
};

type Result<T> = std::result::Result<T, magnus::Error>;

#[magnus::wrap(class = "Baml::FunctionResult", free_immediately, size)]
pub struct FunctionResult {
    inner: baml_runtime::FunctionResult,
}

impl FunctionResult {
    pub fn new(inner: baml_runtime::FunctionResult) -> Self {
        Self { inner }
    }

    pub fn parsed(&self) -> Result<Value> {
        let Some(ref opt) = self.inner.parsed else {
            return Err(Error::new(runtime_error(), "parsed is None"));
        };
        let (value, _dc) = match opt {
            Ok(ok) => ok,
            Err(err) => {
                return Err(Error::new(
                    runtime_error(),
                    format!("parsed is Err: {}", err),
                ))
            }
        };

        serde_magnus::serialize(&value)
    }

    /// For usage in magnus::init
    ///
    /// This should really be implemented using a combination of traits and macros but this will do
    pub fn define_in(rmod: &RModule) -> Result<()> {
        let cls = rmod.define_class("FunctionResult", class::object())?;

        cls.define_method("parsed", method!(FunctionResult::parsed, 0))?;

        Ok(())
    }
}

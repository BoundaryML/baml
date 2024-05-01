pub(crate) mod internal;

mod runtime;
mod runtime_interface;
mod types;

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

use runtime::InternalBamlRuntime;

pub use runtime_interface::*;
pub use types::*;

pub struct BamlRuntime {
    inner: InternalBamlRuntime,
}

impl BamlRuntime {
    /// Load a runtime from a directory
    pub fn from_directory(path: &PathBuf) -> Result<Self> {
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(path)?,
        })
    }

    pub fn internal(&self) -> &impl InternalRuntimeInterface {
        &self.inner
    }
}

impl RuntimeInterface for BamlRuntime {
    async fn run_test(
        &mut self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<TestResponse> {
        self.inner.run_test(function_name, test_name, ctx).await
    }

    async fn call_function(
        &mut self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        self.inner.call_function(function_name, params, ctx).await
    }
}

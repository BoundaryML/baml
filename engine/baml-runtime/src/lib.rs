pub(crate) mod internal;

mod macros;
mod runtime;
mod runtime_interface;
mod types;

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

use runtime::InternalBamlRuntime;

pub use runtime_interface::RuntimeInterface;
pub use types::*;

#[cfg(feature = "internal")]
pub use internal_baml_jinja::{BamlImage, ChatMessagePart, RenderedPrompt};
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;

#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{BamlImage, ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;

pub struct BamlRuntime {
    inner: InternalBamlRuntime,
}

impl BamlRuntime {
    /// Load a runtime from a directory
    #[cfg(feature = "disk")]
    pub fn from_directory(path: &PathBuf) -> Result<Self> {
        use runtime_interface::RuntimeConstructor;

        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(path)?,
        })
    }

    pub fn from_file_content(root_path: &str, files: &HashMap<String, String>) -> Result<Self> {
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_file_content(root_path, files)?,
        })
    }

    #[cfg(feature = "internal")]
    pub fn internal(&self) -> &impl InternalRuntimeInterface {
        &self.inner
    }

    #[cfg(feature = "internal")]
    pub fn internal_mut(&mut self) -> &mut impl InternalRuntimeInterface {
        &mut self.inner
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
        &self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        self.inner.call_function(function_name, params, ctx).await
    }
}

unsafe impl Send for BamlRuntime {}

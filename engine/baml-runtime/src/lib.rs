// #[cfg(all(feature = "wasm", feature = "no_wasm"))]
// compile_error!(
//     "The features 'wasm' and 'no_wasm' are mutually exclusive. You can only use one at a time."
// );

#[cfg(feature = "internal")]
pub mod internal;
#[cfg(not(feature = "internal"))]
pub(crate) mod internal;

mod macros;
mod runtime;
mod runtime_interface;
mod types;

use std::collections::HashMap;

use anyhow::Result;

use runtime::InternalBamlRuntime;

use runtime_interface::RuntimeConstructor;
pub use runtime_interface::RuntimeInterface;
pub use types::*;

use internal_baml_codegen::{GeneratorArgs, LanguageClientType};
use std::path::PathBuf;

#[cfg(feature = "internal")]
pub use internal_baml_jinja::{BamlImage, ChatMessagePart, RenderedPrompt};
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;

#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{BamlImage, ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;

pub use internal_baml_core::internal_baml_diagnostics::Diagnostics as DiagnosticsError;

pub struct BamlRuntime {
    inner: InternalBamlRuntime,
}

impl BamlRuntime {
    /// Load a runtime from a directory
    #[cfg(feature = "no_wasm")]
    pub fn from_directory(path: &PathBuf) -> Result<Self> {
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(path)?,
        })
    }

    pub fn from_file_content<T: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
    ) -> Result<Self> {
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_file_content(root_path, files)?,
        })
    }

    #[cfg(feature = "internal")]
    pub fn internal(&self) -> &impl InternalRuntimeInterface {
        &self.inner
    }
}

#[cfg(not(feature = "no_wasm"))]
type ResponseType<T> = core::result::Result<T, wasm_bindgen::JsValue>;
#[cfg(feature = "no_wasm")]
type ResponseType<T> = anyhow::Result<T>;

impl RuntimeInterface for BamlRuntime {
    async fn run_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> ResponseType<TestResponse> {
        self.inner.run_test(function_name, test_name, ctx).await
    }

    async fn call_function(
        &self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> ResponseType<crate::FunctionResult> {
        self.inner.call_function(function_name, params, ctx).await
    }

    #[cfg(feature = "no_wasm")]
    fn generate_client(
        &self,
        client_type: &LanguageClientType,
        args: &GeneratorArgs,
    ) -> Result<()> {
        self.inner.generate_client(client_type, args)
    }
}

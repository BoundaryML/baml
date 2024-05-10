#[cfg(all(feature = "wasm", feature = "no_wasm"))]
compile_error!(
    "The features 'wasm' and 'no_wasm' are mutually exclusive. You can only use one at a time."
);

#[cfg(feature = "internal")]
pub mod internal;
#[cfg(not(feature = "internal"))]
pub(crate) mod internal;

#[cfg(feature = "no_wasm")]
mod cli;
mod macros;
mod request;
mod runtime;
mod runtime_interface;
pub mod tracing;
mod types;

use std::collections::HashMap;

use anyhow::Result;

use indexmap::IndexMap;
use runtime::InternalBamlRuntime;

#[cfg(feature = "no_wasm")]
pub use cli::CallerType;
use runtime_interface::ExerimentalTracingInterface;
use runtime_interface::RuntimeConstructor;
pub use runtime_interface::RuntimeInterface;
use tracing::{BamlTracer, TracingSpan};
pub use types::*;

use clap::Parser;
use internal_baml_codegen::{GeneratorArgs, LanguageClientType};
use std::path::PathBuf;

pub use internal_baml_jinja::BamlImage;
#[cfg(feature = "internal")]
pub use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;

#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;

pub use internal_baml_core::internal_baml_diagnostics::Diagnostics as DiagnosticsError;

pub struct BamlRuntime {
    inner: InternalBamlRuntime,
    tracer: BamlTracer,
}

impl BamlRuntime {
    /// Load a runtime from a directory
    #[cfg(feature = "no_wasm")]
    pub fn from_directory(path: &PathBuf, ctx: &RuntimeContext) -> Result<Self> {
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(path)?,
            tracer: BamlTracer::new(None, ctx),
        })
    }

    pub fn from_file_content<T: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        ctx: &RuntimeContext,
    ) -> Result<Self> {
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_file_content(root_path, files)?,
            tracer: BamlTracer::new(None, ctx),
        })
    }

    #[cfg(feature = "internal")]
    pub fn internal(&self) -> &impl InternalRuntimeInterface {
        &self.inner
    }

    #[cfg(feature = "no_wasm")]
    pub fn run_cli(argv: Vec<String>, caller_type: cli::CallerType) -> Result<()> {
        cli::RuntimeCli::parse_from(argv.into_iter()).run(caller_type)
    }
}

// #[cfg(not(feature = "no_wasm"))]
// type ResponseType<T> = core::result::Result<T, wasm_bindgen::JsValue>;
// #[cfg(feature = "no_wasm")]
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
        params: &IndexMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> ResponseType<crate::FunctionResult> {
        let span = self.tracer.start_span(&function_name, ctx, params, None);
        let response = self.inner.call_function(function_name, params, ctx).await;
        if let Some(span) = span {
            if let Err(e) = self.tracer.finish_baml_span(span, &response).await {
                log::debug!("Error during logging: {}", e);
            }
        }
        response
    }

    #[cfg(feature = "no_wasm")]
    fn generate_client(
        &self,
        client_type: &LanguageClientType,
        args: &GeneratorArgs,
    ) -> Result<internal_baml_codegen::GenerateOutput> {
        self.inner.generate_client(client_type, args)
    }
}

impl ExerimentalTracingInterface for BamlRuntime {
    fn start_span(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &IndexMap<String, serde_json::Value>,
    ) -> Option<TracingSpan> {
        self.tracer.start_span(function_name, ctx, params, None)
    }

    async fn finish_function_span(
        &self,
        span: TracingSpan,
        result: &Result<FunctionResult>,
    ) -> Result<()> {
        self.tracer.finish_baml_span(span, result).await
    }

    fn flush(&self) -> Result<()> {
        self.tracer.flush()
    }
}

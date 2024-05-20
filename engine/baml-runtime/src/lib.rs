#[cfg(all(test, feature = "no_wasm"))]
mod tests;

// #[cfg(all(feature = "wasm", feature = "no_wasm"))]
// compile_error!(
//     "The features 'wasm' and 'no_wasm' are mutually exclusive. You can only use one at a time."
// );

#[cfg(feature = "internal")]
pub mod internal;
#[cfg(not(feature = "internal"))]
pub(crate) mod internal;

#[cfg(feature = "no_wasm")]
mod cli;
mod macros;
mod request;
mod runtime;
pub mod runtime_interface;
pub mod tracing;
mod types;

use std::collections::HashMap;

use anyhow::Result;

use baml_types::BamlMap;
use baml_types::BamlValue;
use indexmap::IndexMap;
use runtime::InternalBamlRuntime;

#[cfg(feature = "no_wasm")]
pub use cli::CallerType;
use runtime_interface::ExperimentalTracingInterface;
pub use runtime_interface::PublicInterface;
use runtime_interface::RuntimeConstructor;
use runtime_interface::RuntimeInterface;
use tracing::{BamlTracer, TracingSpan};
pub use types::*;

use clap::Parser;

#[cfg(feature = "internal")]
pub use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;

#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;

pub use internal_baml_core::internal_baml_diagnostics::Diagnostics as DiagnosticsError;
pub use internal_baml_core::ir::{FieldType, TypeValue};

pub struct BamlRuntime {
    inner: InternalBamlRuntime,
    tracer: BamlTracer,
}

impl BamlRuntime {
    /// Load a runtime from a directory
    #[cfg(feature = "no_wasm")]
    pub fn from_directory(path: &std::path::PathBuf, ctx: &RuntimeContext) -> Result<Self> {
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

impl PublicInterface for BamlRuntime {
    async fn run_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: RuntimeContext,
    ) -> (Result<TestResponse>, Option<uuid::Uuid>) {
        let (span, ctx) = self.tracer.start_span(test_name, ctx, &Default::default());

        let params = self.inner.get_test_params(function_name, test_name, &ctx);

        let response = match params {
            Ok(params) => {
                let (response, function_span) = self
                    .call_function(function_name.into(), params.clone(), ctx)
                    .await;

                let response = response.map(|res| TestResponse {
                    function_response: res,
                    function_span,
                });

                response
            }
            Err(e) => Err(e),
        };

        let mut target_id = None;
        if let Some(span) = span {
            match self.tracer.finish_span(span, None).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        }

        (response, target_id)
    }

    async fn call_function(
        &self,
        function_name: String,
        params: IndexMap<String, BamlValue>,
        ctx: RuntimeContext,
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>) {
        let (span, ctx) = self.tracer.start_span(&function_name, ctx, &params);
        let response = self
            .inner
            .call_function_impl(function_name, params, ctx)
            .await;

        let mut target_id = None;
        if let Some(span) = span {
            match self.tracer.finish_baml_span(span, &response).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        }
        (response, target_id)
    }

    fn stream_function(
        &self,
        function_name: String,
        params: IndexMap<String, BamlValue>,
        ctx: RuntimeContext,
    ) -> Result<FunctionResultStream> {
        self.inner.stream_function_impl(function_name, params, ctx)
    }

    #[cfg(feature = "no_wasm")]
    fn generate_client(
        &self,
        client_type: &internal_baml_codegen::LanguageClientType,
        args: &internal_baml_codegen::GeneratorArgs,
    ) -> Result<internal_baml_codegen::GenerateOutput> {
        client_type.generate_client(self.inner.ir(), args)
    }
}

impl ExperimentalTracingInterface for BamlRuntime {
    fn start_span(
        &self,
        function_name: &str,
        ctx: RuntimeContext,
        params: &BamlMap<String, BamlValue>,
    ) -> (Option<TracingSpan>, RuntimeContext) {
        self.tracer.start_span(function_name, ctx, params)
    }

    async fn finish_function_span(
        &self,
        span: TracingSpan,
        result: &Result<FunctionResult>,
    ) -> Result<Option<uuid::Uuid>> {
        self.tracer.finish_baml_span(span, result).await
    }

    async fn finish_span(
        &self,
        span: TracingSpan,
        result: Option<BamlValue>,
    ) -> Result<Option<uuid::Uuid>> {
        self.tracer.finish_span(span, result).await
    }

    fn flush(&self) -> Result<()> {
        self.tracer.flush()
    }
}

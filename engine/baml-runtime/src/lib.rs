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

#[cfg(not(target_arch = "wasm32"))]
mod cli;
pub mod client_registry;
mod macros;
mod request;
mod runtime;
pub mod runtime_interface;
pub mod tracing;
pub mod type_builder;
mod types;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use baml_types::BamlMap;
use baml_types::BamlValue;
use client_registry::ClientRegistry;
use indexmap::IndexMap;
use internal_baml_core::configuration::GeneratorOutputType;
use on_log_event::LogEventCallbackSync;
use runtime::InternalBamlRuntime;

#[cfg(not(target_arch = "wasm32"))]
pub use cli::CallerType;
use runtime_interface::ExperimentalTracingInterface;
use runtime_interface::RuntimeConstructor;
use runtime_interface::RuntimeInterface;
use tracing::{BamlTracer, TracingSpan};
use type_builder::TypeBuilder;
pub use types::*;

use clap::Parser;

#[cfg(feature = "internal")]
pub use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;

#[cfg(feature = "internal")]
pub use internal_baml_core as internal_core;

#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;

pub use internal_baml_core::internal_baml_diagnostics::Diagnostics as DiagnosticsError;
pub use internal_baml_core::ir::{FieldType, IRHelper, TypeValue};

pub struct BamlRuntime {
    inner: InternalBamlRuntime,
    tracer: Arc<BamlTracer>,
    env_vars: HashMap<String, String>,
}

impl BamlRuntime {
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    /// Load a runtime from a directory
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_directory<T: AsRef<str>>(
        path: &std::path::PathBuf,
        env_vars: HashMap<T, T>,
    ) -> Result<Self> {
        let copy = env_vars
            .iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(path)?,
            tracer: BamlTracer::new(None, env_vars.into_iter())?.into(),
            env_vars: copy,
        })
    }

    pub fn from_file_content<T: AsRef<str>, U: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        env_vars: HashMap<U, U>,
    ) -> Result<Self> {
        let copy = env_vars
            .iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_file_content(root_path, files)?,
            tracer: BamlTracer::new(None, env_vars.into_iter())?.into(),
            env_vars: copy,
        })
    }

    #[cfg(feature = "internal")]
    pub fn internal(&self) -> &impl InternalRuntimeInterface {
        &self.inner
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_cli(argv: Vec<String>, caller_type: cli::CallerType) -> Result<()> {
        cli::RuntimeCli::parse_from(argv.into_iter()).run(caller_type)
    }

    pub fn create_ctx_manager(&self, language: BamlValue) -> RuntimeContextManager {
        let ctx = RuntimeContextManager::new_from_env_vars(self.env_vars.clone());
        let tags: HashMap<String, BamlValue> = [("baml.language", language)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ctx.upsert_tags(tags);
        ctx
    }
}

impl BamlRuntime {
    pub async fn run_test<F>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
    ) -> (Result<TestResponse>, Option<uuid::Uuid>)
    where
        F: Fn(FunctionResult) -> (),
    {
        log::info!("running test");
        let span = self.tracer.start_span(test_name, ctx, &Default::default());
        log::info!("got span");

        let response = match ctx.create_ctx(None, None) {
            Ok(rctx) => {
                let params = self.inner.get_test_params(function_name, test_name, &rctx);
                match params {
                    Ok(params) => {
                        match self.stream_function(function_name.into(), &params, ctx, None, None) {
                            Ok(mut stream) => {
                                let (response, span) = stream.run(on_event, ctx, None, None).await;
                                let response = response.map(|res| TestResponse {
                                    function_response: res,
                                    function_span: span,
                                });

                                response
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        };

        let mut target_id = None;
        if let Some(span) = span {
            #[cfg(not(target_arch = "wasm32"))]
            match self.tracer.finish_span(span, ctx, None) {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
            #[cfg(target_arch = "wasm32")]
            match self.tracer.finish_span(span, ctx, None).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        }

        (response, target_id)
    }

    pub async fn call_function(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>) {
        log::trace!("Calling function: {}", function_name);
        let span = self.tracer.start_span(&function_name, ctx, &params);
        log::trace!("Span started");
        let response = match ctx.create_ctx(tb, cb) {
            Ok(rctx) => {
                self.inner
                    .call_function_impl(function_name, params, rctx)
                    .await
            }
            Err(e) => Err(e),
        };

        let mut target_id = None;
        if let Some(span) = span {
            #[cfg(not(target_arch = "wasm32"))]
            match self.tracer.finish_baml_span(span, ctx, &response) {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
            #[cfg(target_arch = "wasm32")]
            match self.tracer.finish_baml_span(span, ctx, &response).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        }
        (response, target_id)
    }

    pub fn stream_function(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
    ) -> Result<FunctionResultStream> {
        self.inner.stream_function_impl(
            function_name,
            params,
            self.tracer.clone(),
            ctx.create_ctx(tb, cb)?,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn generate_client(
        &self,
        client_type: &GeneratorOutputType,
        args: &internal_baml_codegen::GeneratorArgs,
    ) -> Result<internal_baml_codegen::GenerateOutput> {
        use internal_baml_codegen::GenerateClient;

        client_type.generate_client(self.inner.ir(), args)
    }

    pub fn run_generators(
        &self,
        input_files: &IndexMap<PathBuf, String>,
    ) -> Result<Vec<internal_baml_codegen::GenerateOutput>> {
        use internal_baml_codegen::GenerateClient;

        let client_types: Vec<(GeneratorOutputType, internal_baml_codegen::GeneratorArgs)> = self
            .inner
            .ir()
            .configuration()
            .generators
            .iter()
            .map(|(generator, _)| {
                Ok((
                    generator.output_type.clone(),
                    internal_baml_codegen::GeneratorArgs::new(
                        generator.output_dir(),
                        generator.baml_src.clone(),
                        input_files.iter(),
                    )?,
                ))
            })
            .collect::<Result<_>>()?;

        client_types
            .iter()
            .map(|(client_type, args)| client_type.generate_client(self.inner.ir(), args))
            .collect()
    }
}

impl ExperimentalTracingInterface for BamlRuntime {
    fn start_span(
        &self,
        function_name: &str,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> Option<TracingSpan> {
        self.tracer.start_span(function_name, ctx, params)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_function_span(
        &self,
        span: Option<TracingSpan>,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_baml_span(span, ctx, result)
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_function_span(
        &self,
        span: Option<TracingSpan>,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_baml_span(span, ctx, result).await
        } else {
            Ok(None)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_span(
        &self,
        span: Option<TracingSpan>,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_span(span, ctx, result)
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_span(
        &self,
        span: Option<TracingSpan>,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_span(span, ctx, result).await
        } else {
            Ok(None)
        }
    }

    fn flush(&self) -> Result<()> {
        self.tracer.flush()
    }

    fn drain_stats(&self) -> InnerTraceStats {
        self.tracer.drain_stats()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_log_event_callback(&self, log_event_callback: LogEventCallbackSync) -> Result<()> {
        self.tracer.set_log_event_callback(log_event_callback);
        Ok(())
    }
}

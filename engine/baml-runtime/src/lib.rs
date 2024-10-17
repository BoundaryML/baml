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
pub mod cli;
pub mod client_registry;
pub mod errors;
mod macros;
pub mod request;
mod runtime;
pub mod runtime_interface;
pub mod tracing;
pub mod type_builder;
mod types;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;

use baml_types::BamlMap;
use baml_types::BamlValue;
use cfg_if::cfg_if;
use client_registry::ClientRegistry;
use indexmap::IndexMap;
use internal_baml_core::configuration::CloudProject;
use internal_baml_core::configuration::CodegenGenerator;
use internal_baml_core::configuration::Generator;
use internal_baml_core::configuration::GeneratorOutputType;
use on_log_event::LogEventCallbackSync;
use runtime::InternalBamlRuntime;
use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
pub use cli::RuntimeCliDefaults;
pub use runtime_context::BamlSrcReader;
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

#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;

pub use internal_baml_core::internal_baml_diagnostics;
pub use internal_baml_core::internal_baml_diagnostics::Diagnostics as DiagnosticsError;
pub use internal_baml_core::ir::{scope_diagnostics, FieldType, IRHelper, TypeValue};

#[cfg(not(target_arch = "wasm32"))]
static TOKIO_SINGLETON: OnceLock<std::io::Result<Arc<tokio::runtime::Runtime>>> = OnceLock::new();

pub struct BamlRuntime {
    pub(crate) inner: InternalBamlRuntime,
    tracer: Arc<BamlTracer>,
    env_vars: HashMap<String, String>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Arc<tokio::runtime::Runtime>,
}

impl BamlRuntime {
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn get_tokio_singleton() -> Result<Arc<tokio::runtime::Runtime>> {
        match TOKIO_SINGLETON.get_or_init(|| tokio::runtime::Runtime::new().map(Arc::new)) {
            Ok(t) => Ok(t.clone()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn parse_baml_src_path(path: impl Into<PathBuf>) -> Result<PathBuf> {
        let mut path: PathBuf = path.into();

        if !path.exists() {
            anyhow::bail!(
                "Expected --from '{}' to be a baml_src/ directory, but it does not exist",
                path.display()
            );
        }

        if !path.is_dir() {
            anyhow::bail!(
                "Expected --from '{}' to be a baml_src/ directory, but it is not",
                path.display()
            );
        }

        if path.file_name() != Some(std::ffi::OsStr::new("baml_src")) {
            let contained = path.join("baml_src");

            if contained.exists() && contained.is_dir() {
                path = contained;
            } else {
                anyhow::bail!(
                    "Expected --from '{}' to be a baml_src/ directory, but it is not",
                    path.display()
                );
            }
        }

        Ok(path)
    }

    /// Load a runtime from a directory
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_directory<T: AsRef<str>>(
        path: &std::path::PathBuf,
        env_vars: HashMap<T, T>,
    ) -> Result<Self> {
        let path = Self::parse_baml_src_path(path.clone())?;

        let copy = env_vars
            .iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(&path)?,
            tracer: BamlTracer::new(None, env_vars.into_iter())?.into(),
            env_vars: copy,
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime: Self::get_tokio_singleton()?,
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
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime: Self::get_tokio_singleton()?,
        })
    }

    #[cfg(feature = "internal")]
    pub fn internal(&self) -> &impl InternalRuntimeInterface {
        &self.inner
    }

    pub fn create_ctx_manager(
        &self,
        language: BamlValue,
        baml_src_reader: BamlSrcReader,
    ) -> RuntimeContextManager {
        let ctx = RuntimeContextManager::new_from_env_vars(self.env_vars.clone(), baml_src_reader);
        let tags: HashMap<String, BamlValue> = [("baml.language", language)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ctx.upsert_tags(tags);
        ctx
    }
}

impl BamlRuntime {
    pub fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<BamlMap<String, BamlValue>> {
        self.inner.get_test_params(function_name, test_name, ctx)
    }

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
        let span = self.tracer.start_span(test_name, ctx, &Default::default());

        let response = match ctx.create_ctx(None, None) {
            Ok(rctx) => {
                let params = self.get_test_params(function_name, test_name, &rctx);
                match params {
                    Ok(params) => match ctx.create_ctx(None, None) {
                        Ok(rctx_stream) => {
                            let stream = self.inner.stream_function_impl(
                                function_name.into(),
                                &params,
                                self.tracer.clone(),
                                rctx_stream,
                                #[cfg(not(target_arch = "wasm32"))]
                                self.async_runtime.clone(),
                            );
                            match stream {
                                Ok(mut stream) => {
                                    let (response, span) =
                                        stream.run(on_event, ctx, None, None).await;
                                    response.map(|res| TestResponse {
                                        function_response: res,
                                        function_span: span,
                                    })
                                }
                                Err(e) => Err(e),
                            }
                        }
                        Err(e) => Err(e),
                    },
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

    #[cfg(not(target_arch = "wasm32"))]
    pub fn call_function_sync(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>) {
        let fut = self.call_function(function_name, params, ctx, tb, cb);
        self.async_runtime.block_on(fut)
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
            #[cfg(not(target_arch = "wasm32"))]
            self.async_runtime.clone(),
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
}

// Interfaces for generators
impl BamlRuntime {
    /// Determine the file containing the generators.
    pub fn generator_path(&self) -> Option<PathBuf> {
        let path_counts: HashMap<&PathBuf, u32> = self
            .inner
            .ir()
            .configuration()
            .generators
            .iter()
            .filter_map(|generator| match generator {
                Generator::BoundaryCloud(generator) => Some(generator.span.file.path_buf()),
                Generator::Codegen(generator) => Some(generator.span.file.path_buf()),
            })
            .fold(HashMap::new(), |mut acc, path| {
                *acc.entry(path).or_default() += 1;
                acc
            });

        path_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(path, _)| path.clone())
    }

    pub fn cloud_projects(&self) -> Vec<&CloudProject> {
        self.inner
            .ir()
            .configuration()
            .generators
            .iter()
            .filter_map(|generator| match generator {
                Generator::BoundaryCloud(generator) => Some(generator),
                Generator::Codegen(_) => None,
            })
            .collect()
    }

    pub fn codegen_generators(&self) -> impl Iterator<Item = &CodegenGenerator> {
        self.inner
            .ir()
            .configuration()
            .generators
            .iter()
            .filter_map(|generator| match generator {
                Generator::Codegen(generator) => Some(generator),
                Generator::BoundaryCloud(_) => None,
            })
    }

    pub fn run_codegen(
        &self,
        input_files: &IndexMap<PathBuf, String>,
        no_version_check: bool,
    ) -> Result<Vec<internal_baml_codegen::GenerateOutput>> {
        use internal_baml_codegen::GenerateClient;

        let client_types: Vec<(&CodegenGenerator, internal_baml_codegen::GeneratorArgs)> = self
            .codegen_generators()
            .map(|generator| {
                Ok((
                    generator,
                    internal_baml_codegen::GeneratorArgs::new(
                        generator.output_dir(),
                        generator.baml_src.clone(),
                        input_files.iter(),
                        generator.version.clone(),
                        no_version_check,
                        generator.default_client_mode(),
                        generator.on_generate.clone(),
                    )?,
                ))
            })
            .collect::<Result<_>>()
            .context("Internal error: failed to collect generators")?;

        // VSCode / WASM can't run "on_generate", so if any generator specifies on_generate,
        // we disable codegen. (This can be super surprising behavior to someone, but we'll cross
        // that bridge when we get there)
        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                if client_types
                    .iter()
                    .any(|(g, _)| !g.on_generate.is_empty())
                {
                    // We could also return an error here, but what really matters is whether we message
                    // the user about it in VSCode. IMO the user shouldn't get a message about "vscode not
                    // generating code, run 'baml-cli dev' to generate code" because that's surprising
                    //
                    // We _could_ do something like "show that message the first time the user tries to
                    // codegen for rest/openapi", but that's overengineered, I think
                    return Ok(vec![]);
                }
            }
        }

        client_types
            .iter()
            .map(|(generator, args)| {
                generator
                    .output_type
                    .generate_client(self.inner.ir(), args)
                    .with_context(|| {
                        let ((line, col), _) = generator.span.line_and_column();
                        format!(
                            "Error while running generator defined at {}:{line}:{col}",
                            generator.span.file.path()
                        )
                    })
            })
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
    fn set_log_event_callback(
        &self,
        log_event_callback: Option<LogEventCallbackSync>,
    ) -> Result<()> {
        self.tracer.set_log_event_callback(log_event_callback);
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn baml_src_files(dir: &std::path::PathBuf) -> Result<Vec<PathBuf>> {
    static VALID_EXTENSIONS: [&str; 2] = ["baml", "json"];

    log::trace!("Reading files from {:#}", dir.to_string_lossy());

    if !dir.exists() {
        anyhow::bail!("{dir:#?} does not exist (expected a directory containing BAML files)",);
    }
    if dir.is_file() {
        return Err(anyhow::anyhow!(
            "{dir:#?} is a file, not a directory (expected a directory containing BAML files)",
        ));
    }
    if !dir.is_dir() {
        return Err(anyhow::anyhow!(
            "{dir:#?} is not a directory (expected a directory containing BAML files)",
        ));
    }

    let src_files = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| match e {
            Ok(e) => Some(e),
            Err(e) => {
                log::error!("Error while reading files from {dir:#?}: {e}");
                None
            }
        })
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let Some(ext) = e.path().extension() else {
                return false;
            };
            let Some(ext) = ext.to_str() else {
                return false;
            };
            VALID_EXTENSIONS.contains(&ext)
        })
        .map(|e| e.path().to_path_buf())
        .collect::<Vec<_>>();

    if !src_files
        .iter()
        .any(|f| f.extension() == Some("baml".as_ref()))
    {
        anyhow::bail!("no .baml files found in {dir:#?}");
    }

    Ok(src_files)
}

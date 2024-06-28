use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{
    internal::{
        ir_features::{IrFeatures, WithInternal},
        llm_client::{
            llm_provider::LLMProvider,
            orchestrator::{
                orchestrate_call, IterOrchestrator, LLMPrimitiveProvider, OrchestrationScope,
                OrchestratorNode,
            },
            retry_policy::CallablePolicy,
            traits::WithPrompt,
        },
        prompt_renderer::PromptRenderer,
    },
    runtime_interface::{InternalClientLookup, RuntimeConstructor},
    tracing::{BamlTracer, TracingSpan},
    type_builder::TypeBuilder,
    FunctionResult, FunctionResultStream, InternalRuntimeInterface, RuntimeContext,
    RuntimeContextManager, RuntimeInterface, TestResponse,
};
use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlValue};
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, FunctionWalker, IRHelper},
    validate,
};
use internal_baml_jinja::RenderedPrompt;

use super::InternalBamlRuntime;

impl<'a> InternalClientLookup<'a> for InternalBamlRuntime {
    // Gets a top-level client/strategy by name
    fn get_llm_provider(
        &'a self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>> {
        #[cfg(target_arch = "wasm32")]
        let mut clients = self.clients.lock().unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        let clients = &self.clients;

        if let Some(client) = clients.get(client_name) {
            return Ok(client.clone());
        } else {
            let walker = self
                .ir()
                .find_client(client_name)
                .context(format!("Could not find client with name: {}", client_name))?;
            let client = LLMProvider::try_from((&walker, ctx)).map(Arc::new)?;
            clients.insert(client_name.into(), client.clone());
            Ok(client)
        }
    }

    fn get_retry_policy(&self, policy_name: &str, _ctx: &RuntimeContext) -> Result<CallablePolicy> {
        #[cfg(target_arch = "wasm32")]
        let mut retry_policies = self.retry_policies.lock().unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        let retry_policies = &self.retry_policies;

        let inserter = || {
            self.ir()
                .walk_retry_policies()
                .find(|walker| walker.name() == policy_name)
                .ok_or_else(|| {
                    anyhow::anyhow!("Could not find retry policy with name: {}", policy_name)
                })
                .map(CallablePolicy::from)
        };

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(policy_ref) = retry_policies.get(policy_name) {
                return Ok(policy_ref.clone());
            }
            let new_policy = inserter()?;
            retry_policies.insert(policy_name.into(), new_policy.clone());
            Ok(new_policy)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let policy_ref = retry_policies
                .entry(policy_name.into())
                .or_try_insert_with(inserter)?;
            Ok(policy_ref.value().clone())
        }
    }
}

impl InternalRuntimeInterface for InternalBamlRuntime {
    fn diagnostics(&self) -> &internal_baml_core::internal_baml_diagnostics::Diagnostics {
        &self.diagnostics
    }

    fn orchestration_graph(
        &self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Vec<OrchestratorNode>> {
        let client = self.get_llm_provider(client_name, ctx)?;
        Ok(client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self))
    }

    fn features(&self) -> IrFeatures {
        WithInternal::features(self)
    }

    fn render_prompt(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope)> {
        let func = self.get_function(function_name, ctx)?;
        let baml_args = self.ir().check_function_params(&func, params, false)?;

        let renderer = PromptRenderer::from_function(&func, &self.ir(), ctx)?;
        let client_name = renderer.client_name().to_string();

        let client = self.get_llm_provider(&client_name, ctx)?;
        let mut selected =
            client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self);
        let node_index = node_index.unwrap_or(0);

        if node_index >= selected.len() {
            return Err(anyhow::anyhow!(
                "Execution Node out of bounds: {} >= {} for client {}",
                node_index,
                selected.len(),
                client_name
            ));
        }

        let node = selected.swap_remove(node_index);
        return node
            .provider
            .render_prompt(self.ir(), &renderer, ctx, &baml_args)
            .map(|prompt| (prompt, node.scope));
    }

    async fn render_raw_curl(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        prompt: &Vec<internal_baml_jinja::RenderedChatMessage>,
        stream: bool,
        node_index: Option<usize>,
    ) -> Result<String> {
        let func = self.get_function(function_name, ctx)?;

        let renderer = PromptRenderer::from_function(&func, &self.ir(), ctx)?;
        let client_name = renderer.client_name().to_string();

        let client = self.get_llm_provider(&client_name, ctx)?;
        let mut selected =
            client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self);

        let node_index = node_index.unwrap_or(0);

        if node_index >= selected.len() {
            return Err(anyhow::anyhow!(
                "Execution Node out of bounds: {} >= {} for client {}",
                node_index,
                selected.len(),
                client_name
            ));
        }

        let node = selected.swap_remove(node_index);
        return node.provider.render_raw_curl(ctx, prompt, stream).await;
    }

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
        _ctx: &RuntimeContext,
    ) -> Result<FunctionWalker<'ir>> {
        let walker = self.ir().find_function(function_name)?;
        Ok(walker)
    }

    fn ir(&self) -> &IntermediateRepr {
        use std::ops::Deref;
        &self.ir.deref()
    }

    fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<BamlMap<String, BamlValue>> {
        let func = self.get_function(function_name, ctx)?;
        let test = self.ir().find_test(&func, test_name)?;

        match test.test_case_params(&ctx.env) {
            Ok(params) => {
                // Collect all errors and return them as a single error.
                let mut errors = Vec::new();
                let params = params
                    .into_iter()
                    .map(|(k, v)| match v {
                        Ok(v) => (k, v),
                        Err(e) => {
                            errors.push(e);
                            (k, BamlValue::Null)
                        }
                    })
                    .collect::<BamlMap<_, _>>();

                if !errors.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Unable to resolve test params: {:?}",
                        errors
                    ));
                }

                let baml_args = self.ir().check_function_params(&func, &params, true)?;
                Ok(baml_args.as_map_owned().unwrap())
            }
            Err(e) => return Err(anyhow::anyhow!("Unable to resolve test params: {:?}", e)),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn baml_src_files(dir: &std::path::PathBuf) -> Result<Vec<PathBuf>> {
    static VALID_EXTENSIONS: [&str; 2] = ["baml", "json"];

    log::info!("Reading files from {:#}", dir.to_string_lossy());

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

impl RuntimeConstructor for InternalBamlRuntime {
    fn from_file_content<T: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
    ) -> Result<InternalBamlRuntime> {
        let contents = files
            .iter()
            .map(|(path, contents)| {
                Ok(SourceFile::from((
                    PathBuf::from(path.as_ref()),
                    contents.as_ref().to_string(),
                )))
            })
            .collect::<Result<Vec<_>>>()?;
        let directory = PathBuf::from(root_path);
        let mut schema = validate(&PathBuf::from(directory), contents);
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;
        log::info!("Successfully loaded BAML schema");
        log::info!("Diagnostics: {:#?}", schema.diagnostics);

        Ok(Self {
            ir: Arc::new(ir),
            diagnostics: schema.diagnostics,
            clients: Default::default(),
            retry_policies: Default::default(),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from_directory(dir: &std::path::PathBuf) -> Result<InternalBamlRuntime> {
        InternalBamlRuntime::from_files(dir, baml_src_files(dir)?)
    }
}

impl RuntimeInterface for InternalBamlRuntime {
    async fn call_function_impl(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        let func = self.get_function(&function_name, &ctx)?;
        let baml_args = self.ir().check_function_params(&func, &params, false)?;

        let renderer = PromptRenderer::from_function(&func, self.ir(), &ctx)?;
        let client_name = renderer.client_name().to_string();
        let orchestrator = self.orchestration_graph(&client_name, &ctx)?;

        // Now actually execute the code.
        let (history, _) =
            orchestrate_call(orchestrator, self.ir(), &ctx, &renderer, &baml_args, |s| {
                renderer.parse(s, false)
            })
            .await;

        FunctionResult::new_chain(history)
    }

    fn stream_function_impl(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        tracer: Arc<BamlTracer>,
        ctx: RuntimeContext,
    ) -> Result<FunctionResultStream> {
        let func = self.get_function(&function_name, &ctx)?;
        let renderer = PromptRenderer::from_function(&func, self.ir(), &ctx)?;
        let client_name = renderer.client_name().to_string();
        let orchestrator = self.orchestration_graph(&client_name, &ctx)?;
        let Some(baml_args) = self
            .ir
            .check_function_params(&func, &params, false)?
            .as_map_owned()
        else {
            anyhow::bail!("Expected parameters to be a map for: {}", function_name);
        };
        Ok(FunctionResultStream {
            function_name,
            ir: self.ir.clone(),
            params: baml_args,
            orchestrator,
            tracer,
            renderer,
        })
    }
}

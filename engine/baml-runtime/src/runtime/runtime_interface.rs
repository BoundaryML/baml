use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{
    internal::{
        ir_features::{IrFeatures, WithInternal},
        llm_client::{
            llm_provider::LLMProvider,
            orchestrator::{
                orchestrate, IterOrchestrator, LLMPrimitiveProvider, OrchestrationScope,
                OrchestratorNode,
            },
            retry_policy::CallablePolicy,
            traits::WithPrompt,
        },
        prompt_renderer::PromptRenderer,
    },
    runtime_interface::{InternalClientLookup, RuntimeConstructor},
    tracing::{BamlTracer, TracingSpan},
    FunctionResult, FunctionResultStream, InternalRuntimeInterface, RuntimeContext,
    RuntimeInterface, TestResponse,
};
use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlValue};
use dashmap::DashMap;
use indexmap::IndexMap;
use internal_baml_codegen::{GeneratorArgs, LanguageClientType};

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
    ) -> Result<dashmap::mapref::one::Ref<String, Arc<LLMProvider>>> {
        if let Some(client) = self.clients.get(client_name) {
            return Ok(client);
        } else {
            let walker = self
                .ir()
                .find_client(client_name)
                .context(format!("Could not find client with name: {}", client_name))?;
            let client = LLMProvider::try_from((&walker, ctx)).map(Arc::new)?;
            self.clients.insert(client_name.into(), client.clone());
            Ok(self.clients.get(client_name).unwrap())
        }
    }

    fn get_retry_policy(&self, policy_name: &str, _ctx: &RuntimeContext) -> Result<CallablePolicy> {
        let policy_ref = self
            .retry_policies
            .entry(policy_name.into())
            .or_try_insert_with(|| {
                self.ir()
                    .walk_retry_policies()
                    .find(|walker| walker.name() == policy_name)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Could not find retry policy with name: {}", policy_name)
                    })
                    .map(CallablePolicy::from)
            })?;

        Ok(policy_ref.value().clone())
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
        params: &IndexMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope)> {
        let func = self.get_function(function_name, ctx)?;
        let baml_args = self.ir().check_function_params(&func, params)?;

        let renderer = PromptRenderer::from_function(&func)?;
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
            .render_prompt(&renderer, ctx, &baml_args)
            .map(|prompt| (prompt, node.scope));
    }

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
        _ctx: &RuntimeContext,
    ) -> Result<FunctionWalker<'ir>> {
        let walker = self.ir().find_function(function_name)?;
        Ok(walker)
    }

    fn parse_response<'ir>(
        &'ir self,
        function: &FunctionWalker<'ir>,
        response: &crate::internal::llm_client::LLMCompleteResponse,
        ctx: &RuntimeContext,
    ) -> Result<jsonish::BamlValueWithFlags> {
        jsonish::from_str(
            self.ir(),
            &ctx.env,
            function.output(),
            &response.content,
            false,
        )
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
    ) -> Result<IndexMap<String, BamlValue>> {
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
                Ok(params)
            }
            Err(e) => return Err(anyhow::anyhow!("Unable to resolve test params: {:?}", e)),
        }
    }
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

        let ir = IntermediateRepr::from_parser_database(&schema.db)?;
        log::info!("Successfully loaded BAML schema");
        log::info!("Diagnostics: {:#?}", schema.diagnostics);

        Ok(Self {
            ir: Arc::new(ir),
            diagnostics: schema.diagnostics,
            clients: DashMap::new(),
            retry_policies: DashMap::new(),
        })
    }

    #[cfg(feature = "no_wasm")]
    fn from_directory(dir: &std::path::PathBuf) -> Result<InternalBamlRuntime> {
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

        InternalBamlRuntime::from_files(dir, src_files)
    }
}

impl RuntimeInterface for InternalBamlRuntime {
    async fn call_function_impl(
        &self,
        function_name: String,
        params: IndexMap<String, BamlValue>,
        ctx: RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        let func = self.get_function(&function_name, &ctx)?;
        let baml_args = self.ir().check_function_params(&func, &params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();
        let orchestrator = self.orchestration_graph(&client_name, &ctx)?;

        // Now actually execute the code.
        let (history, _) = orchestrate(orchestrator, &ctx, &renderer, &baml_args, |s, ctx| {
            jsonish::from_str(self.ir(), &ctx.env, func.output(), s, false)
        })
        .await;

        FunctionResult::new_chain(history)
    }

    fn stream_function_impl(
        &self,
        function_name: String,
        params: IndexMap<String, BamlValue>,
        ctx: RuntimeContext,
        tracer: Arc<BamlTracer>,
    ) -> Result<FunctionResultStream> {
        let func = self.get_function(&function_name, &ctx)?;
        let baml_args = self.ir().check_function_params(&func, &params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();

        let orchestrator = self.orchestration_graph(&client_name, &ctx)?;
        let first = orchestrator.first().ok_or(anyhow::anyhow!(
            "No orchestrator nodes found for client {}",
            client_name
        ))?;

        Ok(FunctionResultStream {
            provider: first.provider.clone(),
            prompt: first.provider.render_prompt(&renderer, &ctx, &baml_args)?,
            function_name,
            scope: first.scope.clone(),
            ir: self.ir.clone(),
            ctx,
            tracer,
        })
    }
}

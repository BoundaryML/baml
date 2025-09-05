use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use baml_types::{
    tracing::events::{FunctionEnd, FunctionStart, TraceData, TraceEvent},
    BamlMap, BamlValue, Constraint, EvaluationContext,
};
use internal_baml_core::{
    ast::BamlVisDiagramGenerator,
    internal_baml_diagnostics::SourceFile,
    ir::{
        repr::{IntermediateRepr, Node, TypeBuilderEntry},
        ArgCoercer, ExprFunctionWalker, FunctionWalker, IRHelper, TestCase,
    },
    validate,
};
use internal_baml_jinja::RenderedPrompt;
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};

use crate::{
    client_registry::ClientProperty,
    internal::{
        ir_features::{IrFeatures, WithInternal},
        llm_client::{
            llm_provider::LLMProvider,
            orchestrator::{
                orchestrate_call, IterOrchestrator, OrchestrationScope, OrchestratorNode,
            },
            primitive::LLMPrimitiveProvider,
            retry_policy::CallablePolicy,
            traits::{WithClientProperties, WithPrompt, WithRenderRawCurl},
            LLMResponse,
        },
        prompt_renderer::PromptRenderer,
    },
    runtime::CachedClient,
    runtime_interface::{InternalClientLookup, RuntimeConstructor},
    tracing::BamlTracer,
    tracingv2::storage::storage::{Collector, BAML_TRACER},
    type_builder::TypeBuilder,
    FunctionResult, FunctionResultStream, InternalBamlRuntime, InternalRuntimeInterface,
    RenderCurlSettings, RuntimeContext, RuntimeContextManager,
};

impl<'a> InternalClientLookup<'a> for InternalBamlRuntime {
    // Gets a top-level client/strategy by name
    // There are two types of clients:
    // 1. Shorthand clients (e.g. `openai/gpt-4`)
    // 2. Named clients (e.g. `my_custom_client`)
    //
    // For named clients, we first check if the client is cached in the RuntimeContext.
    // If it is, we return the cached client.
    // If it is not, we get the client from the IR and cache it.
    //
    // For shorthand clients, we parse the client spec and return a new LLMProvider.
    fn get_llm_provider(
        &'a self,
        client_spec: &ClientSpec,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>> {
        match client_spec {
            ClientSpec::Shorthand(provider, model) => {
                let client_property = ClientProperty::from_shorthand(provider, model);
                // TODO: allow other providers
                let llm_primitive_provider =
                    LLMPrimitiveProvider::try_from((&client_property, ctx))
                        .context(format!("Failed to parse client: {provider}/{model}"))?;

                Ok(Arc::new(LLMProvider::Primitive(Arc::new(
                    llm_primitive_provider,
                ))))
            }
            ClientSpec::Named(client_name) => {
                if let Some(client) = ctx
                    .client_overrides
                    .as_ref()
                    .and_then(|(_, c)| c.get(client_name))
                {
                    return Ok(client.clone());
                }

                #[cfg(target_arch = "wasm32")]
                let mut clients = self.clients.lock().unwrap();
                #[cfg(not(target_arch = "wasm32"))]
                let clients = &self.clients;

                // if a client exists, check if the env vars have changed
                if clients.contains_key(client_name) {
                    // make sure to clone the client to avoid holding a lock, otherwise dashmap will deadlock!
                    #[allow(clippy::map_clone)]
                    let client = clients.get(client_name).map(|c| c.clone()).unwrap();
                    // if the env vars haven't changed, return the cached client
                    if !client.has_env_vars_changed(ctx.env_vars()) {
                        return Ok(client.provider.clone());
                    } else {
                        // if the env vars have changed, remove the client from the cache, and create a new one.
                        clients.remove(client_name);
                    }
                }

                // Either client doesn't exist or env vars have changed, anyway, create a new one.
                let walker = self
                    .ir()
                    .find_client(client_name)
                    .context(format!("Could not find client with name: {client_name}"))?;
                // Get required env vars from the client walker
                let new_client = LLMProvider::try_from((&walker, ctx)).map(Arc::new)?;
                // Only store the required env vars
                let filtered_env_vars = ctx
                    .env_vars()
                    .iter()
                    .filter(|(k, _)| walker.required_env_vars().contains(*k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                clients.insert(
                    client_name.into(),
                    CachedClient::new(new_client.clone(), filtered_env_vars),
                );
                Ok(new_client)
            }
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
        client_spec: &ClientSpec,
        ctx: &RuntimeContext,
    ) -> Result<Vec<OrchestratorNode>> {
        let client = self.get_llm_provider(client_spec, ctx)?;
        client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self)
    }

    fn function_graph(&self, _function_name: &str, _ctx: &RuntimeContext) -> Result<String> {
        // Use baml-vis to generate a Mermaid diagram for the current AST
        let ast = self.db.ast();
        let graph = BamlVisDiagramGenerator::generate_headers_flowchart(ast);
        Ok(graph)
    }

    fn features(&self) -> IrFeatures {
        WithInternal::features(self)
    }

    async fn render_prompt(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope, AllowedRoleMetadata)> {
        let func = self.get_function(function_name)?;
        let function_params = func.inputs();
        let baml_args = self.ir().check_function_params(
            function_params,
            params,
            ArgCoercer {
                span_path: None,
                allow_implicit_cast_to_string: false,
            },
        )?;

        let renderer = PromptRenderer::from_function(&func, self.ir(), ctx)?;

        let client_spec = renderer.client_spec();
        let client = self.get_llm_provider(client_spec, ctx)?;
        let mut selected =
            client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self)?;
        let node_index = node_index.unwrap_or(0);

        if node_index >= selected.len() {
            return Err(anyhow::anyhow!(
                "Execution Node out of bounds (render prompt): {} >= {} for client {}",
                node_index,
                selected.len(),
                client_spec,
            ));
        }

        // TODO: remove this clone, it's only here because we're being lazy about the type tree beneath render_prompt
        let baml_args =
            BamlValue::Map(baml_args.into_iter().map(|(k, v)| (k, v.value())).collect());
        let node = selected.swap_remove(node_index);
        return node
            .provider
            .render_prompt(self.ir(), &renderer, ctx, &baml_args)
            .await
            .map(|prompt| (prompt, node.scope, node.provider.allowed_metadata().clone()));
    }

    async fn render_raw_curl(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
        render_settings: RenderCurlSettings,
        node_index: Option<usize>,
    ) -> Result<String> {
        let func = self.get_function(function_name)?;

        let renderer = PromptRenderer::from_function(&func, self.ir(), ctx)?;

        let client_spec = renderer.client_spec();
        let client = self.get_llm_provider(client_spec, ctx)?;
        let mut selected =
            client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self)?;

        let node_index = node_index.unwrap_or(0);

        if node_index >= selected.len() {
            return Err(anyhow::anyhow!(
                "Execution Node out of bounds (raw curl): {} >= {} for client {}",
                node_index,
                selected.len(),
                client_spec,
            ));
        }

        let node = selected.swap_remove(node_index);
        node.provider
            .render_raw_curl(ctx, prompt, render_settings)
            .await
    }

    fn get_function<'ir>(&'ir self, function_name: &str) -> Result<FunctionWalker<'ir>> {
        let walker = self.ir().find_function(function_name)?;
        Ok(walker)
    }

    fn get_expr_function<'ir>(
        &'ir self,
        function_name: &str,
        _ctx: &RuntimeContext,
    ) -> Result<ExprFunctionWalker<'ir>> {
        let walker = self.ir().find_expr_fn(function_name)?;
        Ok(walker)
    }

    fn ir(&self) -> &IntermediateRepr {
        use std::ops::Deref;
        self.ir.deref()
    }

    fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>> {
        log::info!("get_test_params: {function_name} {test_name}");
        let maybe_test_and_params = self.get_function(function_name).and_then(|func| {
            let test = self.ir().find_test(&func, test_name)?;
            let test_case_params = test.test_case_params(&ctx.eval_ctx(strict))?;
            let inputs = func.inputs().clone();
            let span = test.span();
            Ok((test_case_params, inputs, span.cloned()))
        });
        let maybe_expr_test_and_params =
            self.get_expr_function(function_name, ctx).and_then(|func| {
                let test = self.ir().find_expr_fn_test(&func, test_name)?;
                let test_case_params = test.test_case_params(&ctx.eval_ctx(strict))?;
                let inputs = func.inputs().clone();
                let span = test.span();
                Ok((test_case_params, inputs, span.cloned()))
            });

        let maybe_params = maybe_test_and_params.or(maybe_expr_test_and_params);

        let eval_ctx = ctx.eval_ctx(strict);

        match maybe_params {
            Ok((params, function_params, span)) => {
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

                self.ir()
                    .check_function_params(
                        &function_params,
                        &params,
                        ArgCoercer {
                            span_path: span.map(|s| s.file.path_buf().clone()),
                            allow_implicit_cast_to_string: true,
                        },
                    )
                    .map(|bv| bv.into_iter().map(|(k, v)| (k, v.value())).collect())
            }
            Err(e) => Err(anyhow::anyhow!("Unable to resolve test params: {:?}", e)),
        }
    }

    fn get_test_constraints(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Vec<Constraint>> {
        let func = self.get_function(function_name)?;
        let walker = self.ir().find_test(&func, test_name)?;
        Ok(walker.item.1.elem.constraints.clone())
    }

    fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Result<Option<TypeBuilder>> {
        let func = self.get_function(function_name)?;
        let test = self.ir().find_test(&func, test_name)?;

        if test.type_builder_contents().is_empty() {
            return Ok(None);
        }

        let type_builder = TypeBuilder::new();

        type_builder.add_entries(test.type_builder_contents());

        type_builder
            .recursive_type_aliases()
            .lock()
            .unwrap()
            .extend(test.type_builder_recursive_aliases().iter().cloned());

        type_builder
            .recursive_classes()
            .lock()
            .unwrap()
            .extend(test.type_builder_recursive_classes().iter().cloned());

        Ok(Some(type_builder))
    }
}

impl RuntimeConstructor for InternalBamlRuntime {
    fn from_file_content<T: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
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
        let mut schema = validate(&directory, contents.clone(), feature_flags);
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;
        log::trace!("Successfully loaded BAML schema");
        log::trace!("Diagnostics: {:#?}", schema.diagnostics);

        Ok(Self {
            ir: Arc::new(ir),
            db: schema.db,
            diagnostics: schema.diagnostics,
            clients: Default::default(),
            retry_policies: Default::default(),
            source_files: contents,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from_directory(
        dir: &std::path::Path,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<InternalBamlRuntime> {
        InternalBamlRuntime::from_files(
            dir,
            crate::baml_src_files(&dir.to_path_buf())?,
            feature_flags,
        )
    }
}

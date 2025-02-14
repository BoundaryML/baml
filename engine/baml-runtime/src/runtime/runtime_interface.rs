use std::{collections::HashMap, path::PathBuf, sync::Arc};

use super::InternalBamlRuntime;
use crate::internal::llm_client::traits::WithClientProperties;
use crate::internal::llm_client::LLMResponse;
use crate::type_builder::TypeBuilder;
use crate::RuntimeContextManager;
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
            traits::{WithPrompt, WithRenderRawCurl},
        },
        prompt_renderer::PromptRenderer,
    },
    runtime_interface::{InternalClientLookup, RuntimeConstructor},
    tracing::BamlTracer,
    FunctionResult, FunctionResultStream, InternalRuntimeInterface, RenderCurlSettings,
    RuntimeContext, RuntimeInterface,
};
use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlValue, Constraint, EvaluationContext};
use internal_baml_core::ir::repr::TypeBuilderEntry;
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, ArgCoercer, FunctionWalker, IRHelper},
    validate,
};
use internal_baml_jinja::RenderedPrompt;
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};

impl<'a> InternalClientLookup<'a> for InternalBamlRuntime {
    // Gets a top-level client/strategy by name
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
                        .context(format!("Failed to parse client: {}/{}", provider, model))?;

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

                if let Some(client) = clients.get(client_name) {
                    Ok(client.clone())
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
        let func = self.get_function(function_name, ctx)?;
        let baml_args = self.ir().check_function_params(
            &func,
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
        let func = self.get_function(function_name, ctx)?;

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
        self.ir.deref()
    }

    fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>> {
        let func = self.get_function(function_name, ctx)?;
        let test = self.ir().find_test(&func, test_name)?;

        let eval_ctx = ctx.eval_ctx(strict);

        match test.test_case_params(&eval_ctx) {
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

                let baml_args = self.ir().check_function_params(
                    &func,
                    &params,
                    ArgCoercer {
                        span_path: test.span().map(|s| s.file.path_buf().clone()),
                        allow_implicit_cast_to_string: true,
                    },
                )?;
                baml_args
                    .as_map_owned()
                    .context("Test params must be a map")
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
        let func = self.get_function(function_name, ctx)?;
        let walker = self.ir().find_test(&func, test_name)?;
        Ok(walker.item.1.elem.constraints.clone())
    }

    fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<TypeBuilder>> {
        let func = self.get_function(function_name, &ctx.create_ctx(None, None)?)?;
        let test = self.ir().find_test(&func, test_name)?;

        if test.type_builder_contents().is_empty() {
            return Ok(None);
        }

        let type_builder = TypeBuilder::new();

        for entry in test.type_builder_contents() {
            match entry {
                TypeBuilderEntry::Class(cls) => {
                    let mutex = type_builder.class(&cls.elem.name);
                    let class_builder = mutex.lock().unwrap();
                    for f in &cls.elem.static_fields {
                        class_builder
                            .property(&f.elem.name)
                            .lock()
                            .unwrap()
                            .r#type(f.elem.r#type.elem.to_owned());
                    }
                }

                TypeBuilderEntry::Enum(enm) => {
                    let mutex = type_builder.r#enum(&enm.elem.name);
                    let enum_builder = mutex.lock().unwrap();
                    for (variant, _) in &enm.elem.values {
                        enum_builder.value(&variant.elem.0).lock().unwrap();
                    }
                }

                TypeBuilderEntry::TypeAlias(alias) => {
                    let mutex = type_builder.type_alias(&alias.elem.name);
                    let alias_builder = mutex.lock().unwrap();
                    alias_builder.target(alias.elem.r#type.elem.to_owned());
                }
            }
        }

        type_builder
            .recursive_type_aliases()
            .lock()
            .unwrap()
            .extend(test.type_builder_recursive_aliases().iter().cloned());

        Ok(Some(type_builder))
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
        let mut schema = validate(&directory, contents);
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;
        log::trace!("Successfully loaded BAML schema");
        log::trace!("Diagnostics: {:#?}", schema.diagnostics);

        Ok(Self {
            ir: Arc::new(ir),
            diagnostics: schema.diagnostics,
            clients: Default::default(),
            retry_policies: Default::default(),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from_directory(dir: &std::path::Path) -> Result<InternalBamlRuntime> {
        InternalBamlRuntime::from_files(dir, crate::baml_src_files(&dir.to_path_buf())?)
    }
}

impl RuntimeInterface for InternalBamlRuntime {
    async fn call_function_impl(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        let func = match self.get_function(&function_name, &ctx) {
            Ok(func) => func,
            Err(e) => {
                return Ok(FunctionResult::new(
                    OrchestrationScope::default(),
                    LLMResponse::UserFailure(format!(
                        "BAML function {function_name} does not exist in baml_src/ (did you typo it?): {:?}",
                        e
                    )),
                    None,
                ))
            }
        };
        let baml_args = self.ir().check_function_params(
            &func,
            params,
            ArgCoercer {
                span_path: None,
                allow_implicit_cast_to_string: false,
            },
        )?;
        // let baml_args = match self.ir().check_function_params(
        //     &func,
        //     &params,
        //     ArgCoercer {
        //         span_path: None,
        //         allow_implicit_cast_to_string: false,
        //     },
        // ) {
        //     Ok(args) => args,
        //     Err(e) => {
        //         return Ok(FunctionResult::new(
        //             OrchestrationScope::default(),
        //             LLMResponse::UserFailure(format!(
        //                 "Failed while validating args for {function_name}: {:?}",
        //                 e
        //             )),
        //             None,
        //         ))
        //     }
        // };

        let renderer = PromptRenderer::from_function(&func, self.ir(), &ctx)?;
        let orchestrator = self.orchestration_graph(renderer.client_spec(), &ctx)?;

        // Now actually execute the code.
        let (history, _) =
            orchestrate_call(orchestrator, self.ir(), &ctx, &renderer, &baml_args, |s| {
                renderer.parse(self.ir(), s, false)
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
        #[cfg(not(target_arch = "wasm32"))] tokio_runtime: Arc<tokio::runtime::Runtime>,
    ) -> Result<FunctionResultStream> {
        let func = self.get_function(&function_name, &ctx)?;
        let renderer = PromptRenderer::from_function(&func, self.ir(), &ctx)?;
        let orchestrator = self.orchestration_graph(renderer.client_spec(), &ctx)?;
        let Some(baml_args) = self
            .ir
            .check_function_params(
                &func,
                params,
                ArgCoercer {
                    span_path: None,
                    allow_implicit_cast_to_string: false,
                },
            )?
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
            #[cfg(not(target_arch = "wasm32"))]
            tokio_runtime,
        })
    }
}

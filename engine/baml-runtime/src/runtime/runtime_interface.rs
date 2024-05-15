use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{
    internal::{
        ir_features::{IrFeatures, WithInternal},
        llm_client::{
            llm_provider::LLMProvider,
            retry_policy::CallablePolicy,
            roundrobin::roundrobin_client::FnGetClientConfig,
            traits::{WithCallable, WithPrompt, WithRetryPolicy},
        },
        prompt_renderer::PromptRenderer,
    },
    runtime_interface::RuntimeConstructor,
    FunctionResultStream, InternalRuntimeInterface, RuntimeContext, RuntimeInterface, TestResponse,
};
use anyhow::Result;
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

#[cfg(not(feature = "no_wasm"))]
use wasm_bindgen::JsValue;

use super::InternalBamlRuntime;

impl InternalRuntimeInterface for InternalBamlRuntime {
    fn diagnostics(&self) -> &internal_baml_core::internal_baml_diagnostics::Diagnostics {
        &self.diagnostics
    }

    fn features(&self) -> IrFeatures {
        WithInternal::features(self)
    }

    fn render_prompt(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &IndexMap<String, BamlValue>,
    ) -> Result<(RenderedPrompt, String)> {
        let func = self.get_function(function_name, ctx)?;
        let baml_args = self.ir().check_function_params(&func, params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();

        let (client, _) = self.get_client(&client_name, ctx)?;
        let response = client.render_prompt(&renderer, &ctx, &baml_args)?;

        Ok((response, client_name))
    }

    fn get_client<'a>(
        &'a self, // ref lifetime 1
        client_name: &str,
        ctx: &RuntimeContext, // ref lifetime 2
    ) -> Result<(Arc<LLMProvider>, Option<CallablePolicy>)> {
        //lifetime may not live long enough
        // cast requires that `'1` must outlive `'static`rustcClick for full compiler diagnostic
        // runtime_interface.rs(62, 9): let's call the lifetime of this reference `'1`
        // lifetime may not live long enough
        // cast requires that `'2` must outlive `'static`rustcClick for full compiler diagnostic
        // runtime_interface.rs(64, 14): let's call the lifetime of this reference `'2`

        let client_ref = self
            .clients
            .entry(client_name.into())
            .or_try_insert_with(|| {
                let walker = self.ir().find_client(client_name)?;
                let ctx_clone = ctx.clone();
                let client = LLMProvider::from_ir(
                    &walker,
                    ctx,
                    Box::new(move |client_name| self.get_client(client_name, &ctx_clone)),
                )?;

                let retry_policy = match client.retry_policy_name() {
                    Some(name) => match self
                        .ir()
                        .walk_retry_policies()
                        .find(|walker| walker.name() == name)
                        .map(|walker| CallablePolicy::from(walker))
                    {
                        Some(policy) => Some(policy),
                        None => {
                            return Err(anyhow::anyhow!(
                                "Could not find retry policy with name: {}",
                                name
                            ))
                        }
                    },
                    None => None,
                };

                Ok((Arc::new(client), retry_policy))
            })?;

        let (client, retry_policy) = client_ref.value();

        Ok((Arc::clone(&client), retry_policy.clone()))
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
        response: crate::internal::llm_client::LLMResponse,
        ctx: &RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        let parsed = response
            .content()
            .ok()
            .map(|content| jsonish::from_str(&self.ir(), &ctx.env, function.output(), content));
        Ok(crate::FunctionResult {
            llm_response: response,
            parsed,
        })
    }

    fn ir(&self) -> &IntermediateRepr {
        &self.ir
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
            ir,
            diagnostics: schema.diagnostics,
            clients: DashMap::new(),
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

// #[cfg(not(feature = "no_wasm"))]
// type ResponseType<T> = Result<T, wasm_bindgen::JsValue>;
// #[cfg(feature = "no_wasm")]
type ResponseType<T> = Result<T>;

impl RuntimeInterface for InternalBamlRuntime {
    async fn run_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> ResponseType<TestResponse> {
        let func = self.get_function(function_name, ctx)?;
        let test = self.ir().find_test(&func, test_name)?;

        let params = match test.test_case_params(&ctx.env) {
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
                params
            }
            Err(e) => {
                return Ok(TestResponse {
                    function_response: Err(anyhow::anyhow!(
                        "Unable to resolve test params: {:?}",
                        e
                    )),
                })
            }
        };

        log::info!("Test params: {:#?}", params);
        let baml_args = self.ir().check_function_params(&func, &params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();

        // let get_client_config_callback = |client_name: &str| -> Result<
        //     (Arc<LLMProvider>, Option<CallablePolicy>),
        //     anyhow::Error,
        // > {
        //     //let ir = self.ir();
        //     let (client, retry_policy) = self.get_client(client_name, ctx, None)?;
        //     Ok((client, retry_policy))
        // };

        let (client, retry_policy) = self.get_client(&client_name, ctx)?;
        let prompt = client.render_prompt(&renderer, &ctx, &baml_args)?;
        log::debug!("Prompt: {:#?}", prompt);

        let response = client.call(retry_policy, ctx, &renderer, &baml_args).await;

        log::debug!("RESPONSE: {:#?}", response);

        // We need to get the function again because self is borrowed mutably.
        let func = self.get_function(function_name, ctx)?;
        let parsed = self.parse_response(&func, response, ctx)?;
        Ok(TestResponse {
            function_response: Ok(parsed),
        })
    }

    async fn call_function(
        &self,
        function_name: String,
        params: IndexMap<String, BamlValue>,
        ctx: &RuntimeContext,
    ) -> ResponseType<crate::FunctionResult> {
        let func = self.get_function(&function_name, ctx)?;
        let baml_args = self.ir().check_function_params(&func, &params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();

        let (client, retry_policy) = self.get_client(&client_name, ctx)?;

        let response = client.call(retry_policy, ctx, &renderer, &baml_args).await;

        log::debug!("call_function(\"{}\") -> {:#?}", function_name, response);

        // We need to get the function again because self is borrowed mutably.
        let func = self.get_function(&function_name, ctx)?;
        let parsed = self.parse_response(&func, response, ctx)?;
        Ok(parsed)
    }

    fn stream_function(
        &self,
        _function_name: String,
        _params: &IndexMap<String, serde_json::Value>,
        _ctx: &RuntimeContext,
    ) -> FunctionResultStream {
        todo!()
        //LLMResponseStream::new()
    }

    #[cfg(feature = "no_wasm")]
    fn generate_client(
        &self,
        client_type: &LanguageClientType,
        args: &GeneratorArgs,
    ) -> Result<internal_baml_codegen::GenerateOutput> {
        client_type.generate_client(self.ir(), args)
    }
}

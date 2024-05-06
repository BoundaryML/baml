use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{
    internal::{
        ir_features::{IrFeatures, WithInternal},
        llm_client::{
            llm_provider::LLMProvider,
            retry_policy::CallablePolicy,
            traits::{WithCallable, WithPrompt, WithRetryPolicy},
        },
        prompt_renderer::PromptRenderer,
    },
    runtime_interface::RuntimeConstructor,
    InternalRuntimeInterface, RuntimeContext, RuntimeInterface, TestResponse,
};
use anyhow::Result;
use dashmap::DashMap;
use internal_baml_codegen::{GeneratorArgs, LanguageClientType};
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, FunctionWalker, IRHelper},
    validate,
};
use internal_baml_jinja::RenderedPrompt;
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
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<(RenderedPrompt, String)> {
        let func = self.get_function(function_name, ctx)?;
        let baml_args = self.ir().check_function_params(&func, params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();

        let (client, _) = self.get_client(&client_name, ctx)?;
        let response = client.render_prompt(&renderer, &ctx, &baml_args)?;

        Ok((response, client_name))
    }

    fn get_client(
        &self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<(Arc<LLMProvider>, Option<CallablePolicy>)> {
        let client_ref = self
            .clients
            .entry(client_name.into())
            .or_try_insert_with(|| {
                let walker = self.ir().find_client(client_name)?;
                let client = LLMProvider::from_ir(&walker, ctx)?;

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
            .map(|content| jsonish::from_str(content, &self.ir(), function.output(), &ctx.env));
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

        Ok(Self {
            ir,
            diagnostics: schema.diagnostics,
            clients: DashMap::new(),
        })
    }

    #[cfg(feature = "disk")]
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

#[cfg(feature = "wasm")]
type ResponseType<T> = Result<T, wasm_bindgen::JsValue>;
#[cfg(not(feature = "wasm"))]
type ResponseType<T> = Result<T>;

impl RuntimeInterface for InternalBamlRuntime {
    async fn run_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> ResponseType<TestResponse> {
        let func = self
            .get_function(function_name, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let test = self
            .ir()
            .find_test(&func, test_name)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let params = match test
            .content()
            .as_json(&ctx.env)
            .map_err(|e| JsValue::from_str(&e.to_string()))?
        {
            serde_json::Value::Object(kv) => {
                let mut params = HashMap::new();
                for (k, v) in kv {
                    params.insert(k, v);
                }
                params
            }
            x => {
                return Ok(TestResponse {
                    function_response: Err(anyhow::anyhow!(
                        "Test content must be an object, found: {:?}",
                        x
                    )),
                })
            }
        };
        log::info!("Test params: {:#?}", params);
        let baml_args = self
            .ir()
            .check_function_params(&func, &params)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let renderer =
            PromptRenderer::from_function(&func).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let client_name = renderer.client_name().to_string();

        let (client, retry_policy) = self
            .get_client(&client_name, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let prompt = client
            .render_prompt(&renderer, &ctx, &baml_args)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        log::debug!("Prompt: {:#?}", prompt);

        let response = client.call(retry_policy, ctx, &prompt).await;

        // We need to get the function again because self is borrowed mutably.
        let func = self
            .get_function(function_name, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let parsed = self
            .parse_response(&func, response, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(TestResponse {
            function_response: Ok(parsed),
        })
    }

    async fn call_function(
        &self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> ResponseType<crate::FunctionResult> {
        let func = self
            .get_function(&function_name, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let baml_args = self
            .ir()
            .check_function_params(&func, &params)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let renderer =
            PromptRenderer::from_function(&func).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let client_name = renderer.client_name().to_string();

        let (client, retry_policy) = self
            .get_client(&client_name, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let prompt = client
            .render_prompt(&renderer, &ctx, &baml_args)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let response = client.call(retry_policy, ctx, &prompt).await;

        // We need to get the function again because self is borrowed mutably.
        let func = self
            .get_function(&function_name, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let parsed = self
            .parse_response(&func, response, ctx)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(parsed)
    }

    #[cfg(feature = "disk")]
    fn generate_client(
        &self,
        client_type: &LanguageClientType,
        args: &GeneratorArgs,
    ) -> Result<()> {
        client_type.generate_client(self.ir(), args)
    }

    // async fn run_test(
    //     &self,
    //     function_name: &str,
    //     test_name: &str,
    //     ctx: &RuntimeContext,
    // ) -> Result<crate::TestResponse> {
    //     let function = self.ir.find_function(&function_name)?;

    //     let test = self.ir.find_test(&function, test_name)?;

    //     let params = match test.content().as_json(&ctx.env)? {
    //         serde_json::Value::Object(kv) => {
    //             let mut params = HashMap::new();
    //             for (k, v) in kv {
    //                 params.insert(k, v);
    //             }
    //             params
    //         }
    //         x => {
    //             return Ok(TestResponse {
    //                 function_response: Err(anyhow::anyhow!(
    //                     "Test content must be an object, found: {:?}",
    //                     x
    //                 )),
    //             })
    //         }
    //     };

    //     let (mut client, propmt) = self.get_prompt(&function, params, ctx)?;

    //     let response = client.call(&self.ir, ctx, &propmt).await;

    //     let parsed = response
    //         .content()
    //         .ok()
    //         .map(|content| jsonish::from_str(content, &self.ir, function.output(), &ctx.env));

    //     Ok(crate::TestResponse {
    //         function_response: Ok(crate::FunctionResult {
    //             llm_response: response,
    //             parsed,
    //         }),
    //     })
    // }

    // async fn call_function(
    //     &self,
    //     function_name: String,
    //     params: std::collections::HashMap<String, serde_json::Value>,
    //     ctx: &RuntimeContext,
    // ) -> Result<crate::FunctionResult> {
    //     let function = self.ir.find_function(&function_name)?;
    //     let (mut client, propmt) = self.get_prompt(&function, params, ctx)?;

    //     let response = client.call(&self.ir, ctx, &propmt).await;

    //     let parsed = response
    //         .content()
    //         .ok()
    //         .map(|content| jsonish::from_str(content, &self.ir, function.output(), &ctx.env));

    //     Ok(crate::FunctionResult {
    //         llm_response: response,
    //         parsed,
    //     })
    // }
}

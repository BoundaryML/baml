//! Runtime API - Entry points for executing BAML functions.
//!
//! This module provides the main entry points for the BAML runtime:
//!
//! - `call_function` - Execute a function synchronously
//! - `stream_function` - Execute a function with streaming
//! - `run_test` - Execute a test with constraint evaluation
//! - `render_prompt` - Render a prompt without executing
//! - `build_request` - Build a provider-specific request
//! - `render_raw_curl` - Generate a curl command

use std::collections::HashMap;

use baml_compiler_tir::Ty;
use baml_db::baml_workspace::Project;
use baml_jinja_runtime::{LlmClientSpec, RenderContext};
use baml_project::ProjectDatabase as RootDatabase;
use ir_stub::{BamlMap, BamlValue};

use baml_llm_interface::RenderedPrompt;

use crate::{
    context::{DynamicBamlContext, PerCallContext, SharedCallContext},
    errors::RuntimeError,
    function_lookup::{FunctionBodyInfo, find_function_by_name, get_function_info},
    llm_request::openai::{OpenAiClientConfig, OpenAiRequest},
    orchestrator::{
        ClientConfig, FunctionResultStream, OrchestrationScope, OrchestratorConfig,
        OrchestratorNode, ProviderType, orchestrate_call, orchestrate_stream,
    },
    prepared_function::PreparedFunction,
    render_options::RenderOptions,
    types::{FunctionResult, TestResult},
};

/// The BAML runtime - main entry point for executing BAML functions.
///
/// BamlProgram holds a compiled BAML database and provides methods for:
/// - Preparing functions for execution
/// - Rendering prompts
/// - Building provider requests
/// - Executing functions (with or without streaming)
pub struct BamlProgram {
    /// The compiled BAML database.
    db: RootDatabase,
    /// The project context (for type resolution).
    project: Project,
}

impl BamlProgram {
    /// Create a new BamlProgram from a compiled database.
    ///
    /// The database must have a project set (via `set_project_root`).
    pub fn new(db: RootDatabase) -> Result<Self, RuntimeError> {
        let project = db
            .project()
            .ok_or_else(|| RuntimeError::Validation("Database has no project set".to_string()))?;
        Ok(Self { db, project })
    }

    /// Create a BamlProgram from a database and explicit project.
    pub fn with_project(db: RootDatabase, project: Project) -> Self {
        Self { db, project }
    }

    /// Get a reference to the underlying database.
    pub fn db(&self) -> &RootDatabase {
        &self.db
    }

    /// Get the project.
    pub fn project(&self) -> Project {
        self.project
    }

    /// Prepare a function for execution by name.
    ///
    /// This looks up the function, validates inputs, and creates a PreparedFunction
    /// that can be used for rendering prompts or executing the function.
    pub fn prepare_function(
        &self,
        function_name: &str,
        args: BamlMap,
    ) -> Result<PreparedFunction, RuntimeError> {
        // Find the function by name
        let func_loc = find_function_by_name(&self.db, self.project, function_name)
            .ok_or_else(|| RuntimeError::FunctionNotFound(function_name.to_string()))?;

        // Get function info with resolved types
        let func_info = get_function_info(&self.db, self.project, func_loc);

        // Extract prompt template
        let prompt_template = match &func_info.body {
            FunctionBodyInfo::Llm(llm) => llm
                .prompt
                .as_ref()
                .map(|p| ir_stub::PromptTemplate::new(p.clone()))
                .ok_or_else(|| {
                    RuntimeError::Validation(format!(
                        "Function '{}' has no prompt template",
                        function_name
                    ))
                })?,
            FunctionBodyInfo::Expr => {
                return Err(RuntimeError::Validation(format!(
                    "Function '{}' is an expression function, not an LLM function",
                    function_name
                )));
            }
            FunctionBodyInfo::Missing => {
                return Err(RuntimeError::Validation(format!(
                    "Function '{}' has no body",
                    function_name
                )));
            }
        };

        // Extract client spec
        let client_name = match &func_info.body {
            FunctionBodyInfo::Llm(llm) => {
                llm.client.clone().unwrap_or_else(|| "default".to_string())
            }
            _ => "default".to_string(),
        };
        let client_spec = ir_stub::ClientSpec::new(&client_name);

        // Convert return type from TIR Ty to ir_stub::TypeRef
        let output_type = ty_to_type_ref(&func_info.signature.return_type);

        // Create typed args (for constraint evaluation)
        let args_with_types = args
            .iter()
            .map(|(k, v)| {
                // Try to find the parameter type
                let type_ref = func_info
                    .signature
                    .params
                    .iter()
                    .find(|p| p.name == *k)
                    .map(|p| ty_to_type_ref(&p.ty))
                    .unwrap_or_else(|| ir_stub::TypeRef::new("unknown"));
                (
                    k.clone(),
                    crate::prepared_function::TypedArg {
                        value: v.clone(),
                        type_ref,
                    },
                )
            })
            .collect();

        Ok(PreparedFunction {
            function_name: function_name.to_string(),
            args,
            args_with_types,
            output_type,
            return_ty: func_info.signature.return_type.clone(),
            client_spec,
            prompt_template,
        })
    }

    /// Execute a function and wait for the complete result.
    ///
    /// This is the primary entry point for non-streaming function execution.
    pub fn call_function(
        &self,
        prepared: &PreparedFunction,
        _shared_ctx: &SharedCallContext,
        _dynamic_ctx: &DynamicBamlContext,
        per_call_ctx: &PerCallContext,
    ) -> Result<FunctionResult, RuntimeError> {
        // Build orchestrator config from prepared function
        let config = build_orchestrator_config(&prepared.client_spec)?;

        // Render the prompt
        let prompt = self.render_prompt(prepared, _dynamic_ctx)?;

        // Execute through orchestrator
        let result = orchestrate_call(
            &prompt,
            &config,
            &per_call_ctx.env_vars,
            &prepared.output_type,
            || per_call_ctx.is_cancelled(),
        )?;

        // Convert to FunctionResult
        Ok(FunctionResult {
            value: result.response.map(|r| r.value).unwrap_or(BamlValue::Null),
            attempts: result
                .attempts
                .iter()
                .map(|a| crate::types::OrchestrationAttemptSummary {
                    client_name: a.node.client.name.clone(),
                    success: a.error.is_none(),
                    error: a.error.as_ref().map(|e| e.to_string()),
                    duration: a.duration,
                })
                .collect(),
            duration: result.total_duration,
        })
    }

    /// Execute a function with streaming, returning a stream handle.
    ///
    /// The caller is responsible for driving the stream to completion.
    pub fn stream_function(
        &self,
        prepared: &PreparedFunction,
        _shared_ctx: &SharedCallContext,
        _dynamic_ctx: &DynamicBamlContext,
        per_call_ctx: &PerCallContext,
    ) -> Result<FunctionResultStream, RuntimeError> {
        // Build orchestrator config from prepared function
        let config = build_orchestrator_config(&prepared.client_spec)?;

        // Render the prompt
        let prompt = self.render_prompt(prepared, _dynamic_ctx)?;

        // Create stream through orchestrator
        orchestrate_stream(
            &prompt,
            config,
            &per_call_ctx.env_vars,
            prepared.output_type.clone(),
            || per_call_ctx.is_cancelled(),
        )
    }

    /// Execute a test, evaluating @assert/@check constraints.
    pub fn run_test(
        &self,
        prepared: &PreparedFunction,
        shared_ctx: &SharedCallContext,
        dynamic_ctx: &DynamicBamlContext,
        per_call_ctx: &PerCallContext,
    ) -> Result<TestResult, RuntimeError> {
        // Execute the function
        let function_result =
            self.call_function(prepared, shared_ctx, dynamic_ctx, per_call_ctx)?;

        // TODO: Evaluate constraints from the function definition
        // For now, return empty constraint results
        Ok(TestResult {
            function_result,
            constraint_results: vec![],
        })
    }

    /// Render a prompt without executing.
    ///
    /// This is useful for debugging and previewing prompts.
    pub fn render_prompt(
        &self,
        prepared: &PreparedFunction,
        _dynamic_ctx: &DynamicBamlContext,
    ) -> Result<RenderedPrompt, RuntimeError> {
        // Build the render context
        let ctx = self.build_render_context(prepared)?;

        // Convert args to BamlValue::Map for the jinja runtime
        let args = BamlValue::Map(prepared.args.clone());

        // Render using the jinja runtime (returns baml_llm_interface::RenderedPrompt directly)
        baml_jinja_runtime::render_prompt(&prepared.prompt_template.template, &args, ctx)
            .map_err(|e| RuntimeError::Render(e.to_string()))
    }

    /// Build a provider-specific request without executing.
    ///
    /// Returns the request that would be sent to the LLM provider.
    pub fn build_request(
        &self,
        prepared: &PreparedFunction,
        dynamic_ctx: &DynamicBamlContext,
        per_call_ctx: &PerCallContext,
        stream: bool,
    ) -> Result<OpenAiRequest, RuntimeError> {
        let prompt = self.render_prompt(prepared, dynamic_ctx)?;

        let client_config = OpenAiClientConfig {
            api_key: per_call_ctx
                .env_vars
                .get("OPENAI_API_KEY")
                .cloned()
                .unwrap_or_default(),
            model: "gpt-4".to_string(), // TODO: Get from client spec
            ..Default::default()
        };

        OpenAiRequest::from_rendered(&prompt, &client_config, stream).map_err(RuntimeError::from)
    }

    /// Generate a curl command for the request.
    ///
    /// This is useful for debugging and sharing requests.
    pub fn render_raw_curl(
        &self,
        prepared: &PreparedFunction,
        dynamic_ctx: &DynamicBamlContext,
        per_call_ctx: &PerCallContext,
        options: &RenderOptions,
    ) -> Result<String, RuntimeError> {
        let request = self.build_request(prepared, dynamic_ctx, per_call_ctx, false)?;
        Ok(request.to_curl(options))
    }

    /// Build the render context for a prepared function.
    fn build_render_context(
        &self,
        prepared: &PreparedFunction,
    ) -> Result<RenderContext, RuntimeError> {
        // Determine client configuration
        let client_name = prepared.client_spec.client_name.clone();
        let (provider, default_role, allowed_roles) = parse_client_config(&client_name);

        Ok(RenderContext {
            client: LlmClientSpec {
                name: client_name,
                provider,
                default_role,
                allowed_roles,
                ..Default::default()
            },
            tags: HashMap::new(),
        })
    }
}

/// Convert a TIR type to an ir_stub TypeRef.
fn ty_to_type_ref(ty: &Ty) -> ir_stub::TypeRef {
    match ty {
        Ty::Int => ir_stub::TypeRef::int(),
        Ty::Float => ir_stub::TypeRef::float(),
        Ty::String => ir_stub::TypeRef::string(),
        Ty::Bool => ir_stub::TypeRef::bool(),
        Ty::Null => ir_stub::TypeRef::new("null"),
        Ty::Media(media_kind) => ir_stub::TypeRef::new(media_kind.to_string()),
        Ty::Named(name) | Ty::Class(name) | Ty::Enum(name) => ir_stub::TypeRef::new(name.as_str()),
        Ty::List(inner) => {
            let inner_ref = ty_to_type_ref(inner);
            ir_stub::TypeRef::new(format!("{}[]", inner_ref.name))
        }
        Ty::Optional(inner) => {
            let inner_ref = ty_to_type_ref(inner);
            ir_stub::TypeRef::new(format!("{}?", inner_ref.name))
        }
        Ty::Map { key, value } => {
            let key_ref = ty_to_type_ref(key);
            let value_ref = ty_to_type_ref(value);
            ir_stub::TypeRef::new(format!("map<{}, {}>", key_ref.name, value_ref.name))
        }
        Ty::Union(variants) => {
            let names: Vec<_> = variants.iter().map(|v| ty_to_type_ref(v).name).collect();
            ir_stub::TypeRef::new(names.join(" | "))
        }
        Ty::Function { params, ret } => {
            let param_names: Vec<_> = params.iter().map(|p| ty_to_type_ref(p).name).collect();
            let ret_name = ty_to_type_ref(ret).name;
            ir_stub::TypeRef::new(format!("({}) -> {}", param_names.join(", "), ret_name))
        }
        Ty::Literal(lit) => ir_stub::TypeRef::new(format!("{}", lit)),
        Ty::Unknown => ir_stub::TypeRef::new("unknown"),
        Ty::Error => ir_stub::TypeRef::new("error"),
        Ty::Void => ir_stub::TypeRef::new("void"),
        Ty::WatchAccessor(inner) => {
            let inner_ref = ty_to_type_ref(inner);
            ir_stub::TypeRef::new(format!("{}.$watch", inner_ref.name))
        }
    }
}

/// Parse client configuration from client name.
fn parse_client_config(client_name: &str) -> (String, String, Vec<String>) {
    let provider = if client_name.to_lowercase().contains("anthropic") {
        "anthropic".to_string()
    } else if client_name.to_lowercase().contains("openai") {
        "openai".to_string()
    } else {
        "openai".to_string() // Default
    };

    let (default_role, allowed_roles) = if provider == "anthropic" {
        (
            "user".to_string(),
            vec!["user".to_string(), "assistant".to_string()],
        )
    } else {
        (
            "system".to_string(),
            vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ],
        )
    };

    (provider, default_role, allowed_roles)
}


/// Build orchestrator config from client specification.
fn build_orchestrator_config(
    client_spec: &ir_stub::ClientSpec,
) -> Result<OrchestratorConfig, RuntimeError> {
    // For now, create a simple single-node config
    // TODO: Parse retry/fallback from client_spec

    let provider = if client_spec.client_name.to_lowercase().contains("anthropic") {
        ProviderType::Anthropic
    } else {
        ProviderType::OpenAi
    };

    let node = OrchestratorNode {
        client: ClientConfig {
            name: client_spec.client_name.clone(),
            provider,
            options: serde_json::json!({}),
        },
        scope: OrchestrationScope::Direct,
        delay: None,
    };

    Ok(OrchestratorConfig::single(node))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ir_stub::{ClientSpec, PromptTemplate, TypeRef};

    use super::*;
    use crate::types::BamlMap;

    fn create_test_prepared() -> PreparedFunction {
        let mut args = BamlMap::new();
        args.insert("name".to_string(), BamlValue::from("Alice"));

        PreparedFunction::new_stub(
            "Greet",
            args,
            TypeRef::string(),
            ClientSpec::new("openai/gpt-4"),
            PromptTemplate::new("Hello, {{ name }}!"),
        )
    }

    fn create_test_runtime() -> BamlProgram {
        // Create a minimal database for testing
        let mut db = RootDatabase::default();
        let project = db.set_project_root(std::path::Path::new("/tmp/test"));

        BamlProgram::with_project(db, project)
    }

    #[test]
    fn test_render_prompt() {
        let runtime = create_test_runtime();
        let prepared = create_test_prepared();
        let dynamic_ctx = DynamicBamlContext::new();
        let result = runtime.render_prompt(&prepared, &dynamic_ctx);

        assert!(result.is_ok());
        let prompt = result.unwrap();
        // For a simple template without _.chat(), it should be a Completion
        match prompt {
            RenderedPrompt::Completion { text } => {
                assert_eq!(text, "Hello, Alice!");
            }
            RenderedPrompt::Chat { messages } => {
                // If it's chat format, check the content
                assert!(!messages.is_empty());
            }
        }
    }

    #[test]
    fn test_build_request() {
        let runtime = create_test_runtime();
        let prepared = create_test_prepared();
        let dynamic_ctx = DynamicBamlContext::new();
        let ctx = PerCallContext::new();

        let result = runtime.build_request(&prepared, &dynamic_ctx, &ctx, false);
        assert!(result.is_ok());

        let request = result.unwrap();
        assert!(!request.stream);
        assert!(request.url.contains("chat/completions"));
    }

    #[test]
    fn test_render_raw_curl() {
        let runtime = create_test_runtime();
        let prepared = create_test_prepared();
        let dynamic_ctx = DynamicBamlContext::new();
        let ctx = PerCallContext::new();
        let options = RenderOptions::default();

        let result = runtime.render_raw_curl(&prepared, &dynamic_ctx, &ctx, &options);
        assert!(result.is_ok());

        let curl = result.unwrap();
        assert!(curl.contains("curl"));
        assert!(curl.contains("-X POST"));
        assert!(curl.contains("[REDACTED]")); // API key should be masked
    }

    #[test]
    fn test_render_raw_curl_with_secrets() {
        let runtime = create_test_runtime();
        let prepared = create_test_prepared();
        let dynamic_ctx = DynamicBamlContext::new();
        let mut env = HashMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-test-key".to_string());
        let ctx = PerCallContext::new().with_env_vars(env);
        let options = RenderOptions::for_execution();

        let result = runtime.render_raw_curl(&prepared, &dynamic_ctx, &ctx, &options);
        assert!(result.is_ok());

        let curl = result.unwrap();
        assert!(curl.contains("sk-test-key")); // API key should be visible
    }

    #[test]
    fn test_ty_to_type_ref() {
        assert_eq!(ty_to_type_ref(&Ty::String).name, "string");
        assert_eq!(ty_to_type_ref(&Ty::Int).name, "int");
        assert_eq!(
            ty_to_type_ref(&Ty::List(Box::new(Ty::String))).name,
            "string[]"
        );
    }
}

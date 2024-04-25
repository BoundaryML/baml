mod llm_client;
mod prompt_renderer;
mod scope_diagnostics;
mod validate_value;

use std::collections::HashMap;

use anyhow::Result;
use internal_baml_core::ir::{
    repr::{IntermediateRepr, Walker},
    Class, Client, Enum, Function, RetryPolicy, TemplateString,
};

use crate::{error_not_found, error_unsupported};

use self::scope_diagnostics::ScopeStack;

pub use self::llm_client::{LLMClientExt, LLMProvider};
pub use self::prompt_renderer::PromptRenderer;

type FunctionWalker<'a> = Walker<'a, &'a Function>;
type EnumWalker<'a> = Walker<'a, &'a Enum>;
type ClassWalker<'a> = Walker<'a, &'a Class>;
type TemplateStringWalker<'a> = Walker<'a, &'a TemplateString>;
type ClientWalker<'a> = Walker<'a, &'a Client>;
type RetryPolicyWalker<'a> = Walker<'a, &'a RetryPolicy>;

#[derive(Default)]
pub struct RuntimeContext {
    env: HashMap<String, String>,
    tags: HashMap<String, serde_json::Value>,
}

impl RuntimeContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_env(mut self, key: String, value: String) -> Self {
        self.env.insert(key, value);
        self
    }

    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    pub fn with_tags(mut self, tags: HashMap<String, serde_json::Value>) -> Self {
        self.tags = tags;
        self
    }
}

pub trait IRHelper {
    fn find_enum(&self, enum_name: &str) -> Result<EnumWalker>;
    fn find_class(&self, class_name: &str) -> Result<ClassWalker>;
    fn find_function<'a>(&'a self, function_name: &str) -> Result<FunctionWalker<'a>>;
    fn find_client(&self, client_name: &str) -> Result<ClientWalker>;
    fn check_function_params<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        params: &HashMap<&str, serde_json::Value>,
    ) -> Result<()>;
}

impl IRHelper for IntermediateRepr {
    fn find_enum(&self, enum_name: &str) -> Result<EnumWalker> {
        match self.walk_enums().find(|e| e.name() == enum_name) {
            Some(e) => Ok(e),
            None => {
                // Get best match.
                let enums = self.walk_enums().map(|e| e.name()).collect::<Vec<_>>();
                error_not_found!("enum", enum_name, &enums)
            }
        }
    }

    fn find_class(&self, class_name: &str) -> Result<ClassWalker> {
        match self.walk_classes().find(|e| e.name() == class_name) {
            Some(e) => Ok(e),
            None => {
                // Get best match.
                let classes = self.walk_classes().map(|e| e.name()).collect::<Vec<_>>();
                error_not_found!("class", class_name, &classes)
            }
        }
    }

    fn find_function<'a>(&'a self, function_name: &str) -> Result<FunctionWalker<'a>> {
        match self.walk_functions().find(|f| f.name() == function_name) {
            Some(f) => match f.item.elem {
                internal_baml_core::ir::repr::Function::V1(_) => {
                    error_unsupported!(
                        "function",
                        function_name,
                        "legacy functions cannot use the runtime"
                    )
                }
                internal_baml_core::ir::repr::Function::V2(_) => Ok(f),
            },
            None => {
                // Get best match.
                let functions = self
                    .walk_functions()
                    .filter(|f| f.is_v2())
                    .map(|f| f.name())
                    .collect::<Vec<_>>();
                error_not_found!("function", function_name, &functions)
            }
        }
    }

    fn find_client(&self, client_name: &str) -> Result<ClientWalker> {
        match self.walk_clients().find(|c| c.elem().name == client_name) {
            Some(c) => Ok(c),
            None => {
                // Get best match.
                let clients = self
                    .walk_clients()
                    .map(|c| c.elem().name.clone())
                    .collect::<Vec<_>>();
                error_not_found!("client", client_name, &clients)
            }
        }
    }

    fn check_function_params<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        params: &HashMap<&str, serde_json::Value>,
    ) -> Result<()> {
        let function_params = match function.inputs() {
            either::Either::Left(_) => {
                // legacy functions are not supported
                error_unsupported!(
                    "function",
                    function.name(),
                    "legacy functions cannot use the runtime"
                )
            }
            either::Either::Right(defs) => defs,
        };

        // Now check that all required parameters are present.
        let mut scope = ScopeStack::new();
        for (param_name, param_type) in function_params {
            scope.push(param_name.to_string());
            if let Some(param_value) = params.get(param_name.as_str()) {
                validate_value::validate_value(self, param_type, param_value, &mut scope);
            } else {
                // Check if the parameter is optional.
                if !param_type.is_optional() {
                    scope.push_error(format!("Missing required parameter: {}", param_name));
                }
            }
            scope.pop(false);
        }

        if scope.has_errors() {
            anyhow::bail!(scope);
        }

        Ok(())
    }
}

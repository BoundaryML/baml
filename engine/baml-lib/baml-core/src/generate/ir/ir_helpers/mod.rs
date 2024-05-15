mod error_utils;
mod scope_diagnostics;
mod to_baml_arg;

use crate::{
    error_not_found, error_unsupported,
    ir::{
        repr::{IntermediateRepr, Walker},
        Class, Client, Enum, EnumValue, Field, Function, RetryPolicy, TemplateString, TestCase,
    },
};
use anyhow::Result;
use baml_types::{BamlMap, BamlValue};
use indexmap::IndexMap;

use self::scope_diagnostics::ScopeStack;

use super::repr;

pub type FunctionWalker<'a> = Walker<'a, &'a Function>;
pub type EnumWalker<'a> = Walker<'a, &'a Enum>;
pub type EnumValueWalker<'a> = Walker<'a, &'a EnumValue>;
pub type ClassWalker<'a> = Walker<'a, &'a Class>;
pub type TemplateStringWalker<'a> = Walker<'a, &'a TemplateString>;
pub type ClientWalker<'a> = Walker<'a, &'a Client>;
pub type RetryPolicyWalker<'a> = Walker<'a, &'a RetryPolicy>;
pub type TestCaseWalker<'a> = Walker<'a, (&'a Function, &'a TestCase)>;
pub type ClassFieldWalker<'a> = Walker<'a, &'a Field>;

pub trait IRHelper {
    fn find_enum(&self, enum_name: &str) -> Result<EnumWalker<'_>>;
    fn find_class(&self, class_name: &str) -> Result<ClassWalker<'_>>;
    fn find_function(&self, function_name: &str) -> Result<FunctionWalker<'_>>;
    fn find_client(&self, client_name: &str) -> Result<ClientWalker<'_>>;
    fn find_test<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        test_name: &str,
    ) -> Result<TestCaseWalker<'a>>;
    fn check_function_params<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        params: &IndexMap<String, BamlValue>,
    ) -> Result<BamlValue>;
}

impl IRHelper for IntermediateRepr {
    fn find_test<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        test_name: &str,
    ) -> Result<TestCaseWalker<'a>> {
        match function.find_test(test_name) {
            Some(t) => Ok(t),
            None => {
                // Get best match.
                let tests = function
                    .walk_tests()
                    .map(|t| t.item.1.elem.name.clone())
                    .collect::<Vec<_>>();
                error_not_found!("test", test_name, &tests)
            }
        }
    }

    fn find_enum(&self, enum_name: &str) -> Result<EnumWalker<'_>> {
        match self.walk_enums().find(|e| e.name() == enum_name) {
            Some(e) => Ok(e),
            None => {
                // Get best match.
                let enums = self.walk_enums().map(|e| e.name()).collect::<Vec<_>>();
                error_not_found!("enum", enum_name, &enums)
            }
        }
    }

    fn find_class(&self, class_name: &str) -> Result<ClassWalker<'_>> {
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
                repr::Function::V1(_) => {
                    error_unsupported!(
                        "function",
                        function_name,
                        "legacy functions cannot use the runtime"
                    )
                }
                repr::Function::V2(_) => Ok(f),
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

    fn find_client<'ir>(&'ir self, client_name: &str) -> Result<ClientWalker<'ir>> {
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
        params: &IndexMap<String, BamlValue>,
    ) -> Result<BamlValue> {
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
        let mut baml_arg_map = BamlMap::new();
        for (param_name, param_type) in function_params {
            scope.push(param_name.to_string());
            if let Some(param_value) = params.get(param_name.as_str()) {
                if let Some(baml_arg) =
                    to_baml_arg::validate_arg(self, param_type, param_value, &mut scope)
                {
                    baml_arg_map.insert(param_name.to_string(), baml_arg);
                }
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

        Ok(BamlValue::Map(baml_arg_map))
    }
}

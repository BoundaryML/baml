use std::collections::HashSet;

use anyhow::Result;
use baml_types::{
    type_meta::base::StreamingBehavior, BamlValue, EvaluationContext, UnresolvedValue,
};
use indexmap::{IndexMap, IndexSet};
use internal_baml_ast::ast::WithIdentifier;
use internal_baml_diagnostics::Span;
use internal_baml_parser_database::RetryPolicyStrategy;
use internal_llm_client::ClientSpec;

use crate::ir::{
    jinja_helpers::render_expression,
    repr::{self, ExprFunction, FunctionConfig, Node, TypeBuilderEntry, WithRepr},
    Class, Client, Enum, EnumValue, ExprFunctionNode, Field, Function, FunctionNode, IRHelper,
    Impl, IntermediateRepr, RetryPolicy, TemplateString, TestCase, TypeAlias, TypeIR, Walker,
};

impl<'a> Walker<'a, &'a ExprFunctionNode> {
    pub fn name(&self) -> &'a str {
        self.elem().name.as_str()
    }

    pub fn inputs(&self) -> &'a Vec<(String, baml_types::TypeIR)> {
        self.elem().inputs()
    }

    pub fn output(&self) -> &'a baml_types::TypeIR {
        &self.elem().output
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }

    pub fn walk_tests(
        &'a self,
    ) -> impl Iterator<Item = Walker<'a, (&'a ExprFunctionNode, &'a TestCase)>> {
        self.elem().tests().iter().map(|i| Walker {
            ir: self.ir,
            item: (self.item, i),
        })
    }

    pub fn elem(&self) -> &'a repr::ExprFunction {
        &self.item.elem
    }

    pub fn find_test(
        &'a self,
        test_name: &str,
    ) -> Option<Walker<'a, (&'a ExprFunctionNode, &'a TestCase)>> {
        self.walk_tests().find(|t| t.item.1.elem.name == test_name)
    }

    pub fn graph(&self) -> String {
        // example mermaid graph
        r#"
        graph TD
            A[Function] --> B[Function]
            B --> C[Function]
            C --> D[Function]
            D --> E[Function]
        "#
        .to_string()
    }
}

impl<'a> Walker<'a, &'a FunctionNode> {
    pub fn name(&self) -> &'a str {
        self.elem().name()
    }

    pub fn is_v1(&self) -> bool {
        false
    }

    pub fn is_v2(&self) -> bool {
        true
    }

    pub fn client_name(&self) -> Option<String> {
        if let Some(c) = self.elem().configs.first() {
            return Some(c.client.as_str());
        }
        None
    }

    pub fn required_env_vars(&'a self) -> Result<HashSet<String>> {
        if let Some(c) = self.elem().configs.first() {
            match &c.client {
                ClientSpec::Named(n) => {
                    let client: super::ClientWalker<'a> = self.ir.find_client(n)?;
                    Ok(client.required_env_vars())
                }
                ClientSpec::Shorthand(provider, model) => {
                    let options = IndexMap::from_iter([(
                        "model".to_string(),
                        (
                            (),
                            baml_types::UnresolvedValue::String(
                                baml_types::StringOr::Value(model.clone()),
                                (),
                            ),
                        ),
                    )]);
                    let properties = internal_llm_client::PropertyHandler::<()>::new(options, ());
                    if let Ok(client) = provider.parse_client_property(properties) {
                        Ok(client.required_env_vars())
                    } else {
                        // We likely can't make a shorthand client from the given provider
                        Ok(HashSet::new())
                    }
                }
            }
        } else {
            anyhow::bail!("Function {} has no client", self.name())
        }
    }

    pub fn walk_impls(
        &'a self,
    ) -> impl Iterator<Item = Walker<'a, (&'a repr::Function, &'a FunctionConfig)>> {
        self.elem().configs.iter().map(|c| Walker {
            ir: self.ir,
            item: (self.elem(), c),
        })
    }
    pub fn walk_tests(
        &'a self,
    ) -> impl Iterator<Item = Walker<'a, (&'a FunctionNode, &'a TestCase)>> {
        self.elem().tests().iter().map(|i| Walker {
            ir: self.ir,
            item: (self.item, i),
        })
    }

    pub fn find_test(
        &'a self,
        test_name: &str,
    ) -> Option<Walker<'a, (&'a FunctionNode, &'a TestCase)>> {
        self.walk_tests().find(|t| t.item.1.elem.name == test_name)
    }

    pub fn elem(&self) -> &'a repr::Function {
        &self.item.elem
    }

    pub fn output(&self) -> &'a baml_types::TypeIR {
        self.elem().output()
    }

    pub fn inputs(&self) -> &'a Vec<(String, baml_types::TypeIR)> {
        self.elem().inputs()
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }
}

impl<'a> Walker<'a, &'a Enum> {
    pub fn name(&self) -> &'a str {
        &self.elem().name
    }

    pub fn alias(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .alias()
            .map(|v| v.resolve(ctx))
            .transpose()
    }

    pub fn description(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .description()
            .map(|v| v.resolve(ctx))
            .transpose()
    }

    pub fn walk_values(&'a self) -> impl Iterator<Item = Walker<'a, &'a EnumValue>> {
        self.item.elem.values.iter().map(|v| Walker {
            ir: self.ir,
            item: &v.0,
        })
    }

    pub fn find_value(&self, name: &str) -> Option<Walker<'a, &'a EnumValue>> {
        self.item
            .elem
            .values
            .iter()
            .find(|v| v.0.elem.0 == name)
            .map(|v| Walker {
                ir: self.ir,
                item: &v.0,
            })
    }

    pub fn elem(&self) -> &'a repr::Enum {
        &self.item.elem
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }
}

impl<'a> Walker<'a, &'a EnumValue> {
    pub fn skip(&self, ctx: &EvaluationContext<'_>) -> Result<bool> {
        Ok(self.item.attributes.skip())
    }

    pub fn name(&'a self) -> &'a str {
        &self.item.elem.0
    }

    pub fn alias(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .alias()
            .map(|v| v.resolve(ctx))
            .transpose()
    }

    pub fn description(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .description()
            .map(|v| v.resolve(ctx))
            .transpose()
    }
}

impl<'a> Walker<'a, (&'a FunctionNode, &'a Impl)> {
    #[allow(dead_code)]
    pub fn function(&'a self) -> Walker<'a, &'a FunctionNode> {
        Walker {
            ir: self.ir,
            item: self.item.0,
        }
    }

    pub fn elem(&self) -> &'a repr::Implementation {
        &self.item.1.elem
    }
}

impl<'a> Walker<'a, (&'a ExprFunctionNode, &'a TestCase)> {
    pub fn matches(&self, function_name: &str, test_name: &str) -> bool {
        self.item.0.elem.name == function_name && self.item.1.elem.name == test_name
    }

    pub fn name(&self) -> String {
        format!("{}::{}", self.item.0.elem.name, self.item.1.elem.name)
    }

    pub fn args(&self) -> &IndexMap<String, UnresolvedValue<()>> {
        &self.item.1.elem.args
    }

    pub fn test_case(&self) -> &repr::TestCase {
        &self.item.1.elem
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.1.attributes.span.as_ref()
    }

    pub fn test_case_params(
        &self,
        ctx: &EvaluationContext<'_>,
    ) -> Result<IndexMap<String, Result<BamlValue>>> {
        self.args()
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    v.resolve_serde_with_templates::<BamlValue>(ctx, self.ir),
                ))
            })
            .collect()
    }

    pub fn function(&'a self) -> Walker<'a, &'a ExprFunctionNode> {
        Walker {
            ir: self.ir,
            item: self.item.0,
        }
    }
}

impl<'a> Walker<'a, (&'a FunctionNode, &'a TestCase)> {
    pub fn matches(&self, function_name: &str, test_name: &str) -> bool {
        self.item.0.elem.name() == function_name && self.item.1.elem.name == test_name
    }

    pub fn name(&self) -> (&'a str, &'a str) {
        (self.item.0.elem.name(), &self.item.1.elem.name)
    }

    pub fn args(&self) -> &IndexMap<String, UnresolvedValue<()>> {
        &self.item.1.elem.args
    }

    pub fn test_case(&self) -> &repr::TestCase {
        &self.item.1.elem
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.1.attributes.span.as_ref()
    }

    pub fn test_case_params(
        &self,
        ctx: &EvaluationContext<'_>,
    ) -> Result<IndexMap<String, Result<BamlValue>>> {
        self.args()
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    v.resolve_serde_with_templates::<BamlValue>(ctx, self.ir),
                ))
            })
            .collect()
    }

    // TODO: #1343 Temporary solution until we implement scoping in the AST.
    pub fn type_builder_contents(&self) -> &[TypeBuilderEntry] {
        &self.item.1.elem.type_builder.entries
    }

    // TODO: #1343 Temporary solution until we implement scoping in the AST.
    pub fn type_builder_recursive_aliases(&self) -> &[IndexMap<String, TypeIR>] {
        &self.item.1.elem.type_builder.recursive_aliases
    }

    // TODO: #1343 Temporary solution until we implement scoping in the AST.
    pub fn type_builder_recursive_classes(&self) -> &[IndexSet<String>] {
        &self.item.1.elem.type_builder.recursive_classes
    }

    pub fn function(&'a self) -> Walker<'a, &'a FunctionNode> {
        Walker {
            ir: self.ir,
            item: self.item.0,
        }
    }
}

impl<'a> Walker<'a, &'a Class> {
    pub fn name(&self) -> &'a str {
        &self.elem().name
    }

    pub fn alias(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .alias()
            .map(|v| v.resolve(ctx))
            .transpose()
    }

    pub fn description(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .description()
            .map(|v| v.resolve(ctx))
            .transpose()
    }

    pub fn streaming_behavior(&self) -> StreamingBehavior {
        self.item.attributes.streaming_behavior()
    }

    pub fn walk_fields(&'a self) -> impl Iterator<Item = Walker<'a, &'a Field>> {
        self.item.elem.static_fields.iter().map(|f| Walker {
            ir: self.ir,
            item: f,
        })
    }

    pub fn find_field(&'a self, name: &str) -> Option<Walker<'a, &'a Field>> {
        self.item
            .elem
            .static_fields
            .iter()
            .find(|f| f.elem.name == name)
            .map(|f| Walker {
                ir: self.ir,
                item: f,
            })
    }

    pub fn elem(&self) -> &'a repr::Class {
        &self.item.elem
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }

    pub fn inputs(&self) -> &'a Vec<(String, baml_types::TypeIR)> {
        self.elem().inputs()
    }
}

impl<'a> Walker<'a, &'a TypeAlias> {
    pub fn elem(&self) -> &'a repr::TypeAlias {
        &self.item.elem
    }

    pub fn name(&self) -> &'a str {
        &self.elem().name
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }
}

impl<'a> Walker<'a, &'a Client> {
    pub fn elem(&self) -> &'a repr::Client {
        &self.item.elem
    }

    pub fn name(&self) -> &'a str {
        &self.elem().name
    }

    pub fn retry_policy(&self) -> &Option<String> {
        &self.elem().retry_policy_id
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }

    pub fn options(&'a self) -> &'a internal_llm_client::UnresolvedClientProperty<()> {
        &self.elem().options
    }

    pub fn required_env_vars(&'a self) -> HashSet<String> {
        self.options().required_env_vars()
    }
}

impl<'a> Walker<'a, &'a RetryPolicy> {
    pub fn name(&self) -> &'a str {
        &self.elem().name.0
    }

    pub fn elem(&self) -> &'a repr::RetryPolicy {
        &self.item.elem
    }

    pub fn max_retries(&self) -> u32 {
        self.elem().max_retries
    }

    pub fn strategy(&self) -> &RetryPolicyStrategy {
        &self.elem().strategy
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }
}

impl<'a> Walker<'a, &'a TemplateString> {
    pub fn elem(&self) -> &'a repr::TemplateString {
        &self.item.elem
    }

    pub fn name(&self) -> &str {
        self.elem().name.as_str()
    }

    pub fn inputs(&self) -> &'a Vec<repr::Field> {
        &self.item.elem.params
    }

    pub fn template(&self) -> &str {
        &self.elem().content
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }
}

impl<'a> Walker<'a, &'a Field> {
    pub fn name(&self) -> &str {
        &self.elem().name
    }

    pub fn r#type(&'a self) -> &'a baml_types::TypeIR {
        &self.elem().r#type.elem
    }

    pub fn elem(&'a self) -> &'a repr::Field {
        &self.item.elem
    }

    pub fn alias(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .alias()
            .map(|v| v.resolve(ctx))
            .transpose()
    }

    pub fn description(&self, ctx: &EvaluationContext<'_>) -> Result<Option<String>> {
        self.item
            .attributes
            .description()
            .map(|v| v.resolve(ctx))
            .transpose()
    }

    pub fn skip(&self, ctx: &EvaluationContext<'_>) -> Result<bool> {
        Ok(self.item.attributes.skip())
    }

    pub fn streaming_behavior(&self) -> StreamingBehavior {
        self.item.attributes.streaming_behavior()
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }
}

pub struct ExprFnAsFunctionWalker<'ir> {
    pub ir: &'ir IntermediateRepr,
    pub functions: Vec<FunctionNode>,
}

impl<'ir> ExprFnAsFunctionWalker<'ir> {
    pub fn new(ir: &'ir IntermediateRepr) -> Self {
        let functions = ir.expr_fns_as_functions();
        Self { ir, functions }
    }

    pub fn walk_functions(&'ir self) -> impl Iterator<Item = Walker<'ir, &'ir FunctionNode>> {
        self.functions.iter().map(|f| Walker {
            ir: self.ir,
            item: f,
        })
    }
}

impl<'ir> Walker<'ir, &'ir ExprFnAsFunctionWalker<'ir>> {
    pub fn walk_functions(&'ir self) -> impl Iterator<Item = Walker<'ir, &'ir FunctionNode>> {
        self.item.walk_functions()
    }
}

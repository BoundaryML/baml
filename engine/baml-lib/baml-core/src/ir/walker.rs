use anyhow::Result;
use baml_types::BamlValue;
use indexmap::IndexMap;

use internal_baml_parser_database::RetryPolicyStrategy;

use std::collections::HashMap;

use super::{
    repr::{self, FunctionConfig},
    Class, Client, Enum, EnumValue, Expression, Field, FunctionNode, Identifier, Impl, RetryPolicy,
    TemplateString, TestCase, Walker,
};

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

    pub fn client_name(&self) -> Option<&'a str> {
        if let Some(c) = self.elem().configs.first() {
            return Some(c.client.as_str());
        }

        None
    }
    pub fn walk_impls(
        &'a self,
    ) -> impl Iterator<Item = Walker<'a, (&'a repr::Function, &'a FunctionConfig)>> {
        self.elem().configs.iter().map(|c| Walker {
            db: self.db,
            item: (self.elem(), c),
        })
    }
    pub fn walk_tests(
        &'a self,
    ) -> impl Iterator<Item = Walker<'a, (&'a FunctionNode, &'a TestCase)>> {
        self.elem().tests().iter().map(|i| Walker {
            db: self.db,
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

    pub fn output(&self) -> &'a baml_types::FieldType {
        self.elem().output()
    }

    pub fn inputs(&self) -> &'a Vec<(String, baml_types::FieldType)> {
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

    pub fn alias(&self, env_values: &HashMap<String, String>) -> Result<Option<String>> {
        self.item
            .attributes
            .get("alias")
            .map(|v| v.as_string_value(env_values))
            .transpose()
    }

    pub fn walk_values(&'a self) -> impl Iterator<Item = Walker<'a, &'a EnumValue>> {
        self.item.elem.values.iter().map(|v| Walker {
            db: self.db,
            item: v,
        })
    }

    pub fn find_value(&self, name: &str) -> Option<Walker<'a, &'a EnumValue>> {
        self.item
            .elem
            .values
            .iter()
            .find(|v| v.elem.0 == name)
            .map(|v| Walker {
                db: self.db,
                item: v,
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
    pub fn skip(&self, env_values: &HashMap<String, String>) -> Result<bool> {
        self.item
            .attributes
            .get("skip")
            .map(|v| v.as_bool(env_values))
            .unwrap_or(Ok(false))
    }

    pub fn name(&'a self) -> &'a str {
        &self.item.elem.0
    }

    pub fn alias(&self, env_values: &HashMap<String, String>) -> Result<Option<String>> {
        self.item
            .attributes
            .get("alias")
            .map(|v| v.as_string_value(env_values))
            .transpose()
    }

    pub fn description(&self, env_values: &HashMap<String, String>) -> Result<Option<String>> {
        self.item
            .attributes
            .get("description")
            .map(|v| v.as_string_value(env_values))
            .transpose()
    }
}

impl Expression {
    pub fn as_bool(&self, env_values: &HashMap<String, String>) -> Result<bool> {
        match self {
            Expression::Bool(b) => Ok(*b),
            Expression::Identifier(Identifier::ENV(s)) => Ok(env_values.contains_key(s)),
            _ => anyhow::bail!("Expected bool value, got {:?}", self),
        }
    }

    pub fn as_string_value(&self, env_values: &HashMap<String, String>) -> Result<String> {
        match self {
            Expression::String(s) => Ok(s.clone()),
            Expression::RawString(s) => Ok(s.clone()),
            Expression::Identifier(Identifier::ENV(s)) => match env_values.get(s) {
                Some(v) => Ok(v.clone()),
                None => anyhow::bail!("Environment variable {} not found", s),
            },
            Expression::Identifier(idn) => Ok(idn.name().to_string()),
            _ => anyhow::bail!("Expected string value, got {:?}", self),
        }
    }

    /// Normalize an `Expression` into a `BamlValue` in a given context.
    ///
    /// TODO: Modify the context, rename it to `env` and make it a map
    /// from `String` to `BamlValue`. This generalizes the context from
    /// known environment variables to variables set by other means. For
    /// example, we will eventually want to normalize `JinjaExpressions` found
    /// inside an `assert` by augmenting the context with the LLM response.
    pub fn normalize(&self, env_values: &HashMap<String, String>) -> Result<BamlValue> {
        match self {
            Expression::Identifier(idn) => match idn {
                repr::Identifier::ENV(s) => match env_values.get(s) {
                    Some(v) => Ok(BamlValue::String(v.clone())),
                    None => anyhow::bail!("Environment variable {} not found", s),
                },
                repr::Identifier::Ref(r) => Ok(BamlValue::String(r.join(".").to_string())),
                repr::Identifier::Local(r) => match r.as_str() {
                    "true" => Ok(BamlValue::Bool(true)),
                    "false" => Ok(BamlValue::Bool(false)),
                    "null" => Ok(BamlValue::Null),
                    _ => Ok(BamlValue::String(r.to_string())),
                },
                repr::Identifier::Primitive(t) => Ok(BamlValue::String(t.to_string())),
            },
            Expression::Bool(b) => Ok(BamlValue::Bool(*b)),
            Expression::Map(m) => {
                let mut map = baml_types::BamlMap::new();
                for (k, v) in m {
                    map.insert(k.as_string_value(env_values)?, v.normalize(env_values)?);
                }
                Ok(BamlValue::Map(map))
            }
            Expression::List(l) => {
                let mut list = Vec::new();
                for v in l {
                    list.push(v.normalize(env_values)?);
                }
                Ok(BamlValue::List(list))
            }
            Expression::RawString(s) | Expression::String(s) => Ok(BamlValue::String(s.clone())),
            repr::Expression::Numeric(n) => {
                if let Ok(n) = n.parse::<i64>() {
                    Ok(BamlValue::Int(n))
                } else if let Ok(n) = n.parse::<f64>() {
                    Ok(BamlValue::Float(n))
                } else {
                    anyhow::bail!("Invalid numeric value: {}", n)
                }
            }
        }
    }
}

impl<'a> Walker<'a, (&'a FunctionNode, &'a Impl)> {
    #[allow(dead_code)]
    pub fn function(&'a self) -> Walker<'a, &'a FunctionNode> {
        Walker {
            db: self.db,
            item: self.item.0,
        }
    }

    pub fn elem(&self) -> &'a repr::Implementation {
        &self.item.1.elem
    }
}

impl<'a> Walker<'a, (&'a FunctionNode, &'a TestCase)> {
    pub fn matches(&self, function_name: &str, test_name: &str) -> bool {
        self.item.0.elem.name() == function_name && self.item.1.elem.name == test_name
    }

    pub fn name(&self) -> String {
        format!("{}::{}", self.item.0.elem.name(), self.item.1.elem.name)
    }

    pub fn args(&self) -> &IndexMap<String, Expression> {
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
        env_values: &HashMap<String, String>,
    ) -> Result<IndexMap<String, Result<BamlValue>>> {
        self.args()
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.normalize(env_values))))
            .collect()
    }

    pub fn function(&'a self) -> Walker<'a, &'a FunctionNode> {
        Walker {
            db: self.db,
            item: self.item.0,
        }
    }
}

impl<'a> Walker<'a, &'a Class> {
    pub fn name(&self) -> &'a str {
        &self.elem().name
    }

    pub fn alias(&self, env_values: &HashMap<String, String>) -> Result<Option<String>> {
        self.item
            .attributes
            .get("alias")
            .map(|v| v.as_string_value(env_values))
            .transpose()
    }

    pub fn walk_fields(&'a self) -> impl Iterator<Item = Walker<'a, &'a Field>> {
        self.item.elem.static_fields.iter().map(|f| Walker {
            db: self.db,
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
                db: self.db,
                item: f,
            })
    }

    pub fn elem(&self) -> &'a repr::Class {
        &self.item.elem
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }

    pub fn inputs(&self) -> &'a Vec<(String, baml_types::FieldType)> {
        self.elem().inputs()
    }
}

impl<'a> Walker<'a, &'a Client> {
    pub fn elem(&self) -> &'a repr::Client {
        &self.item.elem
    }

    pub fn name(&self) -> &str {
        &self.elem().name
    }

    pub fn retry_policy(&self) -> &Option<String> {
        &self.elem().retry_policy_id
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }

    pub fn options(&self) -> &Vec<(String, Expression)> {
        &self.elem().options
    }
}

impl<'a> Walker<'a, &'a RetryPolicy> {
    pub fn name(&self) -> &str {
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

    pub fn r#type(&'a self) -> &'a baml_types::FieldType {
        &self.elem().r#type.elem
    }

    pub fn elem(&'a self) -> &'a repr::Field {
        &self.item.elem
    }

    pub fn alias(&self, env_values: &HashMap<String, String>) -> Result<Option<String>> {
        self.item
            .attributes
            .get("alias")
            .map(|v| v.as_string_value(env_values))
            .transpose()
    }

    pub fn description(&self, env_values: &HashMap<String, String>) -> Result<Option<String>> {
        self.item
            .attributes
            .get("description")
            .map(|v| v.as_string_value(env_values))
            .transpose()
    }

    pub fn span(&self) -> Option<&crate::Span> {
        self.item.attributes.span.as_ref()
    }
}

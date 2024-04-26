use anyhow::Result;
use std::collections::HashMap;

use super::{
    repr::{self, Field, FunctionConfig},
    Class, Client, Enum, EnumValue, Expression, Function, FunctionV2, Identifier, Impl,
    RetryPolicy, TemplateString, TestCase, Walker,
};

impl<'a> Walker<'a, &'a Function> {
    pub fn name(&self) -> &'a str {
        self.elem().name()
    }

    pub fn is_v1(&self) -> bool {
        matches!(self.item.elem, repr::Function::V1(_))
    }

    pub fn is_v2(&self) -> bool {
        matches!(self.item.elem, repr::Function::V2(_))
    }

    pub fn as_v2(&self) -> Option<&'a FunctionV2> {
        match &self.item.elem {
            repr::Function::V1(_) => None,
            repr::Function::V2(f) => Some(f),
        }
    }

    pub fn walk_impls(
        &'a self,
    ) -> either::Either<
        impl Iterator<Item = Walker<'a, (&'a Function, &'a Impl)>>,
        impl Iterator<Item = Walker<'a, (&'a Function, &'a FunctionConfig)>>,
    > {
        match &self.item.elem {
            repr::Function::V1(f) => either::Either::Left(f.impls.iter().map(|i| Walker {
                db: self.db,
                item: (self.item, i),
            })),
            repr::Function::V2(f) => either::Either::Right(f.configs.iter().map(|c| Walker {
                db: self.db,
                item: (self.item, c),
            })),
        }
    }

    pub fn walk_tests(&'a self) -> impl Iterator<Item = Walker<'a, (&'a Function, &'a TestCase)>> {
        self.tests().iter().map(|i| Walker {
            db: self.db,
            item: (self.item, i),
        })
    }

    fn tests(&self) -> &'a Vec<TestCase> {
        match &self.item.elem {
            repr::Function::V1(f) => &f.tests,
            repr::Function::V2(f) => &f.tests,
        }
    }

    pub fn elem(&self) -> &'a repr::Function {
        &self.item.elem
    }

    pub fn output(&self) -> &'a repr::FieldType {
        match &self.item.elem {
            repr::Function::V1(f) => &f.output.elem,
            repr::Function::V2(f) => &f.output.elem,
        }
    }

    pub fn inputs(
        &self,
    ) -> either::Either<&'a repr::FunctionArgs, &'a Vec<(String, repr::FieldType)>> {
        self.item.elem.inputs()
    }
}

impl<'a> Walker<'a, &'a Enum> {
    pub fn name(&self) -> &'a str {
        &self.elem().name
    }

    pub fn walk_values(&'a self) -> impl Iterator<Item = Walker<'a, &'a EnumValue>> {
        self.item.elem.values.iter().map(|v| Walker {
            db: self.db,
            item: v,
        })
    }

    pub fn elem(&self) -> &'a repr::Enum {
        &self.item.elem
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

    pub fn name(&self) -> &str {
        &self.item.elem.0
    }

    pub fn valid_values(&self, env_values: &HashMap<String, String>) -> Result<Vec<String>> {
        let name = self
            .item
            .attributes
            .get("alias")
            .map(|s| s.as_string_value(env_values));

        let name = match name {
            Some(Ok(s)) => s,
            Some(Err(e)) => anyhow::bail!("Error parsing alias: {:?}", e),
            None => self.item.elem.0.clone(),
        };

        let name = name.trim();

        let description = self
            .item
            .attributes
            .get("description")
            .map(|s| s.as_string_value(env_values));

        match &description {
            Some(Ok(s)) => {
                let s = s.trim();
                // For enums, we generate one for "name", one for "description", and one for "name: description"
                // (this means that we currently don't support deserializing "name[^a-zA-Z0-9]{1,5}description" but
                // for now it suffices)
                Ok(vec![name.into(), s.into(), format!("{}: {}", name, s)])
            }
            Some(Err(e)) => anyhow::bail!("Error parsing description: {:?}", e),
            None => Ok(vec![name.into()]),
        }
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
            _ => anyhow::bail!("Expected string value, got {:?}", self),
        }
    }
}

impl<'a> Walker<'a, (&'a Function, &'a Impl)> {
    #[allow(dead_code)]
    pub fn function(&'a self) -> Walker<'a, &'a Function> {
        Walker {
            db: self.db,
            item: self.item.0,
        }
    }

    pub fn elem(&self) -> &'a repr::Implementation {
        &self.item.1.elem
    }
}

impl<'a> Walker<'a, &'a Class> {
    pub fn name(&self) -> &'a str {
        &self.elem().name
    }

    #[allow(dead_code)]
    pub fn walk_fields(&'a self) -> impl Iterator<Item = &'a repr::Field> {
        self.item.elem.static_fields.iter().map(|f| &f.elem)
    }

    pub fn elem(&self) -> &'a repr::Class {
        &self.item.elem
    }
}

impl<'a> Walker<'a, &'a Client> {
    pub fn elem(&self) -> &'a repr::Client {
        &self.item.elem
    }

    pub fn retry_policy(&self) -> Option<Walker<'a, &'a RetryPolicy>> {
        self.elem()
            .retry_policy_id
            .as_ref()
            .and_then(|policy| self.db.walk_retry_policies().find(|r| r.name() == policy))
    }
}

impl<'a> Walker<'a, &'a RetryPolicy> {
    pub fn name(&self) -> &str {
        &self.elem().name.0
    }

    pub fn elem(&self) -> &'a repr::RetryPolicy {
        &self.item.elem
    }
}

impl<'a> Walker<'a, &'a TemplateString> {
    pub fn elem(&self) -> &'a repr::TemplateString {
        &self.item.elem
    }

    pub fn name(&self) -> &str {
        self.elem().name.as_str()
    }

    pub fn inputs(&self) -> &'a Vec<Field> {
        &self.item.elem.params
    }

    pub fn template(&self) -> &str {
        &self.elem().content
    }
}

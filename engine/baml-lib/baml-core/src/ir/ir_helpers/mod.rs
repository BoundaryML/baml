mod error_utils;
pub mod scope_diagnostics;
mod to_baml_arg;

use self::scope_diagnostics::ScopeStack;
use crate::{
    error_not_found,
    ir::{
        repr::{IntermediateRepr, Walker},
        Class, Client, Enum, EnumValue, Field, FunctionNode, RetryPolicy, TemplateString, TestCase,
    },
};
use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta, FieldType, TypeValue};
pub use to_baml_arg::ArgCoercer;

use super::repr;

pub type FunctionWalker<'a> = Walker<'a, &'a FunctionNode>;
pub type EnumWalker<'a> = Walker<'a, &'a Enum>;
pub type EnumValueWalker<'a> = Walker<'a, &'a EnumValue>;
pub type ClassWalker<'a> = Walker<'a, &'a Class>;
pub type TemplateStringWalker<'a> = Walker<'a, &'a TemplateString>;
pub type ClientWalker<'a> = Walker<'a, &'a Client>;
pub type RetryPolicyWalker<'a> = Walker<'a, &'a RetryPolicy>;
pub type TestCaseWalker<'a> = Walker<'a, (&'a FunctionNode, &'a TestCase)>;
pub type ClassFieldWalker<'a> = Walker<'a, &'a Field>;

pub trait IRHelper {
    fn find_enum(&self, enum_name: &str) -> Result<EnumWalker<'_>>;
    fn find_class(&self, class_name: &str) -> Result<ClassWalker<'_>>;
    fn find_function(&self, function_name: &str) -> Result<FunctionWalker<'_>>;
    fn find_client(&self, client_name: &str) -> Result<ClientWalker<'_>>;
    fn find_retry_policy(&self, retry_policy_name: &str) -> Result<RetryPolicyWalker<'_>>;
    fn find_template_string(&self, template_string_name: &str) -> Result<TemplateStringWalker<'_>>;
    fn find_test<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        test_name: &str,
    ) -> Result<TestCaseWalker<'a>>;
    fn check_function_params<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        params: &BamlMap<String, BamlValue>,
        coerce_settings: ArgCoercer,
    ) -> Result<BamlValue>;
    fn distribute_type<'a>(&'a self, value: BamlValue, field_type: &'a FieldType) -> Result<BamlValueWithMeta<&'a FieldType>>;
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
                    .map(|t| t.item.1.elem.name.as_str())
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

    fn find_class<'a>(&'a self, class_name: &str) -> Result<ClassWalker<'a>> {
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
                repr::Function { .. } => Ok(f),
            },
            None => {
                // Get best match.
                let functions = self.walk_functions().map(|f| f.name()).collect::<Vec<_>>();
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
                    .map(|c| c.elem().name.as_str())
                    .collect::<Vec<_>>();
                error_not_found!("client", client_name, &clients)
            }
        }
    }

    // find_retry_policy
    fn find_retry_policy(&self, retry_policy_name: &str) -> Result<RetryPolicyWalker<'_>> {
        match self
            .walk_retry_policies()
            .find(|r| r.name() == retry_policy_name)
        {
            Some(r) => Ok(r),
            None => {
                // Get best match.
                let retry_policies = self
                    .walk_retry_policies()
                    .map(|r| r.elem().name.0.as_str())
                    .collect::<Vec<_>>();
                error_not_found!("retry policy", retry_policy_name, &retry_policies)
            }
        }
    }

    // find_template_string
    fn find_template_string(&self, template_string_name: &str) -> Result<TemplateStringWalker<'_>> {
        match self
            .walk_template_strings()
            .find(|t| t.name() == template_string_name)
        {
            Some(t) => Ok(t),
            None => {
                // Get best match.
                let template_strings = self
                    .walk_template_strings()
                    .map(|t| t.elem().name.as_str())
                    .collect::<Vec<_>>(); // Ensure the collected type is owned
                error_not_found!("template string", template_string_name, &template_strings)
            }
        }
    }

    fn check_function_params<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        params: &BamlMap<String, BamlValue>,
        coerce_settings: ArgCoercer,
    ) -> Result<BamlValue> {
        let function_params = function.inputs();

        // Now check that all required parameters are present.
        let mut scope = ScopeStack::new();
        let mut baml_arg_map = BamlMap::new();
        for (param_name, param_type) in function_params {
            scope.push(param_name.to_string());
            if let Some(param_value) = params.get(param_name.as_str()) {
                if let Ok(baml_arg) =
                    coerce_settings.coerce_arg(self, param_type, param_value, &mut scope)
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
            Err(anyhow::anyhow!(scope))
        } else {
            Ok(BamlValue::Map(baml_arg_map))
        }
    }

    /// For some `BamlValue` with type `FieldType`, walk the structure of both the value
    /// and the type simultaneously, associating each node in the `BamlValue` with its
    /// `FieldType`.
    fn distribute_type<'a>(
        &'a self,
        value: BamlValue,
        field_type: &'a FieldType,
    ) -> anyhow::Result<BamlValueWithMeta<&'a FieldType>> {
        let (unconstrained_type, _) = field_type.distribute_constraints();
        match (value, unconstrained_type) {

            (BamlValue::String(s), FieldType::Primitive(TypeValue::String)) => Ok(BamlValueWithMeta::String(s, field_type)),
            (BamlValue::String(_), _) => anyhow::bail!("Could not unify Strinig with {:?}", field_type),

            (BamlValue::Int(i), FieldType::Primitive(TypeValue::Int)) => Ok(BamlValueWithMeta::Int(i, field_type)),
            (BamlValue::Int(_), _) => anyhow::bail!("Could not unify Int with {:?}", field_type),

            (BamlValue::Float(f), FieldType::Primitive(TypeValue::Float)) => Ok(BamlValueWithMeta::Float(f, field_type)),
            (BamlValue::Float(_), _) => anyhow::bail!("Could not unify Float with {:?}", field_type),

            (BamlValue::Bool(b), FieldType::Primitive(TypeValue::Bool)) => Ok(BamlValueWithMeta::Bool(b, field_type)),
            (BamlValue::Bool(_), _) => anyhow::bail!("Could not unify Bool with {:?}", field_type),

            (BamlValue::Null, FieldType::Primitive(TypeValue::Null)) => Ok(BamlValueWithMeta::Null(field_type)),
            (BamlValue::Null, _) => anyhow::bail!("Could not unify Null with {:?}", field_type),

            (BamlValue::Map(pairs), FieldType::Map(k,val_type)) => {
                let mapped_fields: BamlMap<String, BamlValueWithMeta<&FieldType>> =
                    pairs
                    .into_iter()
                    .map(|(key, val)| {
                        let sub_value = self.distribute_type(val, val_type.as_ref())?;
                        Ok((key, sub_value))
                    })
                    .collect::<anyhow::Result<BamlMap<String,BamlValueWithMeta<&FieldType>>>>()?;
                Ok(BamlValueWithMeta::Map( mapped_fields, field_type ))
            },
            (BamlValue::Map(_), _) => anyhow::bail!("Could not unify Map with {:?}", field_type),

            (BamlValue::List(items), FieldType::List(item_type)) => {
                let mapped_items: Vec<BamlValueWithMeta<&FieldType>> =
                    items
                        .into_iter()
                        .map(|i| self.distribute_type(i, item_type))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(BamlValueWithMeta::List(mapped_items, field_type))
            }
            (BamlValue::List(_), _) => anyhow::bail!("Could not unify List with {:?}", field_type),

            (BamlValue::Media(m), FieldType::Primitive(TypeValue::Media(_))) => Ok(BamlValueWithMeta::Media(m, field_type)),
            (BamlValue::Media(_), _) => anyhow::bail!("Could not unify Media with {:?}", field_type),

            (BamlValue::Enum(name, val), FieldType::Enum(type_name)) => if name == *type_name {
                Ok(BamlValueWithMeta::Enum(name, val, field_type))
            } else {
                Err(anyhow::anyhow!("Could not unify Enum {name} with Enum type {type_name}"))
            }
            (BamlValue::Enum(enum_name,_), _) => anyhow::bail!("Could not unify Enum {enum_name} with {:?}", field_type),

            (BamlValue::Class(name, fields), FieldType::Class(type_name)) => if name == *type_name {
                let class_type = &self.find_class(type_name)?.item.elem;
                let class_fields: BamlMap<&str, &FieldType> = class_type.static_fields.iter().map(|field_node| (field_node.elem.name.as_ref(), &field_node.elem.r#type.elem)).collect();
                let mapped_fields = fields.into_iter().map(|(k,v)| {
                    let field_type = class_fields.get(k.as_str()).context("Could not find field {k} in class {name}")?;
                    let mapped_field = self.distribute_type(v, field_type)?;
                    Ok((k, mapped_field))
                }).collect::<anyhow::Result<BamlMap<String, BamlValueWithMeta<&FieldType>>>>()?;
                Ok(BamlValueWithMeta::Class(name, mapped_fields, field_type))
            } else {
                Err(anyhow::anyhow!("Could not unify Class {name} with Class type {type_name}"))
            }
            (BamlValue::Class(class_name,_), _) => anyhow::bail!("Could not unify Class {class_name} with {:?}", field_type),

        }
    }
}

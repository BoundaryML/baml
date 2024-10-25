mod error_utils;
pub mod scope_diagnostics;
mod to_baml_arg;

use itertools::Itertools;

use self::scope_diagnostics::ScopeStack;
use crate::{
    error_not_found,
    ir::{
        repr::{IntermediateRepr, Walker},
        Class, Client, Enum, EnumValue, Field, FunctionNode, RetryPolicy, TemplateString, TestCase,
    },
};
use anyhow::Result;
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
    fn distribute_type<'a>(
        &'a self,
        value: BamlValue,
        field_type: FieldType,
    ) -> Result<BamlValueWithMeta<FieldType>>;
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
        field_type: FieldType,
    ) -> anyhow::Result<BamlValueWithMeta<FieldType>> {
        match value {
            BamlValue::String(s)
                if FieldType::Primitive(TypeValue::String).is_subtype_of(&field_type) =>
            {
                Ok(BamlValueWithMeta::String(s, field_type))
            }
            BamlValue::String(_) => anyhow::bail!("Could not unify Strinig with {:?}", field_type),

            BamlValue::Int(i)
                if FieldType::Primitive(TypeValue::Int).is_subtype_of(&field_type) =>
            {
                Ok(BamlValueWithMeta::Int(i, field_type))
            }

            BamlValue::Int(_) => anyhow::bail!("Could not unify Int with {:?}", field_type),

            BamlValue::Float(f)
                if FieldType::Primitive(TypeValue::Float).is_subtype_of(&field_type) =>
            {
                Ok(BamlValueWithMeta::Float(f, field_type))
            }
            BamlValue::Float(_) => anyhow::bail!("Could not unify Float with {:?}", field_type),

            BamlValue::Bool(b)
                if FieldType::Primitive(TypeValue::Bool).is_subtype_of(&field_type) =>
            {
                Ok(BamlValueWithMeta::Bool(b, field_type))
            }
            BamlValue::Bool(_) => anyhow::bail!("Could not unify Bool with {:?}", field_type),

            BamlValue::Null if FieldType::Primitive(TypeValue::Null).is_subtype_of(&field_type) => {
                Ok(BamlValueWithMeta::Null(field_type))
            }
            BamlValue::Null => anyhow::bail!("Could not unify Null with {:?}", field_type),

            BamlValue::Map(pairs) => {
                let item_types = pairs
                    .iter()
                    .filter_map(|(_, v)| infer_type(v))
                    .dedup()
                    .collect::<Vec<_>>();
                let maybe_item_type = match item_types.len() {
                    0 => None,
                    1 => Some(item_types[0].clone()),
                    _ => Some(FieldType::Union(item_types)),
                };

                match maybe_item_type {
                    Some(item_type) => {
                        let map_type = FieldType::Map(
                            Box::new(FieldType::Primitive(TypeValue::String)),
                            Box::new(item_type.clone()),
                        );

                        if !map_type.is_subtype_of(&field_type) {
                            anyhow::bail!("Could not unify {:?} with {:?}", map_type, field_type);
                        } else {
                            let mapped_fields: BamlMap<String, BamlValueWithMeta<FieldType>> =
                                    pairs
                                    .into_iter()
                                    .map(|(key, val)| {
                                        let sub_value = self.distribute_type(val, item_type.clone())?;
                                        Ok((key, sub_value))
                                    })
                                    .collect::<anyhow::Result<BamlMap<String,BamlValueWithMeta<FieldType>>>>()?;
                            Ok(BamlValueWithMeta::Map(mapped_fields, field_type))
                        }
                    }
                    None => Ok(BamlValueWithMeta::Map(BamlMap::new(), field_type)),
                }
            }

            BamlValue::List(items) => {
                let item_types = items
                    .iter()
                    .filter_map(|v| infer_type(v))
                    .dedup()
                    .collect::<Vec<_>>();
                let maybe_item_type = match item_types.len() {
                    0 => None,
                    1 => Some(item_types[0].clone()),
                    _ => Some(FieldType::Union(item_types)),
                };
                match maybe_item_type.as_ref() {
                    None => Ok(BamlValueWithMeta::List(vec![], field_type)),
                    Some(item_type) => {
                        let list_type = FieldType::List(Box::new(item_type.clone()));

                        if !list_type.is_subtype_of(&field_type) {
                            anyhow::bail!("Could not unify {:?} with {:?}", list_type, field_type);
                        } else {
                            let mapped_items: Vec<BamlValueWithMeta<FieldType>> = items
                                .into_iter()
                                .map(|i| self.distribute_type(i, item_type.clone()))
                                .collect::<anyhow::Result<Vec<_>>>()?;
                            Ok(BamlValueWithMeta::List(mapped_items, field_type))
                        }
                    }
                }
            }

            BamlValue::Media(m)
                if FieldType::Primitive(TypeValue::Media(m.media_type))
                    .is_subtype_of(&field_type) =>
            {
                Ok(BamlValueWithMeta::Media(m, field_type))
            }
            BamlValue::Media(_) => anyhow::bail!("Could not unify Media with {:?}", field_type),

            BamlValue::Enum(name, val) => {
                if FieldType::Enum(name.clone()).is_subtype_of(&field_type) {
                    Ok(BamlValueWithMeta::Enum(name, val, field_type))
                } else {
                    anyhow::bail!("Could not unify Enum {name} with {:?}", field_type)
                }
            }

            BamlValue::Class(name, fields) => {
                if !FieldType::Class(name.clone()).is_subtype_of(&field_type) {
                    anyhow::bail!("Could not unify Class {name} with {:?}", field_type);
                } else {
                    let class_type = &self.find_class(&name)?.item.elem;
                    let class_fields: BamlMap<String, FieldType> = class_type
                        .static_fields
                        .iter()
                        .map(|field_node| {
                            (
                                field_node.elem.name.clone(),
                                field_node.elem.r#type.elem.clone(),
                            )
                        })
                        .collect();
                    let mapped_fields = fields
                        .into_iter()
                        .map(|(k, v)| {
                            let field_type = match class_fields.get(k.as_str()) {
                                Some(ft) => ft.clone(),
                                None => infer_type(&v).unwrap_or(UNIT_TYPE),
                            };
                            let mapped_field = self.distribute_type(v, field_type)?;
                            Ok((k, mapped_field))
                        })
                        .collect::<anyhow::Result<BamlMap<String, BamlValueWithMeta<FieldType>>>>(
                        )?;
                    Ok(BamlValueWithMeta::Class(name, mapped_fields, field_type))
                }
            }
        }
    }
}

const UNIT_TYPE: FieldType = FieldType::Tuple(vec![]);

/// Derive the simplest type that can categorize a given value. This is meant to be used
/// by `distribute_type`, for dynamic fields of classes, whose types are not known statically.
pub fn infer_type<'a>(value: &'a BamlValue) -> Option<FieldType> {
    let ret = match value {
        BamlValue::Int(_) => Some(FieldType::Primitive(TypeValue::Int)),
        BamlValue::Bool(_) => Some(FieldType::Primitive(TypeValue::Bool)),
        BamlValue::Float(_) => Some(FieldType::Primitive(TypeValue::Float)),
        BamlValue::String(_) => Some(FieldType::Primitive(TypeValue::String)),
        BamlValue::Null => Some(FieldType::Primitive(TypeValue::Null)),
        BamlValue::Map(pairs) => {
            let v_tys = pairs
                .iter()
                .filter_map(|(_, v)| infer_type(v))
                .dedup()
                .collect::<Vec<_>>();
            let k_ty = FieldType::Primitive(TypeValue::String);
            let v_ty = match v_tys.len() {
                0 => None,
                1 => Some(v_tys[0].clone()),
                _ => Some(FieldType::Union(v_tys)),
            }?;
            Some(FieldType::Map(Box::new(k_ty), Box::new(v_ty)))
        }
        BamlValue::List(items) => {
            let item_tys = items
                .iter()
                .filter_map(|v| infer_type(v))
                .dedup()
                .collect::<Vec<_>>();
            let item_ty = match item_tys.len() {
                0 => None,
                1 => Some(item_tys[0].clone()),
                _ => Some(FieldType::Union(item_tys)),
            }?;
            Some(FieldType::List(Box::new(item_ty)))
        }
        BamlValue::Media(m) => Some(FieldType::Primitive(TypeValue::Media(m.media_type))),
        BamlValue::Enum(enum_name, _) => Some(FieldType::Enum(enum_name.clone())),
        BamlValue::Class(class_name, _) => Some(FieldType::Class(class_name.clone())),
    };
    ret
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::{
        BamlMedia, BamlMediaContent, BamlMediaType, BamlValue, FieldType, MediaBase64, TypeValue,
    };
    use repr::make_test_ir;

    fn int_type() -> FieldType {
        FieldType::Primitive(TypeValue::Int)
    }

    fn string_type() -> FieldType {
        FieldType::Primitive(TypeValue::String)
    }

    fn mk_int(i: i64) -> BamlValue {
        BamlValue::Int(i)
    }

    fn mk_list_1() -> BamlValue {
        BamlValue::List(vec![mk_int(1), mk_int(2)])
    }

    fn mk_map_1() -> BamlValue {
        BamlValue::Map(vec![("a".to_string(), mk_int(1))].into_iter().collect())
    }

    fn mk_ir() -> IntermediateRepr {
        make_test_ir(
            r#"
          class Foo {
            f_int int
            f_int_string int | string
            f_list int[]
          }
        "#,
        )
        .unwrap()
    }

    #[test]
    fn infer_int() {
        assert_eq!(infer_type(&mk_int(1)).unwrap(), int_type());
    }

    #[test]
    fn infer_list() {
        let my_list = mk_list_1();
        assert_eq!(
            infer_type(&my_list).unwrap(),
            FieldType::List(Box::new(int_type()))
        );
    }

    #[test]
    fn infer_map() {
        let my_map = mk_map_1();
        assert_eq!(
            infer_type(&my_map).unwrap(),
            FieldType::Map(Box::new(string_type()), Box::new(int_type()))
        );
    }

    #[test]
    fn infer_map_map() {
        let my_map_map = BamlValue::Map(
            vec![("map_a".to_string(), mk_map_1())]
                .into_iter()
                .collect(),
        );
        assert_eq!(
            infer_type(&my_map_map).unwrap(),
            FieldType::Map(
                Box::new(string_type()),
                Box::new(FieldType::Map(
                    Box::new(string_type()),
                    Box::new(int_type())
                ))
            )
        )
    }

    #[test]
    fn distribute_int() {
        let ir = mk_ir();
        let value = ir.distribute_type(mk_int(1), int_type()).unwrap();
        assert_eq!(value.meta(), &int_type());
    }

    #[test]
    fn distribute_media_union() {
        let ir = mk_ir();
        let field_type = FieldType::Union(vec![
            string_type(),
            FieldType::Primitive(TypeValue::Media(BamlMediaType::Image)),
        ]);
        let baml_value = BamlValue::Media(BamlMedia {
            media_type: BamlMediaType::Image,
            mime_type: None,
            content: BamlMediaContent::Base64(MediaBase64 {
                base64: "abcd1234=".to_string(),
            }),
        });
        let value = ir.distribute_type(baml_value, field_type.clone()).unwrap();
        assert_eq!(value.meta(), &field_type);
    }

    #[test]
    fn distribute_list_of_maps() {
        let ir = mk_ir();

        let elem_type = FieldType::Union(vec![
            string_type(),
            int_type(),
            FieldType::Class("Foo".to_string()),
        ]);
        let map_type = FieldType::Map(Box::new(string_type()), Box::new(elem_type.clone()));

        // The compound type we want to test.
        let list_type = FieldType::List(Box::new(map_type.clone()));

        let map_1 = BamlValue::Map(
            vec![
                (
                    "1_string".to_string(),
                    BamlValue::String("1_string_value".to_string()),
                ),
                ("1_int".to_string(), mk_int(1)),
                (
                    "1_foo".to_string(),
                    BamlValue::Class(
                        "Foo".to_string(),
                        vec![
                            ("f_int".to_string(), mk_int(10)),
                            ("f_int_string".to_string(), mk_int(20)),
                            (
                                "f_list".to_string(),
                                BamlValue::List(vec![mk_int(30), mk_int(40), mk_int(50)]),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        );
        let map_2 = BamlValue::Map(vec![].into_iter().collect());

        // The compound value we want to test.
        let list = BamlValue::List(vec![map_1, map_2]);

        let value = ir.distribute_type(list, list_type.clone()).unwrap();
        let mut nodes = value.iter();

        let head = nodes.next().unwrap();
        assert_eq!(head.meta(), &list_type);
    }

    #[test]
    fn distribute_map_of_lists() {
        let ir = mk_ir();

        let elem_type = FieldType::Union(vec![
            string_type(),
            int_type(),
            FieldType::Class("Foo".to_string()),
        ]);

        let list_type = FieldType::List(Box::new(elem_type));

        // The compound type we want to test.
        let map_type = FieldType::Map(Box::new(string_type()), Box::new(list_type));

        let foo_1 = BamlValue::Class(
            "Foo".to_string(),
            vec![
                (
                    "f_string".to_string(),
                    BamlValue::String("f_string_value_1".to_string()),
                ),
                (
                    "f_int_string".to_string(),
                    BamlValue::String("f_int_string_value_1".to_string()),
                ),
                ("f_list".to_string(), BamlValue::List(vec![])),
            ]
            .into_iter()
            .collect(),
        );
        let foo_2 = BamlValue::Class(
            "Foo".to_string(),
            vec![
                (
                    "f_string".to_string(),
                    BamlValue::String("f_string_value".to_string()),
                ),
                ("f_int_string".to_string(), mk_int(2)),
                (
                    "f_list".to_string(),
                    BamlValue::List(vec![mk_int(3), mk_int(4)]),
                ),
            ]
            .into_iter()
            .collect(),
        );

        let list_1 = BamlValue::List(vec![]);
        let list_2 = BamlValue::List(vec![foo_1, foo_2]);

        // The compound value we want to test.
        let map = BamlValue::Map(
            vec![
                ("a".to_string(), list_1.clone()),
                ("b".to_string(), list_1),
                ("c".to_string(), list_2),
            ]
            .into_iter()
            .collect(),
        );

        let value = ir.distribute_type(map, map_type.clone()).unwrap();
        let mut nodes = value.iter();

        let head = nodes.next().unwrap();
        assert_eq!(head.meta(), &map_type);
    }
}

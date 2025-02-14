mod error_utils;
pub mod scope_diagnostics;
mod to_baml_arg;

use indexmap::IndexMap;
use itertools::Itertools;

use self::scope_diagnostics::ScopeStack;
use crate::{
    error_not_found,
    ir::{
        repr::{IntermediateRepr, Walker},
        Class, Client, Enum, EnumValue, Field, FunctionNode, RetryPolicy, TemplateString, TestCase,
        TypeAlias,
    },
};
use anyhow::Result;
use baml_types::{
    BamlMap, BamlValue, BamlValueWithMeta, Constraint, ConstraintLevel, FieldType, LiteralValue,
    StreamingBehavior, TypeValue,
};
pub use to_baml_arg::ArgCoercer;

use super::repr;

pub type FunctionWalker<'a> = Walker<'a, &'a FunctionNode>;
pub type EnumWalker<'a> = Walker<'a, &'a Enum>;
pub type EnumValueWalker<'a> = Walker<'a, &'a EnumValue>;
pub type ClassWalker<'a> = Walker<'a, &'a Class>;
pub type TypeAliasWalker<'a> = Walker<'a, &'a TypeAlias>;
pub type TemplateStringWalker<'a> = Walker<'a, &'a TemplateString>;
pub type ClientWalker<'a> = Walker<'a, &'a Client>;
pub type RetryPolicyWalker<'a> = Walker<'a, &'a RetryPolicy>;
pub type TestCaseWalker<'a> = Walker<'a, (&'a FunctionNode, &'a TestCase)>;
pub type ClassFieldWalker<'a> = Walker<'a, &'a Field>;

pub trait IRHelper {
    fn find_enum<'a>(&'a self, enum_name: &str) -> Result<EnumWalker<'a>>;
    fn find_class<'a>(&'a self, class_name: &str) -> Result<ClassWalker<'a>>;
    fn find_type_alias<'a>(&'a self, alias_name: &str) -> Result<TypeAliasWalker<'a>>;
    fn find_function<'a>(&'a self, function_name: &str) -> Result<FunctionWalker<'a>>;
    fn find_client<'a>(&'a self, client_name: &str) -> Result<ClientWalker<'a>>;
    fn find_retry_policy<'a>(&'a self, retry_policy_name: &str) -> Result<RetryPolicyWalker<'a>>;
    fn find_template_string<'a>(
        &'a self,
        template_string_name: &str,
    ) -> Result<TemplateStringWalker<'a>>;
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

    /// BAML does not support class-based subtyping. Nonetheless some builtin
    /// BAML types are subtypes of others, and we need to be able to test this
    /// when checking the types of values.
    ///
    /// For examples of pairs of types and their subtyping relationship, see
    /// this module's test suite.
    ///
    /// Consider renaming this to `is_assignable`.
    fn is_subtype(&self, base: &FieldType, other: &FieldType) -> bool;

    /// For some `BamlValue` with type `FieldType`, walk the structure of both the value
    /// and the type simultaneously, associating each node in the `BamlValue` with its
    /// `FieldType`.
    fn distribute_type(
        &self,
        value: BamlValue,
        field_type: FieldType,
    ) -> anyhow::Result<BamlValueWithMeta<FieldType>>;

    /// For some `BamlValueWithMeta` with type `FieldType`, walk the structure of both the value
    /// and the type simultaneously, associating each node in the `BamlValue` with its
    /// `FieldType`.
    /// TODO (Greg): Make this function DynamicTypes-aware. Right now it assigns default metadata
    /// to unknown classes, which may have been created with TypeBuilder.
    fn distribute_type_with_meta<T: Clone + std::fmt::Debug>(
        &self,
        value: BamlValueWithMeta<T>,
        field_type: FieldType,
        // default_meta: Option<&T>,
    ) -> Result<BamlValueWithMeta<(T, FieldType)>>;

    /// For any FieldType, check if the field type is FieldType::WithMetadata,
    /// and if so, return the metadata alongside the base type.
    /// All other field types will be returned as is, alongside default metadata.
    fn distribute_metadata<'a>(
        &'a self,
        field_type: &'a FieldType,
    ) -> (&'a FieldType, (Vec<Constraint>, StreamingBehavior));

    /// Constraints may live in several places. A constrained base type stors its
    /// constraints by wrapping itself in the `FieldType::WithMetadata` constructor.
    /// Additionally, `FieldType::Class` may have constraints stored in its class node,
    /// and `FieldType::Enum` can store constraints in its `Enum` node.
    /// And the `FieldType::WithMetadata` constructor might wrap another
    /// `FieldType::WithMetadata` constructor.
    ///
    /// This function collects constraints for a given type from all these
    /// possible sources. Whenever querying a type for its constraints, you
    /// should do so with this function, instead of searching manually for all
    /// the places that Constraints can live.
    fn distribute_constraints<'a>(
        &'a self,
        field_type: &'a FieldType,
    ) -> (&'a FieldType, Vec<Constraint>);
    fn type_has_constraints(&self, field_type: &FieldType) -> bool;
    fn type_has_checks(&self, field_type: &FieldType) -> bool;
    fn recursive_alias_definition(&self, alias_name: &str) -> Option<&FieldType>;
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

    fn find_type_alias<'a>(&'a self, alias_name: &str) -> Result<TypeAliasWalker<'a>> {
        match self.walk_type_aliases().find(|e| e.name() == alias_name) {
            Some(e) => Ok(e),
            None => {
                // Get best match.
                let aliases = self
                    .walk_type_aliases()
                    .map(|e| e.name())
                    .collect::<Vec<_>>();
                error_not_found!("type alias", alias_name, &aliases)
            }
        }
    }

    fn find_function<'a>(&'a self, function_name: &str) -> Result<FunctionWalker<'a>> {
        match self.walk_functions().find(|f| f.name() == function_name) {
            Some(f) => Ok(f),

            None => {
                // Get best match.
                let functions = self.walk_functions().map(|f| f.name()).collect::<Vec<_>>();
                error_not_found!("function", function_name, &functions)
            }
        }
    }

    fn find_client<'a>(&'a self, client_name: &str) -> Result<ClientWalker<'a>> {
        match self.walk_clients().find(|c| c.name() == client_name) {
            Some(c) => Ok(c),
            None => {
                // Get best match.
                let clients = self
                    .walk_clients()
                    .map(|c| c.name().to_string())
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

    fn is_subtype(&self, base: &FieldType, other: &FieldType) -> bool {
        if base == other {
            return true;
        }

        if let FieldType::Union(items) = other {
            if items.iter().any(|item| self.is_subtype(base, item)) {
                return true;
            }
        }

        match (base, other) {
            // TODO: O(n)
            (FieldType::RecursiveTypeAlias(name), _) => self
                .structural_recursive_alias_cycles()
                .iter()
                .any(|cycle| match cycle.get(name) {
                    Some(target) => self.is_subtype(target, other),
                    None => false,
                }),
            (_, FieldType::RecursiveTypeAlias(name)) => self
                .structural_recursive_alias_cycles()
                .iter()
                .any(|cycle| match cycle.get(name) {
                    Some(target) => self.is_subtype(base, target),
                    None => false,
                }),

            (FieldType::Primitive(TypeValue::Null), FieldType::Optional(_)) => true,
            (FieldType::Optional(base_item), FieldType::Optional(other_item)) => {
                self.is_subtype(base_item, other_item)
            }
            (_, FieldType::Optional(t)) => self.is_subtype(base, t),
            (FieldType::Optional(_), _) => false,

            // Handle types that nest other types.
            (FieldType::List(base_item), FieldType::List(other_item)) => {
                self.is_subtype(&base_item, other_item)
            }
            (FieldType::List(_), _) => false,

            (FieldType::Map(base_k, base_v), FieldType::Map(other_k, other_v)) => {
                self.is_subtype(other_k, base_k) && self.is_subtype(&**base_v, other_v)
            }
            (FieldType::Map(_, _), _) => false,

            (
                FieldType::WithMetadata {
                    base: constrained_base,
                    ..
                },
                _,
            ) => self.is_subtype(constrained_base, other),
            (
                _,
                FieldType::WithMetadata {
                    base: other_base, ..
                },
            ) => self.is_subtype(base, other_base),

            (FieldType::Literal(LiteralValue::Bool(_)), FieldType::Primitive(TypeValue::Bool)) => {
                true
            }
            (FieldType::Literal(LiteralValue::Bool(_)), _) => {
                self.is_subtype(base, &FieldType::Primitive(TypeValue::Bool))
            }
            (FieldType::Literal(LiteralValue::Int(_)), FieldType::Primitive(TypeValue::Int)) => {
                true
            }
            (FieldType::Literal(LiteralValue::Int(_)), _) => {
                self.is_subtype(base, &FieldType::Primitive(TypeValue::Int))
            }
            (
                FieldType::Literal(LiteralValue::String(_)),
                FieldType::Primitive(TypeValue::String),
            ) => true,
            (FieldType::Literal(LiteralValue::String(_)), _) => {
                self.is_subtype(base, &FieldType::Primitive(TypeValue::String))
            }

            (FieldType::Union(items), _) => items.iter().all(|item| self.is_subtype(item, other)),

            (FieldType::Tuple(base_items), FieldType::Tuple(other_items)) => {
                base_items.len() == other_items.len()
                    && base_items
                        .iter()
                        .zip(other_items)
                        .all(|(base_item, other_item)| self.is_subtype(base_item, other_item))
            }
            (FieldType::Tuple(_), _) => false,
            (FieldType::Primitive(_), _) => false,
            (FieldType::Enum(_), _) => false,
            (FieldType::Class(_), _) => false,
        }
    }

    fn distribute_type(
        &self,
        value: BamlValue,
        field_type: FieldType,
    ) -> anyhow::Result<BamlValueWithMeta<FieldType>> {
        match value {
            BamlValue::String(s) => {
                let literal_type = FieldType::Literal(LiteralValue::String(s.clone()));
                let primitive_type = FieldType::Primitive(TypeValue::String);

                if self.is_subtype(&literal_type, &field_type)
                    || self.is_subtype(&primitive_type, &field_type)
                {
                    return Ok(BamlValueWithMeta::String(s, field_type));
                }
                anyhow::bail!("Could not unify String with {:?}", field_type)
            }
            BamlValue::Int(i) => {
                let literal_type = FieldType::Literal(LiteralValue::Int(i));
                let primitive_type = FieldType::Primitive(TypeValue::Int);

                if self.is_subtype(&literal_type, &field_type)
                    || self.is_subtype(&primitive_type, &field_type)
                {
                    return Ok(BamlValueWithMeta::Int(i, field_type));
                }
                anyhow::bail!("Could not unify Int with {:?}", field_type)
            }

            BamlValue::Float(f) => {
                if self.is_subtype(&FieldType::Primitive(TypeValue::Float), &field_type) {
                    return Ok(BamlValueWithMeta::Float(f, field_type));
                }
                anyhow::bail!("Could not unify Float with {:?}", field_type)
            }

            BamlValue::Bool(b) => {
                let literal_type = FieldType::Literal(LiteralValue::Bool(b));
                let primitive_type = FieldType::Primitive(TypeValue::Bool);

                if self.is_subtype(&literal_type, &field_type)
                    || self.is_subtype(&primitive_type, &field_type)
                {
                    Ok(BamlValueWithMeta::Bool(b, field_type))
                } else {
                    anyhow::bail!("Could not unify Bool with {:?}", field_type)
                }
            }

            BamlValue::Null
                if self.is_subtype(&FieldType::Primitive(TypeValue::Null), &field_type) =>
            {
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
                            Box::new(match &field_type {
                                FieldType::Map(key, _) => match key.as_ref() {
                                    FieldType::Enum(name) => FieldType::Enum(name.clone()),
                                    _ => FieldType::string(),
                                },
                                _ => FieldType::string(),
                            }),
                            Box::new(item_type.clone()),
                        );

                        if !self.is_subtype(&map_type, &field_type) {
                            anyhow::bail!("Could not unify {:?} with {:?}", map_type, field_type);
                        }

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
                    None => Ok(BamlValueWithMeta::Map(BamlMap::new(), field_type)),
                }
            }

            BamlValue::List(items) => {
                let item_types = items
                    .iter()
                    .filter_map(infer_type)
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

                        if !self.is_subtype(&list_type, &field_type) {
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
                if self.is_subtype(
                    &FieldType::Primitive(TypeValue::Media(m.media_type)),
                    &field_type,
                ) =>
            {
                Ok(BamlValueWithMeta::Media(m, field_type))
            }
            BamlValue::Media(_) => anyhow::bail!("Could not unify Media with {:?}", field_type),

            BamlValue::Enum(name, val) => {
                if self.is_subtype(&FieldType::Enum(name.clone()), &field_type) {
                    Ok(BamlValueWithMeta::Enum(name, val, field_type))
                } else {
                    anyhow::bail!("Could not unify Enum {} with {:?}", name, field_type)
                }
            }

            BamlValue::Class(name, fields) => {
                if !self.is_subtype(&FieldType::Class(name.clone()), &field_type) {
                    anyhow::bail!("Could not unify Class {} with {:?}", name, field_type);
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

    fn distribute_type_with_meta<T: Clone + std::fmt::Debug>(
        &self,
        value: BamlValueWithMeta<T>,
        field_type: FieldType,
    ) -> anyhow::Result<BamlValueWithMeta<(T, FieldType)>> {
        let field_base_type = self.distribute_metadata(&field_type).0;
        match value {
            BamlValueWithMeta::String(s, meta) => {
                let literal_type = FieldType::Literal(LiteralValue::String(s.clone()));
                let primitive_type = FieldType::Primitive(TypeValue::String);

                if self.is_subtype(&literal_type, &field_base_type)
                    || self.is_subtype(&primitive_type, &field_base_type)
                {
                    return Ok(BamlValueWithMeta::String(s, (meta, field_type)));
                }
                anyhow::bail!("Could not unify String with {:?}", field_base_type)
            }
            BamlValueWithMeta::Int(i, meta)
                if self.is_subtype(&FieldType::Literal(LiteralValue::Int(i)), &field_base_type) =>
            {
                Ok(BamlValueWithMeta::Int(i, (meta, field_type)))
            }
            BamlValueWithMeta::Int(i, meta)
                if self.is_subtype(&FieldType::Primitive(TypeValue::Int), &field_type) =>
            {
                Ok(BamlValueWithMeta::Int(i, (meta, field_type)))
            }
            BamlValueWithMeta::Int(_i, _meta) => {
                anyhow::bail!("Could not unify Int with {:?}", field_base_type)
            }

            BamlValueWithMeta::Float(f, meta)
                if self.is_subtype(&FieldType::Primitive(TypeValue::Float), &field_base_type) =>
            {
                Ok(BamlValueWithMeta::Float(f, (meta, field_type)))
            }
            BamlValueWithMeta::Float(_, _) => {
                anyhow::bail!("Could not unify Float with {:?}", field_base_type)
            }

            BamlValueWithMeta::Bool(b, meta) => {
                let literal_type = FieldType::Literal(LiteralValue::Bool(b));
                let primitive_type = FieldType::Primitive(TypeValue::Bool);

                if self.is_subtype(&literal_type, &field_base_type)
                    || self.is_subtype(&primitive_type, &field_base_type)
                {
                    Ok(BamlValueWithMeta::Bool(b, (meta, field_type)))
                } else {
                    anyhow::bail!("Could not unify Bool with {:?}", field_base_type)
                }
            }

            BamlValueWithMeta::Null(meta) => Ok(BamlValueWithMeta::Null((meta, field_type))),

            BamlValueWithMeta::Map(pairs, meta) => {
                let (annotation_key_type, annotation_value_type) = map_types(self, &field_type)
                    .ok_or(anyhow::anyhow!("Could not unify map with {field_type:?}"))?;

                let mapped_fields: BamlMap<String, BamlValueWithMeta<(T, FieldType)>> = pairs
                    .into_iter()
                    .map(|(key, val)| {
                        let sub_value = item_type(self, &field_type, &val)
                            .ok_or(anyhow::anyhow!(
                                "Could not determine item_type of item in map"
                            ))
                            .and_then(|item_type| self.distribute_type_with_meta(val, item_type))?;
                        Ok((key, sub_value))
                    })
                    .collect::<anyhow::Result<BamlMap<String, BamlValueWithMeta<(T, FieldType)>>>>(
                    )?;

                Ok(BamlValueWithMeta::Map(mapped_fields, (meta, field_type)))
            }

            BamlValueWithMeta::List(items, meta) => {
                let new_items = items
                    .into_iter()
                    .map(|i| {
                        item_type(self, &field_type, &i)
                            .ok_or(anyhow::anyhow!("Could not infer child type"))
                            .and_then(|item_type| self.distribute_type_with_meta(i, item_type))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(BamlValueWithMeta::List(new_items, (meta, field_type)))
            }

            BamlValueWithMeta::Media(m, meta)
                if self.is_subtype(
                    &FieldType::Primitive(TypeValue::Media(m.media_type)),
                    &field_base_type,
                ) =>
            {
                Ok(BamlValueWithMeta::Media(m, (meta, field_type)))
            }
            BamlValueWithMeta::Media(_, _) => {
                anyhow::bail!("Could not unify Media with {:?}", field_base_type)
            }

            BamlValueWithMeta::Enum(name, val, meta) => {
                if self.is_subtype(&FieldType::Enum(name.clone()), &field_base_type) {
                    Ok(BamlValueWithMeta::Enum(name, val, (meta, field_type)))
                } else {
                    anyhow::bail!("Could not unify Enum {} with {:?}", name, field_base_type)
                }
            }

            BamlValueWithMeta::Class(name, fields, meta) => {
                if self.find_class(&name).is_err() {
                    // // Classes not present in the IR may be dynamically generated.
                    // // In this case, all types will be inferred, rather than distributed
                    // // from the `field_type` parameter.
                    return distribute_infer_class(self, &name, fields, meta);
                }
                if !self.is_subtype(&FieldType::Class(name.clone()), &field_base_type) {
                    anyhow::bail!("Could not unify Class {} with {:?}", name, field_base_type);
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
                                None => infer_type_with_meta(&v).unwrap_or(UNIT_TYPE),
                            };
                            let mapped_field = self.distribute_type_with_meta(v, field_type)?;
                            Ok((k, mapped_field))
                        })
                        .collect::<anyhow::Result<BamlMap<String, BamlValueWithMeta<(T, FieldType)>>>>(
                        )?;
                    Ok(BamlValueWithMeta::Class(
                        name,
                        mapped_fields,
                        (meta, field_type),
                    ))
                }
            }
        }
    }

    fn distribute_constraints<'a>(
        &'a self,
        field_type: &'a FieldType,
    ) -> (&'a FieldType, Vec<Constraint>) {
        let (field_type, metadata) = self.distribute_metadata(field_type);
        (field_type, metadata.0)
    }

    fn distribute_metadata<'a>(
        &'a self,
        field_type: &'a FieldType,
    ) -> (&'a FieldType, (Vec<Constraint>, StreamingBehavior)) {
        match field_type {
            FieldType::Class(class_name) => match self.find_class(class_name) {
                Err(_) => (field_type, (Vec::new(), StreamingBehavior::default())),
                Ok(class_node) => (
                    field_type,
                    (
                        class_node.item.attributes.constraints.clone(),
                        class_node.item.attributes.streaming_behavior(),
                    ),
                ),
            },
            FieldType::Enum(enum_name) => match self.find_enum(enum_name) {
                Err(_) => (field_type, (Vec::new(), StreamingBehavior::default())),
                Ok(enum_node) => (
                    field_type,
                    (
                        enum_node.item.attributes.constraints.clone(),
                        StreamingBehavior::default(),
                    ),
                ),
            },
            // Check the first level to see if it's constrained.
            FieldType::WithMetadata {
                base,
                constraints,
                streaming_behavior,
            } => {
                match base.as_ref() {
                    // If so, we must check the second level to see if we need to combine
                    // constraints across levels.
                    // The recursion here means that arbitrarily nested `FieldType::WithMetadata`s
                    // will be collapsed before the function returns.
                    FieldType::WithMetadata { .. } => {
                        let (sub_base, (sub_constraints, sub_streaming_behavior)) =
                            self.distribute_metadata(base.as_ref());
                        let combined_constraints = vec![constraints.clone(), sub_constraints]
                            .into_iter()
                            .flatten()
                            .collect();
                        let combined_streaming_behavior =
                            streaming_behavior.combine(&sub_streaming_behavior);
                        (
                            sub_base,
                            (combined_constraints, combined_streaming_behavior),
                        )
                    }
                    _ => (base, (constraints.clone(), streaming_behavior.clone())),
                }
            }
            _ => (field_type, (Vec::new(), StreamingBehavior::default())),
        }
    }

    fn type_has_constraints(&self, field_type: &FieldType) -> bool {
        let (_, constraints) = self.distribute_constraints(field_type);
        !constraints.is_empty()
    }

    fn type_has_checks(&self, field_type: &FieldType) -> bool {
        let (_, constraints) = self.distribute_constraints(field_type);
        constraints
            .iter()
            .any(|Constraint { level, .. }| *level == ConstraintLevel::Check)
    }

    fn recursive_alias_definition(&self, alias_name: &str) -> Option<&FieldType> {
        if let Some(cycle) = self
            .structural_recursive_alias_cycles()
            .iter()
            .find(|cycle| cycle.contains_key(alias_name))
        {
            cycle.get(alias_name)
        } else {
            None
        }
    }
}

/// For types of values that contain other values (e.g. lists, maps), compute
/// the type of the contained value.
/// TODO: Does this always terminate, especially in the case of recursive type
/// aliases?
///
/// When the field_type is a union, different variants may have
/// children of different types. We take a baml_value itself as a
/// parameter, and typecheck it against every variant of the union.
/// The first typechecking union variant is used as the type of
/// the children. This feels unsound, but it's not clear what we
/// should declare as the `item_type` in the case of unions that
/// admit multiple different children. (Perhaps a union of all the
/// child-having variants?).
fn item_type<T: std::fmt::Debug>(
    ir: &IntermediateRepr,
    field_type: &FieldType,
    baml_child_values: &BamlValueWithMeta<T>,
) -> Option<FieldType> {
    let res = match ir.distribute_metadata(field_type).0 {
        FieldType::Class(_) => None,
        FieldType::Enum(_) => None,
        FieldType::List(inner) => Some(*inner.clone()),
        FieldType::Literal(_) => None,
        FieldType::Map(k, v) => Some(*v.clone()),
        FieldType::Optional(inner) => item_type(ir, &*inner, baml_child_values),
        FieldType::Primitive(_) => None,
        FieldType::RecursiveTypeAlias(alias_name) => ir
            .recursive_alias_definition(alias_name)
            .and_then(|resolved_type| item_type(ir, resolved_type, baml_child_values)),
        FieldType::Union(variants) => {
            let variant_children = variants
                .iter()
                .filter_map(|variant| item_type(ir, variant, baml_child_values))
                .collect::<Vec<_>>();
            match variant_children.len() {
                0 => None,
                1 => Some(variant_children[0].clone()),
                _ => Some(FieldType::Union(variant_children)),
            }
        }
        FieldType::Tuple(_) => None,
        FieldType::WithMetadata { base, .. } => item_type(ir, base, baml_child_values),
    };
    res
}

/// Like item_type, but specialized for maps.
fn map_types<'ir, 'a>(
    ir: &'ir IntermediateRepr,
    field_type: &'a FieldType,
) -> Option<(&'a FieldType, &'a FieldType)>
where
    'ir: 'a,
{
    match ir.distribute_metadata(field_type).0 {
        FieldType::Map(key, value) => Some((key.as_ref(), value.as_ref())),
        FieldType::RecursiveTypeAlias(alias_name) => ir
            .recursive_alias_definition(alias_name)
            .and_then(|alias_definition| map_types(ir, &alias_definition)),
        FieldType::Primitive(_) => None,
        FieldType::Enum(_) => None,
        FieldType::List(_) => None,
        FieldType::Literal(_) => None,
        FieldType::Optional(base) => map_types(ir, base.as_ref()),
        FieldType::Tuple(_) => None,
        FieldType::Union(variants) => {
            // When encountering a union, we return the key/value types of the
            // first map we find inside the union.
            // TODO: Give more thought to what `map_types` should return for
            // unions, because the current logic is faulty for unions containing
            // multiple maps.
            let mut variant_map_types = variants
                .into_iter()
                .filter_map(|variant| map_types(ir, variant));
            variant_map_types.next()
        }
        FieldType::Class(_) => None,
        FieldType::WithMetadata { .. } => {
            unreachable!("distribute_metadata never returns this variant")
        }
    }
}

const UNIT_TYPE: FieldType = FieldType::Tuple(vec![]);

/// A helper function for `distribute_type_with_meta`, for cases where a class
/// is not present in the IR. In this case, when we don't have a class
/// definition in the IR (e.g. because the class was introduced through
/// TypeBuilder), we enhance the `BamlValueWithMeta` using types inferred from
/// each field of the class instance.
fn distribute_infer_class<T: Clone + std::fmt::Debug>(
    ir: &IntermediateRepr,
    class_name: &str,
    class_fields: IndexMap<String, BamlValueWithMeta<T>>,
    meta: T,
) -> Result<BamlValueWithMeta<(T, FieldType)>> {
    let fields = class_fields
        .into_iter()
        .map(|(k, v)| {
            let field_type = infer_type_with_meta(&v).unwrap_or(UNIT_TYPE);
            let field = ir.distribute_type_with_meta(v, field_type)?;
            Ok((k.to_string(), field))
        })
        .collect::<Result<IndexMap<_, _>>>()?;
    Ok(BamlValueWithMeta::Class(
        class_name.to_string(),
        fields,
        (meta, FieldType::class(class_name)),
    ))
}

pub fn infer_type(value: &BamlValue) -> Option<FieldType> {
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
                .filter_map(infer_type)
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

/// Derive the simplest type that can categorize a given value. This is meant to be used
/// by `distribute_type`, for dynamic fields of classes, whose types are not known statically.
/// TODO: Tests.
pub fn infer_type_with_meta<T>(value: &BamlValueWithMeta<T>) -> Option<FieldType> {
    let ret = match value {
        BamlValueWithMeta::Int(_, _) => Some(FieldType::Primitive(TypeValue::Int)),
        BamlValueWithMeta::Bool(_, _) => Some(FieldType::Primitive(TypeValue::Bool)),
        BamlValueWithMeta::Float(_, _) => Some(FieldType::Primitive(TypeValue::Float)),
        BamlValueWithMeta::String(_, _) => Some(FieldType::Primitive(TypeValue::String)),
        BamlValueWithMeta::Null(_) => Some(FieldType::Primitive(TypeValue::Null)),
        BamlValueWithMeta::Map(pairs, _) => {
            let v_tys = pairs
                .iter()
                .filter_map(|(_, v)| infer_type_with_meta(v))
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
        BamlValueWithMeta::List(items, _) => {
            let item_tys = items
                .iter()
                .filter_map(infer_type_with_meta)
                .dedup()
                .collect::<Vec<_>>();
            let item_ty = match item_tys.len() {
                0 => None,
                1 => Some(item_tys[0].clone()),
                _ => Some(FieldType::Union(item_tys)),
            }?;
            Some(FieldType::List(Box::new(item_ty)))
        }
        BamlValueWithMeta::Media(m, _) => {
            Some(FieldType::Primitive(TypeValue::Media(m.media_type)))
        }
        BamlValueWithMeta::Enum(enum_name, _, _) => Some(FieldType::Enum(enum_name.clone())),
        BamlValueWithMeta::Class(class_name, _, _) => Some(FieldType::Class(class_name.clone())),
    };
    ret
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::{
        BamlMedia, BamlMediaContent, BamlMediaType, BamlValue, Constraint, ConstraintLevel,
        FieldType, JinjaExpression, MediaBase64, StreamingBehavior, TypeValue,
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
    fn distribute_media() {
        let ir = mk_ir();
        let v = BamlValue::Media(BamlMedia {
            media_type: BamlMediaType::Audio,
            mime_type: None,
            content: BamlMediaContent::Base64(MediaBase64 {
                base64: "abcd=".to_string(),
            }),
        });
        let t = FieldType::Primitive(TypeValue::Media(BamlMediaType::Audio));
        let _value_with_meta = ir.distribute_type(v, t).unwrap();
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

    #[test]
    fn test_malformed_check_in_argument() {
        let ir = make_test_ir(
            r##"
            client<llm> GPT4 {
              provider openai
              options {
                model gpt-4o
                api_key env.OPENAI_API_KEY
              }
            }
            function Foo(a: int @assert(malformed, {{ this.length() > 0 }})) -> int {
              client GPT4
              prompt #""#
            }
            "##,
        )
        .unwrap();
        let function = ir.find_function("Foo").unwrap();
        let params = vec![("a".to_string(), BamlValue::Int(1))]
            .into_iter()
            .collect();
        let arg_coercer = ArgCoercer {
            span_path: None,
            allow_implicit_cast_to_string: true,
        };
        let res = ir.check_function_params(&function, &params, arg_coercer);
        assert!(res.is_err());
    }

    #[test]
    fn test_nested_constraint_distribution() {
        let ir = make_test_ir("").unwrap();
        fn mk_constraint(s: &str) -> Constraint {
            Constraint {
                level: ConstraintLevel::Assert,
                expression: JinjaExpression(s.to_string()),
                label: Some(s.to_string()),
            }
        }

        let input = FieldType::WithMetadata {
            constraints: vec![mk_constraint("a")],
            streaming_behavior: StreamingBehavior::default(),
            base: Box::new(FieldType::WithMetadata {
                constraints: vec![mk_constraint("b")],
                streaming_behavior: StreamingBehavior::default(),
                base: Box::new(FieldType::WithMetadata {
                    constraints: vec![mk_constraint("c")],
                    streaming_behavior: StreamingBehavior::default(),
                    base: Box::new(FieldType::Primitive(TypeValue::Int)),
                }),
            }),
        };

        let expected_base = FieldType::Primitive(TypeValue::Int);
        let expected_constraints = vec![mk_constraint("a"), mk_constraint("b"), mk_constraint("c")];

        let (base, constraints) = ir.distribute_constraints(&input);

        assert_eq!(base, &expected_base);
        assert_eq!(constraints, expected_constraints);
    }
}

// TODO: Copy pasted from baml-lib/baml-types/src/field_type/mod.rs and poorly
// refactored to match the `is_subtype` changes. Do something with this.
#[cfg(test)]
mod subtype_tests {
    use baml_types::BamlMediaType;
    use repr::make_test_ir;

    use super::*;

    fn mk_int() -> FieldType {
        FieldType::Primitive(TypeValue::Int)
    }
    fn mk_bool() -> FieldType {
        FieldType::Primitive(TypeValue::Bool)
    }
    fn mk_str() -> FieldType {
        FieldType::Primitive(TypeValue::String)
    }

    fn mk_optional(ft: FieldType) -> FieldType {
        FieldType::Optional(Box::new(ft))
    }

    fn mk_list(ft: FieldType) -> FieldType {
        FieldType::List(Box::new(ft))
    }

    fn mk_tuple(ft: Vec<FieldType>) -> FieldType {
        FieldType::Tuple(ft)
    }
    fn mk_union(ft: Vec<FieldType>) -> FieldType {
        FieldType::Union(ft)
    }
    fn mk_str_map(ft: FieldType) -> FieldType {
        FieldType::Map(Box::new(mk_str()), Box::new(ft))
    }

    fn ir() -> IntermediateRepr {
        make_test_ir("").unwrap()
    }

    #[test]
    fn subtype_trivial() {
        assert!(ir().is_subtype(&mk_int(), &mk_int()))
    }

    #[test]
    fn subtype_union() {
        let i = mk_int();
        let u = mk_union(vec![mk_int(), mk_str()]);
        assert!(ir().is_subtype(&i, &u));
        assert!(!ir().is_subtype(&u, &i));

        let u3 = mk_union(vec![mk_int(), mk_bool(), mk_str()]);
        assert!(ir().is_subtype(&i, &u3));
        assert!(ir().is_subtype(&u, &u3));
        assert!(!ir().is_subtype(&u3, &u));
    }

    #[test]
    fn subtype_optional() {
        let i = mk_int();
        let o = mk_optional(mk_int());
        assert!(ir().is_subtype(&i, &o));
        assert!(!ir().is_subtype(&o, &i));
    }

    #[test]
    fn subtype_list() {
        let l_i = mk_list(mk_int());
        let l_o = mk_list(mk_optional(mk_int()));
        assert!(ir().is_subtype(&l_i, &l_o));
        assert!(!ir().is_subtype(&l_o, &l_i));
    }

    fn subtype_list_with_metadata() {
        let l_i = FieldType::WithMetadata {
            base: Box::new(mk_list(mk_int())),
            constraints: vec![],
            streaming_behavior: StreamingBehavior {
                done: true,
                state: false,
            },
        };
        let l_o = mk_list(mk_int());
        assert!(ir().is_subtype(&l_i, &l_o));
        assert!(ir().is_subtype(&l_o, &l_i));
    }

    #[test]
    fn subtype_tuple() {
        let x = mk_tuple(vec![mk_int(), mk_optional(mk_int())]);
        let y = mk_tuple(vec![mk_int(), mk_int()]);
        assert!(ir().is_subtype(&y, &x));
        assert!(!ir().is_subtype(&x, &y));
    }

    #[test]
    fn subtype_map_of_list_of_unions() {
        let x = mk_str_map(mk_list(FieldType::Class("Foo".to_string())));
        let y = mk_str_map(mk_list(mk_union(vec![
            mk_str(),
            mk_int(),
            FieldType::Class("Foo".to_string()),
        ])));
        assert!(ir().is_subtype(&x, &y));
    }

    #[test]
    fn subtype_media() {
        let x = FieldType::Primitive(TypeValue::Media(BamlMediaType::Audio));
        assert!(ir().is_subtype(&x, &x));
    }

    // Given:
    // BamlValue::List ["a", {}]
    // field_type: RTA("JsonValue")
    //
    // List [
    //  "a" (Meta: Type: JsonValue),
    //  {}  (Meta: Type: JsonValue),
    // ] (Meta: Type: JsonValue)

    #[test]
    fn test_item_type() {
        let ir = make_test_ir(
            r##"
        type A = A[]
        type B = B[][]
        type C = map<string, C>

        type JsonValue = float | JsonValue[] | map<string, JsonValue>

        type JsonValue2 = float | JsonValue2List | JsonValue2Object
        type JsonValue2List = JsonValue2[]
        type JsonValue2Object = map<string, JsonValue2>

        type Foo = float | JsonValue | JsonValue2
        type U = string | Foo
        "##,
        )
        .unwrap();

        let example_a = BamlValueWithMeta::List(vec![], ());
        let example_b = BamlValueWithMeta::List(vec![BamlValueWithMeta::List(vec![], ())], ());
        let example_c = BamlValueWithMeta::Map(vec![].into_iter().collect(), ());
        let example_json = BamlValueWithMeta::Map(
            vec![
                ("foo".to_string(), BamlValueWithMeta::Bool(true, ())),
                (
                    "bar".to_string(),
                    BamlValueWithMeta::List(
                        vec![
                            BamlValueWithMeta::Int(1, ()),
                            BamlValueWithMeta::Int(2, ()),
                            BamlValueWithMeta::Int(3, ()),
                        ],
                        (),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
            (),
        );
        assert_eq!(
            item_type(
                &ir,
                &FieldType::RecursiveTypeAlias("A".to_string()),
                &example_a
            ),
            Some(FieldType::RecursiveTypeAlias("A".to_string()))
        );
        assert_eq!(
            item_type(
                &ir,
                &FieldType::RecursiveTypeAlias("B".to_string()),
                &example_b
            ),
            Some(FieldType::List(Box::new(FieldType::RecursiveTypeAlias(
                "B".to_string()
            ))))
        );
        assert_eq!(
            item_type(
                &ir,
                &FieldType::RecursiveTypeAlias("C".to_string()),
                &example_c
            ),
            Some(FieldType::RecursiveTypeAlias("C".to_string()))
        );
        assert_eq!(
            item_type(
                &ir,
                &FieldType::RecursiveTypeAlias("JsonValue".to_string()),
                &example_json
            ),
            Some(FieldType::Union(vec![
                FieldType::RecursiveTypeAlias("JsonValue".to_string()),
                FieldType::RecursiveTypeAlias("JsonValue".to_string())
            ]))
        );
    }
}

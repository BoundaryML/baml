mod error_utils;
pub mod scope_diagnostics;
mod to_baml_arg;

use std::collections::HashSet;

use anyhow::Result;
use baml_types::{
    ir_type::{TypeGeneric, TypeNonStreaming, UnionConstructor},
    BamlMap, BamlMediaType, BamlValue, BamlValueWithMeta, Constraint, ConstraintLevel,
    LiteralValue, TemplateStringRenderer, TypeIR, TypeValue, UnionType,
};
use indexmap::IndexMap;
use internal_baml_ast::ast::{WithIdentifier, WithSpan};
use internal_baml_diagnostics::Span;
use internal_baml_parser_database::walkers::ExprFnWalker;
use itertools::Itertools;
pub use to_baml_arg::ArgCoercer;

use self::scope_diagnostics::ScopeStack;
use super::{repr, ExprFunctionNode};
use crate::{
    error_not_found,
    ir::{
        repr::{IntermediateRepr, Walker},
        Class, Client, Enum, EnumValue, Field, FunctionNode, RetryPolicy, TemplateString, TestCase,
        TypeAlias,
    },
};

pub type FunctionWalker<'a> = Walker<'a, &'a FunctionNode>;
pub type ExprFunctionWalker<'a> = Walker<'a, &'a ExprFunctionNode>;
pub type EnumWalker<'a> = Walker<'a, &'a Enum>;
pub type EnumValueWalker<'a> = Walker<'a, &'a EnumValue>;
pub type ClassWalker<'a> = Walker<'a, &'a Class>;
pub type TypeAliasWalker<'a> = Walker<'a, &'a TypeAlias>;
pub type TemplateStringWalker<'a> = Walker<'a, &'a TemplateString>;
pub type ClientWalker<'a> = Walker<'a, &'a Client>;
pub type RetryPolicyWalker<'a> = Walker<'a, &'a RetryPolicy>;
pub type TestCaseWalker<'a> = Walker<'a, (&'a FunctionNode, &'a TestCase)>;
pub type TestCaseExprWalker<'a> = Walker<'a, (&'a ExprFunctionNode, &'a TestCase)>;
pub type ClassFieldWalker<'a> = Walker<'a, &'a Field>;

pub trait IRHelper {
    fn find_enum<'a>(&'a self, enum_name: &str) -> Result<EnumWalker<'a>>;
    fn find_class<'a>(&'a self, class_name: &str) -> Result<ClassWalker<'a>>;
    fn find_type_alias<'a>(&'a self, alias_name: &str) -> Result<TypeAliasWalker<'a>>;
    fn find_expr_fn<'a>(&'a self, function_name: &str) -> Result<ExprFunctionWalker<'a>>;
    fn find_function<'a>(&'a self, function_name: &str) -> Result<FunctionWalker<'a>>;
    fn find_client<'a>(&'a self, client_name: &str) -> Result<ClientWalker<'a>>;
    fn find_retry_policy<'a>(&'a self, retry_policy_name: &str) -> Result<RetryPolicyWalker<'a>>;
    fn find_expr_fn_test<'a>(
        &'a self,
        function: &'a ExprFunctionWalker<'a>,
        test_name: &str,
    ) -> Result<TestCaseExprWalker<'a>>;
    fn find_template_string<'a>(
        &'a self,
        template_string_name: &str,
    ) -> Result<TemplateStringWalker<'a>>;
    fn find_test<'a>(
        &'a self,
        function: &'a FunctionWalker<'a>,
        test_name: &str,
    ) -> Result<TestCaseWalker<'a>>;

    fn find_class_locations(&self, type_name: &str) -> Vec<Span>;
    fn find_enum_locations(&self, type_name: &str) -> Vec<Span>;
    fn find_type_alias_locations(&self, type_name: &str) -> Vec<Span>;

    fn check_function_params(
        &self,
        function_params: &[(String, TypeIR)],
        params: &BamlMap<String, BamlValue>,
        coerce_settings: ArgCoercer,
    ) -> Result<IndexMap<String, BamlValueWithMeta<TypeIR>>>;

    /// Pretty-print a list of arguments suitable for use in a `test` block.
    fn get_dummy_args(
        &self,
        indent: usize,
        allow_multiline: bool,
        params: &BamlMap<String, TypeIR>,
    ) -> String;
}

pub trait IRSemanticStreamingHelper {
    fn class_streaming_needed_fields(&self, class_name: &str) -> Result<HashSet<String>>;

    fn class_field_names(&self, class_name: &str) -> Result<indexmap::IndexSet<String>> {
        Ok(self
            .class_fields(class_name)?
            .into_iter()
            .map(|(k, _)| k)
            .collect())
    }

    fn class_fields(&self, class_name: &str) -> Result<BamlMap<String, TypeIR>>;
    fn find_class_fields_needing_null_filler(
        &self,
        class_name: &str,
        value_names: &std::collections::HashSet<String>,
    ) -> Result<HashSet<String>>;

    fn get_all_recursive_aliases(&self, alias_name: &str) -> impl Iterator<Item = &TypeIR>;
}

pub trait IRHelperExtended: IRSemanticStreamingHelper {
    /// BAML does not support class-based subtyping. Nonetheless some builtin
    /// BAML types are subtypes of others, and we need to be able to test this
    /// when checking the types of values.
    ///
    /// For examples of pairs of types and their subtyping relationship, see
    /// this module's test suite.
    ///
    /// Consider renaming this to `is_assignable`.
    fn is_subtype(&self, base: &TypeIR, other: &TypeIR) -> bool {
        if base == other {
            return true;
        }

        if let TypeIR::Union(items, _) = other {
            if items
                .iter_include_null()
                .iter()
                .any(|item| self.is_subtype(base, item))
            {
                return true;
            }
        }

        match (base, other) {
            // top can meet any other type.
            (TypeIR::Top(_), _) | (_, TypeIR::Top(_)) => true,
            // TODO: O(n)
            (TypeIR::RecursiveTypeAlias { name, .. }, _) => self
                .get_all_recursive_aliases(name)
                .any(|target| self.is_subtype(target, other)),
            (_, TypeIR::RecursiveTypeAlias { name, .. }) => self
                .get_all_recursive_aliases(name)
                .any(|target| self.is_subtype(base, target)),
            (TypeIR::Primitive(p1, _), TypeIR::Primitive(p2, _)) => p1 == p2,
            (TypeIR::Primitive(TypeValue::Null, _), _) => false,
            (TypeIR::Primitive(p1, _), _) => false,

            // Handle types that nest other types.
            (TypeIR::List(base_item, _), TypeIR::List(other_item, _)) => {
                self.is_subtype(base_item, other_item)
            }
            (TypeIR::List(_, _), _) => false,

            (TypeIR::Map(base_k, base_v, _), TypeIR::Map(other_k, other_v, _)) => {
                self.is_subtype(other_k, base_k) && self.is_subtype(base_v, other_v)
            }
            (TypeIR::Map(_, _, _), _) => false,
            (TypeIR::Literal(LiteralValue::Bool(_), _), TypeIR::Primitive(TypeValue::Bool, _)) => {
                true
            }
            (TypeIR::Literal(LiteralValue::Bool(_), _), _) => {
                self.is_subtype(base, &TypeIR::bool())
            }
            (TypeIR::Literal(LiteralValue::Int(_), _), TypeIR::Primitive(TypeValue::Int, _)) => {
                true
            }
            (TypeIR::Literal(LiteralValue::Int(_), _), _) => {
                self.is_subtype(base, &TypeIR::Primitive(TypeValue::Int, Default::default()))
            }
            (
                TypeIR::Literal(LiteralValue::String(_), _),
                TypeIR::Primitive(TypeValue::String, _),
            ) => true,
            (TypeIR::Literal(LiteralValue::String(_), _), _) => {
                self.is_subtype(base, &TypeIR::string())
            }

            (TypeIR::Union(items, _), _) => items
                .iter_include_null()
                .iter()
                .all(|item| self.is_subtype(item, other)),

            (TypeIR::Tuple(base_items, _), TypeIR::Tuple(other_items, _)) => {
                base_items.len() == other_items.len()
                    && base_items
                        .iter()
                        .zip(other_items)
                        .all(|(base_item, other_item)| self.is_subtype(base_item, other_item))
            }
            (TypeIR::Tuple(_, _), _) => false,
            (TypeIR::Arrow(_, _), _) => false,
            (
                TypeIR::Enum {
                    name: base_name, ..
                },
                TypeIR::Enum {
                    name: other_name, ..
                },
            ) => base_name == other_name,
            (TypeIR::Enum { .. }, _) => false,
            (
                TypeIR::Class {
                    name: base_name, ..
                },
                TypeIR::Class {
                    name: other_name, ..
                },
            ) => base_name == other_name,
            (TypeIR::Class { .. }, _) => false,
        }
    }

    /// For some `BamlValue` with type `FieldType`, walk the structure of both the value
    /// and the type simultaneously, associating each node in the `BamlValue` with its
    /// `FieldType`.
    fn distribute_type(
        &self,
        value: BamlValue,
        field_type: TypeIR,
    ) -> anyhow::Result<BamlValueWithMeta<TypeIR>> {
        let value_with_empty_meta = BamlValueWithMeta::with_same_meta_at_all_nodes(&value, ());
        let res = self
            .distribute_type_with_meta(value_with_empty_meta, field_type)?
            .map_meta_owned(|(_, meta)| meta);
        Ok(res)
    }

    /// For some `BamlValueWithMeta` with type `FieldType`, walk the structure of both the value
    /// and the type simultaneously, associating each node in the `BamlValue` with its
    /// `FieldType`.
    fn distribute_type_with_meta<T: Clone + std::fmt::Debug>(
        &self,
        value: BamlValueWithMeta<T>,
        field_type: TypeIR,
    ) -> anyhow::Result<BamlValueWithMeta<(T, TypeIR)>> {
        match value {
            BamlValueWithMeta::String(s, meta) => {
                let literal_type =
                    TypeIR::Literal(LiteralValue::String(s.clone()), Default::default());
                let primitive_type = TypeIR::Primitive(TypeValue::String, Default::default());

                if self.is_subtype(&literal_type, &field_type)
                    || self.is_subtype(&primitive_type, &field_type)
                {
                    return Ok(BamlValueWithMeta::String(s, (meta, field_type)));
                }
                anyhow::bail!("Could not unify String with {:?}", field_type)
            }
            BamlValueWithMeta::Int(i, meta)
                if self.is_subtype(
                    &TypeIR::Literal(LiteralValue::Int(i), Default::default()),
                    &field_type,
                ) =>
            {
                Ok(BamlValueWithMeta::Int(i, (meta, field_type)))
            }
            BamlValueWithMeta::Int(i, meta)
                if self.is_subtype(
                    &TypeIR::Primitive(TypeValue::Int, Default::default()),
                    &field_type,
                ) =>
            {
                Ok(BamlValueWithMeta::Int(i, (meta, field_type)))
            }
            BamlValueWithMeta::Int(_i, _meta) => {
                anyhow::bail!("Could not unify Int with {:?}", field_type)
            }

            BamlValueWithMeta::Float(f, meta)
                if self.is_subtype(
                    &TypeIR::Primitive(TypeValue::Float, Default::default()),
                    &field_type,
                ) =>
            {
                Ok(BamlValueWithMeta::Float(f, (meta, field_type)))
            }
            BamlValueWithMeta::Float(_, _) => {
                anyhow::bail!("Could not unify Float with {:?}", field_type)
            }

            BamlValueWithMeta::Bool(b, meta) => {
                let literal_type = TypeIR::Literal(LiteralValue::Bool(b), Default::default());
                let primitive_type = TypeIR::Primitive(TypeValue::Bool, Default::default());

                if self.is_subtype(&literal_type, &field_type)
                    || self.is_subtype(&primitive_type, &field_type)
                {
                    Ok(BamlValueWithMeta::Bool(b, (meta, field_type)))
                } else {
                    anyhow::bail!("Could not unify Bool with {:?}", field_type)
                }
            }

            BamlValueWithMeta::Null(meta) => Ok(BamlValueWithMeta::Null((meta, field_type))),

            BamlValueWithMeta::Map(pairs, meta) => {
                let (annotation_key_type, annotation_value_type) = map_types(self, &field_type)
                    .ok_or(anyhow::anyhow!("Could not unify map with {field_type:?}"))?;

                let mapped_fields: BamlMap<String, BamlValueWithMeta<(T, TypeIR)>> = pairs
                    .into_iter()
                    .map(|(key, val)| {
                        let sub_value =
                            self.distribute_type_with_meta(val, annotation_value_type.clone())?;

                        Ok((key, sub_value))
                    })
                    .collect::<anyhow::Result<BamlMap<String, BamlValueWithMeta<(T, TypeIR)>>>>()?;

                Ok(BamlValueWithMeta::Map(mapped_fields, (meta, field_type)))
            }

            BamlValueWithMeta::List(items, meta) => {
                let new_items = items
                    .into_iter()
                    .map(|i| {
                        item_type(self, &field_type)
                            .ok_or(anyhow::anyhow!("Could not infer child type"))
                            .and_then(|item_type| self.distribute_type_with_meta(i, item_type))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(BamlValueWithMeta::List(new_items, (meta, field_type)))
            }

            BamlValueWithMeta::Media(m, meta)
                if self.is_subtype(
                    &TypeIR::Primitive(TypeValue::Media(m.media_type), Default::default()),
                    &field_type,
                ) =>
            {
                Ok(BamlValueWithMeta::Media(m, (meta, field_type)))
            }
            BamlValueWithMeta::Media(_, _) => {
                anyhow::bail!("Could not unify Media with {:?}", field_type)
            }

            BamlValueWithMeta::Enum(name, val, meta) => {
                if self.is_subtype(
                    &TypeIR::Enum {
                        name: name.clone(),
                        dynamic: false,
                        meta: Default::default(),
                    },
                    &field_type,
                ) {
                    Ok(BamlValueWithMeta::Enum(name, val, (meta, field_type)))
                } else {
                    anyhow::bail!("Could not unify Enum {} with {:?}", name, field_type)
                }
            }

            BamlValueWithMeta::Class(name, fields, meta) => {
                if !self.is_subtype(&TypeIR::class(name.as_str()), &field_type) {
                    anyhow::bail!("Could not unify Class {} with {:?}", name, field_type);
                } else {
                    let class_fields = self.class_fields(&name)?;
                    let mapped_fields = fields
                        .into_iter()
                        .map(|(k, v)| {
                            let field_type = match class_fields.get(k.as_str()) {
                                Some(ft) => ft.clone(),
                                None => infer_type_with_meta(&v).unwrap_or(UNIT_TYPE.clone()),
                            };
                            let mapped_field = self.distribute_type_with_meta(v, field_type)?;
                            Ok((k, mapped_field))
                        })
                        .collect::<anyhow::Result<BamlMap<String, BamlValueWithMeta<(T, TypeIR)>>>>(
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

    fn recursive_alias_definition(&self, alias_name: &str) -> Option<&TypeIR>;

    fn type_has_constraints(&self, field_type: &TypeIR) -> bool {
        let metadata = field_type.meta();
        !metadata.constraints.is_empty()
    }

    fn type_has_checks(&self, field_type: &TypeIR) -> bool {
        let metadata = field_type.meta();
        metadata
            .constraints
            .iter()
            .any(|constraint| constraint.level == ConstraintLevel::Check)
    }
}

fn get_dummy_value(
    ir: &IntermediateRepr,
    indent: usize,
    allow_multiline: bool,
    t: &TypeIR,
    visited: &mut std::collections::HashSet<String>,
) -> String {
    fn type_complexity(t: &TypeIR) -> usize {
        match t {
            TypeIR::Primitive(TypeValue::Null, _) => 0,
            TypeIR::Primitive(TypeValue::Bool, _) => 1,
            TypeIR::Primitive(TypeValue::Int, _) => 2,
            TypeIR::Primitive(TypeValue::Float, _) => 3,
            TypeIR::Primitive(TypeValue::String, _) => 4,
            TypeIR::Primitive(TypeValue::Media(_), _) => 5,
            TypeIR::Literal(_, _) => 6,
            TypeIR::Enum { .. } => 7,
            TypeIR::List(_, _) => 10,
            TypeIR::Map(_, _, _) => 12,
            TypeIR::Class { .. } => 15,
            TypeIR::Tuple(_, _) => 20,
            TypeIR::Union(_, _) => 25,
            TypeIR::RecursiveTypeAlias { .. } => 30,
            TypeIR::Arrow(_, _) => 35,
            TypeIR::Top(_) => 40,
        }
    }
    let indent_str = "  ".repeat(indent);
    match t {
        TypeIR::Primitive(t, _) => {
            match t {
                TypeValue::String => {
                    if allow_multiline {
                        format!(
                            "#\"\n{indent1}hello world\n{indent_str}\"#",
                            indent1 = "  ".repeat(indent + 1)
                        )
                    } else {
                        "\"a_string\"".to_string()
                    }
                }
                TypeValue::Int => "123".to_string(),
                TypeValue::Float => "0.5".to_string(),
                TypeValue::Bool => "true".to_string(),
                TypeValue::Null => "null".to_string(),
                TypeValue::Media(BamlMediaType::Image) => {
                    "{ url \"https://imgs.xkcd.com/comics/standards.png\" }".to_string()
                }
                TypeValue::Media(BamlMediaType::Audio) => {
                    "{ url \"https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg\" }".to_string()
                }
                TypeValue::Media(BamlMediaType::Pdf) => {
                    "{ url \"https://ia801801.us.archive.org/15/items/the-great-gatsby_202101/TheGreatGatsby.pdf\" }".to_string()
                }
                TypeValue::Media(BamlMediaType::Video) => {
                    "{ url \"https://samplelib.com/lib/preview/mp4/sample-5s.mp4\" }".to_string()
                }
            }
        }
        TypeIR::Literal(literal_value, _) => match literal_value {
            LiteralValue::String(s) => format!("\"{s}\""),
            LiteralValue::Int(i) => i.to_string(),
            LiteralValue::Bool(b) => b.to_string(),
        },
        TypeIR::Enum { name, .. } => {
            // Try to get the first enum value from the IR
            if let Ok(enum_walker) = ir.find_enum(name) {
                if let Some(first_value) = enum_walker.walk_values().next() {
                    first_value.name().to_string()
                } else {
                    format!("ENUM_VALUE_{}", name.to_uppercase())
                }
            } else {
                format!("ENUM_VALUE_{}", name.to_uppercase())
            }
        }
        TypeIR::Class { name, .. } => {
            // Try to get the class fields from the IR
            if let Ok(class_walker) = ir.find_class(name) {
                let field_lines: Vec<String> = class_walker
                    .walk_fields()
                    .map(|field| {
                        let field_name = field.name();
                        let field_type = field.r#type();
                        let field_dummy = get_dummy_value(ir, indent + 1, allow_multiline, field_type, visited);
                        format!("{}  {} {}", "  ".repeat(indent + 1), field_name, field_dummy)
                    })
                    .collect();

                if field_lines.is_empty() {
                    if allow_multiline {
                        format!("{{\n{indent1}// Empty class\n{indent_str}}}", indent1 = "  ".repeat(indent + 1))
                    } else {
                        "{}".to_string()
                    }
                } else if allow_multiline {
                    format!("{{\n{}\n{indent_str}}}", field_lines.join("\n"))
                } else {
                    format!("{{ {} }}", field_lines.join(", "))
                }
            } else {
                // Fallback if class not found in IR
                if allow_multiline {
                    format!(
                        "{{\n{indent1}// Unknown class {name}\n{indent_str}}}",
                        indent1 = "  ".repeat(indent + 1)
                    )
                } else {
                    "{}".to_string()
                }
            }
        }
        TypeIR::RecursiveTypeAlias { name, .. } => {
            // Prevent infinite recursion by tracking visited aliases
            if visited.contains(name) {
                return "null".to_string();
            }

            visited.insert(name.clone());

            // Try to resolve the alias and find the simplest variant
            let result = if let Some(resolved_type) = ir.recursive_alias_definition(name) {
                // For unions, pick the simplest variant (typically primitives first)
                match resolved_type {
                    TypeIR::Union(variants, _) => {
                        // Find the simplest variant (prefer primitives, then enums, etc.)
                        let default_null = TypeIR::Primitive(TypeValue::Null, Default::default());
                        let simplest = variants
                            .iter_include_null()
                            .iter()
                            .min_by_key(|variant| type_complexity(variant))
                            .map_or(&default_null, |v| v);
                        get_dummy_value(ir, indent, allow_multiline, simplest, visited)
                    }
                    _ => get_dummy_value(ir, indent, allow_multiline, resolved_type, visited)
                }
            } else {
                "null".to_string()
            };

            visited.remove(name);
            result
        }
        TypeIR::List(item, _) => {
            let dummy = get_dummy_value(ir, indent + 1, allow_multiline, item, visited);
            if allow_multiline {
                format!(
                    "[\n{indent1}{dummy},\n{indent1}{dummy}\n{indent_str}]",
                    dummy = dummy,
                    indent1 = "  ".repeat(indent + 1)
                )
            } else {
                format!("[{dummy}, {dummy}]")
            }
        }
        TypeIR::Map(k, v, _) => {
            let dummy_k = get_dummy_value(ir, indent, false, k, visited);
            let dummy_v = get_dummy_value(ir, indent + 1, allow_multiline, v, visited);
            if allow_multiline {
                format!(
                    r#"{{
{indent1}{dummy_k} {dummy_v}
{indent_str}}}"#,
                    indent1 = "  ".repeat(indent + 1),
                )
            } else {
                format!("{{ {dummy_k} {dummy_v} }}")
            }
        }
        TypeIR::Union(fields, _) => {
            // Find the simplest variant to avoid infinite loops
            let default_null = TypeIR::Primitive(TypeValue::Null, Default::default());
            let simplest = fields
                .iter_include_null()
                .iter()
                .min_by_key(|variant| type_complexity(variant))
                .map_or(&default_null, |v| v);
            get_dummy_value(ir, indent, allow_multiline, simplest, visited)
        }
        TypeIR::Tuple(vals, _) => {
            let dummy = vals
                .iter()
                .map(|f| get_dummy_value(ir, 0, false, f, visited))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({dummy},)")
        }
        TypeIR::Arrow(_, _) => "null /* Arrow types not supported in dummy generation */".to_string(),
        TypeIR::Top(_) => "null /* Top type - should be resolved before dummy generation */".to_string(),
    }
}

fn get_dummy_field(ir: &IntermediateRepr, indent: usize, name: &str, t: &TypeIR) -> String {
    let indent_str = "  ".repeat(indent);
    let mut visited = std::collections::HashSet::new();
    let dummy = get_dummy_value(ir, indent, true, t, &mut visited);
    format!("{indent_str}{name} {dummy}")
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

    fn find_expr_fn_test<'a>(
        &'a self,
        function: &'a ExprFunctionWalker<'a>,
        test_name: &str,
    ) -> Result<TestCaseExprWalker<'a>> {
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
                // Get best match from both LLM functions and expr functions
                let mut functions = self.walk_functions().map(|f| f.name()).collect::<Vec<_>>();
                functions.extend(self.walk_expr_fns().map(|f| f.item.elem.name.as_str()));
                error_not_found!("function", function_name, &functions)
            }
        }
    }

    fn find_expr_fn<'a>(&'a self, function_name: &str) -> Result<ExprFunctionWalker<'a>> {
        let expr_fn_names = self
            .walk_expr_fns()
            .map(|f| f.item.elem.name.clone())
            .collect::<Vec<_>>();
        match self
            .walk_expr_fns()
            .find(|f| f.item.elem.name == function_name)
        {
            Some(f) => Ok(f),

            None => {
                // Get best match from both expr functions and LLM functions
                let mut functions = self
                    .walk_expr_fns()
                    .map(|f| f.item.elem.name.clone())
                    .collect::<Vec<_>>();
                functions.extend(self.walk_functions().map(|f| f.name().to_string()));
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

    fn find_class_locations(&self, class_name: &str) -> Vec<Span> {
        let mut locations = vec![];

        for cls in self.walk_classes() {
            // First look for the definition of the class.
            if cls.name() == class_name {
                locations.push(
                    cls.item
                        .attributes
                        .identifier_span
                        .as_ref()
                        .unwrap()
                        .to_owned(),
                );
            }

            // After that we'll find all the references to this class in the
            // fields of other classes.
            for field in cls.walk_fields() {
                field
                    .elem()
                    .r#type
                    .attributes
                    .symbol_spans
                    .iter()
                    .for_each(|(name, spans)| {
                        if name == class_name {
                            locations.extend(spans.iter().cloned());
                        }
                    });
            }
        }

        // Now do the same for type aliases.
        for alias in self.walk_type_aliases() {
            alias
                .elem()
                .r#type
                .attributes
                .symbol_spans
                .iter()
                .for_each(|(name, spans)| {
                    if name == class_name {
                        locations.extend(spans.iter().cloned());
                    }
                });
        }

        // Then find function inputs and outputs pointing to this class.
        for func in self.walk_functions() {
            func.item
                .attributes
                .symbol_spans
                .iter()
                .for_each(|(name, spans)| {
                    if name == class_name {
                        locations.extend(spans.iter().cloned());
                    }
                });
        }

        locations
    }

    fn find_enum_locations(&self, enum_name: &str) -> Vec<Span> {
        let mut locations = vec![];

        // First look for the definition of the enum.
        for enm in self.walk_enums() {
            if enm.name() == enum_name {
                locations.push(
                    enm.item
                        .attributes
                        .identifier_span
                        .as_ref()
                        .unwrap()
                        .to_owned(),
                );
            }
        }

        // Then find all the references to this enum in the fields of other
        // classes.
        for cls in self.walk_classes() {
            for field in cls.walk_fields() {
                field
                    .elem()
                    .r#type
                    .attributes
                    .symbol_spans
                    .iter()
                    .for_each(|(name, spans)| {
                        if name == enum_name {
                            locations.extend(spans.iter().cloned());
                        }
                    });
            }
        }

        // Now do the same for type aliases.
        for alias in self.walk_type_aliases() {
            alias
                .elem()
                .r#type
                .attributes
                .symbol_spans
                .iter()
                .for_each(|(name, spans)| {
                    if name == enum_name {
                        locations.extend(spans.iter().cloned());
                    }
                });
        }

        // Then find function inputs and outputs pointing to this enum.
        for func in self.walk_functions() {
            func.item
                .attributes
                .symbol_spans
                .iter()
                .for_each(|(name, spans)| {
                    if name == enum_name {
                        locations.extend(spans.iter().cloned());
                    }
                });
        }

        locations
    }

    fn find_type_alias_locations(&self, type_alias_name: &str) -> Vec<Span> {
        let mut locations = vec![];

        // First look for the definition of the type alias.
        for alias in self.walk_type_aliases() {
            if alias.name() == type_alias_name {
                locations.push(
                    alias
                        .item
                        .attributes
                        .identifier_span
                        .as_ref()
                        .unwrap()
                        .to_owned(),
                );
            }
        }

        // Then find all the references to this type alias in the fields of other
        // classes.
        for cls in self.walk_classes() {
            for field in cls.walk_fields() {
                field
                    .elem()
                    .r#type
                    .attributes
                    .symbol_spans
                    .iter()
                    .for_each(|(name, spans)| {
                        if name == type_alias_name {
                            locations.extend(spans.iter().cloned());
                        }
                    });
            }
        }

        // Now do the same for type aliases.
        for alias in self.walk_type_aliases() {
            alias
                .elem()
                .r#type
                .attributes
                .symbol_spans
                .iter()
                .for_each(|(name, spans)| {
                    if name == type_alias_name {
                        locations.extend(spans.iter().cloned());
                    }
                });
        }

        // Then find function inputs and outputs pointing to this type alias.
        for func in self.walk_functions() {
            func.item
                .attributes
                .symbol_spans
                .iter()
                .for_each(|(name, spans)| {
                    if name == type_alias_name {
                        locations.extend(spans.iter().cloned());
                    }
                });
        }

        locations
    }

    fn check_function_params(
        &self,
        function_params: &[(String, TypeIR)],
        params: &BamlMap<String, BamlValue>,
        coerce_settings: ArgCoercer,
    ) -> Result<IndexMap<String, BamlValueWithMeta<TypeIR>>> {
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
                    scope.push_error(format!("Missing required parameter: {param_name}"));
                }
            }
            scope.pop(false);
        }

        if scope.has_errors() {
            Err(anyhow::anyhow!(scope))
        } else {
            Ok(baml_arg_map)
        }
    }

    fn get_dummy_args(
        &self,
        indent: usize,
        allow_multiline: bool,
        params: &BamlMap<String, TypeIR>,
    ) -> String {
        params
            .iter()
            .map(|(param_name, param_type)| get_dummy_field(self, indent, param_name, param_type))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl IRHelperExtended for IntermediateRepr {
    fn recursive_alias_definition(&self, alias_name: &str) -> Option<&TypeIR> {
        if let Some(alias) = self
            .structural_recursive_alias_cycles()
            .iter()
            .find_map(|cycle| cycle.get(alias_name))
        {
            Some(alias)
        } else {
            None
        }
    }
}

impl IRSemanticStreamingHelper for IntermediateRepr {
    fn class_streaming_needed_fields(&self, class_name: &str) -> Result<HashSet<String>> {
        let class = self.find_class(class_name)?;
        Ok(class
            .walk_fields()
            .filter_map(|field: Walker<'_, &Field>| {
                if field.r#type().streaming_behavior().needed {
                    Some(field.name().to_string())
                } else {
                    None
                }
            })
            .collect())
    }

    fn class_fields(&self, class_name: &str) -> Result<BamlMap<String, TypeIR>> {
        let class = self.find_class(class_name)?.elem();
        Ok(class
            .static_fields
            .iter()
            .map(|field_node| {
                (
                    field_node.elem.name.clone(),
                    field_node.elem.r#type.elem.clone(),
                )
            })
            .collect())
    }

    fn find_class_fields_needing_null_filler(
        &self,
        class_name: &str,
        value_names: &std::collections::HashSet<String>,
    ) -> Result<HashSet<String>> {
        match self.find_class(class_name) {
            Err(_) => Ok(HashSet::new()),
            Ok(class) => {
                let missing_fields = class
                    .walk_fields()
                    .filter_map(|field: Walker<'_, &Field>| {
                        if !value_names.contains(field.name()) {
                            Some(field.name().to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                Ok(missing_fields)
            }
        }
    }

    fn get_all_recursive_aliases(&self, alias_name: &str) -> impl Iterator<Item = &TypeIR> {
        self.structural_recursive_alias_cycles()
            .iter()
            .filter_map(|cycle| cycle.get(alias_name))
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
fn item_type(ir: &(impl IRHelperExtended + ?Sized), field_type: &TypeIR) -> Option<TypeIR> {
    let res = match field_type {
        TypeIR::Top(_) => None,
        TypeIR::Class { .. } => None,
        TypeIR::Enum { .. } => None,
        TypeIR::List(inner, _) => Some(*inner.clone()),
        TypeIR::Literal(_, _) => None,
        TypeIR::Map(k, v, _) => Some(*v.clone()),
        TypeIR::Primitive(_, _) => None,
        TypeIR::RecursiveTypeAlias {
            name: alias_name, ..
        } => ir
            .recursive_alias_definition(alias_name)
            .and_then(|resolved_type| item_type(ir, resolved_type)),
        TypeIR::Union(variants, _) => match variants.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => None,
            baml_types::ir_type::UnionTypeViewGeneric::Optional(field_type) => {
                item_type(ir, field_type)
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(field_types)
            | baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(field_types) => {
                let variant_children = field_types
                    .iter()
                    .filter_map(|variant| item_type(ir, variant))
                    .collect::<Vec<_>>();
                match variant_children.len() {
                    0 => None,
                    1 => Some(variant_children[0].clone()),
                    _ => Some(TypeIR::union(variant_children)),
                }
            }
        },
        TypeIR::Tuple(_, _) => None,
        TypeIR::Arrow(_, _) => None,
    };
    res
}

/// Like item_type, but specialized for maps.
pub fn map_types<'ir, 'a>(
    ir: &'ir (impl IRHelperExtended + ?Sized),
    field_type: &'a TypeIR,
) -> Option<(TypeIR, TypeIR)>
where
    'ir: 'a,
{
    let res = match field_type {
        TypeIR::Map(key, value, _) => Some((*key.clone(), *value.clone())),
        TypeIR::RecursiveTypeAlias {
            name: alias_name, ..
        } => ir
            .recursive_alias_definition(alias_name)
            .and_then(|alias_definition| map_types(ir, alias_definition)),
        TypeIR::Top(_)
        | TypeIR::Primitive(_, _)
        | TypeIR::Enum { .. }
        | TypeIR::List(_, _)
        | TypeIR::Literal(_, _)
        | TypeIR::Tuple(_, _) => None,
        TypeIR::Union(variants, _) => {
            let variant_map_types: Vec<(TypeIR, TypeIR)> = variants
                .iter_include_null()
                .iter()
                .filter_map(|variant| map_types(ir, variant))
                .collect();
            if variant_map_types.is_empty() {
                return None;
            } else {
                let first_key_type = variant_map_types[0].0.clone();
                if !variant_map_types
                    .iter()
                    .all(|(key, _)| key == &first_key_type)
                {
                    return None;
                } else {
                    let value_types = variant_map_types
                        .into_iter()
                        .map(|(_, value_type)| value_type.clone())
                        .unique()
                        .collect::<Vec<_>>();
                    let value_type = match value_types.len() {
                        0 => None,
                        1 => Some(value_types[0].clone()),
                        _ => Some(TypeIR::union(value_types)),
                    }?;
                    return Some((first_key_type, value_type));
                }
            }
        }
        TypeIR::Class { .. } => None,
        TypeIR::Arrow(_, _) => None,
    };
    res
}

pub static UNIT_TYPE: once_cell::sync::Lazy<TypeIR> =
    once_cell::sync::Lazy::new(|| TypeIR::Tuple(vec![], Default::default()));

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
) -> Result<BamlValueWithMeta<(T, TypeIR)>> {
    let fields = class_fields
        .into_iter()
        .map(|(k, v)| {
            let field_type = infer_type_with_meta(&v).unwrap_or(UNIT_TYPE.clone());
            let field = ir.distribute_type_with_meta(v, field_type)?;
            Ok((k.to_string(), field))
        })
        .collect::<Result<IndexMap<_, _>>>()?;
    Ok(BamlValueWithMeta::Class(
        class_name.to_string(),
        fields,
        (meta, TypeIR::class(class_name)),
    ))
}

pub fn infer_type<Meta>(value: &BamlValue) -> Option<TypeGeneric<Meta>>
where
    Meta: Clone + Default + PartialEq,
    TypeGeneric<Meta>: UnionConstructor<Meta>,
{
    let baml_value_with_meta = infer_value_with_type(value);
    Some(baml_value_with_meta.meta().clone())
}

pub fn infer_value_with_type<Meta>(value: &BamlValue) -> BamlValueWithMeta<TypeGeneric<Meta>>
where
    Meta: Clone + Default + PartialEq,
    TypeGeneric<Meta>: UnionConstructor<Meta>,
{
    match value {
        BamlValue::Int(i) => BamlValueWithMeta::Int(
            *i,
            TypeGeneric::Primitive(TypeValue::Int, Default::default()),
        ),
        BamlValue::Bool(b) => BamlValueWithMeta::Bool(
            *b,
            TypeGeneric::Primitive(TypeValue::Bool, Default::default()),
        ),
        BamlValue::Float(f) => BamlValueWithMeta::Float(
            *f,
            TypeGeneric::Primitive(TypeValue::Float, Default::default()),
        ),
        BamlValue::String(s) => BamlValueWithMeta::String(
            s.clone(),
            TypeGeneric::Primitive(TypeValue::String, Default::default()),
        ),
        BamlValue::Null => {
            BamlValueWithMeta::Null(TypeGeneric::Primitive(TypeValue::Null, Default::default()))
        }
        BamlValue::Map(pairs) => {
            let pairs: BamlMap<String, BamlValueWithMeta<TypeGeneric<Meta>>> = pairs
                .iter()
                .map(|(k, v)| (k.clone(), infer_value_with_type(v)))
                .collect();
            let v_tys = pairs.values().map(|v| v.meta().clone()).collect::<Vec<_>>();
            let k_ty = TypeGeneric::Primitive(TypeValue::String, Default::default());
            let v_ty = match v_tys.len() {
                0 => TypeGeneric::Primitive(TypeValue::Null, Default::default()),
                _ => TypeGeneric::union(v_tys.to_vec()),
            };
            BamlValueWithMeta::Map(pairs, TypeGeneric::map(k_ty, v_ty))
        }
        BamlValue::List(items) => {
            let items: Vec<BamlValueWithMeta<TypeGeneric<Meta>>> =
                items.iter().map(infer_value_with_type).collect();
            let item_tys = items
                .iter()
                .map(|v| v.meta().clone())
                .dedup()
                .collect::<Vec<_>>();
            let item_ty = match item_tys.len() {
                0 => TypeGeneric::Primitive(TypeValue::Null, Default::default()),
                _ => TypeGeneric::union(item_tys),
            };
            BamlValueWithMeta::List(
                items,
                TypeGeneric::List(Box::new(item_ty), Default::default()),
            )
        }
        BamlValue::Media(m) => BamlValueWithMeta::Media(
            m.clone(),
            TypeGeneric::Primitive(TypeValue::Media(m.media_type), Default::default()),
        ),
        BamlValue::Enum(enum_name, v) => BamlValueWithMeta::Enum(
            enum_name.clone(),
            v.clone(),
            TypeGeneric::Enum {
                name: enum_name.clone(),
                dynamic: false,
                meta: Default::default(),
            },
        ),
        BamlValue::Class(class_name, fields) => {
            let fields: BamlMap<String, BamlValueWithMeta<TypeGeneric<Meta>>> = fields
                .iter()
                .map(|(k, v)| (k.clone(), infer_value_with_type(v)))
                .collect();
            BamlValueWithMeta::Class(
                class_name.clone(),
                fields,
                TypeGeneric::Class {
                    name: class_name.clone(),
                    mode: baml_types::ir_type::StreamingMode::NonStreaming,
                    dynamic: false,
                    meta: Default::default(),
                },
            )
        }
    }
}

/// Derive the simplest type that can categorize a given value. This is meant to be used
/// by `distribute_type`, for dynamic fields of classes, whose types are not known statically.
/// TODO: Tests.
pub fn infer_type_with_meta<T>(value: &BamlValueWithMeta<T>) -> Option<TypeIR> {
    let ret = match value {
        BamlValueWithMeta::Int(_, _) => Some(TypeIR::Primitive(TypeValue::Int, Default::default())),
        BamlValueWithMeta::Bool(_, _) => {
            Some(TypeIR::Primitive(TypeValue::Bool, Default::default()))
        }
        BamlValueWithMeta::Float(_, _) => {
            Some(TypeIR::Primitive(TypeValue::Float, Default::default()))
        }
        BamlValueWithMeta::String(_, _) => {
            Some(TypeIR::Primitive(TypeValue::String, Default::default()))
        }
        BamlValueWithMeta::Null(_) => Some(TypeIR::Primitive(TypeValue::Null, Default::default())),
        BamlValueWithMeta::Map(pairs, _) => {
            let v_tys = pairs
                .iter()
                .filter_map(|(_, v)| infer_type_with_meta(v))
                .dedup()
                .collect::<Vec<_>>();
            let k_ty = TypeIR::Primitive(TypeValue::String, Default::default());
            let v_ty = match v_tys.len() {
                0 => None,
                1 => Some(v_tys[0].clone()),
                _ => Some(TypeIR::union(v_tys)),
            }?;
            Some(TypeIR::Map(
                Box::new(k_ty),
                Box::new(v_ty),
                Default::default(),
            ))
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
                _ => Some(TypeIR::union(item_tys)),
            }?;
            Some(TypeIR::List(Box::new(item_ty), Default::default()))
        }
        BamlValueWithMeta::Media(m, _) => Some(TypeIR::Primitive(
            TypeValue::Media(m.media_type),
            Default::default(),
        )),
        BamlValueWithMeta::Enum(enum_name, _, _) => Some(TypeIR::Enum {
            name: enum_name.clone(),
            dynamic: false,
            meta: Default::default(),
        }),
        BamlValueWithMeta::Class(class_name, _, _) => Some(TypeIR::Class {
            name: class_name.clone(),
            mode: baml_types::ir_type::StreamingMode::NonStreaming,
            dynamic: false,
            meta: Default::default(),
        }),
    };
    ret
}

#[cfg(test)]
mod tests {
    use baml_types::{
        BamlMedia, BamlMediaContent, BamlMediaType, BamlValue, Constraint, ConstraintLevel,
        JinjaExpression, MediaBase64, TypeIR, TypeValue,
    };
    use repr::make_test_ir;

    use super::*;

    fn int_type() -> TypeIR {
        TypeIR::Primitive(TypeValue::Int, Default::default())
    }

    fn string_type() -> TypeIR {
        TypeIR::Primitive(TypeValue::String, Default::default())
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
        let actual = infer_type(&my_list).unwrap();
        let expected = int_type().as_list();
        assert_eq!(actual, expected, "{actual} != {expected}");
    }

    #[test]
    fn infer_map() {
        let my_map = mk_map_1();
        assert_eq!(
            infer_type(&my_map).unwrap(),
            TypeIR::Map(
                Box::new(string_type()),
                Box::new(int_type()),
                Default::default(),
            )
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
            TypeIR::Map(
                Box::new(string_type()),
                Box::new(TypeIR::Map(
                    Box::new(string_type()),
                    Box::new(int_type()),
                    Default::default(),
                )),
                Default::default(),
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
        let t = TypeIR::Primitive(TypeValue::Media(BamlMediaType::Audio), Default::default());
        let _value_with_meta = ir.distribute_type(v, t).unwrap();
    }

    #[test]
    fn distribute_media_union() {
        let ir = mk_ir();
        let field_type = TypeIR::union(vec![
            string_type(),
            TypeIR::Primitive(TypeValue::Media(BamlMediaType::Image), Default::default()),
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

        let elem_type = TypeIR::union(vec![
            string_type(),
            int_type(),
            TypeIR::Class {
                name: "Foo".to_string(),
                mode: baml_types::ir_type::StreamingMode::NonStreaming,
                dynamic: false,
                meta: Default::default(),
            },
        ]);
        let map_type = TypeIR::Map(
            Box::new(string_type()),
            Box::new(elem_type.clone()),
            Default::default(),
        );

        // The compound type we want to test.
        let list_type = TypeIR::List(Box::new(map_type.clone()), Default::default());

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

        let elem_type = TypeIR::union(vec![string_type(), int_type(), TypeIR::class("Foo")]);

        let list_type = TypeIR::List(Box::new(elem_type), Default::default());

        // The compound type we want to test.
        let map_type = TypeIR::Map(
            Box::new(string_type()),
            Box::new(list_type),
            Default::default(),
        );

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
        let res = ir.check_function_params(function.inputs(), &params, arg_coercer);
        eprintln!("res: {res:?}");
        assert!(res.is_err());
    }

    #[test]
    fn test_distribute_optional_string_with_meta() {
        let ir = make_test_ir(r#""#).unwrap();

        let res = ir
            .distribute_type_with_meta(BamlValueWithMeta::Null(()), TypeIR::null())
            .expect("Distribution should succeed");
        let res2 = ir
            .distribute_type(
                BamlValue::Null,
                TypeIR::Primitive(TypeValue::String, Default::default()).as_optional(),
            )
            .expect("Distribution should succeed");

        let res3 = ir
            .distribute_type_with_meta(
                BamlValueWithMeta::List(
                    vec![
                        BamlValueWithMeta::String("foo".to_string(), ()),
                        BamlValueWithMeta::String("bar".to_string(), ()),
                    ],
                    (),
                ),
                TypeIR::List(
                    Box::new(TypeIR::Primitive(TypeValue::String, Default::default())),
                    Default::default(),
                ),
            )
            .expect("Distribution should succeed");

        let res4 = ir
            .distribute_type(
                BamlValue::List(vec![
                    BamlValue::String("foo".to_string()),
                    BamlValue::String("bar".to_string()),
                ]),
                TypeIR::List(
                    Box::new(TypeIR::Primitive(TypeValue::String, Default::default())),
                    Default::default(),
                ),
            )
            .expect("Distribution should succeed");
    }

    #[test]
    fn test_block_constraint_argument_class() {
        let ir = make_test_ir(
            r##"
            class BlockConstraintForParam {
              bcfp int
              bcfp2 string
              @@assert(hi, {{ this.bcfp2|length < this.bcfp }})
            }
            client<llm> GPT4 {
              provider openai
              options {
                model gpt-4o
                api_key env.OPENAI_API_KEY
              }
            }
            function UseBlockConstraint(inp: BlockConstraintForParam) -> int {
              client GPT4
              prompt #""#
            }
            "##,
        )
        .unwrap();
        let params = vec![(
            "inp".to_string(),
            BamlValue::Class(
                "BlockConstraintForParam".to_string(),
                vec![
                    ("bcfp".to_string(), BamlValue::Int(1)),
                    (
                        "bcfp2".to_string(),
                        BamlValue::String("too long!".to_string()),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        )]
        .into_iter()
        .collect();
        let function = ir.find_function("UseBlockConstraint").unwrap();
        let arg_coercer = ArgCoercer {
            span_path: None,
            allow_implicit_cast_to_string: true,
        };
        let res = ir.check_function_params(function.inputs(), &params, arg_coercer);
        let err = res.expect_err("Should fail due to block constraint");
        let msg = format!("{err}");
        assert!(
            msg.contains("Failed assert: hi"),
            "Error message should mention the failed block constraint: {msg}"
        );
    }
}

// TODO: Copy pasted from baml-lib/baml-types/src/field_type/mod.rs and poorly
// refactored to match the `is_subtype` changes. Do something with this.
#[cfg(test)]
mod subtype_tests {
    use baml_types::{
        type_meta::base::{StreamingBehavior, TypeMeta},
        BamlMediaType,
    };
    use minijinja::machinery::ast::Expr;
    use repr::make_test_ir;

    use super::*;

    fn mk_int() -> TypeIR {
        TypeIR::Primitive(TypeValue::Int, Default::default())
    }
    fn mk_bool() -> TypeIR {
        TypeIR::Primitive(TypeValue::Bool, Default::default())
    }
    fn mk_str() -> TypeIR {
        TypeIR::Primitive(TypeValue::String, Default::default())
    }

    fn mk_optional(ft: TypeIR) -> TypeIR {
        ft.as_optional()
    }

    fn mk_list(ft: TypeIR) -> TypeIR {
        TypeIR::List(Box::new(ft), Default::default())
    }

    fn mk_tuple(ft: Vec<TypeIR>) -> TypeIR {
        TypeIR::Tuple(ft, Default::default())
    }
    fn mk_union(ft: Vec<TypeIR>) -> TypeIR {
        TypeIR::union(ft)
    }
    fn mk_str_map(ft: TypeIR) -> TypeIR {
        TypeIR::Map(Box::new(mk_str()), Box::new(ft), Default::default())
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
        let l_i = TypeIR::List(
            Box::new(mk_list(mk_int())),
            TypeMeta {
                constraints: vec![],
                streaming_behavior: StreamingBehavior {
                    done: true,
                    state: false,
                    needed: false,
                },
            },
        );
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
    fn subtype_class_with_metadata() {
        let x = TypeIR::class("Foo");
        let mut y = TypeIR::class("Foo");
        y.set_meta(TypeMeta {
            constraints: vec![Constraint {
                expression: baml_types::JinjaExpression("this is a test".to_string()),
                label: Some("test".to_string()),
                level: ConstraintLevel::Check,
            }],
            streaming_behavior: StreamingBehavior {
                done: false,
                state: false,
                needed: false,
            },
        });

        assert!(ir().is_subtype(&x, &y));
    }

    #[test]
    fn subtype_map_of_list_of_unions() {
        let x = mk_str_map(mk_list(TypeIR::class("Foo")));
        let y = mk_str_map(mk_list(mk_union(vec![
            mk_str(),
            mk_int(),
            TypeIR::class("Foo"),
        ])));
        assert!(ir().is_subtype(&x, &y));
    }

    #[test]
    fn subtype_media() {
        let x = TypeIR::Primitive(TypeValue::Media(BamlMediaType::Audio), Default::default());
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
    fn test_get_dummy_args() {
        let ir = make_test_ir(
            r##"
            class Person {
              name string
              age int
            }

            type JsonValue = float | JsonValue[] | map<string, JsonValue>
            "##,
        )
        .unwrap();

        let mut params = BamlMap::new();
        params.insert("user_name".to_string(), TypeIR::string());
        params.insert(
            "score".to_string(),
            TypeIR::Primitive(TypeValue::Float, Default::default()),
        );
        params.insert("person".to_string(), TypeIR::class("Person"));
        params.insert(
            "data".to_string(),
            TypeIR::union(vec![TypeIR::string(), TypeIR::int()]),
        );
        params.insert(
            "json_data".to_string(),
            TypeIR::recursive_type_alias("JsonValue"),
        );

        let result = ir.get_dummy_args(1, true, &params);

        // Check that all parameters are included
        assert!(result.contains("user_name"));
        assert!(result.contains("score"));
        assert!(result.contains("person"));
        assert!(result.contains("data"));
        assert!(result.contains("json_data"));

        // Check basic formatting
        assert!(result.contains("  user_name")); // proper indentation
        assert!(result.contains("0.5")); // float value
        assert!(result.contains("name") && result.contains("age")); // Person class fields

        println!("Generated dummy args:\n{result}");
    }

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

        // A[]
        let ret = item_type(&ir, &TypeIR::recursive_type_alias("A")).expect("should be Some");
        let mut expected = TypeIR::recursive_type_alias("A");
        expected.meta_mut().streaming_behavior.needed = true;
        assert_eq!(ret, expected, "{ret} != {expected}");

        // B[][]
        let ret = item_type(&ir, &TypeIR::recursive_type_alias("B")).expect("should be Some");
        let mut expected = TypeIR::recursive_type_alias("B");
        expected.meta_mut().streaming_behavior.needed = true;
        let mut expected = expected.as_list();
        expected.meta_mut().streaming_behavior.needed = true;

        assert_eq!(ret, expected, "{ret} != {expected}");

        // map<string, C>
        let ret = item_type(&ir, &TypeIR::recursive_type_alias("C")).expect("should be Some");
        let mut expected = TypeIR::recursive_type_alias("C");
        expected.meta_mut().streaming_behavior.needed = true;
        assert_eq!(ret, expected, "{ret} != {expected}");

        // JsonValue
        let ret =
            item_type(&ir, &TypeIR::recursive_type_alias("JsonValue")).expect("should be Some");
        let mut expected = TypeIR::recursive_type_alias("JsonValue");
        expected.meta_mut().streaming_behavior.needed = true;
        assert_eq!(ret, expected, "{ret} != {expected}");
    }
}

/// Implementation of TemplateStringRenderer for IntermediateRepr.
/// This allows template_string calls to be resolved during test argument evaluation.
impl TemplateStringRenderer for IntermediateRepr {
    fn render_template(&self, name: &str, args: &[serde_json::Value]) -> Result<String> {
        // Find the template string definition
        let template = self.find_template_string(name)?;
        let template_content = template.template();
        let template_params = template.inputs();

        // Validate argument count
        if args.len() != template_params.len() {
            anyhow::bail!(
                "Template string '{}' expects {} arguments, but {} were provided",
                name,
                template_params.len(),
                args.len()
            );
        }

        // Build the arguments map for minijinja
        let mut args_map = serde_json::Map::new();
        for (param, arg) in template_params.iter().zip(args.iter()) {
            args_map.insert(param.name.clone(), arg.clone());
        }

        // Collect all template_strings as Jinja macro definitions.
        // This allows nested template_string calls to work (e.g., Outer() calling Inner()).
        // This matches how the prompt renderer handles template_strings.
        let macro_defs: String = self
            .walk_template_strings()
            .map(|t| {
                let args_str = t
                    .inputs()
                    .iter()
                    .map(|i| i.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{{% macro {}({}) %}}{}{{% endmacro %}}\n",
                    t.name(),
                    args_str,
                    t.template()
                )
            })
            .collect();

        // Prepend macro definitions to the template content
        let full_template = format!("{}{}", macro_defs, template_content);

        // Create a minijinja environment and render the template
        let mut env = minijinja::Environment::new();
        env.add_template("__template__", &full_template)
            .map_err(|e| anyhow::anyhow!("Failed to parse template '{}': {}", name, e))?;

        let tmpl = env
            .get_template("__template__")
            .map_err(|e| anyhow::anyhow!("Failed to get template '{}': {}", name, e))?;

        let context = minijinja::Value::from_serialize(&args_map);
        let rendered = tmpl
            .render(context)
            .map_err(|e| anyhow::anyhow!("Failed to render template '{}': {}", name, e))?;

        Ok(rendered)
    }
}

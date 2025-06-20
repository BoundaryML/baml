use anyhow::Result;
use baml_types::{FieldType, LiteralValue, TypeValue, ir_type::UnionTypeViewGeneric};
use indexmap::IndexSet;
use itertools::Itertools;
use std::{borrow::Cow, ops::Index};

use crate::{field_type_attributes, type_check_attributes, TypeCheckAttributes};

use super::{go_language_features::ToGo, ToUnionName};
use internal_baml_core::ir::{
    repr::{Docstring, IntermediateRepr, Walker},
    ClassWalker, EnumWalker, IRHelper, IRHelperExtended,
};

#[derive(askama::Template)]
#[template(path = "types.go.j2", escape = "none")]
pub(crate) struct GoTypes<'ir> {
    classes: Vec<GoClass<'ir>>,
    structural_recursive_alias_cycles: Vec<GoTypeAlias<'ir>>,
}

pub(crate) fn cast_value(container_variable_name: &str, field_type: &GoType) -> String {
    if field_type.is_slice || field_type.is_map {
        return format!("({}).({})", container_variable_name, field_type.name);
    } else if field_type.is_pointer {
        let inner_type = field_type.underlying_type.as_ref().unwrap();
        return format!(
            "castOptional({container_variable_name}, func (item any) *{} {{
    return {}
}})",
            inner_type.name,
            cast_value("item", inner_type),
        );
    } else {
        return format!("({}).(*{})", container_variable_name, field_type.name);
    }
}

fn render_value_coercion(container_variable_name: &str, field_type: &GoType) -> String {
    if field_type.is_pointer {
        return format!(
            "func () {} {{
    val := baml.Decode({})
    if val == nil {{
        return nil
    }}
    return val.({})
}}()",
            field_type.name, container_variable_name, field_type.name,
        );
    } else if field_type.is_slice {
        let inner_type = field_type.underlying_type.as_ref().unwrap();
        return format!(
            r#"baml.DecodeList({container_variable_name}, func(__holder *cffi.CFFIValueHolder) {} {{
    return {}
}})"#,
            inner_type.name,
            render_value_coercion("__holder", inner_type),
        );
    } else if field_type.is_slice || field_type.is_map {
        return format!(
            "baml.Decode({container_variable_name}).({})",
            filters::type_name_without_pointer(&field_type.name).unwrap()
        );
    } else {
        return format!(
            "*baml.Decode({container_variable_name}).(*{})",
            field_type.name
        );
    }
}

fn render_value_encode(
    destination_variable_name: &str,
    destination_index: &String,
    source_variable_name: &String,
    field_type: &GoType,
) -> String {
    let mut rendering = String::new();
    rendering.push_str("{\n");
    if field_type.is_class || field_type.is_enum {
        rendering.push_str(
            format!(
                "encodedValue, err := {}.BamlEncode()\n",
                source_variable_name
            )
            .as_str(),
        );
        rendering.push_str(format!("if err != nil {{\n").as_str());
        rendering.push_str(format!("return nil, err\n").as_str());
        rendering.push_str("}\n");
        rendering.push_str(
            format!(
                "{}[{}] = json.RawMessage(encodedValue)\n",
                destination_variable_name, destination_index
            )
            .as_str(),
        );
    } else if field_type.is_slice {
        rendering
            .push_str(format!("list := make([]any, len({}))\n", source_variable_name,).as_str());
        rendering.push_str(format!("for i, v := range {} {{\n", source_variable_name).as_str());
        rendering.push_str(
            render_value_encode(
                "list",
                &String::from("i"),
                &String::from("v"),
                field_type.underlying_type.as_ref().unwrap(),
            )
            .as_str(),
        );
        rendering.push_str("}\n");
        rendering.push_str(
            format!(
                "{}[{}] = list\n",
                destination_variable_name, destination_index
            )
            .as_str(),
        );
    } else {
        rendering.push_str(
            format!(
                "{}[{}] = {}\n",
                destination_variable_name, destination_index, source_variable_name
            )
            .as_str(),
        );
    }
    rendering.push_str("}\n");
    rendering
}

fn to_exported_name(name: &str) -> String {
    let s = name.to_string();
    // make first letter uppercase
    let first_letter = s.chars().next().unwrap().to_uppercase();
    let rest = s[1..].to_string();
    format!("{}{}", first_letter, rest)
}

#[derive(askama::Template)]
#[template(path = "types-enums.go.j2", escape = "none")]
pub(crate) struct GoEnums<'ir> {
    enums: Vec<GoEnum<'ir>>,
}

#[derive(askama::Template)]
#[template(path = "types-unions.go.j2", escape = "none")]
pub(crate) struct GoUnions {
    unions: Vec<GoUnion>,
}

#[derive(askama::Template)]
#[template(path = "encode.go.j2", escape = "none")]
pub(crate) struct GoEncode<'ir> {
    package_name: String,
    classes: Vec<GoClass<'ir>>,
    enums: Vec<GoEnum<'ir>>,
    unions: Vec<GoUnion>,
}

// Any filter defined in the module `filters` is accessible in your template.
mod filters {
    // This filter does not have extra arguments
    pub fn exported_name<T: std::fmt::Display>(s: T) -> askama::Result<String> {
        let s = s.to_string();
        // make first letter uppercase
        let first_letter = s.chars().next().unwrap().to_uppercase();
        let rest = s[1..].to_string();
        Ok(format!("{}{}", first_letter, rest))
    }

    pub fn length<T>(v: &Vec<T>) -> Result<usize, askama::Error> {
        Ok(v.len())
    }

    pub fn is_pointer(s: &str) -> Result<bool, askama::Error> {
        Ok(s.starts_with("*"))
    }

    pub fn type_name_without_pointer(s: &str) -> Result<String, askama::Error> {
        if s.starts_with("*") {
            Ok(s[1..].to_string())
        } else {
            Ok(s.to_string())
        }
    }
}

// #[derive(askama::Template)]
// #[template(path = "type_builder.go.j2", escape = "none")]
pub(crate) struct TypeBuilder<'ir> {
    enums: Vec<GoEnum<'ir>>,
    classes: Vec<GoClass<'ir>>,
}

struct GoEnum<'ir> {
    name: &'ir str,
    values: Vec<(&'ir str, Option<String>)>,
    dynamic: bool,
    docstring: Option<String>,
}

struct GoUnion {
    name: String,
    variants: Vec<(String, GoType)>,
    docstring: Option<String>,
}

struct GoClass<'ir> {
    name: Cow<'ir, str>,
    /// The docstring for the class, including comment delimiters.
    docstring: Option<String>,
    // the name, type and docstring of the field.
    fields: Vec<GoField<'ir>>,
    dynamic: bool,
}

struct GoField<'ir> {
    name: Cow<'ir, str>,
    go_type: GoType,
    docstring: Option<String>,
}

pub struct GoType {
    pub name: String,
    pub is_pointer: bool,
    pub is_slice: bool,
    pub is_map: bool,
    pub is_primitive: bool,
    pub is_class: bool,
    pub is_integer: bool,
    pub is_enum: bool,
    pub is_union: bool,
    pub underlying_type: Option<Box<GoType>>,
}

struct GoTypeAlias<'ir> {
    name: Cow<'ir, str>,
    target: String,
    is_baml_serializable: bool,
    is_union: bool,
}

#[derive(askama::Template)]
#[template(path = "partial_types.go.j2", escape = "none")]
pub(crate) struct GoStreamTypes<'ir> {
    package_name: String,
    partial_classes: Vec<PartialGoClass<'ir>>,
}

/// The Go class corresponding to Partial<TypeDefinedInBaml>
struct PartialGoClass<'ir> {
    name: &'ir str,
    dynamic: bool,
    /// The docstring for the class, including comment delimiters.
    docstring: Option<String>,
    // the name, type and docstring of the field.
    fields: Vec<(&'ir str, String, Option<String>)>,
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for GoEnums<'ir> {
    type Error = anyhow::Error;

    fn try_from(
        (ir, _): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs),
    ) -> Result<GoEnums<'ir>> {
        Ok(GoEnums {
            enums: ir.walk_enums().map(GoEnum::from).collect::<Vec<_>>(),
        })
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for GoEncode<'ir> {
    type Error = anyhow::Error;

    fn try_from(
        (ir, gen): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs),
    ) -> Result<GoEncode<'ir>> {
        Ok(GoEncode {
            package_name: gen.client_package_name.as_ref().unwrap().clone(),
            classes: ir.walk_classes().map(GoClass::from).collect::<Vec<_>>(),
            enums: ir.walk_enums().map(GoEnum::from).collect::<Vec<_>>(),
            unions: GoUnions::try_from((ir, gen))?.unions,
        })
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for GoUnions {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<GoUnions> {
        // Collect all the unions in the IR.
        let mut unions = ir
            .walk_functions()
            .flat_map(|f| {
                f.inputs()
                    .iter()
                    .map(|arg| arg.1.find_union_types())
                    .chain(std::iter::once(f.elem().output().find_union_types()))
            })
            .chain(ir.walk_classes().flat_map(|c| {
                c.elem()
                    .static_fields
                    .iter()
                    .map(|f| f.elem.r#type.elem.find_union_types())
            }))
            .chain(
                ir.walk_type_aliases()
                    .map(|a| a.elem().r#type.elem.find_union_types()),
            )
            .chain(ir.walk_alias_cycles().map(|c| c.item.1.find_union_types()))
            .flat_map(|set| set.into_iter())
            .filter_map(|t| match &t {
                FieldType::Union(variants, _) => match variants.view() {
                    UnionTypeViewGeneric::Null => None,
                    UnionTypeViewGeneric::Optional(_) => None,
                    _ => Some(t),
                },
                _ => None,
            })
            .collect::<IndexSet<_>>()
            .into_iter()
            .map(|union_type| {
                let union_name = union_type.to_union_name();
                let FieldType::Union(variants, _) = union_type else {
                    unreachable!("This should have been filtered out earlier");
                };
                GoUnion {
                    name: union_name,
                    variants: variants
                        .iter_skip_null()
                        .iter()
                        .map(|v| (v.to_union_name(), v.to_type_ref_2(ir, false)))
                        .collect(),
                    docstring: None,
                }
            })
            .collect::<Vec<_>>();

        unions.sort_by_key(|u| u.name.clone());
        Ok(GoUnions { unions })
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for GoTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from(
        (ir, _): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs),
    ) -> Result<GoTypes<'ir>> {
        Ok(GoTypes {
            classes: ir.walk_classes().map(GoClass::from).collect::<Vec<_>>(),
            structural_recursive_alias_cycles: {
                let mut cycles = ir
                    .walk_alias_cycles()
                    .map(GoTypeAlias::from)
                    .collect::<Vec<_>>();
                cycles.sort_by_key(|alias| alias.name.clone());
                cycles
            },
        })
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for TypeBuilder<'ir> {
    type Error = anyhow::Error;

    fn try_from(
        (ir, _): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs),
    ) -> Result<TypeBuilder<'ir>> {
        Ok(TypeBuilder {
            enums: ir.walk_enums().map(GoEnum::from).collect::<Vec<_>>(),
            classes: ir.walk_classes().map(GoClass::from).collect::<Vec<_>>(),
        })
    }
}

impl<'ir> From<EnumWalker<'ir>> for GoEnum<'ir> {
    fn from(e: EnumWalker<'ir>) -> GoEnum<'ir> {
        GoEnum {
            name: e.name(),
            dynamic: e.item.attributes.get("dynamic_type").is_some(),
            values: e
                .item
                .elem
                .values
                .iter()
                .map(|v| (v.0.elem.0.as_str(), v.1.as_ref().map(render_docstring)))
                .collect(),
            docstring: e.item.elem.docstring.as_ref().map(render_docstring),
        }
    }
}

impl<'ir> From<ClassWalker<'ir>> for GoClass<'ir> {
    fn from(c: ClassWalker<'ir>) -> Self {
        GoClass {
            name: Cow::Borrowed(c.name()),
            dynamic: c.item.attributes.get("dynamic_type").is_some(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| GoField {
                    name: Cow::Borrowed(f.elem.name.as_str()),
                    go_type: f.elem.r#type.elem.to_type_ref_2(c.ir, false),
                    docstring: f.elem.docstring.as_ref().map(render_docstring),
                })
                .collect(),
            docstring: c.item.elem.docstring.as_ref().map(render_docstring),
        }
    }
}

// TODO: Define AliasWalker to simplify type.
impl<'ir> From<Walker<'ir, (&'ir String, &'ir FieldType)>> for GoTypeAlias<'ir> {
    fn from(walker: Walker<(&'ir String, &'ir FieldType)>) -> Self {
        let type_ref = walker.item.1.to_type_ref_2(walker.ir, false);
        GoTypeAlias {
            name: Cow::Borrowed(walker.item.0),
            target: type_ref.name,
            is_union: type_ref.is_union,
            is_baml_serializable: type_ref.is_class || type_ref.is_enum || type_ref.is_union,
        }
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for GoStreamTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from((ir, gen): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(Self {
            partial_classes: ir
                .walk_classes()
                .map(PartialGoClass::from)
                .collect::<Vec<_>>(),
            package_name: gen.client_package_name.as_ref().unwrap().clone(),
        })
    }
}

impl<'ir> From<ClassWalker<'ir>> for PartialGoClass<'ir> {
    fn from(c: ClassWalker<'ir>) -> PartialGoClass<'ir> {
        PartialGoClass {
            name: c.name(),
            dynamic: c.item.attributes.get("dynamic_type").is_some(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| {
                    // Fields with @stream.done should take their type from
                    let needed: bool = f.attributes.get("stream.not_null").is_some();
                    let metadata = f.elem.r#type.elem.meta().clone();
                    let done: bool = metadata.streaming_behavior.done;
                    let field = match (done, needed) {
                        // A normal partial field.
                        (false, false) => {
                            f.elem.r#type.elem.to_partial_type_ref_2(c.ir, false, false)
                        }
                        // A field with @stream.done and no @stream.not_null
                        (true, false) => {
                            optional(&f.elem.r#type.elem.to_type_ref_2(c.ir, true).name)
                        }
                        (false, true) => {
                            f.elem.r#type.elem.to_partial_type_ref_2(c.ir, false, true)
                        }
                        (true, true) => f.elem.r#type.elem.to_type_ref_2(c.ir, true).name, // TODO: Fix.
                    };
                    (
                        f.elem.name.as_str(),
                        field,
                        f.elem.docstring.as_ref().map(render_docstring),
                    )
                })
                .collect(),
            docstring: c.item.elem.docstring.as_ref().map(render_docstring),
        }
    }
}

/// Helper function for determining whether a field type should
/// be given a default None value when generating a go class
/// with that field.
fn has_none_default(ir: &IntermediateRepr, field_type: &FieldType) -> bool {
    match field_type {
        FieldType::Primitive(TypeValue::Null, _) => true,
        FieldType::Primitive(_, _) => false,
        FieldType::Class { .. } => false,
        FieldType::Enum { .. } => false,
        FieldType::List(_, _) => false,
        FieldType::Literal(_, _) => false,
        FieldType::Map(_, _, _) => false,
        FieldType::RecursiveTypeAlias(_, _) => false,
        FieldType::Tuple(_, _) => false,
        FieldType::Union(variants, _) => variants.is_optional(),
        FieldType::Arrow(_, _) => panic!("Generation is not supported with expr fns"),
    }
}

/// Returns the Go `Literal` representation of `self`.
pub fn to_go_literal(literal: &LiteralValue) -> String {
    // Go bools are a little special...
    match literal {
        LiteralValue::Bool(_) => format!("bool"),
        LiteralValue::Int(_) => format!("int"),
        LiteralValue::String(_) => format!("string"),
    }
}

pub trait ToTypeReferenceInTypeDefinition {
    fn to_type_ref_2(&self, ir: &IntermediateRepr, module_prefix: bool) -> GoType;
    fn to_type_ref_impl_2(&self, ir: &IntermediateRepr, module_prefix: bool) -> String;
    fn to_partial_type_ref_2(&self, ir: &IntermediateRepr, wrapped: bool, needed: bool) -> String;
    fn to_partial_type_ref_impl_2(
        &self,
        ir: &IntermediateRepr,
        wrapped: bool,
        needed: bool,
    ) -> String;
}

impl ToTypeReferenceInTypeDefinition for FieldType {
    fn to_type_ref_2(&self, ir: &IntermediateRepr, module_prefix: bool) -> GoType {
        let simplified = self.simplify();
        GoType {
            name: simplified.to_type_ref_impl_2(ir, module_prefix),
            is_pointer: self.is_optional(),
            is_union: matches!(simplified, FieldType::Union(_, _)),
            is_slice: matches!(simplified, FieldType::List(_, _)),
            is_map: matches!(simplified, FieldType::Map(_, _, _)),
            is_primitive: self.is_primitive(),
            is_class: matches!(simplified, FieldType::Class { .. }),
            is_integer: matches!(simplified, FieldType::Primitive(TypeValue::Int, _)),
            is_enum: matches!(simplified, FieldType::Enum { .. }),
            underlying_type: match simplified {
                FieldType::List(value, _) => Some(Box::new(value.to_type_ref_2(ir, module_prefix))),
                FieldType::Union(inner, _) => match inner.view() {
                    UnionTypeViewGeneric::Null => None,
                    UnionTypeViewGeneric::Optional(field_type) => {
                        Some(Box::new(field_type.to_type_ref_2(ir, module_prefix)))
                    }
                    UnionTypeViewGeneric::OneOf(field_types)
                    | UnionTypeViewGeneric::OneOfOptional(field_types) => {
                        None
                    }
                },
                _ => None,
            },
        }
    }

    fn to_partial_type_ref_2(&self, ir: &IntermediateRepr, wrapped: bool, needed: bool) -> String {
        self.simplify()
            .to_partial_type_ref_impl_2(ir, wrapped, needed)
    }

    // TODO: use_module_prefix boolean blindness. Replace with str?
    fn to_type_ref_impl_2(&self, ir: &IntermediateRepr, use_module_prefix: bool) -> String {
        let module_prefix = if use_module_prefix { "types." } else { "" };
        let base_rep = match self {
            FieldType::Enum { name, .. } => {
                // The enum owns its own dynamicism.
                format!("{module_prefix}{name}")
            }
            FieldType::RecursiveTypeAlias(name, _) => format!("{module_prefix}{name}"),
            FieldType::Literal(value, _) => to_go_literal(value),
            FieldType::Class { name, .. } => format!("{module_prefix}{name}"),
            FieldType::List(inner, _) => {
                format!("[]{}", inner.to_type_ref_2(ir, use_module_prefix).name)
            }
            FieldType::Map(key, value, _) => {
                format!(
                    "map[{}]{}",
                    key.to_type_ref_2(ir, use_module_prefix).name,
                    value.to_type_ref_2(ir, use_module_prefix).name
                )
            }
            FieldType::Primitive(r#type, _) => r#type.to_go(),
            FieldType::Union(inner, _) => match inner.view() {
                UnionTypeViewGeneric::Null => "any".to_string(),
                UnionTypeViewGeneric::Optional(field_type) => {
                    format!("*{}", field_type.to_type_ref_impl_2(ir, use_module_prefix))
                }
                UnionTypeViewGeneric::OneOf(field_types) => {
                    format!("{module_prefix}{}", self.to_union_name())
                }
                UnionTypeViewGeneric::OneOfOptional(field_types) => {
                    format!("*{module_prefix}{}", self.to_union_name())
                }
            },
            FieldType::Tuple(inner, _) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref_2(ir, use_module_prefix).name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Arrow(_, _) => panic!("Generation is not supported with expr fns"),
        };
        match field_type_attributes(self) {
            Some(_) => {
                format!("{module_prefix}Checked[{base_rep}]")
            }
            None => base_rep,
        }
    }

    fn to_partial_type_ref_impl_2(
        &self,
        ir: &IntermediateRepr,
        wrapped: bool,
        needed: bool,
    ) -> String {
        let metadata = self.meta().clone();
        let is_partial_type = !metadata.streaming_behavior.done;
        let use_module_prefix = !is_partial_type
            || matches!(
                self,
                FieldType::Union(_, _) | FieldType::RecursiveTypeAlias(_, _)
            );
        let with_state = metadata.streaming_behavior.state;
        let constraints = metadata.constraints;
        let module_prefix = if use_module_prefix { "types." } else { "" };
        let base_rep = match self {
            FieldType::Class { name, .. } => {
                if wrapped || needed {
                    format!("{module_prefix}{name}")
                } else {
                    format!("*{module_prefix}{name}")
                }
            }
            FieldType::Enum { name, .. } => {
                if needed || wrapped {
                    format!("types.{name}")
                } else {
                    format!("*types.{name}")
                }
            }
            FieldType::RecursiveTypeAlias(name, _) => {
                if wrapped {
                    format!("{module_prefix}{name}")
                } else {
                    format!("*{module_prefix}{name}")
                }
            }
            FieldType::Literal(value, _) => {
                if needed || wrapped {
                    to_go_literal(&value)
                } else {
                    format!("*{}", to_go_literal(&value))
                }
            }

            FieldType::List(inner, _) => {
                format!("[]{}", inner.to_partial_type_ref_2(ir, true, false))
            }
            FieldType::Map(key, value, _) => format!(
                "map[{}]{}",
                key.to_type_ref_2(ir, use_module_prefix).name,
                value.to_partial_type_ref_2(ir, false, false)
            ),
            FieldType::Primitive(r#type, _) => {
                if needed || wrapped {
                    r#type.to_go()
                } else {
                    format!("*{}", r#type.to_go())
                }
            }
            FieldType::Union(inner, _) => match inner.view() {
                UnionTypeViewGeneric::Null => "any".to_string(),
                UnionTypeViewGeneric::Optional(field_type) => {
                    format!("*{}", field_type.to_type_ref_impl_2(ir, use_module_prefix))
                }
                UnionTypeViewGeneric::OneOf(field_types) => {
                    if needed {
                        format!("{module_prefix}{}", self.to_union_name())
                    } else {
                        format!("*{module_prefix}{}", self.to_union_name())
                    }
                }
                UnionTypeViewGeneric::OneOfOptional(field_types) => {
                    format!("*{module_prefix}{}", self.to_union_name())
                }
            },
            FieldType::Tuple(inner, _) => {
                todo!("Tuples are not supported in partial types.")
            }
            FieldType::Arrow(_, _) => panic!("Generation is not supported with expr fns"),
        };
        let base_type_ref = if is_partial_type {
            base_rep
        } else {
            if needed {
                base_rep
            } else {
                format!("*{}", base_rep)
            }
        };
        let rep_with_checks = match field_type_attributes(self) {
            Some(_) => {
                format!("types.Checked[{base_type_ref}]")
            }
            None => base_type_ref,
        };
        let rep_with_stream_state = if with_state {
            stream_state(&rep_with_checks)
        } else {
            rep_with_checks
        };
        rep_with_stream_state
    }
}

/// Render the BAML documentation (a bare string with padding stripped)
/// into a Go docstring. (Indented once and surrounded by """).
fn render_docstring(d: &Docstring) -> String {
    d.0.as_str()
        .lines()
        .map(|line| format!("// {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn optional(base: &str) -> String {
    format!("*{}", base)
}

fn stream_state(base: &str) -> String {
    format!("StreamState[{base}]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use internal_baml_core::ir::repr::{make_test_ir, IntermediateRepr};

    #[test]
    fn test_optional_list() {
        let ir = make_test_ir("").unwrap();
        let optional_list = FieldType::string().as_list().as_optional();
        let full = optional_list.to_type_ref_2(&ir, false);
        let partial = optional_list.to_partial_type_ref_2(&ir, false, false);
        assert_eq!(full.name, "*[]string");
        assert_eq!(partial, "*[]string");
    }
    #[test]
    fn test_union() {
        let ir = make_test_ir("").unwrap();
        let union = FieldType::union(vec![FieldType::string(), FieldType::int()]);
        let full = union.to_type_ref_2(&ir, false);
        let partial = union.to_partial_type_ref_2(&ir, false, false);
        assert_eq!(full.name, "Union__string__int");
        assert_eq!(partial, "*types.Union__string__int");
    }
    #[test]
    fn test_union_with_optional() {
        let ir = make_test_ir("").unwrap();
        let union = FieldType::union(vec![
            FieldType::optional(FieldType::string()),
            FieldType::int(),
        ]);
        let full = union.to_type_ref_2(&ir, false);
        let partial = union.to_partial_type_ref_2(&ir, false, false);
        assert_eq!(full.name, "*Union__string__int");
        assert_eq!(partial, "*types.Union__string__int");
    }
}

use anyhow::Result;
use baml_types::{FieldType, LiteralValue, TypeValue};
use itertools::Itertools;
use std::borrow::Cow;

use crate::{field_type_attributes, type_check_attributes, TypeCheckAttributes};

use super::python_language_features::ToPython;
use internal_baml_core::ir::{
    repr::{Docstring, IntermediateRepr, Walker},
    ClassWalker, EnumWalker, IRHelper, IRHelperExtended,
};

#[derive(askama::Template)]
#[template(path = "types.py.j2", escape = "none")]
pub(crate) struct PythonTypes<'ir> {
    enums: Vec<PythonEnum<'ir>>,
    classes: Vec<PythonClass<'ir>>,
    is_pydantic_2: bool,
    structural_recursive_alias_cycles: Vec<PythonTypeAlias<'ir>>,
}

#[derive(askama::Template)]
#[template(path = "type_builder.py.j2", escape = "none")]
pub(crate) struct TypeBuilder<'ir> {
    enums: Vec<PythonEnum<'ir>>,
    classes: Vec<PythonClass<'ir>>,
}

struct PythonEnum<'ir> {
    name: &'ir str,
    values: Vec<(&'ir str, Option<String>)>,
    dynamic: bool,
    docstring: Option<String>,
}

struct PythonClass<'ir> {
    name: Cow<'ir, str>,
    /// The docstring for the class, including comment delimiters.
    docstring: Option<String>,
    // the name, type and docstring of the field.
    fields: Vec<(Cow<'ir, str>, String, Option<String>)>,
    dynamic: bool,
}

struct PythonTypeAlias<'ir> {
    name: Cow<'ir, str>,
    target: String,
}

#[derive(askama::Template)]
#[template(path = "partial_types.py.j2", escape = "none")]
pub(crate) struct PythonStreamTypes<'ir> {
    partial_classes: Vec<PartialPythonClass<'ir>>,
    is_pydantic_2: bool,
    structural_recursive_alias_cycles: Vec<PythonTypeAlias<'ir>>,
}

/// The Python class corresponding to Partial<TypeDefinedInBaml>
struct PartialPythonClass<'ir> {
    name: &'ir str,
    dynamic: bool,
    /// The docstring for the class, including comment delimiters.
    docstring: Option<String>,
    // the name, type and docstring of the field.
    fields: Vec<(&'ir str, String, Option<String>)>,
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from(
        (ir, gen): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs),
    ) -> Result<PythonTypes<'ir>> {
        Ok(PythonTypes {
            enums: ir.walk_enums().map(PythonEnum::from).collect::<Vec<_>>(),
            classes: ir.walk_classes().map(PythonClass::from).collect::<Vec<_>>(),
            is_pydantic_2: matches!(
                gen.client_type,
                baml_types::GeneratorOutputType::PythonPydantic
            ),
            structural_recursive_alias_cycles: {
                let mut cycles = ir
                    .walk_alias_cycles()
                    .map(PythonTypeAlias::from)
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
            enums: ir.walk_enums().map(PythonEnum::from).collect::<Vec<_>>(),
            classes: ir.walk_classes().map(PythonClass::from).collect::<Vec<_>>(),
        })
    }
}

impl<'ir> From<EnumWalker<'ir>> for PythonEnum<'ir> {
    fn from(e: EnumWalker<'ir>) -> PythonEnum<'ir> {
        PythonEnum {
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

impl<'ir> From<ClassWalker<'ir>> for PythonClass<'ir> {
    fn from(c: ClassWalker<'ir>) -> Self {
        PythonClass {
            name: Cow::Borrowed(c.name()),
            dynamic: c.item.attributes.get("dynamic_type").is_some(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| {
                    (
                        Cow::Borrowed(f.elem.name.as_str()),
                        add_default_value(
                            c.ir,
                            &f.elem.r#type.elem,
                            &f.elem.r#type.elem.to_type_ref(c.ir, false),
                        ),
                        f.elem.docstring.as_ref().map(render_docstring),
                    )
                })
                .collect(),
            docstring: c.item.elem.docstring.as_ref().map(render_docstring),
        }
    }
}

// TODO: Define AliasWalker to simplify type.
impl<'ir> From<Walker<'ir, (&'ir String, &'ir FieldType)>> for PythonTypeAlias<'ir> {
    fn from(
        Walker {
            ir,
            item: (name, target),
        }: Walker<(&'ir String, &'ir FieldType)>,
    ) -> Self {
        PythonTypeAlias {
            name: Cow::Borrowed(name),
            target: target.to_type_ref(ir, false),
        }
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonStreamTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from((ir, gen): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(Self {
            partial_classes: ir
                .walk_classes()
                .map(PartialPythonClass::from)
                .collect::<Vec<_>>(),
            is_pydantic_2: matches!(
                gen.client_type,
                baml_types::GeneratorOutputType::PythonPydantic
            ),
            structural_recursive_alias_cycles: {
                let mut cycles = ir
                    .walk_alias_cycles()
                    .map(PythonTypeAlias::from)
                    .collect::<Vec<_>>();
                cycles.sort_by_key(|alias| alias.name.clone());
                cycles
            },
        })
    }
}

impl<'ir> From<ClassWalker<'ir>> for PartialPythonClass<'ir> {
    fn from(c: ClassWalker<'ir>) -> PartialPythonClass<'ir> {
        PartialPythonClass {
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
                    let (_, metadata) = c.ir.distribute_metadata(&f.elem.r#type.elem);
                    let done: bool = metadata.1.done;
                    let (field, optional) = match (done, needed) {
                        (false, false) => {
                            f.elem.r#type.elem.to_partial_type_ref(c.ir, false, false)
                        }
                        (true, false) => (
                            format!("Optional[{}]", f.elem.r#type.elem.to_type_ref(c.ir, true)),
                            true,
                        ),
                        (false, true) => f.elem.r#type.elem.to_partial_type_ref(c.ir, true, true),
                        (true, true) => (f.elem.r#type.elem.to_type_ref(c.ir, true), false),
                    };
                    (
                        f.elem.name.as_str(),
                        add_default_value(c.ir, &f.elem.r#type.elem, &field),
                        f.elem.docstring.as_ref().map(render_docstring),
                    )
                })
                .collect(),
            docstring: c.item.elem.docstring.as_ref().map(render_docstring),
        }
    }
}

/// Add a default None value to class fields that support defaulting, i.e.
/// `Optional[T]` fields and fields with a type that is unioned with `None`.
pub fn add_default_value(
    ir: &IntermediateRepr,
    field_type: &FieldType,
    type_str: &String,
) -> String {
    // Short-circuite with a None default value if the type starts with
    // `Optional` because this case always unambiguously requires None.
    if type_str.starts_with("Optional[") {
        return format!("{} = None", type_str);
    }
    if has_none_default(ir, field_type) {
        format!("{} = None", type_str)
    } else {
        type_str.clone()
    }
}

/// Helper function for determining whether a field type should
/// be given a default None value when generating a python class
/// with that field.
fn has_none_default(ir: &IntermediateRepr, field_type: &FieldType) -> bool {
    let base_type = ir.distribute_metadata(field_type).0;
    match base_type {
        FieldType::Primitive(TypeValue::Null) => true,
        FieldType::Primitive(_) => false,
        FieldType::Optional(_) => true,
        FieldType::Class(_) => false,
        FieldType::Enum(_) => false,
        FieldType::List(_) => false,
        FieldType::Literal(_) => false,
        FieldType::Map(_, _) => false,
        FieldType::RecursiveTypeAlias(_) => false,
        FieldType::Tuple(_) => false,
        FieldType::Union(variants) => variants
            .iter()
            .map(|variant| has_none_default(ir, variant))
            .any(|b| b),
        FieldType::WithMetadata { .. } => {
            unreachable!("FieldType::WithMetadata is always consumed by distribute_metadata")
        }
        FieldType::Arrow(_) => false,
    }
}

pub fn type_name_for_checks(checks: &TypeCheckAttributes) -> String {
    let check_names = checks
        .0
        .iter()
        .map(|check| format!("\"{check}\""))
        .sorted()
        .join(", ");

    format!["Literal[{check_names}]"]
}

/// Returns the Python `Literal` representation of `self`.
pub fn to_python_literal(literal: &LiteralValue) -> String {
    // Python bools are a little special...
    let value = match literal {
        LiteralValue::Bool(bool) => String::from(match *bool {
            true => "True",
            false => "False",
        }),

        // Rest of types match the fmt::Display impl.
        other => other.to_string(),
    };

    format!("Literal[{value}]")
}

trait ToTypeReferenceInTypeDefinition {
    fn to_type_ref(&self, ir: &IntermediateRepr, module_prefix: bool) -> String;
    fn to_partial_type_ref(
        &self,
        ir: &IntermediateRepr,
        wrapped: bool,
        needed: bool,
    ) -> (String, bool);
}

impl ToTypeReferenceInTypeDefinition for FieldType {
    // TODO: use_module_prefix boolean blindness. Replace with str?
    fn to_type_ref(&self, ir: &IntermediateRepr, use_module_prefix: bool) -> String {
        let module_prefix = if use_module_prefix { "types." } else { "" };
        match self {
            FieldType::Enum(name) => {
                if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    format!("Union[\"{module_prefix}{name}\", str]")
                } else {
                    format!("\"{module_prefix}{name}\"")
                }
            }
            FieldType::RecursiveTypeAlias(name) => format!("\"{name}\""),
            FieldType::Literal(value) => to_python_literal(value),
            FieldType::Class(name) => format!("\"{module_prefix}{name}\""),
            FieldType::List(inner) => format!("List[{}]", inner.to_type_ref(ir, use_module_prefix)),
            FieldType::Map(key, value) => {
                format!(
                    "Dict[{}, {}]",
                    key.to_type_ref(ir, use_module_prefix),
                    value.to_type_ref(ir, use_module_prefix)
                )
            }
            FieldType::Primitive(r#type) => r#type.to_python(),
            FieldType::Union(inner) => format!(
                "Union[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref(ir, use_module_prefix))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref(ir, use_module_prefix))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => {
                format!("Optional[{}]", inner.to_type_ref(ir, use_module_prefix))
            }
            FieldType::WithMetadata { base, .. } => match field_type_attributes(self) {
                Some(checks) => {
                    let base_type_ref = base.to_type_ref(ir, use_module_prefix);
                    let checks_type_ref = type_name_for_checks(&checks);
                    format!("Checked[{base_type_ref},{checks_type_ref}]")
                }
                None => base.to_type_ref(ir, use_module_prefix),
            },
            FieldType::Arrow(_) => {
                todo!("Arrow types should not be used in generated type definitions")
            }
        }
    }

    /// How to serialize the partial version of a field type.
    /// Also returns whether the field is optional during streaming.
    fn to_partial_type_ref(
        &self,
        ir: &IntermediateRepr,
        wrapped: bool,
        needed: bool,
    ) -> (String, bool) {
        let (base_type, metadata) = ir.distribute_metadata(self);
        let is_partial_type = !metadata.1.done;
        let use_module_prefix = !is_partial_type;
        let with_state = metadata.1.state;
        let constraints = metadata.0;
        let module_prefix = if is_partial_type { "" } else { "types." };
        let base_rep = match base_type {
            FieldType::Class(name) => {
                if wrapped || needed {
                    (format!("\"{module_prefix}{name}\""), false)
                } else {
                    (format!("Optional[\"{module_prefix}{name}\"]"), true)
                }
            }
            FieldType::Enum(name) => {
                if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    if needed || wrapped {
                        (format!("Union[types.{name}, str]"), false)
                    } else {
                        (format!("Optional[Union[types.{name}, str]]"), true)
                    }
                } else {
                    if needed || wrapped {
                        (format!("types.{name}"), false)
                    } else {
                        (format!("Optional[types.{name}]"), true)
                    }
                }
            }
            FieldType::RecursiveTypeAlias(name) => {
                if wrapped {
                    (format!("\"{name}\""), false)
                } else {
                    (format!("Optional[\"{name}\"]"), true)
                }
            }
            FieldType::Literal(value) => {
                if needed || wrapped {
                    (to_python_literal(value), false)
                } else {
                    (format!("Optional[{}]", to_python_literal(value)), true)
                }
            } // TODO: Handle `needed` here.

            FieldType::List(inner) => (
                format!("List[{}]", inner.to_partial_type_ref(ir, true, false).0),
                false,
            ),
            FieldType::Map(key, value) => (
                format!(
                    "Dict[{}, {}]",
                    key.to_type_ref(ir, use_module_prefix),
                    value.to_partial_type_ref(ir, false, false).0
                ),
                false,
            ),
            FieldType::Primitive(r#type) => {
                if needed || wrapped {
                    (r#type.to_python(), false)
                } else {
                    (format!("Optional[{}]", r#type.to_python()), true)
                }
            }
            FieldType::Union(inner) => {
                let union_contents = inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir, true, false).0)
                    .collect::<Vec<_>>()
                    .join(", ");
                if needed || wrapped {
                    (format!("Union[{union_contents}]"), false)
                } else {
                    (format!("Optional[Union[{union_contents}]]"), true)
                }
            }
            FieldType::Tuple(inner) => {
                let tuple_contents = inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir, false, false).0)
                    .collect::<Vec<_>>()
                    .join(", ");
                if needed || wrapped {
                    (format!("Tuple[{tuple_contents}]"), false)
                } else {
                    (format!("Optional[Tuple[{tuple_contents}]]"), true)
                }
            }
            FieldType::Optional(inner) => (
                format!("Optional[{}]", inner.to_partial_type_ref(ir, true, false).0),
                true,
            ),
            FieldType::WithMetadata { .. } => {
                unreachable!("distribute_metadata makes this branch unreachable.")
            }
            FieldType::Arrow(_) => {
                todo!("Arrow types should not be used in generated type definitions")
            }
        };
        let base_type_ref = if is_partial_type {
            base_rep
        } else {
            if needed {
                (base_type.to_type_ref(ir, use_module_prefix), false)
            } else {
                base_rep
            }
        };
        let rep_with_checks = match field_type_attributes(self) {
            Some(checks) => {
                let checks_type_ref = type_name_for_checks(&checks);
                (
                    format!("Checked[{},{checks_type_ref}]", base_type_ref.0),
                    base_type_ref.1,
                )
            }
            None => base_type_ref,
        };
        let rep_with_stream_state = if with_state {
            (stream_state(&rep_with_checks.0), rep_with_checks.1)
        } else {
            rep_with_checks
        };
        rep_with_stream_state
    }
}

/// Render the BAML documentation (a bare string with padding stripped)
/// into a Python docstring. (Indented once and surrounded by """).
fn render_docstring(d: &Docstring) -> String {
    let lines = d.0.as_str().replace("\n", "\n    ");
    format!("\"\"\"{lines}\"\"\"")
}

fn optional(base: &str) -> String {
    format!("Optional[{base}]")
}

fn stream_state(base: &str) -> String {
    format!("StreamState[{base}]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::StreamingBehavior;
    use internal_baml_core::ir::repr::{make_test_ir, IntermediateRepr};

    #[test]
    fn test_optional_list() {
        let ir = make_test_ir("").unwrap();
        let optional_list = FieldType::Optional(Box::new(FieldType::List(Box::new(
            FieldType::Primitive(TypeValue::String),
        ))));
        let full = optional_list.to_type_ref(&ir, false);
        let partial = optional_list.to_partial_type_ref(&ir, false, false);
        assert_eq!(full, "Optional[List[str]]");
        assert_eq!(partial.0, "Optional[List[str]]");
    }

    #[test]
    fn test_union() {
        let ir = make_test_ir("").unwrap();
        let optional_list = FieldType::Optional(Box::new(FieldType::List(Box::new(
            FieldType::Primitive(TypeValue::String),
        ))));
        let full = optional_list.to_type_ref(&ir, false);
        let partial = optional_list.to_partial_type_ref(&ir, false, false);
        assert_eq!(full, "Optional[List[str]]");
        assert_eq!(partial.0, "Optional[List[str]]");
    }

    #[test]
    fn test_stream_done_type() {
        let ir = make_test_ir("").unwrap();
        let done_type = FieldType::WithMetadata {
            base: Box::new(FieldType::class("Foo")),
            streaming_behavior: StreamingBehavior {
                done: true,
                state: false,
                needed: false,
            },
            constraints: vec![],
        };
        let full = done_type.to_type_ref(&ir, false);
        let partial = done_type.to_partial_type_ref(&ir, false, false);
        assert_eq!(full, "\"Foo\"");
        assert_eq!(partial.0, "Optional[\"types.Foo\"]");
    }
}

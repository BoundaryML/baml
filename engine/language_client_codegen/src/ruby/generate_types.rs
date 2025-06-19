use std::borrow::Cow;
use std::collections::HashSet;

use anyhow::Result;
use baml_types::{ir_type::UnionTypeViewGeneric, LiteralValue};
use itertools::Itertools;

use crate::{field_type_attributes, type_check_attributes, TypeCheckAttributes};

use super::ruby_language_features::ToRuby;
use internal_baml_core::ir::{
    repr::{Docstring, IntermediateRepr},
    ClassWalker, EnumWalker, FieldType, IRHelper, IRHelperExtended,
};

#[derive(askama::Template)]
#[template(path = "types.rb.j2", escape = "none")]
pub(crate) struct RubyTypes<'ir> {
    enums: Vec<RubyEnum<'ir>>,
    classes: Vec<RubyStruct<'ir>>,
}

struct RubyEnum<'ir> {
    pub name: &'ir str,
    pub values: Vec<&'ir str>,
    dynamic: bool,
    docstring: Option<String>,
}

struct RubyStruct<'ir> {
    name: Cow<'ir, str>,
    fields: Vec<(Cow<'ir, str>, String, Option<String>)>,
    dynamic: bool,
    docstring: Option<String>,
}

#[derive(askama::Template)]
#[template(path = "partial-types.rb.j2", escape = "none")]
pub(crate) struct RubyStreamTypes<'ir> {
    partial_classes: Vec<PartialRubyStruct<'ir>>,
}

/// The Python class corresponding to Partial<TypeDefinedjInBaml>
struct PartialRubyStruct<'ir> {
    name: &'ir str,
    // the name, type and docstring of the field
    fields: Vec<(&'ir str, String, Option<String>)>,
    docstring: Option<String>,
}

#[derive(askama::Template)]
#[template(path = "type-registry.rb.j2", escape = "none")]
pub(crate) struct TypeRegistry<'ir> {
    enums: Vec<RubyEnum<'ir>>,
    classes: Vec<RubyStruct<'ir>>,
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'ir crate::GeneratorArgs)> for RubyTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&'ir IntermediateRepr, &'ir crate::GeneratorArgs)) -> Result<Self> {
        Ok(RubyTypes {
            enums: ir.walk_enums().map(|e| e.into()).collect(),
            classes: ir.walk_classes().map(|c| c.into()).collect(),
        })
    }
}

impl<'ir> From<EnumWalker<'ir>> for RubyEnum<'ir> {
    fn from(e: EnumWalker<'ir>) -> RubyEnum<'ir> {
        RubyEnum {
            name: e.name(),
            dynamic: e.item.attributes.get("dynamic_type").is_some(),
            values: e
                .item
                .elem
                .values
                .iter()
                .map(|v| v.0.elem.0.as_str())
                .collect(),
            docstring: e
                .item
                .elem
                .docstring
                .as_ref()
                .map(|d| render_docstring(d, true)),
        }
    }
}

impl<'ir> From<ClassWalker<'ir>> for RubyStruct<'ir> {
    fn from(c: ClassWalker<'ir>) -> RubyStruct<'ir> {
        RubyStruct {
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
                        f.elem.r#type.elem.to_type_ref(),
                        f.elem.docstring.as_ref().map(|d| render_docstring(d, true)),
                    )
                })
                .collect(),
            docstring: c
                .item
                .elem
                .docstring
                .as_ref()
                .map(|d| render_docstring(d, false)),
        }
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'ir crate::GeneratorArgs)> for RubyStreamTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&'ir IntermediateRepr, &'ir crate::GeneratorArgs)) -> Result<Self> {
        Ok(RubyStreamTypes {
            partial_classes: ir.walk_classes().map(|c| c.into()).collect(),
        })
    }
}

impl<'ir> From<ClassWalker<'ir>> for PartialRubyStruct<'ir> {
    fn from(c: ClassWalker<'ir>) -> PartialRubyStruct<'ir> {
        PartialRubyStruct {
            name: c.name(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| {
                    let not_null: bool = f.attributes.get("stream.not_null").is_some();
                    let metadata = f.elem.r#type.elem.meta();
                    let done = metadata.streaming_behavior.done;
                    let field_type = f.elem.r#type.elem.clone();
                    let generated_field_type = match (done, not_null) {
                        (false, false) => {
                            format!("{}", field_type.to_partial_type_ref(c.ir, false))
                        }
                        (true, false) => format!("T.nilable({})", field_type.to_type_ref()),
                        (false, true) => field_type.to_partial_type_ref(c.ir, true),
                        (true, true) => field_type.to_type_ref(),
                    };
                    (
                        f.elem.name.as_str(),
                        generated_field_type,
                        f.elem.docstring.as_ref().map(|d| render_docstring(d, true)),
                    )
                })
                .collect(),
            docstring: c
                .item
                .elem
                .docstring
                .as_ref()
                .map(|d| render_docstring(d, false)),
        }
    }
}

pub(super) trait ToTypeReferenceInTypeDefinition<'a> {
    fn to_type_ref(&self) -> String;
    fn to_partial_type_ref(&self, ir: &'a IntermediateRepr, already_nilable: bool) -> String;
}

impl ToTypeReferenceInTypeDefinition<'_> for FieldType {
    fn to_type_ref(&self) -> String {
        use ToRuby;
        self.to_ruby()
    }

    /// Render a type into a string for use in a partial-types context.
    /// The `already_nilable` field indicates whether the caller will wrap
    /// the returned string with `nilable`, and this function does not need
    fn to_partial_type_ref(&self, ir: &IntermediateRepr, already_nilable: bool) -> String {
        let metadata = self.meta();
        let inner = match self {
            FieldType::Class { name, .. } => {
                if already_nilable {
                    format!("Baml::PartialTypes::{}", name.clone())
                } else {
                    format!("T.nilable(Baml::PartialTypes::{})", name.clone())
                }
            }
            FieldType::Enum { name, .. } => {
                if already_nilable {
                    format!("T.nilable(Baml::Types::{})", name.clone())
                } else {
                    format!("T.nilable(Baml::Types::{})", name.clone())
                }
            }
            // TODO: Can we define recursive aliases in Ruby with Sorbet?
            FieldType::RecursiveTypeAlias(..) => "T.anything".to_string(),
            // TODO: Temporary solution until we figure out Ruby literals.
            FieldType::Literal(value, _) => value
                .literal_base_type()
                .to_partial_type_ref(ir, already_nilable),
            // https://sorbet.org/docs/stdlib-generics
            FieldType::List(inner, _) => format!("T::Array[{}]", inner.to_partial_type_ref(ir, false)),
            FieldType::Map(key, value, _) => format!(
                "T::Hash[{}, {}]",
                match key.as_ref() {
                    // For enums just default to strings.
                    FieldType::Enum { .. }
                    | FieldType::Literal(LiteralValue::String(_), _)
                    | FieldType::Union(_, _) => FieldType::string().to_type_ref(),
                    _ => key.to_type_ref(),
                },
                value.to_partial_type_ref(ir, false)
            ),
            FieldType::Primitive { .. } => {
                if already_nilable {
                    self.to_type_ref()
                } else {
                    format!("T.nilable({})", self.to_type_ref())
                }
            }
            FieldType::Union(inner, _) => {
                match inner.view() {
                    UnionTypeViewGeneric::Null => "NilClass".to_string(),
                    UnionTypeViewGeneric::Optional(field_type) => format!("T.nilable({})", field_type.to_partial_type_ref(ir, already_nilable)),
                    UnionTypeViewGeneric::OneOf(field_types) => format!("T.any({})", field_types.iter().map(|t| t.to_partial_type_ref(ir, already_nilable)).collect::<Vec<_>>().join(", ")),
                    UnionTypeViewGeneric::OneOfOptional(field_types) => format!("T.nilable(T.any({}))", field_types.iter().map(|t| t.to_partial_type_ref(ir, already_nilable)).collect::<Vec<_>>().join(", ")),
                }
            }
            FieldType::Tuple(inner, _) => {
                let inner_string =
                // https://sorbet.org/docs/tuples
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir, false))
                    .collect::<Vec<_>>()
                    .join(", ")
                ;
                if already_nilable {
                    format!("[{}]", inner_string)
                } else {
                    format!("T.nilable([{}])", inner_string)
                }
            }
            FieldType::Arrow(_, _) => todo!("Arrow types should not be used in generated type definitions"),
        };
        let meta_repr = match field_type_attributes(self) {
            Some(checks) => {
                let base_type_ref = self.to_partial_type_ref(ir, false);
                format!("Baml::Checked[{base_type_ref}]")
            }
            None => self.to_partial_type_ref(ir, false),
        };
        if metadata.streaming_behavior.state {
            format!("Baml::StreamState[{inner}]")
        } else {
            inner
        }
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'_ crate::GeneratorArgs)> for TypeRegistry<'ir> {
    type Error = anyhow::Error;

    fn try_from(
        (ir, _): (&'ir IntermediateRepr, &'_ crate::GeneratorArgs),
    ) -> Result<TypeRegistry<'ir>> {
        Ok(TypeRegistry {
            enums: ir.walk_enums().map(RubyEnum::from).collect::<Vec<_>>(),
            classes: ir.walk_classes().map(RubyStruct::from).collect::<Vec<_>>(),
        })
    }
}

/// Render the BAML documentation (a bare string with padding stripped)
/// into a Ruby docstring.
fn render_docstring(d: &Docstring, indented: bool) -> String {
    if indented {
        let lines = d.0.as_str().replace("\n", "\n      # ");
        format!("# {lines}")
    } else {
        let lines = d.0.as_str().replace("\n", "\n    # ");
        format!("# {lines}")
    }
}

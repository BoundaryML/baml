use baml_types::ir_type::{Type, TypeValue, UnionTypeViewGeneric};
use baml_types::{BamlMediaType, Constraint, ConstraintLevel, LiteralValue};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::ir_helpers::IRHelper;
use internal_baml_core::ir::repr::IntermediateRepr;
use serde::Serialize;
use std::hash::Hash;
/// The abstract type system of OpenAPI.
/// We convert IR into this, before generating the OpenAPI yaml file.
///
/// We serialize to the openapi.yaml file directly from this type,
/// so changes to variant and field names must be accounted for with serde
/// attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum TypeOpenApi {
    Ref {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(rename = "$ref")]
        r#ref: String,
    },
    Inline {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(flatten)]
        r#type: TypePrimitive,
    },
    Union {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(rename = "oneOf")]
        one_of: Vec<TypeOpenApi>,
    },
    AnyValue {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(rename = "AnyValue")]
        any_value: IndexMap<String, TypeOpenApi>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum TypePrimitive {
    String,
    Number,
    Integer,
    Boolean,
    Array {
        items: Box<TypeOpenApi>,
    },
    Object {
        #[serde(skip_serializing_if = "IndexMap::is_empty")]
        properties: IndexMap<String, TypeOpenApi>,
        required: IndexSet<String>,
        #[serde(rename = "additionalProperties")]
        additional_properties: AdditionalProperties,
    },
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdditionalProperties {
    Closed,
    Open,
    Schema(Box<TypeOpenApi>),
}

impl Serialize for AdditionalProperties {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AdditionalProperties::Closed => serializer.serialize_bool(false),
            AdditionalProperties::Open => serializer.serialize_bool(true),
            AdditionalProperties::Schema(schema) => schema.serialize(serializer),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenApiMeta {
    /// Pydantic includes this by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// JSON schema considers 'enum' to be a validation rule, not a type,
    /// so it can be attached to any type.
    /// We only allow string-shaped enums
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#enum: Option<Vec<String>>,

    /// We only allow string-shaped const values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#const: Option<String>,
    // description: Option<String>,
    /// Nulls in OpenAPI are weird: https://swagger.io/docs/specification/data-models/data-types/
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nullable: bool,
}

impl Default for OpenApiMeta {
    fn default() -> Self {
        Self {
            nullable: false,
            title: None,
            r#enum: None,
            r#const: None,
        }
    }
}

impl Hash for TypeOpenApi {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let s = serde_yaml::to_string(self).expect("Should serialize");
        s.hash(state);
    }
}

impl TypeOpenApi {
    pub fn meta(&self) -> &OpenApiMeta {
        match self {
            TypeOpenApi::Ref { meta, .. } => meta,
            TypeOpenApi::Inline { meta, .. } => meta,
            TypeOpenApi::Union { meta, .. } => meta,
            TypeOpenApi::AnyValue { meta, .. } => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut OpenApiMeta {
        match self {
            TypeOpenApi::Ref { meta, .. } => meta,
            TypeOpenApi::Inline { meta, .. } => meta,
            TypeOpenApi::Union { meta, .. } => meta,
            TypeOpenApi::AnyValue { meta, .. } => meta,
        }
    }

    pub fn with_meta(mut self, meta: OpenApiMeta) -> Self {
        *self.meta_mut() = meta;
        self
    }
}

pub fn convert_ir_type(ir: &IntermediateRepr, ty: &Type) -> TypeOpenApi {
    let meta_enum: Option<Vec<String>> = match ty {
        Type::Enum { name, .. } => ir.find_enum(name).ok().map(|e| {
            e.item
                .elem
                .values
                .iter()
                .map(|v| v.0.elem.0.to_string())
                .collect()
        }),
        _ => None,
    };
    let meta_enum = None;
    let meta_const = match ty {
        Type::Literal(literal, _) => Some(literal.to_string()),
        _ => None,
    };
    let meta_const = None;
    let meta = OpenApiMeta {
        nullable: ty.is_optional(),
        r#enum: meta_enum,
        r#const: meta_const,
        title: None, // TODO: Correct?
    };
    let meta_copy = meta.clone();
    let base_rep = match ty {
        Type::Primitive(inner, _) => match inner {
            TypeValue::String => TypeOpenApi::Inline {
                r#type: TypePrimitive::String,
                meta: meta_copy,
            },
            TypeValue::Int => TypeOpenApi::Inline {
                r#type: TypePrimitive::Integer,
                meta: meta_copy,
            },
            TypeValue::Float => TypeOpenApi::Inline {
                r#type: TypePrimitive::Number,
                meta: meta_copy,
            },
            TypeValue::Bool => TypeOpenApi::Inline {
                r#type: TypePrimitive::Boolean,
                meta: meta_copy,
            },
            TypeValue::Null => TypeOpenApi::Inline {
                r#type: TypePrimitive::Null,
                meta: meta_copy,
            },
            TypeValue::Media(media_type) => {
                let media_type = match media_type {
                    BamlMediaType::Image => "BamlImage",
                    BamlMediaType::Audio => "BamlAudio",
                };
                TypeOpenApi::Ref {
                    r#ref: format!("#/components/schemas/{}", media_type),
                    meta: meta_copy,
                }
            }
        },
        Type::Class { name, .. } => TypeOpenApi::Ref {
            r#ref: format!("#/components/schemas/{}", name),
            meta: meta_copy,
        },
        Type::List(inner, _) => TypeOpenApi::Inline {
            r#type: TypePrimitive::Array {
                items: Box::new(convert_ir_type(ir, inner)),
            },
            meta: meta_copy,
        },
        Type::Enum { name, .. } => TypeOpenApi::Ref {
            r#ref: format!("#/components/schemas/{}", name),
            meta: meta_copy,
        },
        Type::Literal(literal, _) => TypeOpenApi::Inline {
            r#type: match literal {
                LiteralValue::String(_) => TypePrimitive::String,
                LiteralValue::Int(_) => TypePrimitive::Integer,
                LiteralValue::Bool(_) => TypePrimitive::Boolean,
            },
            meta: meta_copy,
        },
        Type::Arrow(_, _) => panic!("Arrow types are not supported in code generation"),
        Type::Union(inner, _) => match inner.view() {
            UnionTypeViewGeneric::Null => TypeOpenApi::Inline {
                r#type: TypePrimitive::Null,
                meta: meta_copy,
            },
            UnionTypeViewGeneric::Optional(inner) => convert_ir_type(ir, inner),
            UnionTypeViewGeneric::OneOf(inner) => TypeOpenApi::Union {
                one_of: inner.iter().map(|i| convert_ir_type(ir, i)).collect(),
                meta: meta_copy,
            },
            UnionTypeViewGeneric::OneOfOptional(inner) => {
                let one_of = inner.iter().map(|i| convert_ir_type(ir, i)).collect();
                TypeOpenApi::Union {
                    one_of,
                    meta: meta_copy,
                }
            }
        },
        Type::Map(_key_type, value_type, _) => TypeOpenApi::Inline {
            r#type: TypePrimitive::Object {
                properties: IndexMap::new(),
                required: IndexSet::new(),
                additional_properties: AdditionalProperties::Schema(Box::new(convert_ir_type(
                    ir, value_type,
                ))),
            },
            meta: meta_copy,
        },
        Type::Tuple(..) => panic!("Tuple types are not supported in code generation"),
        Type::RecursiveTypeAlias { .. } => TypeOpenApi::AnyValue {
            any_value: IndexMap::new(),
            meta: meta_copy,
        },
    };

    let checks = ty
        .meta()
        .constraints
        .iter()
        .filter_map(|Constraint { level, label, .. }| {
            if level == &ConstraintLevel::Check {
                label.clone()
            } else {
                None
            }
        })
        .collect::<Vec<String>>();
    if checks.is_empty() {
        base_rep
    } else {
        TypeOpenApi::Inline {
            r#type: TypePrimitive::Object {
                properties: [
                    ("value".to_string(), base_rep),
                    ("checks".to_string(), type_def_for_checks(checks)),
                ]
                .into_iter()
                .collect(),
                required: IndexSet::from_iter(vec!["value".to_string(), "checks".to_string()]),
                additional_properties: AdditionalProperties::Closed,
            },
            meta,
        }
    }
}

/// The type definition for a single "Checked_*" type. Note that we don't
/// produce a named type for each of these the way we do for SDK
/// codegeneration.
fn type_def_for_checks(checks: Vec<String>) -> TypeOpenApi {
    let mut properties: IndexMap<String, TypeOpenApi> = checks
        .iter()
        .map(|check_name| {
            (
                check_name.clone(),
                TypeOpenApi::Ref {
                    r#ref: format!("#/components/schemas/Check"),
                    meta: OpenApiMeta::default(),
                },
            )
        })
        .collect();
    properties.sort_keys();

    let mut required: IndexSet<String> = checks.into_iter().collect();
    required.sort();

    TypeOpenApi::Inline {
        r#type: TypePrimitive::Object {
            properties,
            required,
            additional_properties: AdditionalProperties::Closed,
        },
        meta: OpenApiMeta::default(),
    }
}

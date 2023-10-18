use crate::{context::Context, interner::StringId, DatamodelError};

use either::Either;
use enumflags2::bitflags;
use internal_baml_schema_ast::ast::{self, WithSpan};
use rustc_hash::FxHashMap as HashMap;
use std::{collections::BTreeMap, fmt};

pub(super) fn resolve_types(ctx: &mut Context<'_>) {
    for (top_id, top) in ctx.ast.iter_tops() {
        match (top_id, top) {
            (ast::TopId::Enum(_), ast::Top::Enum(enm)) => visit_enum(enm, ctx),
            (ast::TopId::Class(_), ast::Top::Class(model)) => visit_class(model, ctx),
            (ast::TopId::Function(_), ast::Top::Function(function)) => {
                visit_function(function, ctx)
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Types {
    // pub(super) composite_type_fields: BTreeMap<(ast::CompositeTypeId, ast::FieldId), CompositeTypeField>,
    scalar_fields: Vec<ScalarField>,
    pub(super) enum_attributes: HashMap<ast::EnumId, EnumAttributes>,
    // pub(super) model_attributes: HashMap<ast::ModelId, ModelAttributes>,
    /// Sorted array of scalar fields that have an `@default()` attribute with a function that is
    /// not part of the base Prisma ones. This is meant for later validation in the datamodel
    /// connector.
    pub(super) unknown_function_defaults: Vec<ScalarFieldId>,
}

impl Types {
    pub(super) fn push_scalar_field(&mut self, scalar_field: ScalarField) -> ScalarFieldId {
        let id = ScalarFieldId(self.scalar_fields.len() as u32);
        self.scalar_fields.push(scalar_field);
        id
    }
}

impl std::ops::Index<ScalarFieldId> for Types {
    type Output = ScalarField;

    fn index(&self, index: ScalarFieldId) -> &Self::Output {
        &self.scalar_fields[index.0 as usize]
    }
}

impl std::ops::IndexMut<ScalarFieldId> for Types {
    fn index_mut(&mut self, index: ScalarFieldId) -> &mut Self::Output {
        &mut self.scalar_fields[index.0 as usize]
    }
}

#[derive(Debug)]
enum FieldType {
    // Model(ast::ModelId),
    Scalar(ScalarFieldType),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnsupportedType {
    name: StringId,
}

impl UnsupportedType {
    pub(crate) fn new(name: StringId) -> Self {
        Self { name }
    }
}

/// The type of a scalar field, parsed and categorized.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScalarFieldType {
    /// An enum
    Enum(ast::EnumId),
    /// A Prisma scalar type
    BuiltInScalar(ScalarType),
    /// An `Unsupported("...")` type
    Unsupported(UnsupportedType),
}

impl ScalarFieldType {
    /// Try to interpret this field type as a known Prisma scalar type.
    pub fn as_builtin_scalar(self) -> Option<ScalarType> {
        match self {
            ScalarFieldType::BuiltInScalar(s) => Some(s),
            _ => None,
        }
    }

    /// Try to interpret this field type as an enum.
    pub fn as_enum(self) -> Option<ast::EnumId> {
        match self {
            ScalarFieldType::Enum(id) => Some(id),
            _ => None,
        }
    }

    /// Is the type of the field `Unsupported("...")`?
    pub fn is_unsupported(self) -> bool {
        matches!(self, Self::Unsupported(_))
    }

    /// True if the field's type is Json.
    pub fn is_json(self) -> bool {
        matches!(self, Self::BuiltInScalar(ScalarType::Json))
    }

    /// True if the field's type is String.
    pub fn is_string(self) -> bool {
        matches!(self, Self::BuiltInScalar(ScalarType::String))
    }

    /// True if the field's type is Bytes.
    pub fn is_bytes(self) -> bool {
        matches!(self, Self::BuiltInScalar(ScalarType::Bytes))
    }

    /// True if the field's type is Float.
    pub fn is_float(self) -> bool {
        matches!(self, Self::BuiltInScalar(ScalarType::Float))
    }

    /// True if the field's type is Int.
    pub fn is_int(self) -> bool {
        matches!(self, Self::BuiltInScalar(ScalarType::Int))
    }

    /// True if the field's type is BigInt.
    pub fn is_bigint(self) -> bool {
        matches!(self, Self::BuiltInScalar(ScalarType::BigInt))
    }
}

// #[derive(Debug, Clone)]
// pub(crate) struct DefaultAttribute {
//     pub(crate) mapped_name: Option<StringId>,
//     pub(crate) argument_idx: usize,
//     pub(crate) default_attribute: ast::AttributeId,
// }

#[derive(Debug)]
pub(crate) struct ScalarField {
    // pub(crate) model_id: ast::ModelId,
    // pub(crate) field_id: ast::FieldId,
    pub(crate) r#type: ScalarFieldType,
    pub(crate) is_ignored: bool,
    pub(crate) is_updated_at: bool,
    // pub(crate) default: Option<DefaultAttribute>,
    /// @map
    pub(crate) mapped_name: Option<StringId>,
    /// Native type name and arguments
    ///
    /// (attribute scope, native type name, arguments, span)
    ///
    /// For example: `@db.Text` would translate to ("db", "Text", &[], <the span>)
    pub(crate) native_type: Option<(StringId, StringId, Vec<String>, ast::Span)>,
}

#[derive(Debug, Default)]
pub(super) struct EnumAttributes {
    pub(super) mapped_name: Option<StringId>,
    /// @map on enum values.
    pub(super) mapped_values: HashMap<u32, StringId>,
    /// ```ignore
    /// @@schema("public")
    ///          ^^^^^^^^
    /// ```
    pub(crate) schema: Option<(StringId, ast::Span)>,
}

fn visit_enum<'db>(enm: &'db ast::Enum, ctx: &mut Context<'db>) {
    if enm.values.is_empty() {
        let msg = "An enum must have at least one value.";
        ctx.push_error(DatamodelError::new_validation_error(
            msg,
            enm.span().clone(),
        ))
    }
}

fn visit_class<'db>(class: &'db ast::Class, ctx: &mut Context<'db>) {
    if class.fields().is_empty() {
        let msg = "A class must have at least one field.";
        ctx.push_error(DatamodelError::new_validation_error(
            msg,
            class.span().clone(),
        ))
    }
}

fn visit_function<'db>(function: &'db ast::Function, ctx: &mut Context<'db>) {}

/// Either a structured, supported type, or an Err(unsupported) if the type name
/// does not match any we know of.
// fn field_type<'db>(field: &'db ast::Field, ctx: &mut Context<'db>) -> Result<FieldType, &'db str> {
//     let supported = match &field.field_type {
//         ast::FieldType::Supported(ident) => &ident.name,
//         ast::FieldType::Unsupported(name, _) => {
//             let unsupported = UnsupportedType::new(ctx.interner.intern(name));
//             return Ok(FieldType::Scalar(ScalarFieldType::Unsupported(unsupported)));
//         }
//     };
//     let supported_string_id = ctx.interner.intern(supported);

//     if let Some(tpe) = ScalarType::try_from_str(supported) {
//         return Ok(FieldType::Scalar(ScalarFieldType::BuiltInScalar(tpe)));
//     }

//     match ctx.names.tops.get(&supported_string_id).map(|id| (*id, &ctx.ast[*id])) {
//         Some((ast::TopId::Enum(enum_id), ast::Top::Enum(_))) => Ok(FieldType::Scalar(ScalarFieldType::Enum(enum_id))),
//         None => Err(supported),
//         _ => unreachable!(),
//     }
// }

/// Prisma's builtin scalar types.
#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
#[allow(missing_docs)]
pub enum ScalarType {
    Int,
    BigInt,
    Float,
    Boolean,
    String,
    Json,
    Bytes,
}

impl ScalarType {
    /// The string representation of the scalar type in the schema.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScalarType::Int => "Int",
            ScalarType::BigInt => "BigInt",
            ScalarType::Float => "Float",
            ScalarType::Boolean => "Boolean",
            ScalarType::String => "String",
            ScalarType::Json => "Json",
            ScalarType::Bytes => "Bytes",
        }
    }

    /// True if the type is bytes.
    pub fn is_bytes(&self) -> bool {
        matches!(self, ScalarType::Bytes)
    }

    pub(crate) fn try_from_str(s: &str) -> Option<ScalarType> {
        match s {
            "Int" => Some(ScalarType::Int),
            "BigInt" => Some(ScalarType::BigInt),
            "Float" => Some(ScalarType::Float),
            "Boolean" => Some(ScalarType::Boolean),
            "String" => Some(ScalarType::String),
            "Json" => Some(ScalarType::Json),
            "Bytes" => Some(ScalarType::Bytes),
            _ => None,
        }
    }
}

/// An opaque identifier for a model scalar field in a schema.
#[derive(Copy, Clone, PartialEq, Debug, Eq, Hash)]
pub struct ScalarFieldId(u32);

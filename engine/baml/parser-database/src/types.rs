use crate::{context::Context, DatamodelError};

use internal_baml_schema_ast::ast::{
    self, ClassId, EnumId, EnumValueId, FieldId, SerializerFieldId, TopId, VariantConfigId,
    VariantSerializerId, WithSpan,
};
use rustc_hash::FxHashMap as HashMap;

mod to_string_attributes;
mod types;

pub(crate) use to_string_attributes::{
    DynamicStringAttributes, StaticStringAttributes, ToStringAttributes,
};
pub(crate) use types::EnumAttributes;
pub use types::*;

pub(super) fn resolve_types(ctx: &mut Context<'_>) {
    for (top_id, top) in ctx.ast.iter_tops() {
        match (top_id, top) {
            (ast::TopId::Enum(_), ast::Top::Enum(enm)) => visit_enum(enm, ctx),
            (ast::TopId::Class(_), ast::Top::Class(model)) => visit_class(model, ctx),
            (ast::TopId::Function(_), ast::Top::Function(function)) => {
                visit_function(function, ctx)
            }
            (ast::TopId::Variant(_), ast::Top::Variant(variant)) => visit_variant(variant, ctx),
            (ast::TopId::Client(_), ast::Top::Client(client)) => {}
            (ast::TopId::Generator(_), ast::Top::Generator(generator)) => {}
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Types {
    pub(super) enum_attributes: HashMap<ast::EnumId, EnumAttributes>,
    pub(super) class_attributes: HashMap<ast::ClassId, ClassAttributes>,
    pub(super) variant_attributes: HashMap<ast::VariantConfigId, VariantAttributes>,
}

impl Types {
    pub(super) fn refine_class_field(
        &self,
        (class_id, field_id): (ClassId, FieldId),
    ) -> either::Either<StaticFieldId, DynamicFieldId> {
        match self.class_attributes.get(&class_id) {
            Some(attrs) => match attrs.field_serilizers.get(&field_id) {
                Some(ToStringAttributes::Dynamic(attrs)) => either::Either::Right(field_id.into()),
                _ => either::Either::Left(field_id.into()),
            },
            None => either::Either::Left(field_id.into()),
        }
    }

    pub(super) fn refine_enum_value(
        &self,
        (enum_id, value_id): (EnumId, EnumValueId),
    ) -> either::Either<StaticFieldId, DynamicFieldId> {
        match self.enum_attributes.get(&enum_id) {
            Some(attrs) => match attrs.value_serilizers.get(&value_id) {
                Some(ToStringAttributes::Dynamic(attrs)) => either::Either::Right(value_id.into()),
                _ => either::Either::Left(value_id.into()),
            },
            None => either::Either::Left(value_id.into()),
        }
    }

    pub(super) fn refine_serializer_field(
        &self,
        (variant_id, serializer_id, value_id): (
            VariantConfigId,
            VariantSerializerId,
            SerializerFieldId,
        ),
    ) -> either::Either<StaticFieldId, DynamicFieldId> {
        match self
            .variant_attributes
            .get(&variant_id)
            .and_then(|r| r.serializers.get(&serializer_id))
        {
            Some(attrs) => match attrs.field_serilizers.get(&value_id) {
                Some(ToStringAttributes::Dynamic(_)) => either::Either::Right(value_id.into()),
                _ => either::Either::Left(value_id.into()),
            },
            None => either::Either::Left(value_id.into()),
        }
    }
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

fn visit_variant<'db>(class: &'db ast::Variant, ctx: &mut Context<'db>) {}

/// Prisma's builtin scalar types.
#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
#[allow(missing_docs)]
pub enum StaticType {
    Int,
    BigInt,
    Float,
    Boolean,
    String,
    Json,
    Bytes,
}

impl StaticType {
    /// The string representation of the scalar type in the schema.
    pub fn as_str(&self) -> &'static str {
        match self {
            StaticType::Int => "Int",
            StaticType::BigInt => "BigInt",
            StaticType::Float => "Float",
            StaticType::Boolean => "Boolean",
            StaticType::String => "String",
            StaticType::Json => "Json",
            StaticType::Bytes => "Bytes",
        }
    }

    /// True if the type is bytes.
    pub fn is_bytes(&self) -> bool {
        matches!(self, StaticType::Bytes)
    }

    pub(crate) fn try_from_str(s: &str) -> Option<StaticType> {
        match s {
            "Int" => Some(StaticType::Int),
            "BigInt" => Some(StaticType::BigInt),
            "Float" => Some(StaticType::Float),
            "Boolean" => Some(StaticType::Boolean),
            "String" => Some(StaticType::String),
            "Json" => Some(StaticType::Json),
            "Bytes" => Some(StaticType::Bytes),
            _ => None,
        }
    }
}

/// An opaque identifier for a class field in a schema that is dynamic.
#[derive(Copy, Clone, PartialEq, Debug, Hash, Eq, PartialOrd, Ord)]
pub struct DynamicFieldId(u32);

impl From<SerializerFieldId> for DynamicFieldId {
    fn from(id: SerializerFieldId) -> Self {
        DynamicFieldId(id.0 as u32)
    }
}

impl From<FieldId> for DynamicFieldId {
    fn from(id: FieldId) -> Self {
        DynamicFieldId(id.0 as u32)
    }
}

impl From<EnumValueId> for DynamicFieldId {
    fn from(id: EnumValueId) -> Self {
        DynamicFieldId(id.0 as u32)
    }
}

/// An opaque identifier for a class field.
#[derive(Copy, Clone, PartialEq, Debug, Eq, Hash)]
pub struct StaticFieldId(u32);

impl From<SerializerFieldId> for StaticFieldId {
    fn from(id: SerializerFieldId) -> Self {
        StaticFieldId(id.0 as u32)
    }
}

impl From<FieldId> for StaticFieldId {
    fn from(id: FieldId) -> Self {
        StaticFieldId(id.0 as u32)
    }
}

impl From<EnumValueId> for StaticFieldId {
    fn from(id: EnumValueId) -> Self {
        StaticFieldId(id.0 as u32)
    }
}

use std::collections::HashMap;
use std::hash::Hash;

use crate::coerce;
use crate::{context::Context, DatamodelError};

use internal_baml_diagnostics::Span;
use internal_baml_prompt_parser::ast::{PrinterBlock, Variable};
use internal_baml_schema_ast::ast::{
    self, ClassId, ClientId, EnumId, EnumValueId, Expression, FieldId, SerializerFieldId, TopId,
    VariantConfigId, VariantSerializerId, WithIdentifier, WithName, WithSpan,
};

pub(crate) mod post_prompt;
mod prompt;
mod to_string_attributes;
mod types;

use prompt::validate_prompt;

pub(crate) use to_string_attributes::{
    DynamicStringAttributes, StaticStringAttributes, ToStringAttributes,
};
pub(crate) use types::EnumAttributes;
pub(crate) use types::*;
pub(super) fn resolve_types(ctx: &mut Context<'_>) {
    for (top_id, top) in ctx.ast.iter_tops() {
        match (top_id, top) {
            (ast::TopId::Enum(_), ast::Top::Enum(enm)) => visit_enum(enm, ctx),
            (ast::TopId::Class(_), ast::Top::Class(model)) => visit_class(model, ctx),
            (ast::TopId::Function(_), ast::Top::Function(function)) => {
                visit_function(function, ctx)
            }
            (ast::TopId::Variant(idx), ast::Top::Variant(variant)) => {
                visit_variant(idx, variant, ctx)
            }
            (ast::TopId::Client(idx), ast::Top::Client(client)) => {
                visit_client(idx, client, ctx);
            }
            (ast::TopId::Generator(_), ast::Top::Generator(_generator)) => {}
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// Variables used inside of raw strings.
pub enum PromptVariable {
    /// Input variable.
    Input(Variable),
    /// Output variable.
    Enum(PrinterBlock),
    /// Output variable.
    Type(PrinterBlock),
}

impl Hash for PromptVariable {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            PromptVariable::Input(var) => {
                "input".hash(state);
                var.text.hash(state);
            }
            PromptVariable::Enum(blk) => {
                "enum".hash(state);
                blk.printer.0.hash(state);
                blk.target.text.hash(state);
            }
            PromptVariable::Type(blk) => {
                "type".hash(state);
                blk.printer.0.hash(state);
                blk.target.text.hash(state);
            }
        }
    }
}

impl<'a> PromptVariable {
    /// Unique Key
    pub fn key(&self) -> String {
        match self {
            PromptVariable::Input(var) => var.key(),
            PromptVariable::Enum(blk) => blk.key(),
            PromptVariable::Type(blk) => blk.key(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StringValue {
    pub value: String,
    pub span: Span,
    pub key_span: Span,
}

#[derive(Debug, Clone)]
pub struct VariantProperties {
    pub client: StringValue,
    pub prompt: StringValue,
    pub prompt_replacements: Vec<PromptVariable>,
    pub replacers: (HashMap<Variable, String>, HashMap<PrinterBlock, String>),
}

#[derive(Debug, Clone)]
pub struct ClientProperties {
    pub provider: String,
    pub options: HashMap<String, Expression>,
}

#[derive(Debug, Default)]
pub(super) struct Types {
    pub(super) enum_attributes: HashMap<ast::EnumId, EnumAttributes>,
    pub(super) class_attributes: HashMap<ast::ClassId, ClassAttributes>,
    pub(super) variant_attributes: HashMap<ast::VariantConfigId, VariantAttributes>,
    pub(super) variant_properties: HashMap<ast::VariantConfigId, VariantProperties>,
    pub(super) client_properties: HashMap<ast::ClientId, ClientProperties>,
}

impl Types {
    pub(super) fn refine_class_field(
        &self,
        (class_id, field_id): (ClassId, FieldId),
    ) -> either::Either<StaticFieldId, DynamicFieldId> {
        match self.class_attributes.get(&class_id) {
            Some(attrs) => match attrs.field_serilizers.get(&field_id) {
                Some(ToStringAttributes::Dynamic(_attrs)) => either::Either::Right(field_id.into()),
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
                Some(ToStringAttributes::Dynamic(_attrs)) => either::Either::Right(value_id.into()),
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

fn visit_function<'db>(_function: &'db ast::Function, _ctx: &mut Context<'db>) {}

fn visit_client<'db>(idx: ClientId, client: &'db ast::Client, ctx: &mut Context<'db>) {
    //
    let mut provider = None;
    let mut options: HashMap<String, Expression> = HashMap::new();
    client
        .iter_fields()
        .for_each(|(_idx, field)| match field.name() {
            "provider" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `provider` instead of `provider<...>`?",
                        field.span().clone(),
                    ));
                }
                provider = field.value.as_ref()
            }
            "retry" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `retry` instead of `retry<...>`?",
                        field.span().clone(),
                    ));
                }
            }
            "options" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `options` instead of `options<...>`?",
                        field.span().clone(),
                    ));
                }

                match field.value.as_ref() {
                    Some(ast::Expression::Map(map, span)) => {
                        map.iter().for_each(|(key, value)| {
                            if let Some(key) = coerce::string(key, &mut ctx.diagnostics) {
                                options.insert(key.to_string(), value.clone());
                            } else {
                                ctx.push_error(DatamodelError::new_validation_error(
                                    "Expected a string key.",
                                    span.clone(),
                                ));
                            }
                        });
                    }
                    Some(_) => {
                        ctx.push_error(DatamodelError::new_validation_error(
                            "Expected a map.",
                            field.span().clone(),
                        ));
                    }
                    _ => {}
                };
            }
            config => ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in client", config),
                field.span().clone(),
            )),
        });

    match (provider, options) {
        (Some(provider), options) => {
            match (coerce::string(provider, &mut ctx.diagnostics), options) {
                (Some(provider), options) => {
                    ctx.types.client_properties.insert(
                        idx,
                        ClientProperties {
                            provider: provider.to_string(),
                            options,
                        },
                    );
                }
                _ => {
                    // Errors are handled by coerce.
                }
            }
        }
        (None, _) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `provider` field in client. e.g. `provider openai`",
            client.span().clone(),
        )),
    }
}

fn visit_variant<'db>(idx: VariantConfigId, variant: &'db ast::Variant, ctx: &mut Context<'db>) {
    if !variant.is_llm() {
        ctx.push_error(DatamodelError::new_validation_error(
            "Only LLM variants are supported. Use: variant<llm>",
            variant.span().clone(),
        ));
        return;
    }

    let mut client = None;
    let mut prompt = None;

    variant
        .iter_fields()
        .for_each(|(_idx, field)| match field.name() {
            "client" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `client` instead of `client<...>`?",
                        field.span().clone(),
                    ));
                }
                match field.value.as_ref() {
                    Some(item) => client = Some((item, field.identifier().span().clone())),
                    _ => {}
                }
            }
            "prompt" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `prompt` instead of `prompt<...>`?",
                        field.span().clone(),
                    ));
                }
                match field.value.as_ref() {
                    Some(item) => prompt = Some((item, field.identifier().span().clone())),
                    _ => {}
                }
            }
            config => ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in impl<llm>", config),
                field.span().clone(),
            )),
        });

    match (client, prompt) {
        (Some((client, client_key_span)), Some((prompt, prompt_key_span))) => match (
            coerce::string_with_span(client, &mut ctx.diagnostics),
            coerce::string_with_span(prompt, &mut ctx.diagnostics),
        ) {
            (Some((client, client_span)), Some((prompt_string, prompt_span))) => {
                match validate_prompt(ctx, (prompt_string, prompt_span.clone()), &prompt_span) {
                    Some((prompt, replacer)) => {
                        ctx.types.variant_properties.insert(
                            idx,
                            VariantProperties {
                                client: StringValue {
                                    value: client.to_string(),
                                    span: client_span.clone(),
                                    key_span: client_key_span,
                                },
                                prompt: StringValue {
                                    value: prompt.to_string(),
                                    span: prompt_span.clone(),
                                    key_span: prompt_key_span,
                                },
                                prompt_replacements: replacer,
                                replacers: Default::default(),
                            },
                        );
                    }
                    None => {}
                }
            }
            _ => {
                // Errors are handled by coerce.
            }
        },
        (None, Some(_)) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `client` field in variant<llm>",
            variant.span().clone(),
        )),
        (Some(_), None) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `prompt` field in variant<llm>",
            variant.span().clone(),
        )),
        (None, None) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `client` and `prompt` fields in variant<llm>",
            variant.span().clone(),
        )),
    }
}

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

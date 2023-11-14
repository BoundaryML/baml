use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::coerce;
use crate::{context::Context, DatamodelError};

use internal_baml_diagnostics::{DatamodelWarning, Span};
use internal_baml_prompt_parser::ast::{PrinterBlock, Variable};
use internal_baml_schema_ast::ast::{
    self, ClassId, ClientId, ConfigurationId, EnumId, EnumValueId, Expression, FieldId, FunctionId,
    RawString, SerializerFieldId, VariantConfigId, VariantSerializerId, WithIdentifier, WithName,
    WithSpan,
};

mod configurations;
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
            (_, ast::Top::Enum(enm)) => visit_enum(enm, ctx),
            (ast::TopId::Class(idx), ast::Top::Class(model)) => visit_class(idx, model, ctx),
            (_, ast::Top::Class(_)) => unreachable!("Class misconfigured"),
            (ast::TopId::Function(idx), ast::Top::Function(function)) => {
                visit_function(idx, function, ctx)
            }
            (_, ast::Top::Function(_)) => unreachable!("Function misconfigured"),
            (ast::TopId::Variant(idx), ast::Top::Variant(variant)) => {
                visit_variant(idx, variant, ctx)
            }
            (_, ast::Top::Variant(_)) => unreachable!("Variant misconfigured"),
            (ast::TopId::Client(idx), ast::Top::Client(client)) => {
                visit_client(idx, client, ctx);
            }
            (_, ast::Top::Client(_)) => unreachable!("Client misconfigured"),
            (_, ast::Top::Generator(_generator)) => {}
            (ast::TopId::Config((idx, _)), ast::Top::Config(cfg)) => {
                visit_config(idx, cfg, ctx);
            }
            (_, ast::Top::Config(_)) => unreachable!("Config misconfigured"),
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
                blk.hash(state);
            }
            PromptVariable::Type(blk) => {
                "type".hash(state);
                blk.hash(state);
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

#[derive(Debug)]
pub struct VariantProperties {
    pub client: StringValue,
    pub prompt: StringValue,
    pub prompt_replacements: Vec<PromptVariable>,
    pub replacers: (HashMap<Variable, String>, HashMap<PrinterBlock, String>),
    pub pre_deserializer: Option<(Vec<RawString>, Span)>,
}

impl VariantProperties {
    pub fn pre_deserializer_for_lang<'db>(&'db self, language: &str) -> Option<&'db str> {
        match self.pre_deserializer.as_ref() {
            Some((pre_deserializer, _)) => pre_deserializer
                .iter()
                .find(|f| f.language.as_ref().unwrap().0 == language)
                .map(|f| f.value()),
            None => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientProperties {
    pub provider: (String, Span),
    pub retry_policy: Option<(String, Span)>,
    pub options: Vec<(String, Expression)>,
}

#[derive(Debug, Clone)]
pub struct Printer {
    pub template: (String, Span),
}

#[derive(Debug, Clone)]
/// The type of printer.
pub enum PrinterType {
    /// For types
    Type(Printer),
    /// For enums
    Enum(Printer),
}

impl PrinterType {
    /// The code template.
    pub fn template(&self) -> &str {
        match self {
            PrinterType::Type(printer) => &printer.template.0,
            PrinterType::Enum(printer) => &printer.template.0,
        }
    }
}

/// How to retry a request.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// The maximum number of retries.
    pub max_retries: u32,
    /// The strategy to use.
    pub strategy: RetryPolicyStrategy,
    /// Any additional options.
    pub options: Option<Vec<((String, Span), Expression)>>,
}

#[derive(Debug, Clone)]
/// The strategy to use for retrying a request.
pub enum RetryPolicyStrategy {
    /// Constant delay.
    ConstantDelay(ContantDelayStrategy),
    /// Exponential backoff.
    ExponentialBackoff(ExponentialBackoffStrategy),
}

#[derive(Debug, Clone)]
/// The strategy to use for retrying a request.
pub struct ContantDelayStrategy {
    /// The delay in milliseconds.
    pub delay_ms: u32,
}

#[derive(Debug, Clone)]
/// The strategy to use for retrying a request.
pub struct ExponentialBackoffStrategy {
    /// The delay in milliseconds.
    pub delay_ms: u32,
    /// The multiplier.
    pub multiplier: f32,
    /// The maximum delay in milliseconds.
    pub max_delay_ms: u32,
}

#[derive(Debug, Default)]
pub(super) struct Types {
    pub(super) enum_attributes: HashMap<ast::EnumId, EnumAttributes>,
    pub(super) class_attributes: HashMap<ast::ClassId, ClassAttributes>,
    pub(super) class_dependencies: HashMap<ast::ClassId, HashSet<String>>,
    pub(super) function_dependencies: HashMap<ast::FunctionId, (HashSet<String>, HashSet<String>)>,
    pub(super) variant_attributes: HashMap<ast::VariantConfigId, VariantAttributes>,
    pub(super) variant_properties: HashMap<ast::VariantConfigId, VariantProperties>,
    pub(super) client_properties: HashMap<ast::ClientId, ClientProperties>,
    pub(super) retry_policies: HashMap<ast::ConfigurationId, RetryPolicy>,
    pub(super) printers: HashMap<ast::ConfigurationId, PrinterType>,
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

    #[allow(dead_code)]
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

fn visit_enum<'db>(_enm: &'db ast::Enum, _ctx: &mut Context<'db>) {}

fn visit_class<'db>(class_id: ast::ClassId, class: &'db ast::Class, ctx: &mut Context<'db>) {
    let used_types = class
        .iter_fields()
        .flat_map(|(_, f)| f.field_type.flat_idns())
        .filter(|id| {
            id.is_valid_type()
                && match id {
                    ast::Identifier::Primitive(..) => false,
                    _ => true,
                }
        })
        .map(|f| f.name().to_string())
        .collect::<HashSet<_>>();
    ctx.types.class_dependencies.insert(class_id, used_types);
}

fn visit_function<'db>(idx: FunctionId, function: &'db ast::Function, ctx: &mut Context<'db>) {
    let input_deps = function
        .input()
        .flat_idns()
        .iter()
        .map(|f| f.name().to_string())
        .collect::<HashSet<_>>();

    let output_deps = function
        .output()
        .flat_idns()
        .iter()
        .map(|f| f.name().to_string())
        .collect::<HashSet<_>>();

    ctx.types
        .function_dependencies
        .insert(idx, (input_deps, output_deps));
}

fn visit_client<'db>(idx: ClientId, client: &'db ast::Client, ctx: &mut Context<'db>) {
    if !client.is_llm() {
        ctx.push_error(DatamodelError::new_validation_error(
            "Only LLM clients are supported. Use: client<llm>",
            client.span().clone(),
        ));
        return;
    }

    let mut provider = None;
    let mut retry_policy = None;
    let mut options: Vec<(String, Expression)> = Vec::new();
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
            "retry_policy" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `retry_policy` instead of `retry_policy<...>`?",
                        field.span().clone(),
                    ));
                }
                retry_policy = field.value.as_ref()
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
                                options.push((key.to_string(), value.clone()));
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

    let retry_policy = match retry_policy {
        Some(retry_policy) => match coerce::string_with_span(retry_policy, &mut ctx.diagnostics) {
            Some((retry_policy, span)) => Some((retry_policy.to_string(), span.clone())),
            _ => {
                // Errors are handled by coerce.
                None
            }
        },
        None => None,
    };

    match (provider, options) {
        (Some(provider), options) => {
            match (
                coerce::string_with_span(provider, &mut ctx.diagnostics),
                options,
            ) {
                (Some(provider), options) => {
                    ctx.types.client_properties.insert(
                        idx,
                        ClientProperties {
                            provider: (provider.0.to_string(), provider.1.clone()),
                            retry_policy,
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
            "Missing `provider` field in client. e.g. `provider baml-openai-chat`",
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
    let mut pre_deserializer = None;

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
            "pre_deserializer" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `prompt` instead of `prompt<...>`?",
                        field.span().clone(),
                    ));
                }
                match field.value.as_ref() {
                    Some(item) => {
                        pre_deserializer = Some((item, field.identifier().span().clone()))
                    }
                    _ => {}
                }
            }
            config => ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in impl<llm>", config),
                field.span().clone(),
            )),
        });

    let client = if let Some((client, client_key_span)) = client {
        if let Some(client) = coerce::string_with_span(client, &mut ctx.diagnostics) {
            Some((client, client_key_span))
        } else {
            // Errors are handled by coerce.
            None
        }
    } else {
        ctx.push_error(DatamodelError::new_validation_error(
            "Missing `client` field in variant<llm>",
            variant.identifier().span().clone(),
        ));
        None
    };

    let prompt = if let Some((prompt, prompt_key_span)) = prompt {
        if let Some(prompt) = prompt.as_raw_string_value() {
            match validate_prompt(ctx, prompt) {
                Some((cleaned_prompt, replacer)) => {
                    Some(((cleaned_prompt, prompt.span(), replacer), prompt_key_span))
                }
                None => None,
            }
        } else if let Some((prompt, span)) = coerce::string_with_span(prompt, &mut ctx.diagnostics)
        {
            // warn the user that we are using this without validation.
            ctx.push_warning(DatamodelWarning::new(
                "To use comments and {#vars} use a block string. #\"...\"# instead.".into(),
                span.clone(),
            ));
            Some((
                (prompt.to_string(), span, Default::default()),
                prompt_key_span,
            ))
        } else {
            // Errors are handled by coerce.
            None
        }
    } else {
        ctx.push_error(DatamodelError::new_validation_error(
            "Missing `prompt` field in variant<llm>",
            variant.identifier().span().clone(),
        ));
        None
    };

    let pre_deserializer =
        if let Some((pre_deserializer, pre_deserializer_key_span)) = pre_deserializer {
            if let Some((pre_deserializer, _)) = pre_deserializer.as_array() {
                let pre_deserializer = pre_deserializer
                    .iter()
                    .filter_map(|f| coerce::raw_string(f, &mut ctx.diagnostics))
                    .map(|f| f.clone())
                    .collect::<Vec<_>>();
                Some((pre_deserializer, pre_deserializer_key_span))
            } else if let Some(pre_deserializer) =
                coerce::raw_string(pre_deserializer, &mut ctx.diagnostics)
            {
                Some((vec![pre_deserializer.clone()], pre_deserializer_key_span))
            } else {
                // Errors are handled by coerce.
                None
            }
        } else {
            None
        };

    match (client, prompt) {
        (
            Some(((client, client_span), client_key_span)),
            Some(((prompt, prompt_span, replacers), prompt_key_span)),
        ) => {
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
                    prompt_replacements: replacers,
                    replacers: Default::default(),
                    pre_deserializer,
                },
            );
        }
        _ => {}
    }
}

fn visit_config<'db>(
    idx: ConfigurationId,
    config: &'db ast::Configuration,
    ctx: &mut Context<'db>,
) {
    match config {
        ast::Configuration::RetryPolicy(retry) => {
            configurations::visit_retry_policy(idx, retry, ctx);
        }
        ast::Configuration::Printer(printer) => {
            configurations::visit_printer(idx, printer, ctx);
        }
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

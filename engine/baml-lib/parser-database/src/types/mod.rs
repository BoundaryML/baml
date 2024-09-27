use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::coerce;
use crate::types::configurations::visit_test_case;
use crate::{context::Context, DatamodelError};

use indexmap::IndexMap;
use internal_baml_diagnostics::Span;
use internal_baml_prompt_parser::ast::{ChatBlock, PrinterBlock, Variable};
use internal_baml_schema_ast::ast::{
    self, Expression, FieldId, RawString, TypeExpId, ValExpId, WithIdentifier, WithName, WithSpan,
};

mod configurations;
mod prompt;
mod to_string_attributes;
mod types;

pub use to_string_attributes::{StaticStringAttributes, ToStringAttributes};
pub(crate) use types::EnumAttributes;
pub(crate) use types::*;

use self::configurations::visit_retry_policy;

pub(super) fn resolve_types(ctx: &mut Context<'_>) {
    for (top_id, top) in ctx.ast.iter_tops() {
        match (top_id, top) {
            (ast::TopId::Enum(idx), ast::Top::Enum(model)) => visit_enum(idx, model, ctx),
            (_, ast::Top::Enum(_)) => unreachable!("Enum misconfigured"),

            (ast::TopId::Class(idx), ast::Top::Class(model)) => {
                visit_class(idx, model, ctx);
            }
            (_, ast::Top::Class(_)) => unreachable!("Class misconfigured"),
            (ast::TopId::TemplateString(idx), ast::Top::TemplateString(template_string)) => {
                visit_template_string(idx, template_string, ctx)
            }
            (_, ast::Top::TemplateString(_)) => unreachable!("TemplateString misconfigured"),

            (ast::TopId::Function(idx), ast::Top::Function(function)) => {
                visit_function(idx, function, ctx)
            }
            (_, ast::Top::Function(_)) => unreachable!("Function misconfigured"),
            (ast::TopId::Client(idx), ast::Top::Client(client)) => {
                visit_client(idx, client, ctx);
            }

            (_, ast::Top::Client(_)) => unreachable!("Client misconfigured"),
            (ast::TopId::RetryPolicy(idx), ast::Top::RetryPolicy(config)) => {
                visit_retry_policy(idx, config, ctx);
            }
            (_, ast::Top::RetryPolicy(_)) => unreachable!("RetryPolicy misconfigured"),
            (ast::TopId::TestCase(idx), ast::Top::TestCase(config)) => {
                visit_test_case(idx, config, ctx);
            }
            (_, ast::Top::TestCase(_)) => unreachable!("TestCase misconfigured"),

            _ => {}
        }
    }
}
#[derive(Debug, Clone)]
/// Variables used inside of raw strings.
pub enum PromptVariable {
    /// Input variable.
    Input(Variable),
    /// Output variable.
    Enum(PrinterBlock),
    /// Output variable.
    Type(PrinterBlock),
    /// Chat
    Chat(ChatBlock),
}

impl Hash for PromptVariable {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            PromptVariable::Chat(blk) => {
                "chat".hash(state);
                blk.role.0.hash(state);
            }
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
            PromptVariable::Chat(blk) => blk.key(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StringValue {
    pub value: String,
    pub span: Span,
    pub key_span: Span,
}

/// The representation of a prompt.
pub enum PromptAst<'a> {
    /// For single string prompts
    /// Prompt + Any used input replacers (key, val)
    String(String, Vec<(String, String)>),

    /// For prompts with multiple parts
    /// ChatBlock + Prompt + Any used input replacers (key, val)
    Chat(Vec<(Option<&'a ChatBlock>, String)>, Vec<(String, String)>),
}

#[derive(Debug, Clone)]
pub struct ClientProperties {
    pub provider: (String, Span),
    pub retry_policy: Option<(String, Span)>,
    pub options: Vec<(String, Expression)>,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub functions: Vec<(String, Span)>,
    // The span is the span of the argument (the expression has its own span)
    pub args: IndexMap<String, (Span, Expression)>,
    pub args_field_span: Span,
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

#[derive(Debug, Clone, Copy, serde::Serialize)]
/// The strategy to use for retrying a request.
pub enum RetryPolicyStrategy {
    /// Constant delay.
    ConstantDelay(ContantDelayStrategy),
    /// Exponential backoff.
    ExponentialBackoff(ExponentialBackoffStrategy),
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
/// The strategy to use for retrying a request.
pub struct ContantDelayStrategy {
    /// The delay in milliseconds.
    pub delay_ms: u32,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
/// The strategy to use for retrying a request.
pub struct ExponentialBackoffStrategy {
    /// The delay in milliseconds.
    pub delay_ms: u32,
    /// The multiplier.
    pub multiplier: f32,
    /// The maximum delay in milliseconds.
    pub max_delay_ms: u32,
}

#[derive(Debug, Clone)]
pub struct FunctionType {
    pub dependencies: (HashSet<String>, HashSet<String>),
    pub prompt: Option<RawString>,
    pub client: Option<(String, Span)>,
}

#[derive(Debug, Clone)]
pub struct TemplateStringProperties {
    // Not all template strings have names (e.g. function prompt)
    pub name: Option<String>,
    pub type_dependencies: HashSet<String>,
    /// This is dedented and trimmed.
    pub template: String,
}

#[derive(Debug, Default)]
pub(super) struct Types {
    pub(super) enum_attributes: HashMap<ast::TypeExpId, EnumAttributes>,
    pub(super) class_attributes: HashMap<ast::TypeExpId, ClassAttributes>,
    pub(super) class_dependencies: HashMap<ast::TypeExpId, HashSet<String>>,
    pub(super) enum_dependencies: HashMap<ast::TypeExpId, HashSet<String>>,

    pub(super) function: HashMap<ast::ValExpId, FunctionType>,

    pub(super) client_properties: HashMap<ast::ValExpId, ClientProperties>,
    pub(super) retry_policies: HashMap<ast::ValExpId, RetryPolicy>,
    pub(super) test_cases: HashMap<ast::ValExpId, TestCase>,
    pub(super) template_strings:
        HashMap<either::Either<ast::TemplateStringId, ast::ValExpId>, TemplateStringProperties>,
}

fn visit_template_string<'db>(
    idx: ast::TemplateStringId,
    template_string: &'db ast::TemplateString,
    ctx: &mut Context<'db>,
) {
    ctx.types.template_strings.insert(
        either::Left(idx),
        TemplateStringProperties {
            name: Some(template_string.name().to_string()),
            type_dependencies: template_string
                .input()
                .map(|f| f.flat_idns())
                .unwrap_or_default()
                .iter()
                .map(|f| f.name().to_string())
                .collect::<HashSet<_>>(),
            template: template_string
                .value()
                .as_raw_string_value()
                .map(|v| v.value().to_string())
                .unwrap(),
        },
    );
}

fn visit_enum<'db>(
    enm_id: ast::TypeExpId,
    enm: &'db ast::TypeExpressionBlock,
    ctx: &mut Context<'db>,
) {
    // Ensure that every value in the enum does not have an expression.
    enm.fields
        .iter()
        .filter_map(|field| {
            if field.expr.is_some() {
                Some((field.span(), field.name()))
            } else {
                None
            }
        })
        .for_each(|(span, field)| {
            ctx.push_error(DatamodelError::new_validation_error(
                format!("Unexpected type specified for value `{}`", field).as_str(),
                span.clone(),
            ));
        });

    let input_deps = enm.input().map(|f| f.flat_idns()).unwrap_or_default();
    ctx.types.enum_dependencies.insert(
        enm_id,
        input_deps.iter().map(|id| id.name().to_string()).collect(),
    );
}

fn visit_class<'db>(
    class_id: ast::TypeExpId,
    class: &'db ast::TypeExpressionBlock,
    ctx: &mut Context<'db>,
) {
    // Ensure that every value in the class is actually a name: type.
    class
        .fields
        .iter()
        .filter_map(|field| {
            if field.expr.is_none() {
                Some((field.span(), field.name()))
            } else {
                None
            }
        })
        .for_each(|(span, field)| {
            ctx.push_error(DatamodelError::new_validation_error(
                format!("No type specified for field `{}`", field).as_str(),
                span.clone(),
            ));
        });

    let mut used_types = class
        .iter_fields()
        .flat_map(|(_, f)| f.expr.iter().flat_map(|e| e.flat_idns()))
        .map(|id| id.name().to_string())
        .collect::<HashSet<_>>();
    let input_deps = class.input().map(|f| f.flat_idns()).unwrap_or_default();

    ctx.types.class_dependencies.insert(class_id, {
        used_types.extend(input_deps.iter().map(|id| id.name().to_string()));
        used_types
    });
}

fn visit_function<'db>(idx: ValExpId, function: &'db ast::ValueExprBlock, ctx: &mut Context<'db>) {
    let input_deps = function
        .input()
        .map(|input| input.flat_idns())
        .unwrap_or_else(Vec::new)
        .iter()
        .map(|f| f.name().to_string())
        .collect::<HashSet<_>>();
    let output_deps = function
        .output()
        .map(|output| output.field_type.flat_idns())
        .unwrap_or_else(Vec::new)
        .iter()
        .map(|f| f.name().to_string())
        .collect::<HashSet<_>>();

    let mut prompt = None;
    let mut client = None;
    function
        .iter_fields()
        .for_each(|(_idx, field)| match field.name() {
            "prompt" => {
                prompt = match &field.expr {
                    Some(val) => coerce::template_string(val, ctx.diagnostics),
                    None => None,
                }
            }
            "client" => {
                client = match &field.expr {
                    Some(val) => coerce::string_with_span(val, ctx.diagnostics)
                        .map(|(v, span)| (v.to_string(), span.clone())),
                    None => None,
                }
            }
            config => ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in function", config),
                field.span().clone(),
            )),
        });

    match (prompt, client) {
        (Some(prompt), Some(client)) => {
            ctx.types.function.insert(
                idx,
                FunctionType {
                    dependencies: (input_deps.clone(), output_deps),
                    prompt: Some(prompt.clone()),
                    client: Some(client),
                },
            );

            ctx.types.template_strings.insert(
                either::Right(idx),
                TemplateStringProperties {
                    name: None,
                    type_dependencies: input_deps,
                    template: prompt.value().to_string(),
                },
            );
        }
        (Some(_), None) => {
            ctx.push_error(DatamodelError::new_validation_error(
                "Missing `client` field in function. Add to the block:\n```\nclient GPT4\n```",
                function.identifier().span().clone(),
            ));
        }
        (None, Some(_)) => {
            ctx.push_error(DatamodelError::new_validation_error(
                "Missing `prompt` field in function. Add to the block:\n```\nprompt #\"...\"#\n```",
                function.identifier().span().clone(),
            ));
        }
        (None, None) => {
            ctx.push_error(DatamodelError::new_validation_error(
                "Missing `prompt` and `client` fields in function. Add to the block:\n```\nclient GPT4\nprompt #\"...\"#\n```",
                function.identifier().span().clone(),
            ));
        }
    }
}

fn visit_client<'db>(idx: ValExpId, client: &'db ast::ValueExprBlock, ctx: &mut Context<'db>) {
    let mut provider = None;
    let mut retry_policy = None;
    let mut options: Vec<(String, Expression)> = Vec::new();
    client
        .iter_fields()
        .for_each(|(_idx, field)| match field.name() {
            "provider" => provider = field.expr.as_ref(),
            "retry_policy" => retry_policy = field.expr.as_ref(),
            "options" => {
                match field.expr.as_ref() {
                    Some(ast::Expression::Map(map, span)) => {
                        map.iter().for_each(|(key, value)| {
                            if let Some(key) = coerce::string(key, ctx.diagnostics) {
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
        Some(retry_policy) => match coerce::string_with_span(retry_policy, ctx.diagnostics) {
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
            match (coerce::string_with_span(provider, ctx.diagnostics), options) {
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
            "Missing `provider` field in client. e.g. `provider openai`",
            client.span().clone(),
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

/// An opaque identifier for a class field.
#[derive(Copy, Clone, PartialEq, Debug, Eq, Hash)]
pub struct StaticFieldId(u32);

impl From<FieldId> for StaticFieldId {
    fn from(id: FieldId) -> Self {
        StaticFieldId(id.0)
    }
}

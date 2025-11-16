use baml_types::{BamlMedia, BamlValue, EvaluationContext};
use colored::*;
mod chat_message_part;

mod output_format;
#[cfg(test)]
mod test_enum_comparison;
#[cfg(test)]
mod test_enum_template;
#[cfg(test)]
mod test_media;
use indexmap::IndexMap;
use internal_baml_core::ir::{jinja_helpers::get_env, repr::IntermediateRepr};
pub use output_format::types;
mod baml_value_to_jinja_value;

use std::collections::HashMap;

use minijinja::{self, context, value::Kwargs, ErrorKind};
use output_format::types::OutputFormatContent;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub use crate::chat_message_part::ChatMessagePart;
use crate::{
    baml_value_to_jinja_value::{
        IntoMiniJinjaValue, MinijinjaBamlClass, MinijinjaBamlEnumType, MinijinjaBamlEnumValue,
        MinijinjaBamlList,
    },
    output_format::OutputFormat,
};

/// Convert a minijinja::Value to serde_json::Value, properly handling BAML custom types
/// and preserving aliases for enums and classes.
pub fn minijinja_value_to_json(value: &minijinja::Value) -> Result<serde_json::Value, String> {
    use minijinja::value::ValueKind;

    match value.kind() {
        ValueKind::None | ValueKind::Undefined => Ok(serde_json::Value::Null),
        ValueKind::Bool => Ok(serde_json::Value::Bool(value.is_true())),
        ValueKind::Number => {
            if let Some(n) = value.as_i64() {
                Ok(serde_json::Value::Number(n.into()))
            } else if let Ok(f_str) = value.to_string().parse::<f64>() {
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(f_str)
                        .ok_or_else(|| "Invalid float value for JSON encoding".to_string())?,
                ))
            } else {
                Err("Cannot convert number to JSON".to_string())
            }
        }
        ValueKind::String => Ok(serde_json::Value::String(value.to_string())),
        ValueKind::Seq => {
            // Check if it's a MinijinjaBamlList (custom object with Serialize)
            if let Some(obj) = value.as_object() {
                if let Some(baml_list) = obj.downcast_ref::<MinijinjaBamlList>() {
                    // Recursively convert list items
                    let arr: Result<Vec<serde_json::Value>, String> =
                        baml_list.list.iter().map(minijinja_value_to_json).collect();
                    return Ok(serde_json::Value::Array(arr?));
                }
            }

            // Regular sequence
            if let Ok(iter) = value.try_iter() {
                let arr: Result<Vec<serde_json::Value>, String> =
                    iter.map(|v| minijinja_value_to_json(&v)).collect();
                Ok(serde_json::Value::Array(arr?))
            } else {
                Ok(serde_json::Value::Array(vec![]))
            }
        }
        ValueKind::Map => {
            // Check if it's a custom BAML object
            if let Some(obj) = value.as_object() {
                // MinijinjaBamlClass - use aliased keys
                if let Some(baml_class) = obj.downcast_ref::<MinijinjaBamlClass>() {
                    let mut map = serde_json::Map::new();
                    for (k, v) in baml_class.class.iter() {
                        let alias = baml_class.key_to_alias.get(k).unwrap_or(k);
                        map.insert(alias.clone(), minijinja_value_to_json(v)?);
                    }
                    return Ok(serde_json::Value::Object(map));
                }

                // MinijinjaBamlEnumValue - use alias or value
                if let Some(enum_val) = obj.downcast_ref::<MinijinjaBamlEnumValue>() {
                    return Ok(serde_json::Value::String(
                        enum_val.alias.as_ref().unwrap_or(&enum_val.value).clone(),
                    ));
                }
            }

            // Regular map
            if let Ok(keys) = value.try_iter() {
                let keys_vec: Vec<minijinja::Value> = keys.collect();
                if keys_vec.is_empty() {
                    // Empty iterator - non-enumerable custom object
                    Ok(serde_json::Value::String(format!("{}", value)))
                } else {
                    // Has keys, treat as a proper map
                    let mut map = serde_json::Map::new();
                    for key in keys_vec {
                        if let Some(key_str) = key.as_str() {
                            if let Ok(val) = value.get_item(&key) {
                                map.insert(key_str.to_string(), minijinja_value_to_json(&val)?);
                            }
                        }
                    }
                    Ok(serde_json::Value::Object(map))
                }
            } else {
                // try_iter failed - non-enumerable custom object
                Ok(serde_json::Value::String(format!("{}", value)))
            }
        }
        _ => Ok(serde_json::Value::String(value.to_string())),
    }
}

/// Convert a minijinja::Value to a YAML string while preserving all BAML-specific aliases.
pub fn minijinja_value_to_yaml(value: &minijinja::Value) -> Result<String, String> {
    // Reuse the JSON conversion which already preserves aliases for enums/classes/lists.
    let json_value = minijinja_value_to_json(value)?;
    serde_yaml::to_string(&json_value).map_err(|e| format!("Failed to serialize to YAML: {}", e))
}

fn encode_value_to_toon(
    value: &minijinja::Value,
    kwargs: &Kwargs,
) -> Result<String, minijinja::Error> {
    let json_value = minijinja_value_to_json(value)
        .map_err(|e| minijinja::Error::new(minijinja::ErrorKind::BadSerialization, e))?;

    let mut options = toon::EncodeOptions::default();

    if let Ok(indent) = kwargs.get::<usize>("indent") {
        options.indent = indent;
    }

    if let Ok(delimiter_str) = kwargs.get::<String>("delimiter") {
        options.delimiter = match delimiter_str.as_str() {
            "comma" => toon::Delimiter::Comma,
            "tab" => toon::Delimiter::Tab,
            "pipe" => toon::Delimiter::Pipe,
            _ => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!(
                        "Invalid delimiter '{}'. Use 'comma', 'tab', or 'pipe'",
                        delimiter_str
                    ),
                ))
            }
        };
    }

    if let Ok(marker) = kwargs.get::<String>("length_marker") {
        if marker.chars().count() == 1 {
            options.length_marker = marker.chars().next();
        } else {
            return Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("length_marker must be a single character, got '{}'", marker),
            ));
        }
    }

    Ok(toon::encode(&json_value, Some(options)))
}

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Serialize)]
pub struct RenderContext_Client {
    // The name of the actual client
    pub name: String,
    // The provider for this client
    pub provider: String,
    pub default_role: String,
    pub allowed_roles: Vec<String>,
    // how to remap allowed roles to the ones the client expects
    // this is done last, if not present, we use use role as is
    pub remap_role: HashMap<String, String>,

    // properties of the client
    pub options: IndexMap<String, serde_json::Value>,
}

#[derive(Debug)]
pub struct RenderContext {
    pub client: RenderContext_Client,
    pub output_format: OutputFormatContent,
    pub tags: HashMap<String, BamlValue>,
}

pub struct TemplateStringMacro {
    pub name: String,
    pub args: Vec<(String, String)>,
    pub template: String,
}

const MAGIC_CHAT_ROLE_DELIMITER: &str = "BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER";
const MAGIC_MEDIA_DELIMITER: &str = "BAML_MEDIA_MAGIC_STRING_DELIMITER";

struct MinijinjaRenderParams<'a> {
    template: &'a str,
    args: &'a minijinja::Value,
    ctx: RenderContext,
    template_string_macros: &'a [TemplateStringMacro],
    default_role: String,
    allowed_roles: Vec<String>,
    remap_role: HashMap<String, String>,
    enum_values_by_name: IndexMap<String, Vec<MinijinjaBamlEnumValue>>,
}

fn render_minijinja(params: MinijinjaRenderParams) -> Result<RenderedPrompt, minijinja::Error> {
    let MinijinjaRenderParams {
        template,
        args,
        ctx,
        template_string_macros,
        default_role,
        allowed_roles,
        remap_role,
        enum_values_by_name,
    } = params;
    let mut env = get_env();

    // Add generic format filter (needs access to MinijinjaBamlClass/etc)
    env.add_filter(
        "format",
        |value: minijinja::Value, kwargs: minijinja::value::Kwargs| {
            let format_type = kwargs
                .get::<String>("type")
                .or_else(|_| kwargs.get::<String>("format"))
                .map_err(|_| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "format filter requires 'type' keyword argument",
                    )
                })?;

            match format_type.to_lowercase().as_str() {
                "yaml" => minijinja_value_to_yaml(&value)
                    .map_err(|e| minijinja::Error::new(minijinja::ErrorKind::BadSerialization, e)),
                "json" => {
                    let json_value = minijinja_value_to_json(&value).map_err(|e| {
                        minijinja::Error::new(minijinja::ErrorKind::BadSerialization, e)
                    })?;
                    serde_json::to_string(&json_value).map_err(|e| {
                        minijinja::Error::new(minijinja::ErrorKind::BadSerialization, e.to_string())
                    })
                }
                "toon" => encode_value_to_toon(&value, &kwargs),
                other => Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!(
                        "Unsupported format type '{}'. Supported types: 'yaml', 'json', 'toon'",
                        other
                    ),
                )),
            }
        },
    );
    // dedent
    let whitespace_length = template
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);
    let template = template
        .split('\n')
        .map(|line| line.chars().skip(whitespace_length).collect::<String>())
        .collect::<Vec<String>>()
        .join("\n");

    // trim
    let template = template.trim();
    log::debug!("Rendering template: \n{template}\n------\n");
    // let args_dict = minijinja::Value::from_serializable(args);

    // inject macros
    let template = template_string_macros
        .iter()
        .map(|tsm| {
            format!(
                "{{% macro {name}({template_args}) %}}{template}{{% endmacro %}}",
                name = tsm.name,
                template_args = tsm
                    .args
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                template = tsm.template,
            )
        })
        .chain(std::iter::once(template.to_string()))
        .collect::<Vec<_>>()
        .join("\n");

    env.add_template("prompt", &template)?;
    let client = ctx.client.clone();
    let tags = ctx.tags.clone();
    let formatter = OutputFormat::new(ctx);
    env.add_global(
        "ctx",
        context! {
            client => client,
            tags => tags,
            output_format => minijinja::value::Value::from_object(formatter),
        },
    );
    for (enum_name, enum_values) in enum_values_by_name {
        env.add_global(
            enum_name.clone(),
            minijinja::value::Value::from_object(MinijinjaBamlEnumType {
                enum_name,
                enum_values: enum_values
                    .into_iter()
                    .map(|v| (v.value.clone(), v))
                    .collect(),
            }),
        );
    }

    let role_fn = minijinja::Value::from_function(
        |role: Option<String>, kwargs: Kwargs| -> Result<String, minijinja::Error> {
            let role = match (role, kwargs.get::<String>("role")) {
                (Some(b), Ok(a)) => {
                    // If both are present, we should error
                    return Err(minijinja::Error::new(
                        ErrorKind::TooManyArguments,
                        format!("role() called with two roles: '{a}' and '{b}'"),
                    ));
                }
                (Some(role), _) => role,
                (_, Ok(role)) => role,
                _ => {
                    // If neither are present, we should error
                    return Err(minijinja::Error::new(
                        ErrorKind::MissingArgument,
                        "role() called without role. Try role('role') or role(role='role').",
                    ));
                }
            };

            let allow_duplicate_role = match kwargs.get::<bool>("__baml_allow_dupe_role__") {
                Ok(allow_duplicate_role) => allow_duplicate_role,
                Err(e) => match e.kind() {
                    ErrorKind::MissingArgument => false,
                    _ => return Err(e),
                },
            };

            let additional_properties = {
                let mut props = kwargs
                    .args()
                    .filter(|&k| k != "role")
                    .map(|k| {
                        Ok((
                            k,
                            serde_json::Value::deserialize(kwargs.get::<minijinja::Value>(k)?)?,
                        ))
                    })
                    .collect::<Result<HashMap<&str, serde_json::Value>, minijinja::Error>>()?;

                props.insert("role", role.clone().into());
                props.insert("__baml_allow_dupe_role__", allow_duplicate_role.into());

                props
            };

            let additional_properties = json!(additional_properties).to_string();

            Ok(format!("{MAGIC_CHAT_ROLE_DELIMITER}:baml-start-baml:{additional_properties}:baml-end-baml:{MAGIC_CHAT_ROLE_DELIMITER}"))
        },
    );

    env.add_global(
        "_",
        context! {
            chat => role_fn,
            role => role_fn
        },
    );

    let tmpl = env.get_template("prompt")?;

    let rendered = tmpl.render(args)?;

    if !rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) && !rendered.contains(MAGIC_MEDIA_DELIMITER) {
        return Ok(RenderedPrompt::Completion(rendered));
    }

    let mut chat_messages = vec![];
    let mut role = None;
    let mut meta = None;
    let mut allow_duplicate_role = false;
    for chunk in rendered.split(MAGIC_CHAT_ROLE_DELIMITER) {
        if chunk.starts_with(":baml-start-baml:") && chunk.ends_with(":baml-end-baml:") {
            let parsed = chunk
                .strip_prefix(":baml-start-baml:")
                .unwrap_or(chunk)
                .strip_suffix(":baml-end-baml:")
                .unwrap_or(chunk);
            if let Ok(mut parsed) =
                serde_json::from_str::<HashMap<String, serde_json::Value>>(parsed)
            {
                if let Some(role_val) = parsed.remove("role") {
                    role = Some(role_val.as_str().unwrap().to_string());
                }

                allow_duplicate_role = parsed
                    .remove("__baml_allow_dupe_role__")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if parsed.is_empty() {
                    meta = None;
                } else {
                    meta = Some(parsed);
                }
            }
        } else if role.is_none() && chunk.is_empty() {
            // If there's only whitespace before the first `_.chat()` directive, we discard that chunk
        } else {
            let mut parts = vec![];
            for part in chunk.split(MAGIC_MEDIA_DELIMITER) {
                let part = if part.starts_with(":baml-start-media:")
                    && part.ends_with(":baml-end-media:")
                {
                    let media_data = part
                        .strip_prefix(":baml-start-media:")
                        .unwrap_or(part)
                        .strip_suffix(":baml-end-media:")
                        .unwrap_or(part);

                    match serde_json::from_str::<BamlMedia>(media_data) {
                        Ok(m) => Some(ChatMessagePart::Media(m)),
                        Err(_) => Err(minijinja::Error::new(
                            ErrorKind::CannotUnpack,
                            format!("Media variable had unrecognizable data: {media_data}"),
                        ))?,
                    }
                } else if !part.trim().is_empty() {
                    Some(ChatMessagePart::Text(part.trim().to_string()))
                } else {
                    None
                };

                if let Some(part) = part {
                    if let Some(meta) = &meta {
                        parts.push(part.with_meta(meta.clone()));
                    } else {
                        parts.push(part);
                    }
                }
            }

            // Only add the message if it contains meaningful content
            if !parts.is_empty() {
                chat_messages.push(RenderedChatMessage {
                    role: match role.as_ref() {
                        Some(r) if allowed_roles.contains(r) => r.clone(),
                        Some(_) => default_role.clone(),
                        None => default_role.clone(),
                    },
                    allow_duplicate_role,
                    parts,
                });
            }
        }
    }

    chat_messages.iter_mut().for_each(|m| {
        if let Some(remap) = remap_role.get(&m.role) {
            m.role = remap.clone();
        }
    });
    Ok(RenderedPrompt::Chat(chat_messages))
}

#[derive(Debug, PartialEq, Serialize, Clone)]
pub struct RenderedChatMessage {
    pub role: String,
    pub allow_duplicate_role: bool,
    pub parts: Vec<ChatMessagePart>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct ImageUrl {
    pub url: String,
}

impl ImageUrl {
    pub fn new(url: String) -> ImageUrl {
        ImageUrl { url }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct ImageBase64 {
    pub base64: String,
    pub media_type: String,
}

impl ImageBase64 {
    pub fn new(base64: String, media_type: String) -> ImageBase64 {
        ImageBase64 { base64, media_type }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub enum RenderedPrompt {
    Completion(String),
    Chat(Vec<RenderedChatMessage>),
}

impl std::fmt::Display for RenderedPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RenderedPrompt::Completion(s) => write!(f, "{s}"),
            RenderedPrompt::Chat(messages) => {
                for message in messages {
                    writeln!(
                        f,
                        "{}{}",
                        format!("{}: ", message.role).on_yellow(),
                        message
                            .parts
                            .iter()
                            .map(ChatMessagePart::to_string)
                            .collect::<Vec<String>>()
                            .join("")
                    )?;
                }
                Ok(())
            }
        }
    }
}

pub struct ChatOptions {
    default_role: String,
    #[allow(dead_code)]
    valid_roles: Option<Vec<String>>,
}

impl ChatOptions {
    pub fn new(default_role: String, valid_roles: Option<Vec<String>>) -> ChatOptions {
        ChatOptions {
            default_role,
            valid_roles,
        }
    }
}

pub struct CompletionOptions {
    joiner: String,
}

impl CompletionOptions {
    pub fn new(joiner: String) -> CompletionOptions {
        CompletionOptions { joiner }
    }
}

impl RenderedPrompt {
    pub fn as_chat(self, chat_options: &ChatOptions) -> RenderedPrompt {
        match self {
            RenderedPrompt::Chat(messages) => RenderedPrompt::Chat(messages),
            RenderedPrompt::Completion(message) => {
                RenderedPrompt::Chat(vec![RenderedChatMessage {
                    role: chat_options.default_role.clone(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(message)],
                }])
            }
        }
    }

    pub fn as_completion(self, completion_options: &CompletionOptions) -> RenderedPrompt {
        match self {
            RenderedPrompt::Chat(messages) => RenderedPrompt::Completion(
                messages
                    .into_iter()
                    .flat_map(|m| {
                        m.parts.into_iter().map(|p| match p {
                            ChatMessagePart::Text(t) => t,
                            ChatMessagePart::Media(_) => "".to_string(), // we are choosing to ignore the image for now
                            ChatMessagePart::WithMeta(p, _) => p.to_string(),
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(&completion_options.joiner),
            ),
            RenderedPrompt::Completion(message) => RenderedPrompt::Completion(message),
        }
    }
}

// pub fn render_prompt(
//     template: &str,
//     args: &minijinja::Value,
//     ctx: RenderContext,
//     template_string_macros: &[TemplateStringMacro],
// ) -> anyhow::Result<RenderedPrompt> {
//     let rendered = render_minijinja(template, args, ctx, template_string_macros);

//     match rendered {
//         Ok(r) => Ok(r),
//         Err(err) => {
//             let mut minijinja_err = "".to_string();
//             minijinja_err += &format!("{err:#}");

//             let mut err = &err as &dyn std::error::Error;
//             while let Some(next_err) = err.source() {
//                 minijinja_err += &format!("\n\ncaused by: {next_err:#}");
//                 err = next_err;
//             }

//             anyhow::bail!("Error occurred while rendering prompt: {minijinja_err}");
//         }
//     }
// }

pub fn render_prompt(
    template: &str,
    args: &BamlValue,
    ctx: RenderContext,
    template_string_macros: &[TemplateStringMacro],
    ir: &IntermediateRepr,
    env_vars: &HashMap<String, String>,
) -> anyhow::Result<RenderedPrompt> {
    if !matches!(args, BamlValue::Map(_)) {
        anyhow::bail!("args must be a map");
    }
    let eval_ctx = EvaluationContext::new(env_vars, false);
    let minijinja_args: minijinja::Value = args.clone().to_minijinja_value(ir, &eval_ctx);
    let default_role = ctx.client.default_role.clone();
    let allowed_roles = ctx.client.allowed_roles.clone();
    let remap_role = ctx.client.remap_role.clone();
    let enum_values_by_name = ir
        .walk_enums()
        .map(|e| {
            let enum_name = e.name().to_string();
            let enum_values = e
                .walk_values()
                .map(|v| MinijinjaBamlEnumValue {
                    value: v.name().to_string(),
                    alias: v.alias(&eval_ctx).unwrap_or(None),
                    enum_name: enum_name.clone(),
                })
                .collect::<Vec<_>>();
            (enum_name, enum_values)
        })
        .collect::<IndexMap<_, _>>();

    let rendered = render_minijinja(MinijinjaRenderParams {
        template,
        args: &minijinja_args,
        ctx,
        template_string_macros,
        default_role,
        allowed_roles,
        remap_role,
        enum_values_by_name,
    });

    match rendered {
        Ok(r) => Ok(r),
        Err(err) => {
            let mut minijinja_err = "".to_string();
            minijinja_err += &format!("{err:#}");

            let mut err = &err as &dyn std::error::Error;
            while let Some(next_err) = err.source() {
                minijinja_err += &format!("\n\ncaused by: {next_err:#}");
                err = next_err;
            }

            anyhow::bail!("Error occurred while rendering prompt: {minijinja_err}");
        }
    }
}

#[cfg(test)]
mod render_tests {

    use std::sync::Once;

    use baml_types::{BamlMap, BamlMediaType};
    use indexmap::IndexMap;

    use super::*;

    static INIT: Once = Once::new();

    pub fn setup_logging() {
        INIT.call_once(|| {
            env_logger::init();
        });
    }

    pub fn make_test_ir(source_code: &str) -> anyhow::Result<IntermediateRepr> {
        use std::path::PathBuf;

        use internal_baml_core::{validate, FeatureFlags, ValidatedSchema};
        use internal_baml_diagnostics::SourceFile;
        let path: PathBuf = "fake_file.baml".into();
        let source_file: SourceFile = (path.clone(), source_code).into();
        let validated_schema: ValidatedSchema =
            validate(&path, vec![source_file], FeatureFlags::new());
        let diagnostics = &validated_schema.diagnostics;
        if diagnostics.has_errors() {
            return Err(anyhow::anyhow!(
                "Source code was invalid: \n{:?}",
                diagnostics.errors()
            ));
        }
        let ir = IntermediateRepr::from_parser_database(
            &validated_schema.db,
            validated_schema.configuration,
        )?;
        Ok(ir)
    }

    #[test]
    fn render_image() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "img".to_string(),
            BamlValue::Media(BamlMedia::url(
                BamlMediaType::Image,
                "https://example.com/image.jpg".to_string(),
                None,
            )),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "{{ _.chat(\"system\") }}
            Here is an image: {{ img }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text(["Here is an image:"].join("\n")),
                    ChatMessagePart::Media(BamlMedia::url(
                        BamlMediaType::Image,
                        "https://example.com/image.jpg".to_string(),
                        None
                    )),
                ]
            },])
        );

        Ok(())
    }

    #[test]
    fn render_image_nested() -> anyhow::Result<()> {
        setup_logging();
        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "myObject".to_string(),
            BamlValue::Map(BamlMap::from([(
                "img".to_string(),
                BamlValue::Media(BamlMedia::url(
                    BamlMediaType::Image,
                    "https://example.com/image.jpg".to_string(),
                    None,
                )),
            )])),
        )]));

        let rendered = render_prompt(
            "{{ _.chat(\"system\") }}
            Here is an image: {{ myObject.img }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text(["Here is an image:"].join("\n")),
                    ChatMessagePart::Media(BamlMedia::url(
                        BamlMediaType::Image,
                        "https://example.com/image.jpg".to_string(),
                        None
                    )),
                ]
            },])
        );

        Ok(())
    }

    #[test]
    fn render_image_suffix() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "img".to_string(),
            BamlValue::Media(BamlMedia::url(
                BamlMediaType::Image,
                "https://example.com/image.jpg".to_string(),
                None,
            )),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "{{ _.chat(\"system\") }}
            Here is an image: {{ img }}. Please help me.",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text(["Here is an image:"].join("\n")),
                    ChatMessagePart::Media(BamlMedia::url(
                        BamlMediaType::Image,
                        "https://example.com/image.jpg".to_string(),
                        None
                    )),
                    ChatMessagePart::Text([". Please help me."].join("\n")),
                ]
            },])
        );

        Ok(())
    }

    #[test]
    fn render_chat() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "

                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}

                    {{ _.chat(ctx.tags['ROLE']) }}

                    Tell me a haiku about {{ haiku_subject }}. {{ ctx.output_format }}

                    End the haiku with a line about your maker, {{ ctx.client.provider }}.

            ",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string(), "john doe".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![
                RenderedChatMessage {
                    role: "system".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        [
                            "You are an assistant that always responds",
                            "in a very excited way with emojis",
                            "and also outputs this word 4 times",
                            "after giving a response: sakura"
                        ]
                        .join("\n")
                    )]
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        [
                            "Tell me a haiku about sakura. ",
                            "",
                            "End the haiku with a line about your maker, openai."
                        ]
                        .join("\n")
                    )]
                }
            ])
        );

        Ok(())
    }

    #[test]
    fn render_completion() -> anyhow::Result<()> {
        setup_logging();

        let _args = context! {
            haiku_subject => "sakura"
        };

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "
                You are an assistant that always responds
                in a very excited way with emojis
                and also outputs this word 4 times
                after giving a response: {{ haiku_subject }}
            ",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion(
                [
                    "You are an assistant that always responds",
                    "in a very excited way with emojis",
                    "and also outputs this word 4 times",
                    "after giving a response: sakura"
                ]
                .join("\n")
            )
        );

        Ok(())
    }

    #[test]
    fn render_output_format_directly() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "{{ ctx.output_format }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("".to_string()));

        Ok(())
    }

    #[test]
    fn render_output_format_prefix_unspecified() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "HI! {{ ctx.output_format }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("HI! ".to_string()));

        Ok(())
    }

    #[test]
    fn render_output_format_prefix_null() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "{{ ctx.output_format(prefix=null) }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("".into()));

        Ok(())
    }

    #[test]
    fn render_output_format_prefix_str() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "{{ ctx.output_format(prefix='custom format:') }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("custom format:string".to_string())
        );

        Ok(())
    }

    #[test]
    fn render_chat_param_failures() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "name".to_string(),
            BamlValue::String("world".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        // rendering should fail: template contains '{{ name }' (missing '}' at the end)
        let rendered = render_prompt(
            r#"
                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}

                    {{ _.role(role=ctx.tags.ROLE) }}

                    Tell me a haiku about {{ haiku_subject }} in {{ ctx.output_format }}.

                    {{ _.role(ctx.tags.ROLE) }}
                    End the haiku with a line about your maker, {{ ctx.client.provider }}.

                    {{ _.role("a", role="aa") }}
                    hi!

                    {{ _.role() }}
                    hi!
            "#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        );

        match rendered {
            Ok(_) => {
                anyhow::bail!("Expected template rendering to fail, but it succeeded");
            }
            Err(e) => assert!(e
                .to_string()
                .contains("role() called with two roles: 'aa' and 'a'")),
        }

        Ok(())
    }

    #[test]
    fn render_with_kwargs() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            r#"

                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}

                    {{ _.chat(role=ctx.tags.ROLE) }}

                    Tell me a haiku about {{ haiku_subject }}. {{ ctx.output_format }}

                    {{ _.chat(ctx.tags.ROLE) }}
                    End the haiku with a line about your maker, {{ ctx.client.provider }}.
            "#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string(), "john doe".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![
                RenderedChatMessage {
                    role: "system".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        [
                            "You are an assistant that always responds",
                            "in a very excited way with emojis",
                            "and also outputs this word 4 times",
                            "after giving a response: sakura"
                        ]
                        .join("\n")
                    )]
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        "Tell me a haiku about sakura.".to_string()
                    )]
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        "End the haiku with a line about your maker, openai.".to_string()
                    )]
                }
            ])
        );

        Ok(())
    }

    #[test]
    fn render_with_kwargs_default_role() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            r#"

                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}

                    {{ _.chat(role=ctx.tags.ROLE) }}

                    Tell me a haiku about {{ haiku_subject }}. {{ ctx.output_format }}

                    {{ _.chat("user") }}
                    End the haiku with a line about your maker, {{ ctx.client.provider }}.
            "#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string(), "user".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![
                RenderedChatMessage {
                    role: "system".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        [
                            "You are an assistant that always responds",
                            "in a very excited way with emojis",
                            "and also outputs this word 4 times",
                            "after giving a response: sakura"
                        ]
                        .join("\n")
                    )]
                },
                RenderedChatMessage {
                    role: "system".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        "Tell me a haiku about sakura.".to_string()
                    )]
                },
                RenderedChatMessage {
                    role: "user".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        "End the haiku with a line about your maker, openai.".to_string()
                    )]
                }
            ])
        );

        Ok(())
    }

    #[test]
    fn render_chat_starts_with_system() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "haiku_subject".to_string(),
            BamlValue::String("sakura".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        let rendered = render_prompt(
            "
                {{ _.chat(\"system\") }}

                You are an assistant that always responds
                in a very excited way with emojis
                and also outputs this word 4 times
                after giving a response: {{ haiku_subject }}
            ",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![ChatMessagePart::Text(
                    [
                        "You are an assistant that always responds",
                        "in a very excited way with emojis",
                        "and also outputs this word 4 times",
                        "after giving a response: sakura"
                    ]
                    .join("\n")
                )]
            },])
        );

        Ok(())
    }

    #[test]
    fn render_malformed_jinja() -> anyhow::Result<()> {
        setup_logging();
        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "name".to_string(),
            BamlValue::String("world".to_string()),
        )]));

        let ir = make_test_ir(
            "
            class C {

            }
            ",
        )?;

        // rendering should fail: template contains '{{ name }' (missing '}' at the end)
        let rendered = render_prompt(
            "Hello, {{ name }!",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        );

        match rendered {
            Ok(_) => {
                anyhow::bail!("Expected template rendering to fail, but it succeeded");
            }
            Err(e) => assert!(e
                .to_string()
                .contains("Error occurred while rendering prompt:")),
        }

        Ok(())
    }

    #[test]
    fn render_class_with_aliases() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            // class args are not aliased yet when passed in to jinja
            BamlValue::Class(
                "C".to_string(),
                BamlMap::from([("prop1".to_string(), BamlValue::String("value".to_string()))]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class C {
                prop1 string @alias("key1")
            }
            "#,
        )?;

        let rendered = render_prompt(
            " {{ class_arg }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("{\n    \"key1\": \"value\",\n}".to_string())
        );

        Ok(())
    }

    // render class with if condition on class property test
    #[test]
    fn render_class_with_if_condition() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::Class(
                "C".to_string(),
                BamlMap::from([("prop1".to_string(), BamlValue::String("value".to_string()))]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class C {
                prop1 string @alias("key1")
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{% if class_arg.prop1 == 'value' %}true{% else %}false{% endif %}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("true".to_string()));

        let rendered = render_prompt(
            "{% if class_arg.prop1 != 'value' %}true{% else %}false{% endif %}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("false".to_string()));

        Ok(())
    }

    #[test]
    fn render_number_comparison_with_alias() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::Class(
                "C".to_string(),
                BamlMap::from([("prop1".to_string(), BamlValue::Int(4))]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class C {
                prop1 int @alias("key1")
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{% if class_arg.prop1 < 40 %}true{% else %}false{% endif %}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("true".to_string()));

        let rendered = render_prompt(
            "{% if class_arg.prop1 > 50 %}true{% else %}false{% endif %}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("false".to_string()));

        Ok(())
    }

    #[test]
    fn render_number_comparison_with_alias2() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::Class(
                "C".to_string(),
                BamlMap::from([("prop1".to_string(), BamlValue::Int(13))]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class C {
                prop1 int @alias("key1")
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ class_arg.prop1 < 2 }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(rendered, RenderedPrompt::Completion("false".to_string()));

        Ok(())
    }

    // Test nested class B
    #[test]
    fn render_nested_class() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::Class(
                "A".to_string(),
                IndexMap::from([
                    (
                        "a_prop1".to_string(),
                        BamlValue::String("value_a".to_string()),
                    ),
                    (
                        "a_prop2".to_string(),
                        BamlValue::Class(
                            "B".to_string(),
                            IndexMap::from([
                                (
                                    "b_prop1".to_string(),
                                    BamlValue::String("value_b".to_string()),
                                ),
                                (
                                    "b_prop2".to_string(),
                                    BamlValue::List(vec![
                                        BamlValue::String("item1".to_string()),
                                        BamlValue::String("item2".to_string()),
                                    ]),
                                ),
                            ]),
                        ),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class A {
                a_prop1 string @alias("alias_a_prop1")
                a_prop2 B
            }

            class B {
                b_prop1 string @alias("alias_b_prop1")
                b_prop2 string[]
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ class_arg }}\n{{ class_arg.a_prop1 }} - {{ class_arg.a_prop2.b_prop1 }} - {{ class_arg.a_prop2.b_prop2|length }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("{\n    \"alias_a_prop1\": \"value_a\",\n    \"a_prop2\": {\n        \"alias_b_prop1\": \"value_b\",\n        \"b_prop2\": [\n            \"item1\",\n            \"item2\",\n        ],\n    },\n}\nvalue_a - value_b - 2".to_string())
        );

        Ok(())
    }

    // Test B as a list
    #[test]
    fn render_b_as_list() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::Class(
                "A".to_string(),
                IndexMap::from([
                    (
                        "a_prop1".to_string(),
                        BamlValue::String("value_a".to_string()),
                    ),
                    (
                        "a_prop2".to_string(),
                        BamlValue::List(vec![
                            BamlValue::Class(
                                "B".to_string(),
                                IndexMap::from([
                                    (
                                        "b_prop1".to_string(),
                                        BamlValue::String("value_b1".to_string()),
                                    ),
                                    (
                                        "b_prop2".to_string(),
                                        BamlValue::List(vec![
                                            BamlValue::String("item1".to_string()),
                                            BamlValue::String("item2".to_string()),
                                        ]),
                                    ),
                                ]),
                            ),
                            BamlValue::Class(
                                "B".to_string(),
                                IndexMap::from([
                                    (
                                        "b_prop1".to_string(),
                                        BamlValue::String("value_b2".to_string()),
                                    ),
                                    (
                                        "b_prop2".to_string(),
                                        BamlValue::List(vec![BamlValue::String(
                                            "item3".to_string(),
                                        )]),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class A {
                a_prop1 string @alias("alias_a_prop1")
                a_prop2 B[]
            }

            class B {
                b_prop1 string @alias("alias_b_prop1")
                b_prop2 string[]
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ class_arg.a_prop1 }} - {{ class_arg.a_prop2|length }} - {{ class_arg.a_prop2[0].b_prop1 }} - {{ class_arg.a_prop2[1].b_prop2|length }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("value_a - 2 - value_b1 - 1".to_string())
        );

        Ok(())
    }

    // Test A and B as lists
    #[test]
    fn render_a_and_b_as_lists() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::List(vec![
                BamlValue::Class(
                    "A".to_string(),
                    IndexMap::from([
                        (
                            "a_prop1".to_string(),
                            BamlValue::String("value_a1".to_string()),
                        ),
                        (
                            "a_prop2".to_string(),
                            BamlValue::List(vec![BamlValue::Class(
                                "B".to_string(),
                                IndexMap::from([
                                    (
                                        "b_prop1".to_string(),
                                        BamlValue::String("value_b1".to_string()),
                                    ),
                                    (
                                        "b_prop2".to_string(),
                                        BamlValue::List(vec![
                                            BamlValue::String("item1".to_string()),
                                            BamlValue::String("item2".to_string()),
                                        ]),
                                    ),
                                ]),
                            )]),
                        ),
                    ]),
                ),
                BamlValue::Class(
                    "A".to_string(),
                    IndexMap::from([
                        (
                            "a_prop1".to_string(),
                            BamlValue::String("value_a2".to_string()),
                        ),
                        (
                            "a_prop2".to_string(),
                            BamlValue::List(vec![BamlValue::Class(
                                "B".to_string(),
                                IndexMap::from([
                                    (
                                        "b_prop1".to_string(),
                                        BamlValue::String("value_b2".to_string()),
                                    ),
                                    (
                                        "b_prop2".to_string(),
                                        BamlValue::List(vec![BamlValue::String(
                                            "item3".to_string(),
                                        )]),
                                    ),
                                ]),
                            )]),
                        ),
                    ]),
                ),
            ]),
        )]));

        let ir = make_test_ir(
            r#"
            class A {
                a_prop1 string @alias("alias_a_prop1")
                a_prop2 B[]
            }

            class B {
                b_prop1 string @alias("alias_b_prop1")
                b_prop2 string[]
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ class_arg|length }} - {{ class_arg[0].a_prop1 }} - {{ class_arg[1].a_prop2[0].b_prop1 }} - {% if class_arg[0].a_prop2[0].b_prop2|length > 1 %}true{% else %}false{% endif %}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("2 - value_a1 - value_b2 - true".to_string())
        );

        Ok(())
    }

    // Test aliased key is the nested one
    #[test]
    fn render_aliased_nested_key() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::List(vec![BamlValue::Class(
                "A".to_string(),
                IndexMap::from([
                    (
                        "a_prop1".to_string(),
                        BamlValue::String("value_a1".to_string()),
                    ),
                    (
                        "a_prop2".to_string(),
                        BamlValue::List(vec![BamlValue::Class(
                            "B".to_string(),
                            IndexMap::from([
                                (
                                    "b_prop1".to_string(),
                                    BamlValue::String("value_b1".to_string()),
                                ),
                                (
                                    "b_prop2".to_string(),
                                    BamlValue::List(vec![
                                        BamlValue::String("item1".to_string()),
                                        BamlValue::String("item2".to_string()),
                                    ]),
                                ),
                            ]),
                        )]),
                    ),
                ]),
            )]),
        )]));

        let ir = make_test_ir(
            r#"
            class A {
                a_prop1 string
                a_prop2 B[] @alias("alias_a_prop2")
            }

            class B {
                b_prop1 string @alias("alias_b_prop1")
                b_prop2 string[]
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ class_arg[0].a_prop1 }} - {{ class_arg[0].a_prop2|length }} - {{ class_arg[0].a_prop2[0].b_prop1 }} - {{ class_arg[0].a_prop2[0].b_prop2|length }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("value_a1 - 1 - value_b1 - 2".to_string())
        );

        Ok(())
    }

    #[test]
    fn render_class_with_image() -> anyhow::Result<()> {
        setup_logging();

        let args: BamlValue = BamlValue::Map(BamlMap::from([(
            "class_arg".to_string(),
            BamlValue::Class(
                "A".to_string(),
                IndexMap::from([
                    (
                        "a_prop1".to_string(),
                        BamlValue::String("value_a".to_string()),
                    ),
                    (
                        "a_prop2".to_string(),
                        BamlValue::Media(BamlMedia::url(
                            BamlMediaType::Image,
                            "https://example.com/image.jpg".to_string(),
                            None,
                        )),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class A {
                a_prop1 string
                a_prop2 image @alias("alias_a_prop2")
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ class_arg }}\n{{ class_arg.a_prop1 }} - {{ class_arg.alias_a_prop2 }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text(
                        "{\n    \"a_prop1\": \"value_a\",\n    \"alias_a_prop2\":".to_string()
                    ),
                    ChatMessagePart::Media(BamlMedia::url(
                        BamlMediaType::Image,
                        "https://example.com/image.jpg".to_string(),
                        None
                    )),
                    ChatMessagePart::Text(",\n}\nvalue_a -".to_string()),
                ]
            }])
        );

        Ok(())
    }

    // See the note in baml_value_to_jinja_value.rs for Enum for why we don't support aliases.
    // tl;dr we don't havea  way to override the equality operator for enum comparisons to NOT use the alias.
    #[test]
    fn test_render_prompt_with_enum() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "enum_arg".to_string(),
            BamlValue::Enum("MyEnum".to_string(), "VALUE_B".to_string()),
        )]));

        let ir = make_test_ir(
            r#"
            enum MyEnum {
                VALUE_A @alias("alpha")
                VALUE_B @alias("ALIAS_B")
                VALUE_C
            }
            "#,
        )?;

        let rendered = render_prompt(
            r#"
Enum value: {{ enum_arg }}

handwritten enum values:
  - first: {{ MyEnum.VALUE_A }}
  - second: {{ MyEnum.VALUE_B }}
  - third: {{ MyEnum.VALUE_C }}

{% if enum_arg == MyEnum.VALUE_B %}
Enum value is equal to MyEnum.VALUE_B, as expected
{% else %}
Enum value should equal MyEnum.VALUE_B, but it does not
{% endif %}

{% if enum_arg != MyEnum.VALUE_A %}
Enum value is not equal to MyEnum.VALUE_A, as expected
{% else %}
Enum value should not equal MyEnum.VALUE_A, but it does
{% endif %}

{% if enum_arg == "VALUE_B" %}
Enum value is equal to the "VALUE_B" string, as expected
{% else %}
Enum value should equal the "VALUE_B" string, but it does not
{% endif %}

{% if enum_arg != "ALIAS_B" %}
Enum value is not equal to the "ALIAS_B" string, as expected
{% else %}
Enum value should not equal the "ALIAS_B" string, but it does
{% endif %}
"#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion(
                r#"Enum value: ALIAS_B

handwritten enum values:
  - first: alpha
  - second: ALIAS_B
  - third: VALUE_C

Enum value is equal to MyEnum.VALUE_B, as expected

Enum value is not equal to MyEnum.VALUE_A, as expected

Enum value is equal to the "VALUE_B" string, as expected

Enum value is not equal to the "ALIAS_B" string, as expected
"#
                .to_string()
            )
        );

        Ok(())
    }

    #[test]
    fn test_render_prompt_with_enum_no_alias() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "enum_arg".to_string(),
            BamlValue::Enum("MyEnum".to_string(), "VALUE_A".to_string()),
        )]));

        let ir = make_test_ir(
            r#"
            enum MyEnum {
                VALUE_A
                VALUE_B
                VALUE_C
            }
            "#,
        )?;

        let rendered = render_prompt(
            "Enum value: {{ enum_arg }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("Enum value: VALUE_A".to_string())
        );

        Ok(())
    }

    // TODO -- Fix this -- in the future we should know whether the enum is being rendered in an expression or as a string and use the alias or the value.
    //
    // #[test]
    // fn test_render_prompt_with_enum_if_statement() -> anyhow::Result<()> {
    //     setup_logging();

    //     let args = BamlValue::Map(BamlMap::from([(
    //         "enum_arg".to_string(),
    //         BamlValue::Enum("MyEnum".to_string(), "VALUE_B".to_string()),
    //     )]));

    //     let ir = make_test_ir(
    //         r#"
    //         enum MyEnum {
    //             VALUE_A
    //             VALUE_B @alias("ALIAS_B")
    //             VALUE_C
    //         }
    //         "#,
    //     )?;

    //     let rendered = render_prompt(
    //         "Result: {% if enum_arg == 'VALUE_B' %}true{% else %}false{% endif %}",
    //         &args,
    //         RenderContext {
    //             client: RenderContext_Client {
    //                 name: "gpt4".to_string(),
    //                 provider: "openai".to_string(),
    //                 default_role: "system".to_string(),
    //             },
    //             output_format: OutputFormatContent::new_string(),
    //             tags: HashMap::new(),
    //         },
    //         &vec![],
    //         &ir,
    //         &HashMap::new(),
    //     )?;

    //     assert_eq!(
    //         rendered,
    //         RenderedPrompt::Completion("Result: true".to_string())
    //     );

    //     Ok(())
    // }

    #[test]
    fn render_with_truthy_test() {
        let result = render_minijinja(MinijinjaRenderParams {
            template: r#"
            {% if inp %}
            {{ inp.name }}
            {% endif %}
            "#,
            args: &minijinja::Value::from_serialize(HashMap::from([(
                "inp",
                HashMap::from([("name", "Greg")]),
            )])),
            ctx: RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
            },
            template_string_macros: &[],
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string(), "system".to_string()],
            remap_role: HashMap::new(),
            enum_values_by_name: IndexMap::new(),
        })
        .expect("Rendering should succeed");
        match result {
            RenderedPrompt::Completion(msg) => assert_eq!(msg, "Greg\n"),
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn render_prompt_with_truthy_test() {
        let ir = make_test_ir(
            r##"
        class Foo {
          name string
        }
        "##,
        )
        .unwrap();
        let template = r##"
          {% if inp %}
          {{ inp.name }}
          {% endif %}
        "##;
        let args = BamlValue::Map(
            vec![(
                "inp".to_string(),
                BamlValue::Class(
                    "Foo".to_string(),
                    vec![("name".to_string(), BamlValue::String("Greg".to_string()))]
                        .into_iter()
                        .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        );
        let ctx = RenderContext {
            client: RenderContext_Client {
                name: "gpt4".to_string(),
                provider: "openai".to_string(),
                default_role: "system".to_string(),
                allowed_roles: vec!["system".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            output_format: OutputFormatContent::new_string(),
            tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
        };
        let env_vars = HashMap::new();
        let prompt =
            render_prompt(template, &args, ctx, &[], &ir, &env_vars).expect("should render");
        match prompt {
            RenderedPrompt::Completion(msg) => {
                assert_eq!(msg, "Greg\n")
            }
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn render_with_ne_none() {
        let result = render_minijinja(MinijinjaRenderParams {
            template: r#"
            {% if inp != None %}
            {{ inp.name }}
            {% endif %}
            "#,
            args: &minijinja::Value::from_serialize(HashMap::from([(
                "inp",
                HashMap::from([("name", "Greg")]),
            )])),
            ctx: RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
            },
            template_string_macros: &[],
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string(), "system".to_string()],
            remap_role: HashMap::new(),
            enum_values_by_name: IndexMap::new(),
        })
        .expect("Rendering should succeed");
        match result {
            RenderedPrompt::Completion(msg) => assert_eq!(msg, "Greg\n"),
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn render_none_as_null() {
        let result = render_minijinja(MinijinjaRenderParams {
            template: r#"
            {% if inp is none %}
            {{ inp }}
            {% endif %}
            "#,
            args: &minijinja::Value::from_serialize(HashMap::from([("inp", ())])),
            ctx: RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
            },
            template_string_macros: &[],
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string(), "system".to_string()],
            remap_role: HashMap::new(),
            enum_values_by_name: IndexMap::new(),
        })
        .expect("Rendering should succeed");
        match result {
            RenderedPrompt::Completion(msg) => assert_eq!(msg, "null\n"),
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn render_none_as_null_nested() {
        let ir = make_test_ir(
            r##"
        class TakeNull {
          v string?
        }
        "##,
        )
        .unwrap();
        let template = r##"
          {% if t.v is none %}
          {{ t }}
          {% endif %}
        "##;
        let args = BamlValue::Map(
            [(
                "t".to_string(),
                BamlValue::Class(
                    "TakeNull".to_string(),
                    [("v".to_string(), BamlValue::Null)].into_iter().collect(),
                ),
            )]
            .into_iter()
            .collect(),
        );
        let ctx = RenderContext {
            client: RenderContext_Client {
                name: "gpt4".to_string(),
                provider: "openai".to_string(),
                default_role: "system".to_string(),
                allowed_roles: vec!["system".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            output_format: OutputFormatContent::new_string(),
            tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
        };
        let env_vars = HashMap::new();
        let prompt =
            render_prompt(template, &args, ctx, &[], &ir, &env_vars).expect("should render");
        match prompt {
            RenderedPrompt::Completion(msg) => {
                assert_eq!(
                    msg,
                    r#"{
    "v": null,
}
"#
                )
            }
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn render_none_as_null_nested_more_levels() {
        let ir = make_test_ir(
            r##"
        class TakeNull {
          v string?
          nest Nest
        }

        class Nest {
          n string?
          deeper Deeper
        }

        class Deeper {
          d int?
        }
        "##,
        )
        .unwrap();
        let template = r##"
          {% if t.v is none %}
          {{ t }}
          {% endif %}
        "##;
        let args = BamlValue::Map(
            [(
                "t".to_string(),
                BamlValue::Class(
                    "TakeNull".to_string(),
                    [
                        ("v".to_string(), BamlValue::Null),
                        (
                            "nest".to_string(),
                            BamlValue::Class(
                                "Nest".to_string(),
                                [
                                    ("n".to_string(), BamlValue::Null),
                                    (
                                        "deeper".to_string(),
                                        BamlValue::Class(
                                            "Deeper".to_string(),
                                            [("d".to_string(), BamlValue::Null)]
                                                .into_iter()
                                                .collect(),
                                        ),
                                    ),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        );
        let ctx = RenderContext {
            client: RenderContext_Client {
                name: "gpt4".to_string(),
                provider: "openai".to_string(),
                default_role: "system".to_string(),
                allowed_roles: vec!["system".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            output_format: OutputFormatContent::new_string(),
            tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
        };
        let env_vars = HashMap::new();
        let prompt =
            render_prompt(template, &args, ctx, &[], &ir, &env_vars).expect("should render");
        match prompt {
            RenderedPrompt::Completion(msg) => {
                assert_eq!(
                    msg,
                    r#"{
    "v": null,
    "nest": {
        "n": null,
        "deeper": {
            "d": null,
        },
    },
}
"#
                )
            }
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn render_none_as_null_in_list() {
        let ir = make_test_ir("").unwrap();
        let template = r##"
          {{ l }}
        "##;
        let args = BamlValue::Map(
            vec![(
                "l".to_string(),
                BamlValue::List(vec![BamlValue::Null, BamlValue::Int(2), BamlValue::Null]),
            )]
            .into_iter()
            .collect(),
        );
        let ctx = RenderContext {
            client: RenderContext_Client {
                name: "gpt4".to_string(),
                provider: "openai".to_string(),
                default_role: "system".to_string(),
                allowed_roles: vec!["system".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            output_format: OutputFormatContent::new_string(),
            tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
        };
        let env_vars = HashMap::new();
        let prompt =
            render_prompt(template, &args, ctx, &[], &ir, &env_vars).expect("should render");
        match prompt {
            RenderedPrompt::Completion(msg) => {
                assert_eq!(msg, r#"[null, 2, null]"#)
            }
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn render_none_as_null_in_list_nested_with_objects() {
        let ir = make_test_ir(
            r##"
            class TakeNull {
                v string?
                l (TakeNull | null)[]
            }
        "##,
        )
        .unwrap();
        let template = r##"
          {{ l }}
        "##;
        let args = BamlValue::Map(
            vec![(
                "l".to_string(),
                BamlValue::List(vec![
                    BamlValue::Null,
                    BamlValue::Int(2),
                    BamlValue::Class(
                        "TakeNull".to_string(),
                        vec![
                            ("v".to_string(), BamlValue::Null),
                            (
                                "l".to_string(),
                                BamlValue::List(vec![
                                    BamlValue::Null,
                                    BamlValue::Class(
                                        "TakeNull".to_string(),
                                        vec![
                                            ("v".to_string(), BamlValue::Null),
                                            (
                                                "l".to_string(),
                                                BamlValue::List(vec![
                                                    BamlValue::Null,
                                                    BamlValue::Null,
                                                ]),
                                            ),
                                        ]
                                        .into_iter()
                                        .collect(),
                                    ),
                                ]),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ]),
            )]
            .into_iter()
            .collect(),
        );
        let ctx = RenderContext {
            client: RenderContext_Client {
                name: "gpt4".to_string(),
                provider: "openai".to_string(),
                default_role: "system".to_string(),
                allowed_roles: vec!["system".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            output_format: OutputFormatContent::new_string(),
            tags: HashMap::from([("ROLE".to_string(), BamlValue::String("system".into()))]),
        };
        let env_vars = HashMap::new();
        let prompt =
            render_prompt(template, &args, ctx, &[], &ir, &env_vars).expect("should render");
        match prompt {
            RenderedPrompt::Completion(msg) => {
                assert_eq!(
                    msg,
                    r#"[null, 2, {
    "v": null,
    "l": [
        null,
        {
            "v": null,
            "l": [
                null,
                null,
            ],
        },
    ],
}]"#
                )
            }
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn test_remap_role_basic() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "subject".to_string(),
            BamlValue::String("test".to_string()),
        )]));

        let ir = make_test_ir("class C {}")?;

        let mut remap_role = HashMap::new();
        remap_role.insert("user".to_string(), "human".to_string());
        remap_role.insert("assistant".to_string(), "ai".to_string());

        let rendered = render_prompt(
            r#"
                {{ _.chat("user") }}
                Hello there!
                
                {{ _.chat("assistant") }}
                Hi back!
                
                {{ _.chat("system") }}
                System message here.
            "#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "claude".to_string(),
                    provider: "anthropic".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec![
                        "user".to_string(),
                        "assistant".to_string(),
                        "system".to_string(),
                    ],
                    remap_role,
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Chat(messages) => {
                assert_eq!(messages.len(), 3);
                assert_eq!(messages[0].role, "human"); // user -> human
                assert_eq!(messages[1].role, "ai"); // assistant -> ai
                assert_eq!(messages[2].role, "system"); // system unchanged (not in remap)
            }
            _ => panic!("Expected Chat prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_remap_role_with_default_role() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "subject".to_string(),
            BamlValue::String("test".to_string()),
        )]));

        let ir = make_test_ir("class C {}")?;

        let mut remap_role = HashMap::new();
        remap_role.insert("system".to_string(), "instructions".to_string());

        let rendered = render_prompt(
            r#"
                {{ _.chat("unknown_role") }}
                This role is not in allowed_roles, so it should use default_role
            "#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "claude".to_string(),
                    provider: "anthropic".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["user".to_string(), "system".to_string()], // unknown_role not in allowed_roles
                    remap_role,
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Chat(messages) => {
                assert_eq!(messages.len(), 1);
                // Should fall back to default_role (system) and then be remapped to "instructions"
                assert_eq!(messages[0].role, "instructions");
            }
            _ => panic!("Expected Chat prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_remap_role_with_complex_template() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([
            (
                "user_name".to_string(),
                BamlValue::String("Alice".to_string()),
            ),
            (
                "topic".to_string(),
                BamlValue::String("weather".to_string()),
            ),
        ]));

        let ir = make_test_ir("class C {}")?;

        let mut remap_role = HashMap::new();
        remap_role.insert("user".to_string(), "customer".to_string());
        remap_role.insert("assistant".to_string(), "support_agent".to_string());
        remap_role.insert("system".to_string(), "context".to_string());

        let rendered = render_prompt(
            r#"
                {{ _.chat("system") }}
                You are a helpful assistant discussing {{ topic }}.
                
                {{ _.chat("user") }}
                Hi, I'm {{ user_name }}. Can you tell me about {{ topic }}?
                
                {{ _.chat("assistant") }}
                Hello {{ user_name }}! I'd be happy to discuss {{ topic }} with you.
            "#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "claude".to_string(),
                    provider: "anthropic".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec![
                        "system".to_string(),
                        "user".to_string(),
                        "assistant".to_string(),
                    ],
                    remap_role,
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Chat(messages) => {
                assert_eq!(messages.len(), 3);
                assert_eq!(messages[0].role, "context"); // system -> context
                assert_eq!(messages[1].role, "customer"); // user -> customer
                assert_eq!(messages[2].role, "support_agent"); // assistant -> support_agent

                // Check that content is properly rendered too
                assert!(messages[0].parts[0].to_string().contains("weather"));
                assert!(messages[1].parts[0].to_string().contains("Alice"));
                assert!(messages[2].parts[0].to_string().contains("Alice"));
            }
            _ => panic!("Expected Chat prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_remap_role_with_duplicate_role() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "subject".to_string(),
            BamlValue::String("test".to_string()),
        )]));

        let ir = make_test_ir("class C {}")?;

        let mut remap_role = HashMap::new();
        remap_role.insert("user".to_string(), "participant".to_string());

        let rendered = render_prompt(
            r#"
                {{ _.chat("user") }}
                First message
                
                {{ _.chat("user", __baml_allow_dupe_role__=true) }}
                Second message from same role
            "#,
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "claude".to_string(),
                    provider: "anthropic".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["user".to_string()],
                    remap_role,
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Chat(messages) => {
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0].role, "participant"); // user -> participant
                assert_eq!(messages[1].role, "participant"); // user -> participant
                assert!(!messages[0].allow_duplicate_role);
                assert!(messages[1].allow_duplicate_role);
            }
            _ => panic!("Expected Chat prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_remap_role_completion_prompt_unchanged() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "subject".to_string(),
            BamlValue::String("test".to_string()),
        )]));

        let ir = make_test_ir("class C {}")?;

        let mut remap_role = HashMap::new();
        remap_role.insert("user".to_string(), "human".to_string());

        let rendered = render_prompt(
            "This is a completion prompt about {{ subject }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "claude".to_string(),
                    provider: "anthropic".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["user".to_string()],
                    remap_role,
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                assert_eq!(content, "This is a completion prompt about test");
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_with_toon_filter() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "user_data".to_string(),
            BamlValue::Class(
                "User".to_string(),
                BamlMap::from([
                    ("id".to_string(), BamlValue::Int(42)),
                    ("name".to_string(), BamlValue::String("Alice".to_string())),
                    (
                        "tags".to_string(),
                        BamlValue::List(vec![
                            BamlValue::String("developer".to_string()),
                            BamlValue::String("admin".to_string()),
                        ]),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class User {
                id int
                name string
                tags string[]
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ _.chat('system') }}\nHere's the user data:\n{{ user_data|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        // Verify it rendered and produced a chat message
        match rendered {
            RenderedPrompt::Chat(messages) => {
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].role, "system");

                let content = messages[0].parts[0].to_string();

                // The BAML class should have been serialized to JSON then to TOON
                // Compare against what native TOON would produce
                let json_value = serde_json::json!({
                    "id": 42,
                    "name": "Alice",
                    "tags": ["developer", "admin"]
                });
                let expected_toon = toon::encode(&json_value, None);

                // The rendered content should contain the TOON output
                assert!(content.contains(&expected_toon));
            }
            _ => panic!("Expected Chat prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_with_toon_options() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "items".to_string(),
            BamlValue::List(vec![
                BamlValue::String("apple".to_string()),
                BamlValue::String("banana".to_string()),
                BamlValue::String("cherry".to_string()),
            ]),
        )]));

        let ir = make_test_ir("")?;

        let rendered = render_prompt(
            "{{ items|format(type=\"toon\", delimiter='pipe', length_marker='#') }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // Compare against native TOON with same options
                let json_value = serde_json::json!(["apple", "banana", "cherry"]);
                let mut options = toon::EncodeOptions::default();
                options.delimiter = toon::Delimiter::Pipe;
                options.length_marker = Some('#');
                let expected = toon::encode(&json_value, Some(options));

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_unicode_length_marker() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "items".to_string(),
            BamlValue::List(vec![
                BamlValue::Class(
                    "Item".to_string(),
                    BamlMap::from([
                        ("id".to_string(), BamlValue::Int(1)),
                        ("name".to_string(), BamlValue::String("Widget".to_string())),
                    ]),
                ),
                BamlValue::Class(
                    "Item".to_string(),
                    BamlMap::from([
                        ("id".to_string(), BamlValue::Int(2)),
                        ("name".to_string(), BamlValue::String("Gadget".to_string())),
                    ]),
                ),
            ]),
        )]));

        let ir = make_test_ir(
            r#"
            class Item {
                id int
                name string
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ items|format(type=\"toon\", length_marker='🔥') }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                assert!(
                    content.contains("[🔥2]{id,name}"),
                    "expected unicode length marker prefix inside output, got: {content}"
                );
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_nested_baml_classes() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "data".to_string(),
            BamlValue::Class(
                "Outer".to_string(),
                BamlMap::from([
                    (
                        "field1".to_string(),
                        BamlValue::String("value1".to_string()),
                    ),
                    (
                        "inner".to_string(),
                        BamlValue::Class(
                            "Inner".to_string(),
                            BamlMap::from([("field2".to_string(), BamlValue::Int(123))]),
                        ),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class Outer {
                field1 string
                inner Inner
            }
            class Inner {
                field2 int
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ data|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // Compare against native TOON
                let json_value = serde_json::json!({
                    "field1": "value1",
                    "inner": {
                        "field2": 123
                    }
                });
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_enum_alias() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "status".to_string(),
            BamlValue::Enum("Status".to_string(), "Active".to_string()),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ status|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // When an enum has an alias, the toon filter should use the alias
                let json_value = serde_json::json!("active");
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_enum_in_class() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "user".to_string(),
            BamlValue::Class(
                "User".to_string(),
                BamlMap::from([
                    ("name".to_string(), BamlValue::String("Alice".to_string())),
                    (
                        "status".to_string(),
                        BamlValue::Enum("Status".to_string(), "Active".to_string()),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive") 
                Pending
            }
            class User {
                name string
                status Status
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ user|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // The enum inside the class should use its alias
                let json_value = serde_json::json!({
                    "name": "Alice",
                    "status": "active"
                });
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_class_aliases() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "person".to_string(),
            BamlValue::Class(
                "Person".to_string(),
                BamlMap::from([
                    (
                        "real_name".to_string(),
                        BamlValue::String("Alice".to_string()),
                    ),
                    ("user_age".to_string(), BamlValue::Int(30)),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class Person {
                real_name string @alias("name")
                user_age int @alias("age")
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ person|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // Class fields should use their aliases
                let json_value = serde_json::json!({
                    "name": "Alice",
                    "age": 30
                });
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_list_of_enums() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "statuses".to_string(),
            BamlValue::List(vec![
                BamlValue::Enum("Status".to_string(), "Active".to_string()),
                BamlValue::Enum("Status".to_string(), "Pending".to_string()),
                BamlValue::Enum("Status".to_string(), "Inactive".to_string()),
            ]),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ statuses|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // Enums in list should use aliases
                let json_value = serde_json::json!(["active", "Pending", "inactive"]);
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_list_of_classes() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "users".to_string(),
            BamlValue::List(vec![
                BamlValue::Class(
                    "User".to_string(),
                    BamlMap::from([
                        ("name".to_string(), BamlValue::String("Alice".to_string())),
                        (
                            "status".to_string(),
                            BamlValue::Enum("Status".to_string(), "Active".to_string()),
                        ),
                    ]),
                ),
                BamlValue::Class(
                    "User".to_string(),
                    BamlMap::from([
                        ("name".to_string(), BamlValue::String("Bob".to_string())),
                        (
                            "status".to_string(),
                            BamlValue::Enum("Status".to_string(), "Pending".to_string()),
                        ),
                    ]),
                ),
            ]),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            class User {
                name string
                status Status
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ users|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // Classes in list with enum fields using aliases
                let json_value = serde_json::json!([
                    {"name": "Alice", "status": "active"},
                    {"name": "Bob", "status": "Pending"}
                ]);
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_map_of_enums() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "status_map".to_string(),
            BamlValue::Map(BamlMap::from([
                (
                    "alice".to_string(),
                    BamlValue::Enum("Status".to_string(), "Active".to_string()),
                ),
                (
                    "bob".to_string(),
                    BamlValue::Enum("Status".to_string(), "Pending".to_string()),
                ),
                (
                    "charlie".to_string(),
                    BamlValue::Enum("Status".to_string(), "Inactive".to_string()),
                ),
            ])),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ status_map|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // Map values should use enum aliases
                let json_value = serde_json::json!({
                    "alice": "active",
                    "bob": "Pending",
                    "charlie": "inactive"
                });
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_toon_with_nested_classes_and_enums() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "organization".to_string(),
            BamlValue::Class(
                "Organization".to_string(),
                BamlMap::from([
                    (
                        "org_name".to_string(),
                        BamlValue::String("Acme Corp".to_string()),
                    ),
                    (
                        "members".to_string(),
                        BamlValue::List(vec![
                            BamlValue::Class(
                                "Member".to_string(),
                                BamlMap::from([
                                    (
                                        "user_name".to_string(),
                                        BamlValue::String("Alice".to_string()),
                                    ),
                                    (
                                        "role".to_string(),
                                        BamlValue::Enum("Role".to_string(), "Admin".to_string()),
                                    ),
                                ]),
                            ),
                            BamlValue::Class(
                                "Member".to_string(),
                                BamlMap::from([
                                    (
                                        "user_name".to_string(),
                                        BamlValue::String("Bob".to_string()),
                                    ),
                                    (
                                        "role".to_string(),
                                        BamlValue::Enum("Role".to_string(), "User".to_string()),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            enum Role {
                Admin @alias("admin")
                User @alias("user")
            }
            class Member {
                user_name string @alias("name")
                role Role
            }
            class Organization {
                org_name string @alias("name")
                members Member[]
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ organization|format(type=\"toon\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                // All aliases should be used: class field aliases and enum aliases
                let json_value = serde_json::json!({
                    "name": "Acme Corp",
                    "members": [
                        {"name": "Alice", "role": "admin"},
                        {"name": "Bob", "role": "user"}
                    ]
                });
                let expected = toon::encode(&json_value, None);

                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_basic() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "data".to_string(),
            BamlValue::Map(BamlMap::from([
                ("name".to_string(), BamlValue::String("Alice".to_string())),
                ("age".to_string(), BamlValue::Int(30)),
            ])),
        )]));

        let ir = make_test_ir("")?;

        let rendered = render_prompt(
            "{{ data|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                let expected = serde_yaml::to_string(&serde_json::json!({
                    "name": "Alice",
                    "age": 30
                }))
                .unwrap();
                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_render_json() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "data".to_string(),
            BamlValue::Map(BamlMap::from([
                ("name".to_string(), BamlValue::String("Alice".to_string())),
                ("age".to_string(), BamlValue::Int(30)),
            ])),
        )]));

        let ir = make_test_ir("")?;

        let rendered = render_prompt(
            "{{ data|format(type=\"json\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                let expected =
                    serde_json::to_string(&serde_json::json!({ "name": "Alice", "age": 30 }))
                        .unwrap();
                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_with_enum_alias() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "status".to_string(),
            BamlValue::Enum("Status".to_string(), "Active".to_string()),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ status|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                let expected = serde_yaml::to_string(&serde_json::json!("active")).unwrap();
                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_with_enum_in_class() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "user".to_string(),
            BamlValue::Class(
                "User".to_string(),
                BamlMap::from([
                    ("name".to_string(), BamlValue::String("Alice".to_string())),
                    (
                        "status".to_string(),
                        BamlValue::Enum("Status".to_string(), "Active".to_string()),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            class User {
                name string
                status Status
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ user|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                assert!(content.contains("status: active") || content.contains("status: 'active'"));
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_with_class_aliases() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "person".to_string(),
            BamlValue::Class(
                "Person".to_string(),
                BamlMap::from([
                    (
                        "real_name".to_string(),
                        BamlValue::String("Alice".to_string()),
                    ),
                    ("user_age".to_string(), BamlValue::Int(30)),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            class Person {
                real_name string @alias("name")
                user_age int @alias("age")
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ person|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                let expected = serde_yaml::to_string(&serde_json::json!({
                    "name": "Alice",
                    "age": 30
                }))
                .unwrap();
                assert_eq!(content, expected);
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_with_list_of_enums() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "statuses".to_string(),
            BamlValue::List(vec![
                BamlValue::Enum("Status".to_string(), "Active".to_string()),
                BamlValue::Enum("Status".to_string(), "Pending".to_string()),
                BamlValue::Enum("Status".to_string(), "Inactive".to_string()),
            ]),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ statuses|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                assert!(content.contains("active"));
                assert!(content.contains("Pending"));
                assert!(content.contains("inactive"));
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_with_list_of_classes() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "users".to_string(),
            BamlValue::List(vec![
                BamlValue::Class(
                    "User".to_string(),
                    BamlMap::from([
                        ("name".to_string(), BamlValue::String("Alice".to_string())),
                        (
                            "status".to_string(),
                            BamlValue::Enum("Status".to_string(), "Active".to_string()),
                        ),
                    ]),
                ),
                BamlValue::Class(
                    "User".to_string(),
                    BamlMap::from([
                        ("name".to_string(), BamlValue::String("Bob".to_string())),
                        (
                            "status".to_string(),
                            BamlValue::Enum("Status".to_string(), "Pending".to_string()),
                        ),
                    ]),
                ),
            ]),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            class User {
                name string
                status Status
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ users|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                assert!(content.contains("active"));
                assert!(content.contains("Pending"));
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_with_map_of_enums() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "status_map".to_string(),
            BamlValue::Map(BamlMap::from([
                (
                    "alice".to_string(),
                    BamlValue::Enum("Status".to_string(), "Active".to_string()),
                ),
                (
                    "bob".to_string(),
                    BamlValue::Enum("Status".to_string(), "Pending".to_string()),
                ),
                (
                    "charlie".to_string(),
                    BamlValue::Enum("Status".to_string(), "Inactive".to_string()),
                ),
            ])),
        )]));

        let ir = make_test_ir(
            r#"
            enum Status {
                Active @alias("active")
                Inactive @alias("inactive")
                Pending
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ status_map|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                assert!(content.contains("alice: active") || content.contains("alice: 'active'"));
                assert!(
                    content.contains("charlie: inactive")
                        || content.contains("charlie: 'inactive'")
                );
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }

    #[test]
    fn test_render_prompt_yaml_with_nested_aliases() -> anyhow::Result<()> {
        setup_logging();

        let args = BamlValue::Map(BamlMap::from([(
            "organization".to_string(),
            BamlValue::Class(
                "Organization".to_string(),
                BamlMap::from([
                    (
                        "org_name".to_string(),
                        BamlValue::String("Acme Corp".to_string()),
                    ),
                    (
                        "members".to_string(),
                        BamlValue::List(vec![
                            BamlValue::Class(
                                "Member".to_string(),
                                BamlMap::from([
                                    (
                                        "user_name".to_string(),
                                        BamlValue::String("Alice".to_string()),
                                    ),
                                    (
                                        "role".to_string(),
                                        BamlValue::Enum("Role".to_string(), "Admin".to_string()),
                                    ),
                                ]),
                            ),
                            BamlValue::Class(
                                "Member".to_string(),
                                BamlMap::from([
                                    (
                                        "user_name".to_string(),
                                        BamlValue::String("Bob".to_string()),
                                    ),
                                    (
                                        "role".to_string(),
                                        BamlValue::Enum("Role".to_string(), "User".to_string()),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                ]),
            ),
        )]));

        let ir = make_test_ir(
            r#"
            enum Role {
                Admin @alias("admin")
                User @alias("user")
            }
            class Member {
                user_name string @alias("name")
                role Role
            }
            class Organization {
                org_name string @alias("name")
                members Member[]
            }
            "#,
        )?;

        let rendered = render_prompt(
            "{{ organization|format(type=\"yaml\") }}",
            &args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                    default_role: "system".to_string(),
                    allowed_roles: vec!["system".to_string()],
                    remap_role: HashMap::new(),
                    options: IndexMap::new(),
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &[],
            &ir,
            &HashMap::new(),
        )?;

        match rendered {
            RenderedPrompt::Completion(content) => {
                assert!(!content.contains("org_name"));
                assert!(!content.contains("user_name"));
                assert!(content.contains("role: admin"));
                assert!(content.contains("role: user"));
            }
            _ => panic!("Expected Completion prompt"),
        }

        Ok(())
    }
}

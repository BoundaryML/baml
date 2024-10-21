use baml_types::{BamlMedia, BamlValue};
use colored::*;
mod chat_message_part;

mod output_format;
use internal_baml_core::ir::repr::IntermediateRepr;
pub use output_format::types;
mod baml_value_to_jinja_value;

use minijinja::{self, value::Kwargs};
use minijinja::{context, ErrorKind, Value};
use output_format::types::OutputFormatContent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use crate::baml_value_to_jinja_value::IntoMiniJinjaValue;
pub use crate::chat_message_part::ChatMessagePart;
use crate::output_format::OutputFormat;

fn get_env<'a>() -> minijinja::Environment<'a> {
    let mut env = minijinja::Environment::new();
    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env
}

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Serialize)]
pub struct RenderContext_Client {
    pub name: String,
    pub provider: String,
    pub default_role: String,
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

const MAGIC_CHAT_ROLE_DELIMITER: &'static str = "BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER";
const MAGIC_MEDIA_DELIMITER: &'static str = "BAML_MEDIA_MAGIC_STRING_DELIMITER";

fn render_minijinja(
    template: &str,
    args: &minijinja::Value,
    mut ctx: RenderContext,
    template_string_macros: &[TemplateStringMacro],
    default_role: String,
) -> Result<RenderedPrompt, minijinja::Error> {
    let mut env = get_env();

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
    log::debug!("Rendering template: \n{}\n------\n", template);
    // let args_dict = minijinja::Value::from_serializable(args);

    // inject macros
    let template = template_string_macros
        .into_iter()
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
    let tags = std::mem::take(&mut ctx.tags);
    let formatter = OutputFormat::new(ctx);
    env.add_global(
        "ctx",
        context! {
            client => client,
            tags => tags,
            output_format => minijinja::value::Value::from_object(formatter),
        },
    );

    let role_fn = minijinja::Value::from_function(
        |role: Option<String>, kwargs: Kwargs| -> Result<String, minijinja::Error> {
            let role = match (role, kwargs.get::<String>("role")) {
                (Some(b), Ok(a)) => {
                    // If both are present, we should error
                    return Err(minijinja::Error::new(
                        ErrorKind::TooManyArguments,
                        format!("role() called with two roles: '{}' and '{}'", a, b),
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
                    .into_iter()
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
                            format!("Media variable had unrecognizable data: {}", media_data),
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
                    role: role.as_ref().unwrap_or(&default_role).to_string(),
                    allow_duplicate_role,
                    parts,
                });
            }
        }
    }

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
            RenderedPrompt::Completion(s) => write!(f, "[{}] {}", "completion".dimmed(), s),
            RenderedPrompt::Chat(messages) => {
                write!(f, "[{}] ", "chat".dimmed())?;
                for message in messages {
                    writeln!(
                        f,
                        "{}{}",
                        format!("{}: ", message.role).on_yellow(),
                        message
                            .parts
                            .iter()
                            .map(|p| p.to_string())
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

    let minijinja_args: minijinja::Value = args.clone().into_minijinja_value(&ir, env_vars);
    let default_role = ctx.client.default_role.clone();
    let rendered = render_minijinja(
        template,
        &minijinja_args,
        ctx,
        template_string_macros,
        default_role,
    );

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

    use super::*;

    use baml_types::{BamlMap, BamlMediaType};
    use env_logger;
    use indexmap::IndexMap;
    use std::sync::Once;

    static INIT: Once = Once::new();

    pub fn setup_logging() {
        INIT.call_once(|| {
            env_logger::init();
        });
    }

    pub fn make_test_ir(source_code: &str) -> anyhow::Result<IntermediateRepr> {
        use internal_baml_core::validate;
        use internal_baml_core::{Configuration, ValidatedSchema};
        use internal_baml_diagnostics::SourceFile;
        use std::path::PathBuf;
        let path: PathBuf = "fake_file.baml".into();
        let source_file: SourceFile = (path.clone(), source_code).into();
        let validated_schema: ValidatedSchema = validate(&path, vec![source_file]);
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
                    ChatMessagePart::Media(BamlMedia::url(
                        BamlMediaType::Image,
                        "https://example.com/image.jpg".to_string(),
                        None
                    )),
                    ChatMessagePart::Text(vec![". Please help me.",].join("\n")),
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
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
                        vec![
                            "You are an assistant that always responds",
                            "in a very excited way with emojis",
                            "and also outputs this word 4 times",
                            "after giving a response: sakura",
                        ]
                        .join("\n")
                    )]
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
                    allow_duplicate_role: false,
                    parts: vec![ChatMessagePart::Text(
                        vec![
                            "Tell me a haiku about sakura. ",
                            "",
                            "End the haiku with a line about your maker, openai.",
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion(
                vec![
                    "You are an assistant that always responds",
                    "in a very excited way with emojis",
                    "and also outputs this word 4 times",
                    "after giving a response: sakura",
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
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
                        vec![
                            "You are an assistant that always responds",
                            "in a very excited way with emojis",
                            "and also outputs this word 4 times",
                            "after giving a response: sakura",
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::from([("ROLE".to_string(), BamlValue::String("john doe".into()))]),
            },
            &vec![],
            &ir,
            &HashMap::new(),
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                allow_duplicate_role: false,
                parts: vec![ChatMessagePart::Text(
                    vec![
                        "You are an assistant that always responds",
                        "in a very excited way with emojis",
                        "and also outputs this word 4 times",
                        "after giving a response: sakura",
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
    // #[test]
    // fn test_render_prompt_with_enum() -> anyhow::Result<()> {
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
    //         "Enum value: {{ enum_arg }}",
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
    //         RenderedPrompt::Completion("Enum value: ALIAS_B".to_string())
    //     );

    //     Ok(())
    // }

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
                },
                output_format: OutputFormatContent::new_string(),
                tags: HashMap::new(),
            },
            &vec![],
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
}

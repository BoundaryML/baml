use baml_types::{BamlMedia, BamlMediaType, BamlValue};
use colored::*;
mod evaluate_type;
mod get_vars;
mod output_format;
pub use output_format::types;

use evaluate_type::get_variable_types;
pub use evaluate_type::{PredefinedTypes, Type, TypeError};

use minijinja::{self, value::Kwargs};
use minijinja::{context, ErrorKind, Value};
use output_format::types::OutputFormatContent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::output_format::OutputFormat;

fn get_env<'a>() -> minijinja::Environment<'a> {
    let mut env = minijinja::Environment::new();
    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env
}

#[derive(Debug)]
pub struct ValidationError {
    pub errors: Vec<TypeError>,
    pub parsing_errors: Option<minijinja::Error>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for err in &self.errors {
            writeln!(f, "{}", err)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

pub fn validate_template(
    name: &str,
    template: &str,
    types: &mut PredefinedTypes,
) -> Result<(), ValidationError> {
    let parsed =
        match minijinja::machinery::parse(template, name, Default::default(), Default::default()) {
            Ok(parsed) => parsed,
            Err(err) => {
                return Err(ValidationError {
                    errors: vec![],
                    parsing_errors: Some(err),
                });
            }
        };

    let errs = get_variable_types(&parsed, types);

    if errs.is_empty() {
        Ok(())
    } else {
        Err(ValidationError {
            errors: errs,
            parsing_errors: None,
        })
    }
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
const MAGIC_IMAGE_DELIMITER: &'static str = "BAML_IMAGE_MAGIC_STRING_DELIMITER";

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

            Ok(format!("{MAGIC_CHAT_ROLE_DELIMITER}:baml-start-baml:{role}:baml-end-baml:{MAGIC_CHAT_ROLE_DELIMITER}"))
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

    if !rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) && !rendered.contains(MAGIC_IMAGE_DELIMITER) {
        return Ok(RenderedPrompt::Completion(rendered));
    }

    let mut chat_messages = vec![];
    let mut role = None;
    for chunk in rendered.split(MAGIC_CHAT_ROLE_DELIMITER) {
        if chunk.starts_with(":baml-start-baml:") && chunk.ends_with(":baml-end-baml:") {
            role = Some(
                chunk
                    .strip_prefix(":baml-start-baml:")
                    .unwrap_or(chunk)
                    .strip_suffix(":baml-end-baml:")
                    .unwrap_or(chunk),
            );
        } else if role.is_none() && chunk.is_empty() {
            // If there's only whitespace before the first `_.chat()` directive, we discard that chunk
        } else {
            let mut parts = vec![];
            for part in chunk.split(MAGIC_IMAGE_DELIMITER) {
                if part.starts_with(":baml-start-image:") && part.ends_with(":baml-end-image:") {
                    let image_data = part
                        .strip_prefix(":baml-start-image:")
                        .unwrap_or(part)
                        .strip_suffix(":baml-end-image:")
                        .unwrap_or(part);

                    match serde_json::from_str::<BamlMedia>(image_data) {
                        Ok(media) => match media {
                            BamlMedia::Url(media_type, _) => match media_type {
                                BamlMediaType::Image => parts.push(ChatMessagePart::Image(media)),
                                BamlMediaType::Audio => parts.push(ChatMessagePart::Audio(media)),
                            },
                            BamlMedia::Base64(media_type, _) => match media_type {
                                BamlMediaType::Image => parts.push(ChatMessagePart::Image(media)),
                                BamlMediaType::Audio => parts.push(ChatMessagePart::Audio(media)),
                            },
                        },
                        Err(_) => {
                            Err(minijinja::Error::new(
                                ErrorKind::CannotUnpack,
                                format!("Image variable had unrecognizable data: {}", image_data),
                            ))?;
                        }
                    }
                } else if !part.trim().is_empty() {
                    parts.push(ChatMessagePart::Text(part.trim().to_string()));
                }
            }

            // Only add the message if it contains meaningful content
            if !parts.is_empty() {
                chat_messages.push(RenderedChatMessage {
                    role: role.unwrap_or(&default_role).to_string(),
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

#[derive(Debug, PartialEq, Serialize, Clone)]
pub enum ChatMessagePart {
    Text(String), // raw user-provided text
    Image(BamlMedia),
    Audio(BamlMedia),
}

#[derive(Debug, PartialEq, Clone)]
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
                            .map(|p| match p {
                                ChatMessagePart::Text(t) => t.clone(),
                                ChatMessagePart::Image(media) => match media {
                                    BamlMedia::Url(BamlMediaType::Image, url) =>
                                        format!("<image_placeholder: {}>", url.url),
                                    BamlMedia::Base64(BamlMediaType::Image, _) =>
                                        "<image_placeholder base64>".to_string(),
                                    _ => unreachable!(),
                                },
                                ChatMessagePart::Audio(media) => match media {
                                    BamlMedia::Url(BamlMediaType::Audio, url) =>
                                        format!("<audio_placeholder: {}>", url.url),
                                    BamlMedia::Base64(BamlMediaType::Audio, _) =>
                                        "<audio_placeholder base64>".to_string(),
                                    _ => unreachable!(),
                                },
                            })
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
                            ChatMessagePart::Image(_) | ChatMessagePart::Audio(_) => "".to_string(),
                            // we are choosing to ignore the image for now
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
) -> anyhow::Result<RenderedPrompt> {
    if !matches!(args, BamlValue::Map(_)) {
        anyhow::bail!("args must be a map");
    }

    let minijinja_args: Value = args.clone().into();
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

    use baml_types::BamlMap;
    use env_logger;
    use std::sync::Once;

    static INIT: Once = Once::new();

    pub fn setup_logging() {
        INIT.call_once(|| {
            env_logger::init();
        });
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
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
                    ChatMessagePart::Image(BamlMedia::url(
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
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
                    ChatMessagePart::Image(BamlMedia::url(
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
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
                    ChatMessagePart::Image(BamlMedia::url(
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
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![
                RenderedChatMessage {
                    role: "system".to_string(),
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
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![
                RenderedChatMessage {
                    role: "system".to_string(),
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
                    parts: vec![ChatMessagePart::Text(
                        "Tell me a haiku about sakura.".to_string()
                    )]
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
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
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
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
}

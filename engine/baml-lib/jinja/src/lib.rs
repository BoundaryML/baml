mod baml_image;
use colored::*;
mod evaluate_type;
mod get_vars;

use evaluate_type::get_variable_types;
pub use evaluate_type::{PredefinedTypes, Type, TypeError};
use indexmap::IndexMap;
use log::info;
use minijinja::{self, value::Kwargs};
use minijinja::{context, ErrorKind, Value};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

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
}

#[derive(Clone, Debug, Serialize)]
pub struct RenderContext {
    pub client: RenderContext_Client,
    pub output_format: String,
    pub env: HashMap<String, String>,
}

pub struct TemplateStringMacro {
    pub name: String,
    pub args: Vec<(String, String)>,
    pub template: String,
}

const MAGIC_CHAT_ROLE_DELIMITER: &'static str = "BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER";
const MAGIC_IMAGE_DELIMITER: &'static str = "BAML_IMAGE_MAGIC_STRING_DELIMITER";

#[derive(Debug)]
struct OutputFormat {
    text: String,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Answer in JSON using this schema:\n{}", self.text)
    }
}

impl minijinja::value::Object for OutputFormat {
    fn call(
        &self,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        use minijinja::{
            value::{from_args, Value, ValueKind},
            Error,
        };

        let (args, kwargs): (&[Value], Kwargs) = from_args(args)?;
        if !args.is_empty() {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                format!("output_format() may only be called with named arguments"),
            ));
        }

        let Ok(prefix) = kwargs.get::<Value>("prefix") else {
            // prefix was not specified, defaults to "Use this output format:"
            return Ok(Value::from_safe_string(format!("{}", self)));
        };

        let Ok(_) = kwargs.assert_all_used() else {
            return Err(Error::new(
                ErrorKind::TooManyArguments,
                "output_format() got an unexpected keyword argument (only 'prefix' is allowed)",
            ));
        };

        match prefix.kind() {
            ValueKind::Undefined | ValueKind::None => {
                // prefix specified as none appears to result in ValueKind::Undefined
                return Ok(Value::from_safe_string(self.text.clone()));
            }
            // prefix specified as a string
            ValueKind::String => {
                return Ok(Value::from_safe_string(format!(
                    "{}\n{}",
                    prefix.to_string(),
                    self.text
                )));
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::TooManyArguments,
                    format!(
                        "output_format() expected 'prefix' to be string or none, but was type '{}'",
                        prefix.kind()
                    ),
                ));
            }
        }
    }
    fn call_method(
        &self,
        _state: &minijinja::State<'_, '_>,
        name: &str,
        _args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            ErrorKind::UnknownMethod,
            format!("output_format has no callable attribute '{}'", name),
        ))
    }
}

fn render_minijinja(
    template: &str,
    args: &minijinja::Value,
    ctx: &RenderContext,
    template_string_macros: &[TemplateStringMacro],
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
    println!("Rendering template: \n{}\n------\n", template);
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
    env.add_global(
        "ctx",
        context! {
            client => ctx.client,
            env => ctx.env,
            output_format => minijinja::value::Value::from_object(OutputFormat{ text: ctx.output_format.clone() }),
        },
    );
    env.add_global(
        "_",
        context! {
            chat => minijinja::Value::from_function(|role: Option<String>, kwargs: Kwargs| -> Result<String, minijinja::Error> {
                let role = match (role, kwargs.get::<String>("role")) {
                    (Some(b), Ok(a)) => {
                        // If both are present, we should error
                        return Err(minijinja::Error::new(
                            ErrorKind::TooManyArguments,
                            format!("chat() called with two roles: '{}' and '{}'", a, b),
                        ));
                    },
                    (Some(role), _) => role,
                    (_, Ok(role)) => role,
                    _ => {
                        // If neither are present, we should error
                        return Err(minijinja::Error::new(
                            ErrorKind::MissingArgument,
                            "chat() called without role. Try chat('role') or chat(role='role').",
                        ));
                    }
                };

                Ok(format!("{MAGIC_CHAT_ROLE_DELIMITER}:baml-start-baml:{role}:baml-end-baml:{MAGIC_CHAT_ROLE_DELIMITER}"))
            })
        },
    );
    let tmpl = env.get_template("prompt")?;

    let rendered = tmpl.render(args)?;

    if !rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) {
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

                    match serde_json::from_str::<BamlImage>(image_data) {
                        Ok(image) => {
                            parts.push(ChatMessagePart::Image(image));
                        }
                        Err(_) => {
                            Err(minijinja::Error::new(
                                ErrorKind::CannotUnpack,
                                format!("Image variable had unrecognizable data: {}", image_data),
                            ))?;
                        }
                    }
                } else if part.is_empty() {
                    // only whitespace, so discard
                } else {
                    parts.push(ChatMessagePart::Text(part.trim().to_string()));
                }
            }
            chat_messages.push(RenderedChatMessage {
                role: role.unwrap_or("system").to_string(),
                parts,
            });
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
#[serde(rename_all = "PascalCase")]
// #[serde(untagged)]
pub enum BamlImage {
    Url(ImageUrl),
    Base64(ImageBase64),
}

impl std::fmt::Display for BamlImage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{MAGIC_IMAGE_DELIMITER}:baml-start-image:{}:baml-end-image:{MAGIC_IMAGE_DELIMITER}",
            serde_json::to_string(&self).unwrap()
        )
    }
}

impl minijinja::value::Object for BamlImage {
    fn call(
        &self,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            ErrorKind::UnknownMethod,
            format!("baml image has no callable attribute '{}'", "blah"),
        ))
    }
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
}

impl ImageBase64 {
    pub fn new(base64: String) -> ImageBase64 {
        ImageBase64 { base64 }
    }
}

#[derive(Debug, PartialEq, Serialize, Clone)]
pub enum ChatMessagePart {
    Text(String), // raw user-provided text
    Image(BamlImage),
}

#[derive(Debug, PartialEq)]
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
                        message.message
                    )?;
                }
                Ok(())
            }
        }
    }
}

pub struct ChatOptions {
    default_role: String,
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
                            ChatMessagePart::Image(_) => "".to_string(), // we are choosing to ignore the image for now
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(&completion_options.joiner),
            ),
            RenderedPrompt::Completion(message) => RenderedPrompt::Completion(message),
        }
    }
}

pub fn render_prompt(
    template: &str,
    args: &minijinja::Value,
    ctx: &RenderContext,
    template_string_macros: &[TemplateStringMacro],
) -> anyhow::Result<RenderedPrompt> {
    let rendered = render_minijinja(template, args, ctx, template_string_macros);

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

#[derive(Debug, PartialEq, Serialize, Clone)]
pub enum BamlArgType {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Map(IndexMap<String, BamlArgType>),
    List(Vec<BamlArgType>),
    Image(BamlImage),
    Enum(String, String),
    Class(String, IndexMap<String, BamlArgType>),
    None,
    Unsupported(String),
}

impl From<BamlArgType> for minijinja::Value {
    fn from(arg: BamlArgType) -> minijinja::Value {
        match arg {
            BamlArgType::String(s) => minijinja::Value::from(s),
            BamlArgType::Int(n) => minijinja::Value::from(n),
            BamlArgType::Float(n) => minijinja::Value::from(n),
            BamlArgType::Bool(b) => minijinja::Value::from(b),
            BamlArgType::Map(m) => {
                let map: IndexMap<String, minijinja::Value> = m
                    .into_iter()
                    .map(|(k, v)| (k, minijinja::Value::from(v)))
                    .collect();
                minijinja::Value::from_iter(map.into_iter())
            }
            BamlArgType::List(l) => {
                let list: Vec<minijinja::Value> = l.into_iter().map(|v| v.into()).collect();
                minijinja::Value::from(list)
            }
            BamlArgType::Image(i) => minijinja::Value::from_object(i),
            BamlArgType::Enum(e, v) => minijinja::Value::from(v),
            BamlArgType::Class(c, m) => {
                let map: IndexMap<String, minijinja::Value> =
                    m.into_iter().map(|(k, v)| (k, v.into())).collect();
                minijinja::Value::from_iter(map.into_iter())
            }
            BamlArgType::None => minijinja::Value::from(()),
            BamlArgType::Unsupported(s) => minijinja::Value::from(s),
        }
    }
}
pub fn render_prompt2(
    template: &str,
    args: &BamlArgType,
    ctx: &RenderContext,
    template_string_macros: &[TemplateStringMacro],
) -> anyhow::Result<RenderedPrompt> {
    if !matches!(args, BamlArgType::Map(_)) {
        anyhow::bail!("args must be a map");
    }

    println!("\n\n BamlArgType args: {:#?}", args);

    let minijinja_args = args.clone().into();
    println!("\nminijinja_args: {:#?}", minijinja_args);

    let args = context! {
        img => minijinja::Value::from_object(BamlImage::Url(ImageUrl {
            url: "https://example.com/image.jpg".to_string(),
        })),
        hello => context! {
            img => BamlImage::Url(ImageUrl {
                url: "https://example.com/image.jpg".to_string(),
            }),
        }
    };
    println!("args: {:#?}", args);

    let rendered = render_minijinja(template, &minijinja_args, ctx, template_string_macros);

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
        let args = context! {
            img => minijinja::Value::from_object(BamlImage::Url(ImageUrl {
                url: "https://example.com/image.jpg".to_string(),
            })),
            hello => context! {
                img => BamlImage::Url(ImageUrl {
                    url: "https://example.com/image.jpg".to_string(),
                }),
            }
        };

        // let args = Arg::JsonVal(serde_json::json!({
        //     "haiku_subject": "sakura",
        //     "img": Arg::BamlImage(BamlImage::Url(ImageUrl {
        //         url: "https://example.com/image.jpg".to_string(),
        //     })),
        // }));

        let rendered = render_prompt(
            "{{ _.chat(\"system\") }}
            Here is an image: {{ img }}",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
                    ChatMessagePart::Image(BamlImage::Url(ImageUrl::new(
                        "https://example.com/image.jpg".to_string()
                    )),),
                ]
            },])
        );

        Ok(())
    }

    #[test]
    fn render_image_suffix() -> anyhow::Result<()> {
        setup_logging();
        let args = context! {
            img => minijinja::Value::from_object(BamlImage::Url(ImageUrl {
                url: "https://example.com/image.jpg".to_string(),
            })),
        };

        let rendered = render_prompt(
            "{{ _.chat(\"system\") }}
            Here is an image: {{ img }}. Please help me.",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![RenderedChatMessage {
                role: "system".to_string(),
                parts: vec![
                    ChatMessagePart::Text(vec!["Here is an image:",].join("\n")),
                    ChatMessagePart::Image(BamlImage::Url(ImageUrl::new(
                        "https://example.com/image.jpg".to_string()
                    )),),
                    ChatMessagePart::Text(vec![". Please help me.",].join("\n")),
                ]
            },])
        );

        Ok(())
    }

    #[test]
    fn render_chat() -> anyhow::Result<()> {
        setup_logging();

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            "
                    

                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}
                    
                    {{ _.chat(ctx.env.ROLE) }}
                    
                    Tell me a haiku about {{ haiku_subject }}. {{ ctx.output_format }}

                    End the haiku with a line about your maker, {{ ctx.client.provider }}.
            
            ",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
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
                            "Tell me a haiku about sakura. Answer in JSON using this schema:",
                            "iambic pentameter",
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

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            "
                You are an assistant that always responds
                in a very excited way with emojis
                and also outputs this word 4 times
                after giving a response: {{ haiku_subject }}
            ",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
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

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            "{{ ctx.output_format }}",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion(
                "Answer in JSON using this schema:\niambic pentameter".to_string()
            )
        );

        Ok(())
    }

    #[test]
    fn render_output_format_prefix_unspecified() -> anyhow::Result<()> {
        setup_logging();

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            "{{ ctx.output_format() }}",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion(
                "Answer in JSON using this schema:\niambic pentameter".to_string()
            )
        );

        Ok(())
    }

    #[test]
    fn render_output_format_prefix_null() -> anyhow::Result<()> {
        setup_logging();

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            "{{ ctx.output_format(prefix=null) }}",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("iambic pentameter".to_string())
        );

        Ok(())
    }

    #[test]
    fn render_output_format_prefix_str() -> anyhow::Result<()> {
        setup_logging();

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            "{{ ctx.output_format(prefix='custom format:') }}",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Completion("custom format:\niambic pentameter".to_string())
        );

        Ok(())
    }

    #[test]
    fn render_chat_param_failures() -> anyhow::Result<()> {
        setup_logging();

        let args = context! {
            name => "world"
        };

        // rendering should fail: template contains '{{ name }' (missing '}' at the end)
        let rendered = render_prompt(
            r#"
                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}
                    
                    {{ _.chat(role=ctx.env.ROLE) }}
                    
                    Tell me a haiku about {{ haiku_subject }} in {{ ctx.output_format }}.
                    
                    {{ _.chat(ctx.env.ROLE) }}
                    End the haiku with a line about your maker, {{ ctx.client.provider }}.

                    {{ _.chat("a", role="aa") }}
                    hi!

                    {{ _.chat() }}
                    hi!
            "#,
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        );

        match rendered {
            Ok(_) => {
                anyhow::bail!("Expected template rendering to fail, but it succeeded");
            }
            Err(e) => assert!(e
                .to_string()
                .contains("chat() called with two roles: 'aa' and 'a'")),
        }

        Ok(())
    }

    #[test]
    fn render_with_kwargs() -> anyhow::Result<()> {
        setup_logging();

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            r#"
                    

                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}
                    
                    {{ _.chat(role=ctx.env.ROLE) }}
                    
                    Tell me a haiku about {{ haiku_subject }}. {{ ctx.output_format }}
                    
                    {{ _.chat(ctx.env.ROLE) }}
                    End the haiku with a line about your maker, {{ ctx.client.provider }}.
            "#,
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            &vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![
                RenderedChatMessage {
                    role: "system".to_string(),
                    parts: vec![ChatMessagePart::Text(vec![
                        "You are an assistant that always responds",
                        "in a very excited way with emojis",
                        "and also outputs this word 4 times",
                        "after giving a response: sakura",
                    ].join("\n"))]
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
                    parts: vec![
                        ChatMessagePart::Text("Tell me a haiku about sakura. Answer in JSON using this schema:\niambic pentameter".to_string())
                    ]
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
                    parts: vec![
                        ChatMessagePart::Text("End the haiku with a line about your maker, openai.".to_string())
                    ]
                }
            ])
        );

        Ok(())
    }

    #[test]
    fn render_chat_starts_with_system() -> anyhow::Result<()> {
        setup_logging();

        let args = context! {
            haiku_subject => "sakura"
        };

        let rendered = render_prompt(
            "
                {{ _.chat(\"system\") }}

                You are an assistant that always responds
                in a very excited way with emojis
                and also outputs this word 4 times
                after giving a response: {{ haiku_subject }}
            ",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
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

        let args = context! {
            name => "world"
        };

        // rendering should fail: template contains '{{ name }' (missing '}' at the end)
        let rendered = render_prompt(
            "Hello, {{ name }!",
            &args,
            &RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_format: "output[]".to_string(),
                env: HashMap::new(),
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

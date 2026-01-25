//! Jinja runtime for BAML.
//!
//! This crate provides:
//! - [`render_prompt_vm`] - Render a Jinja template with VM values, returning [`bex_vm_types::PromptAst`]
//!
//! The VM-native types use `HeapPtr` for media references to keep values in the VM heap.

pub mod vm_value_to_jinja;

use std::collections::HashMap;
use std::sync::Arc;

// Re-export types needed for RenderContextVm
pub use bex_llm_types::ResolvedClient;
pub use bex_llm_types::output_format::OutputFormatContent;
use bex_llm_types::output_format::{MapStyle, OutputFormatOptions, render as render_output_format};
use indexmap::IndexMap;
use minijinja::value::{from_args, Kwargs, Object, Value};
use minijinja::{context, ErrorKind};
use serde::{Deserialize, Serialize};

// VM-native types
pub use vm_value_to_jinja::{HeapAccessor, IntoMiniJinjaValue as VmIntoMiniJinjaValue};
use vm_value_to_jinja::MAGIC_MEDIA_DELIMITER as VM_MAGIC_MEDIA_DELIMITER;

const MAGIC_CHAT_ROLE_DELIMITER: &str = "BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER";

// ============================================================================
// Render Context (VM-native)
// ============================================================================

/// Serializable client view exposed to Jinja templates.
#[derive(Clone, Debug, Serialize)]
pub struct LlmClientSpec {
    /// The name of the client.
    pub name: String,
    /// The provider (e.g., "openai", "anthropic").
    pub provider: String,
    /// Default role for messages without explicit role.
    pub default_role: String,
    /// Allowed roles for this client.
    pub allowed_roles: Vec<String>,
    /// Role remapping (e.g., "user" -> "human" for Anthropic).
    pub remap_role: HashMap<String, String>,
    /// Additional client options.
    pub options: IndexMap<String, serde_json::Value>,
}

impl From<&ResolvedClient> for LlmClientSpec {
    fn from(client: &ResolvedClient) -> Self {
        Self {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role().to_string(),
            allowed_roles: client.allowed_roles_or(&["system", "user", "assistant", "tool"]),
            remap_role: client.roles.remap_roles.clone(),
            options: client.options.clone(),
        }
    }
}

/// Context for rendering a prompt with VM-native types.
#[derive(Debug)]
pub struct RenderContextVm {
    /// Client configuration.
    pub client: LlmClientSpec,
    /// Tags available in the template (VM values).
    pub tags: IndexMap<String, bex_vm_types::Value>,
    /// Output format schema for the function's return type.
    pub output_format: OutputFormatContent,
}

impl Default for RenderContextVm {
    fn default() -> Self {
        Self {
            client: LlmClientSpec {
                name: "default".to_string(),
                provider: "unknown".to_string(),
                default_role: String::new(),
                allowed_roles: Vec::new(),
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            tags: IndexMap::new(),
            output_format: OutputFormatContent::empty(),
        }
    }
}

// ============================================================================
// Render Error
// ============================================================================

/// Error during template rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("Minijinja error: {0}")]
    MiniJinja(#[from] minijinja::Error),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Render error: {0}")]
    Other(String),
}

// ============================================================================
// OutputFormatValue - Jinja Object for ctx.output_format
// ============================================================================

/// Wrapper that makes OutputFormatContent callable in Jinja templates.
///
/// Supports both:
/// - `{{ctx.output_format}}` - renders with default options
/// - `{{ctx.output_format(prefix="...", ...)}}` - renders with custom options
#[derive(Debug)]
struct OutputFormatValue {
    content: Arc<OutputFormatContent>,
}

impl OutputFormatValue {
    fn new(content: OutputFormatContent) -> Self {
        Self {
            content: Arc::new(content),
        }
    }

    fn render_with_options(&self, options: &OutputFormatOptions) -> String {
        match render_output_format(&self.content, options) {
            Ok(Some(s)) => s,
            Ok(None) => String::new(),
            Err(e) => format!("<!-- output_format error: {} -->", e),
        }
    }
}

impl std::fmt::Display for OutputFormatValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Default rendering when used as {{ctx.output_format}}
        write!(
            f,
            "{}",
            self.render_with_options(&OutputFormatOptions::default())
        )
    }
}

impl Object for OutputFormatValue {
    fn call(
        self: &Arc<Self>,
        _state: &minijinja::State,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        // Extract kwargs from args - minijinja passes kwargs as the last argument
        let (_, kwargs): (&[Value], Kwargs) = from_args(args)?;
        let options = parse_output_format_kwargs(&kwargs)?;
        kwargs.assert_all_used()?;

        let rendered = self.render_with_options(&options);
        Ok(Value::from(rendered))
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        // This is called when the value is rendered directly in a template
        write!(
            f,
            "{}",
            self.render_with_options(&OutputFormatOptions::default())
        )
    }
}

/// Parse Jinja kwargs into OutputFormatOptions.
///
/// Handles the `Option<Option<T>>` pattern:
/// - Not provided -> None (use default)
/// - Explicitly null -> Some(None) (disable the option)
/// - Value provided -> Some(Some(value))
fn parse_output_format_kwargs(kwargs: &Kwargs) -> Result<OutputFormatOptions, minijinja::Error> {
    // prefix: Option<Option<String>>
    let prefix: Option<Option<String>> = match kwargs.get::<Value>("prefix") {
        Ok(v) if v.is_none() || v.is_undefined() => Some(None), // Explicitly null
        Ok(v) => Some(Some(
            v.as_str()
                .ok_or_else(|| {
                    minijinja::Error::new(
                        ErrorKind::InvalidOperation,
                        "prefix must be a string or null",
                    )
                })?
                .to_string(),
        )),
        Err(e) if matches!(e.kind(), ErrorKind::MissingArgument) => None,
        Err(e) => return Err(e),
    };

    // or_splitter: Option<String>
    let or_splitter: Option<String> = match kwargs.get::<String>("or_splitter") {
        Ok(v) => Some(v),
        Err(e) if matches!(e.kind(), ErrorKind::MissingArgument) => None,
        Err(e) => return Err(e),
    };

    // enum_value_prefix: Option<Option<String>>
    let enum_value_prefix: Option<Option<String>> = match kwargs.get::<Value>("enum_value_prefix") {
        Ok(v) if v.is_none() || v.is_undefined() => Some(None),
        Ok(v) => Some(Some(
            v.as_str()
                .ok_or_else(|| {
                    minijinja::Error::new(
                        ErrorKind::InvalidOperation,
                        "enum_value_prefix must be a string or null",
                    )
                })?
                .to_string(),
        )),
        Err(e) if matches!(e.kind(), ErrorKind::MissingArgument) => None,
        Err(e) => return Err(e),
    };

    // always_hoist_enums: Option<bool>
    let always_hoist_enums: Option<bool> = match kwargs.get::<bool>("always_hoist_enums") {
        Ok(v) => Some(v),
        Err(e) if matches!(e.kind(), ErrorKind::MissingArgument) => None,
        Err(e) => return Err(e),
    };

    // map_style: Option<MapStyle> - accepts "angle" or "object"
    let map_style: Option<MapStyle> = match kwargs.get::<String>("map_style") {
        Ok(s) => Some(s.parse().map_err(|e: String| {
            minijinja::Error::new(ErrorKind::InvalidOperation, e)
        })?),
        Err(e) if matches!(e.kind(), ErrorKind::MissingArgument) => None,
        Err(e) => return Err(e),
    };

    // hoisted_class_prefix: Option<Option<String>>
    let hoisted_class_prefix: Option<Option<String>> =
        match kwargs.get::<Value>("hoisted_class_prefix") {
            Ok(v) if v.is_none() || v.is_undefined() => Some(None),
            Ok(v) => Some(Some(
                v.as_str()
                    .ok_or_else(|| {
                        minijinja::Error::new(
                            ErrorKind::InvalidOperation,
                            "hoisted_class_prefix must be a string or null",
                        )
                    })?
                    .to_string(),
            )),
            Err(e) if matches!(e.kind(), ErrorKind::MissingArgument) => None,
            Err(e) => return Err(e),
        };

    // quote_class_fields: Option<bool>
    let quote_class_fields: Option<bool> = match kwargs.get::<bool>("quote_class_fields") {
        Ok(v) => Some(v),
        Err(e) if matches!(e.kind(), ErrorKind::MissingArgument) => None,
        Err(e) => return Err(e),
    };

    Ok(OutputFormatOptions::new(
        prefix,
        or_splitter,
        enum_value_prefix,
        always_hoist_enums,
        map_style,
        hoisted_class_prefix,
        None, // hoist_classes - complex type, skip for now
        quote_class_fields,
    ))
}

// ============================================================================
// VM-Native Render Functions
// ============================================================================

/// Render a prompt template with VM-native values.
///
/// This is the main entry point for rendering BAML prompts. It uses
/// `bex_vm_types::Value` for arguments and returns `bex_vm_types::PromptAst`.
///
/// # Arguments
/// * `template` - The Jinja template string
/// * `args` - Template arguments as VM values
/// * `heap` - VM heap accessor for dereferencing object indices
/// * `ctx` - Render context with client config, tags, and output format
pub fn render_prompt_vm(
    template: &str,
    args: &IndexMap<String, bex_vm_types::Value>,
    heap: &impl HeapAccessor,
    ctx: RenderContextVm,
) -> Result<bex_vm_types::PromptAst, RenderError> {
    let default_role = ctx.client.default_role.clone();
    let allowed_roles = if ctx.client.allowed_roles.is_empty() {
        vec![
            "system".to_string(),
            "user".to_string(),
            "assistant".to_string(),
            "tool".to_string(),
        ]
    } else {
        ctx.client.allowed_roles.clone()
    };
    let remap_role = ctx.client.remap_role.clone();

    // Convert args to minijinja values using VM-native conversion
    let args_jinja: IndexMap<&str, minijinja::Value> = args
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_minijinja_value(heap)))
        .collect();

    render_minijinja_vm(
        template,
        &args_jinja,
        heap,
        ctx,
        default_role,
        allowed_roles,
        remap_role,
    )
}

fn render_minijinja_vm(
    template: &str,
    args: &IndexMap<&str, minijinja::Value>,
    heap: &impl HeapAccessor,
    ctx: RenderContextVm,
    default_role: String,
    allowed_roles: Vec<String>,
    remap_role: HashMap<String, String>,
) -> Result<bex_vm_types::PromptAst, RenderError> {
    let mut env = minijinja::Environment::new();

    // Allow undefined variables to render as empty string instead of erroring
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);

    // Dedent the template
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
    let template = template.trim();

    env.add_template("prompt", template)?;

    // Add ctx global with output_format
    let client = ctx.client.clone();
    // Convert VM tags to minijinja values
    let tags_jinja: IndexMap<&str, minijinja::Value> = ctx
        .tags
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_minijinja_value(heap)))
        .collect();
    let output_format = Value::from_object(OutputFormatValue::new(ctx.output_format));
    env.add_global(
        "ctx",
        context! {
            client => client,
            tags => tags_jinja,
            output_format => output_format,
        },
    );

    // Add the role function for _.chat() / _.role()
    let role_fn = minijinja::Value::from_function(
        |role: Option<String>, kwargs: Kwargs| -> Result<String, minijinja::Error> {
            let role = match (role, kwargs.get::<String>("role")) {
                (Some(b), Ok(a)) => {
                    return Err(minijinja::Error::new(
                        ErrorKind::TooManyArguments,
                        format!("role() called with two roles: '{a}' and '{b}'"),
                    ));
                }
                (Some(role), _) => role,
                (_, Ok(role)) => role,
                _ => {
                    return Err(minijinja::Error::new(
                        ErrorKind::MissingArgument,
                        "role() called without role. Try role('role') or role(role='role').",
                    ));
                }
            };

            let allow_duplicate_role = match kwargs.get::<bool>("__baml_allow_dupe_role__") {
                Ok(allow) => allow,
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

            let additional_properties = serde_json::json!(additional_properties).to_string();

            Ok(format!(
                "{MAGIC_CHAT_ROLE_DELIMITER}:baml-start-baml:{additional_properties}:baml-end-baml:{MAGIC_CHAT_ROLE_DELIMITER}"
            ))
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
    let rendered = tmpl.render(minijinja::Value::from_iter(args.clone()))?;

    // If no chat delimiters, return as completion
    if !rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) && !rendered.contains(VM_MAGIC_MEDIA_DELIMITER)
    {
        return Ok(bex_vm_types::PromptAst::String(rendered));
    }

    // Parse chat messages into VM-native PromptAst
    parse_rendered_to_vm_prompt_ast(&rendered, &default_role, &allowed_roles, &remap_role)
}

/// Parse rendered template output into VM-native PromptAst.
fn parse_rendered_to_vm_prompt_ast(
    rendered: &str,
    default_role: &str,
    allowed_roles: &[String],
    remap_role: &HashMap<String, String>,
) -> Result<bex_vm_types::PromptAst, RenderError> {
    let mut chat_messages = vec![];
    let mut role: Option<String> = None;

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
                    role = Some(role_val.as_str().unwrap_or("").to_string());
                }
                // Note: __baml_allow_dupe_role__ and other metadata are parsed here
                // but not currently stored in the VM PromptAst. They could be handled by
                // specialize_prompt at a higher level if needed.
            }
        } else if role.is_none() && chunk.is_empty() {
            // Discard whitespace before first _.chat()
        } else {
            let mut parts = vec![];

            for part in chunk.split(VM_MAGIC_MEDIA_DELIMITER) {
                let part = if part.starts_with(":baml-start-media:")
                    && part.ends_with(":baml-end-media:")
                {
                    let media_data = part
                        .strip_prefix(":baml-start-media:")
                        .unwrap_or(part)
                        .strip_suffix(":baml-end-media:")
                        .unwrap_or(part);

                    // Parse media data to extract HeapPtr
                    // Note: This uses unsafe code because we're reconstructing a pointer
                    // from a serialized address. This is only valid during the same execution
                    // where the media was originally rendered.
                    match serde_json::from_str::<serde_json::Value>(media_data) {
                        Ok(json) => {
                            if let Some(ptr_addr) = json.get("heap_ptr").and_then(|v| v.as_u64()) {
                                // SAFETY: The pointer was serialized from a valid HeapPtr
                                // during the same template rendering session. The heap
                                // and its objects are still valid at this point.
                                let heap_ptr = unsafe {
                                    bex_vm_types::HeapPtr::from_ptr(ptr_addr as *mut bex_vm_types::Object)
                                };
                                Some(bex_vm_types::PromptAst::Media(heap_ptr))
                            } else {
                                return Err(RenderError::Other(format!(
                                    "Media missing heap_ptr: {media_data}"
                                )));
                            }
                        }
                        Err(_) => {
                            return Err(RenderError::Other(format!(
                                "Media variable had unrecognizable data: {media_data}"
                            )));
                        }
                    }
                } else if !part.trim().is_empty() {
                    Some(bex_vm_types::PromptAst::String(part.trim().to_string()))
                } else {
                    None
                };

                if let Some(part) = part {
                    parts.push(part);
                }
            }

            if !parts.is_empty() {
                let content = if parts.len() == 1 {
                    parts.pop().unwrap()
                } else {
                    bex_vm_types::PromptAst::Vec(parts)
                };

                // Metadata is stored as Null for now - heap allocation would be needed
                // for proper Map storage. Metadata flags like __baml_allow_dupe_role__
                // are handled at a higher level (specialize_prompt).
                let metadata = bex_vm_types::Value::Null;

                let final_role = match role.as_ref() {
                    Some(r) if allowed_roles.contains(r) => r.clone(),
                    Some(_) => default_role.to_string(),
                    None => default_role.to_string(),
                };

                // Apply role remapping
                let final_role = remap_role.get(&final_role).cloned().unwrap_or(final_role);

                chat_messages.push(bex_vm_types::PromptAst::Message {
                    role: final_role,
                    content: Box::new(content),
                    metadata,
                });
            }
        }
    }

    Ok(bex_vm_types::PromptAst::Vec(chat_messages))
}

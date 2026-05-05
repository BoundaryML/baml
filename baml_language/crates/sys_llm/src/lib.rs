//! LLM operations, prompt specialization, and template rendering.
//!
//! This crate consolidates all LLM-related functionality:
//! - `types` - Error types and output format schema types
//! - `jinja` - Jinja template rendering for BAML prompts
//! - `specialize_prompt()` - Transform a generic `PromptAst` for a specific LLM provider
//! - `execute_*` entry points for trait-based dispatch from `sys_types`

mod auth_request;
pub mod baml_std;
mod build_request;
pub(crate) mod jinja;
mod model_features;
pub(crate) mod parse_response;
mod provider;
mod render_prompt;
pub(crate) mod resolve_media;
mod specialize_prompt;
pub mod stream_accumulator;
pub(crate) mod types;
#[cfg(target_arch = "wasm32")]
pub(crate) mod wasm;

use std::{str::FromStr, sync::Arc};

use bex_external_types::BexExternalValue;
// Used by bex_engine tests
pub use jinja::{
    OutputFormatContent, RenderContext, RenderContextClient, RenderEnum, RenderEnumVariant,
    RenderPromptError, preprocess_template, render_prompt,
};
// --- Crate-internal re-exports (used by submodules via `crate::`) ---
pub(crate) use model_features::{AllowedMetadata, ModelFeatures};
// Used by sys_types (From<LlmOpError> for OpErrorKind)
pub use provider::LlmProvider;
pub use types::LlmOpError;
// --- Public API: only what sys_types and bex_engine tests actually use ---
pub use types::SapStreamCache;

#[cfg(all(not(target_arch = "wasm32"), feature = "ring-crypto"))]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(feature = "ring-crypto"),
    feature = "aws-crypto"
))]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(any(
    target_arch = "wasm32",
    all(not(feature = "ring-crypto"), not(feature = "aws-crypto"))
))]
pub(crate) fn ensure_rustls_crypto_provider() {}

// ============================================================================
// Clean (owned-type) entry points for trait-based dispatch
// ============================================================================

/// Render a Jinja template given already-extracted owned types.
///
/// `args` is expected to be `BexExternalValue::Map { entries, .. }`.
pub fn execute_render_prompt_from_owned(
    client: &baml_std::PrimitiveClient,
    template: &str,
    args: &BexExternalValue,
    return_type: &baml_type::Ty,
    ctx: &::sys_types::SysOpContext,
) -> Result<bex_vm_types::PromptAst, LlmOpError> {
    let BexExternalValue::Map {
        entries: template_args,
        ..
    } = args
    else {
        return Err(LlmOpError::TypeError {
            expected: "map",
            actual: args.type_name().to_string(),
        });
    };

    let output_format = build_output_format_content(return_type, ctx);

    let render_ctx = jinja::RenderContext {
        client: jinja::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles.clone(),
        },
        output_format,
        tags: indexmap::IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let prompt_ast = jinja::render_prompt(template, template_args, &render_ctx)
        .map_err(|e| LlmOpError::RenderPrompt(e.to_string()))?;
    Ok(std::sync::Arc::new(prompt_ast))
}

/// Build an `OutputFormatContent` by walking a `Ty` and collecting all
/// referenced class/enum/type-alias definitions from `SysOpContext`.
fn build_output_format_content(
    ty: &baml_type::Ty,
    ctx: &::sys_types::SysOpContext,
) -> types::OutputFormatContent {
    use std::collections::HashSet;

    let mut content = types::OutputFormatContent::new(ty.clone());
    let mut visited = HashSet::new();
    let mut ancestry = Vec::new();

    walk_ty(ty, ctx, &mut content, &mut visited, &mut ancestry);

    content
}

/// Recursive DFS walk of a type tree. `ancestry` tracks the class names
/// currently on the call stack so mutual recursion (A → B → A) is detected.
fn walk_ty(
    ty: &baml_type::Ty,
    ctx: &::sys_types::SysOpContext,
    content: &mut types::OutputFormatContent,
    visited: &mut std::collections::HashSet<baml_base::Name>,
    ancestry: &mut Vec<baml_base::Name>,
) {
    use baml_type::Ty;

    match ty {
        Ty::Class(type_name, _) => {
            let key = type_name.display_name.clone();

            // If this class is already on the ancestry stack, it's a recursive cycle.
            // Only mark classes from the cycle start, not unrelated ancestors.
            if let Some(start) = ancestry.iter().position(|name| name == &key) {
                for name in &ancestry[start..] {
                    content.recursive_classes.insert(name.to_string());
                }
                return;
            }

            if !visited.insert(key.clone()) {
                return;
            }

            if let Some(class_def) = ctx.class_definitions.get(type_name) {
                let fields: Vec<types::ClassField> = class_def
                    .fields
                    .iter()
                    .filter(|f| !f.skip)
                    .map(|f| types::ClassField {
                        name: f.name.clone(),
                        alias: f.alias.clone(),
                        field_type: f.field_type.clone(),
                        description: f.description.clone(),
                    })
                    .collect();

                content.classes.insert(
                    class_def.name.clone(),
                    types::Class {
                        name: class_def.name.clone(),
                        alias: class_def.alias.clone(),
                        description: class_def.description.clone(),
                        fields,
                    },
                );

                // Push onto ancestry before recursing into fields
                ancestry.push(key);
                for field_def in &class_def.fields {
                    if !field_def.skip {
                        walk_ty(&field_def.field_type, ctx, content, visited, ancestry);
                    }
                }
                ancestry.pop();
            }
        }
        Ty::Enum(type_name, _) => {
            let key = type_name.display_name.clone();
            if !visited.insert(key) {
                return;
            }
            if let Some(enum_def) = ctx.enum_definitions.get(type_name) {
                // Skipped variants are already filtered out in bex_engine extraction.
                let values: Vec<types::EnumValue> = enum_def
                    .variants
                    .iter()
                    .map(|v| types::EnumValue {
                        name: v.name.clone(),
                        alias: v.alias.clone(),
                        description: v.description.clone(),
                    })
                    .collect();

                content.enums.insert(
                    enum_def.name.clone(),
                    types::Enum {
                        name: enum_def.name.clone(),
                        alias: enum_def.alias.clone(),
                        description: enum_def.description.clone(),
                        values,
                    },
                );
            }
        }
        Ty::TypeAlias(type_name, _) => {
            let key = type_name.display_name.clone();
            if !visited.insert(key) {
                return;
            }
            if let Some(target_ty) = ctx.type_alias_definitions.get(type_name) {
                content
                    .recursive_type_aliases
                    .insert(type_name.display_name.to_string(), target_ty.clone());
                walk_ty(target_ty, ctx, content, visited, ancestry);
            }
        }
        Ty::Optional(inner, _) | Ty::List(inner, _) => {
            walk_ty(inner, ctx, content, visited, ancestry);
        }
        Ty::Map { key, value, .. } => {
            walk_ty(key, ctx, content, visited, ancestry);
            walk_ty(value, ctx, content, visited, ancestry);
        }
        Ty::Union(members, _) => {
            for member in members {
                walk_ty(member, ctx, content, visited, ancestry);
            }
        }
        _ => {}
    }
}

/// Specialize a prompt for a provider given already-extracted owned types.
pub fn execute_specialize_prompt_from_owned(
    client: &baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<bex_vm_types::PromptAst, LlmOpError> {
    specialize_prompt::specialize_prompt_from_owned(client, prompt)
        .map_err(|e| LlmOpError::Other(e.to_string()))
}

/// Build an HTTP request from a prompt given already-extracted owned types.
///
/// `io` provides typed async IO operations for auth steps that need HTTP, env,
/// or filesystem access (e.g. Bedrock `SigV4` credential resolution, Vertex AI
/// service account token exchange).
pub async fn execute_build_request_from_owned(
    client: &baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
) -> Result<baml_std::HttpRequest, LlmOpError> {
    build_request::build_request(client, prompt, io)
        .await
        .map_err(|e| LlmOpError::Other(e.to_string()))
}

/// Build an HTTP request with streaming enabled.
///
/// Same as `execute_build_request_from_owned` but adds `"stream": true` to the body.
pub async fn execute_build_request_stream_from_owned(
    client: &baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
) -> Result<baml_std::HttpRequest, LlmOpError> {
    let mut request = execute_build_request_from_owned(client, prompt, io).await?;
    request.body = add_stream_flag_to_request_body(&request.body)?;
    Ok(request)
}

fn add_stream_flag_to_request_body(body: &str) -> Result<String, LlmOpError> {
    let mut body: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| LlmOpError::Other(format!("Failed to parse request body: {e}")))?;
    let obj = body.as_object_mut().ok_or_else(|| {
        LlmOpError::Other("Request body must be a JSON object to enable streaming".into())
    })?;
    obj.insert("stream".to_string(), serde_json::Value::Bool(true));
    serde_json::to_string(&body)
        .map_err(|e| LlmOpError::Other(format!("Failed to serialize request body: {e}")))
}

/// Validate a finish reason against the client's allow/deny policy.
pub fn execute_validate_finish_reason(
    client: &baml_std::PrimitiveClient,
    finish_reason: &str,
) -> Result<(), LlmOpError> {
    let finish_reason = if finish_reason.is_empty() {
        None
    } else {
        Some(finish_reason)
    };

    if client.is_finish_reason_allowed(finish_reason) {
        return Ok(());
    }

    Err(LlmOpError::ParseResponseError(format!(
        "Finish reason not allowed: {}",
        finish_reason.unwrap_or("unknown")
    )))
}

/// Parse a full LLM response and extract the return value given already-extracted owned types.
pub fn execute_parse_response_from_owned(
    client: &baml_std::PrimitiveClient,
    response: &str,
    return_type: &baml_type::Ty,
    ctx: &::sys_types::SysOpContext,
) -> Result<bex_external_types::BexExternalValue, LlmOpError> {
    let mut provider = LlmProvider::from_str(&client.provider)
        .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?;

    // Vertex AI + Anthropic model uses rawPredict, which returns Anthropic-format responses.
    if provider == LlmProvider::VertexAi && build_request::is_anthropic_model(&client.model) {
        provider = LlmProvider::Anthropic;
    }

    let response = parse_response::parse_response(provider, response)
        .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?;

    // Normalize empty finish_reason to None so oneshot and streaming paths agree
    // (matches `execute_validate_finish_reason`'s behavior above).
    let finish_reason = response
        .finish_reason_raw
        .as_deref()
        .filter(|s| !s.is_empty());
    if !client.is_finish_reason_allowed(finish_reason) {
        return Err(LlmOpError::ParseResponseError(format!(
            "Finish reason not allowed: {}",
            finish_reason.unwrap_or("unknown")
        )));
    }

    let compiled = bex_sap::CompiledSapModel::from_sys_op_context(
        ctx,
        return_type.clone(),
        baml_type::Ty::null(), // no streaming
    )
    .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?;
    let sap = SapStreamCache::new(compiled);
    execute_sap_parse_final(&response.content, &sap, ctx)
}

pub fn execute_sap_parse_final(
    json: &str,
    sap: &SapStreamCache,
    _ctx: &::sys_types::SysOpContext,
) -> Result<bex_external_types::BexExternalValue, LlmOpError> {
    // === Jsonish ===
    let jsonish_options = ::bex_sap::jsonish::ParseOptions::default();
    let jsonish =
        ::bex_sap::jsonish::parse(json, jsonish_options, true).map_err(LlmOpError::JsonishError)?;

    let parse_ctx = ::bex_sap::deserializer::coercer::ParsingContext::new(sap.db());
    let target = sap
        .ty_resolved()
        .map_err(|err| parse_ctx.error_type_resolution(err))
        .map_err(LlmOpError::SapError)?;
    let parsed = ::bex_sap::sap_model::TyResolvedRef::coerce(&parse_ctx, target, &jsonish)
        .map_err(LlmOpError::SapError)?
        .ok_or_else(|| {
            LlmOpError::ParseResponseError("SAP parse returned no value when complete".to_string())
        })?;

    // === Convert back to baml ===
    Ok(::bex_sap::to_external::baml_value_to_external(&parsed))
}

pub fn execute_sap_parse_partial(
    json: &str,
    sap: &SapStreamCache,
    _ctx: &::sys_types::SysOpContext,
) -> Result<Option<bex_external_types::BexExternalValue>, LlmOpError> {
    // === Jsonish ===
    let jsonish_options = ::bex_sap::jsonish::ParseOptions::default();
    let jsonish = ::bex_sap::jsonish::parse(json, jsonish_options, false)
        .map_err(LlmOpError::JsonishError)?;

    // === SAP parsing (use the streaming type for partial results) ===
    let parse_ctx = ::bex_sap::deserializer::coercer::ParsingContext::new(sap.db());
    let target = sap
        .stream_ty_resolved()
        .map_err(|err| parse_ctx.error_type_resolution(err))
        .map_err(LlmOpError::SapError)?;
    let parsed = ::bex_sap::sap_model::TyResolvedRef::coerce(&parse_ctx, target, &jsonish)
        .map_err(LlmOpError::SapError)?;
    // === Convert back to baml ===
    match parsed {
        Some(parsed) => {
            let converted = ::bex_sap::to_external::baml_value_to_external(&parsed);
            Ok(Some(converted))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ::baml_base::TyAttr;
    use ::sys_types::{ClassDefinition, ClassFieldDefinition, EnumDefinition, SysOpContext};

    use super::{build_output_format_content, execute_parse_response_from_owned};
    use crate::baml_std;

    fn make_client_with_options(
        mut options: baml_std::PrimitiveClientOptions,
    ) -> baml_std::PrimitiveClient {
        if options.model.is_none() {
            options.model = Some("test-model".to_string());
        }
        baml_std::PrimitiveClient::new("TestClient".to_string(), "openai".to_string(), options)
            .unwrap()
    }

    #[test]
    fn parse_respects_finish_reason_filters() {
        let response_stop = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }]
        }"#;
        let response_length = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "truncated" },
                "finish_reason": "length"
            }]
        }"#;

        let allow_client = make_client_with_options(baml_std::PrimitiveClientOptions {
            finish_reason_allow_list: Some(vec!["stop".to_string()]),
            ..Default::default()
        });

        let ctx = SysOpContext::empty();

        // "stop" is allowed.
        let allowed = execute_parse_response_from_owned(
            &allow_client,
            response_stop,
            &::baml_type::Ty::String {
                attr: TyAttr::default(),
            },
            &ctx,
        );
        assert!(allowed.is_ok());

        // "length" is rejected.
        let blocked = execute_parse_response_from_owned(
            &allow_client,
            response_length,
            &::baml_type::Ty::String {
                attr: TyAttr::default(),
            },
            &ctx,
        );
        assert!(blocked.is_err());

        let deny_client = make_client_with_options(baml_std::PrimitiveClientOptions {
            finish_reason_deny_list: Some(vec!["length".to_string()]),
            ..Default::default()
        });

        // "length" is rejected by deny list.
        let denied = execute_parse_response_from_owned(
            &deny_client,
            response_length,
            &::baml_type::Ty::String {
                attr: TyAttr::default(),
            },
            &ctx,
        );
        assert!(denied.is_err());
    }

    // ========================================================================
    // Vertex + Anthropic response routing
    // ========================================================================

    #[test]
    fn vertex_claude_parses_anthropic_response() {
        let client = baml_std::PrimitiveClient::new(
            "test".to_string(),
            "vertex-ai".to_string(),
            baml_std::PrimitiveClientOptions {
                model: Some("claude-sonnet-4-20250514".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let anthropic_response = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello from Claude on Vertex"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 5, "output_tokens": 6}
        }"#;

        let ctx = SysOpContext::empty();
        let result = execute_parse_response_from_owned(
            &client,
            anthropic_response,
            &::baml_type::Ty::String {
                attr: TyAttr::default(),
            },
            &ctx,
        );
        assert!(
            result.is_ok(),
            "should parse Anthropic response: {result:?}"
        );
    }

    // ========================================================================
    // build_output_format_content / walk_ty tests
    // ========================================================================

    fn ty_class(name: &str) -> baml_type::Ty {
        baml_type::Ty::Class(baml_type::TypeName::local(name.into()), TyAttr::default())
    }
    fn ty_enum(name: &str) -> baml_type::Ty {
        baml_type::Ty::Enum(baml_type::TypeName::local(name.into()), TyAttr::default())
    }
    fn ty_string() -> baml_type::Ty {
        baml_type::Ty::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_optional(inner: baml_type::Ty) -> baml_type::Ty {
        baml_type::Ty::Optional(Box::new(inner), TyAttr::default())
    }
    fn tn(name: &str) -> baml_type::TypeName {
        baml_type::TypeName::local(name.into())
    }

    fn ctx_with(
        classes: Vec<(baml_type::TypeName, ClassDefinition)>,
        enums: Vec<(baml_type::TypeName, EnumDefinition)>,
    ) -> SysOpContext {
        let mut ctx = SysOpContext::empty();
        ctx.class_definitions = Arc::new(classes.into_iter().collect());
        ctx.enum_definitions = Arc::new(enums.into_iter().collect());
        ctx
    }

    #[test]
    fn walk_simple_class() {
        let ctx = ctx_with(
            vec![(
                tn("User"),
                ClassDefinition {
                    name: "User".into(),
                    description: Some("A user".into()),
                    alias: None,
                    fields: vec![ClassFieldDefinition {
                        name: "name".into(),
                        field_type: ty_string(),
                        description: Some("Full name".into()),
                        alias: None,
                        skip: false,
                    }],
                },
            )],
            vec![],
        );
        let content = build_output_format_content(&ty_class("User"), &ctx);
        assert!(content.classes.contains_key("User"));
        assert_eq!(
            content.classes["User"].description.as_deref(),
            Some("A user")
        );
        assert_eq!(content.classes["User"].fields.len(), 1);
        assert_eq!(
            content.classes["User"].fields[0].description.as_deref(),
            Some("Full name")
        );
        assert!(content.recursive_classes.is_empty());
    }

    #[test]
    fn walk_skips_skip_fields() {
        let ctx = ctx_with(
            vec![(
                tn("Foo"),
                ClassDefinition {
                    name: "Foo".into(),
                    description: None,
                    alias: None,
                    fields: vec![
                        ClassFieldDefinition {
                            name: "keep".into(),
                            field_type: ty_string(),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                        ClassFieldDefinition {
                            name: "hidden".into(),
                            field_type: ty_string(),
                            description: None,
                            alias: None,
                            skip: true,
                        },
                    ],
                },
            )],
            vec![],
        );
        let content = build_output_format_content(&ty_class("Foo"), &ctx);
        assert_eq!(content.classes["Foo"].fields.len(), 1);
        assert_eq!(content.classes["Foo"].fields[0].name, "keep");
    }

    #[test]
    fn walk_collects_enum() {
        let ctx = ctx_with(
            vec![],
            vec![(
                tn("Color"),
                EnumDefinition {
                    name: "Color".into(),
                    description: Some("A color".into()),
                    alias: None,
                    variants: vec![
                        ::sys_types::EnumVariantDefinition {
                            name: "Red".into(),
                            description: None,
                            alias: None,
                        },
                        ::sys_types::EnumVariantDefinition {
                            name: "Blue".into(),
                            description: None,
                            alias: None,
                        },
                    ],
                },
            )],
        );
        let content = build_output_format_content(&ty_enum("Color"), &ctx);
        assert!(content.enums.contains_key("Color"));
        assert_eq!(content.enums["Color"].values.len(), 2);
        assert_eq!(
            content.enums["Color"].description.as_deref(),
            Some("A color")
        );
    }

    #[test]
    fn walk_direct_self_recursion() {
        // Node { next: Node? }
        let ctx = ctx_with(
            vec![(
                tn("Node"),
                ClassDefinition {
                    name: "Node".into(),
                    description: None,
                    alias: None,
                    fields: vec![ClassFieldDefinition {
                        name: "next".into(),
                        field_type: ty_optional(ty_class("Node")),
                        description: None,
                        alias: None,
                        skip: false,
                    }],
                },
            )],
            vec![],
        );
        let content = build_output_format_content(&ty_class("Node"), &ctx);
        assert!(
            content.recursive_classes.contains("Node"),
            "Node should be marked recursive"
        );
    }

    #[test]
    fn walk_mutual_recursion() {
        // A { b: B }, B { a: A? }
        let ctx = ctx_with(
            vec![
                (
                    tn("A"),
                    ClassDefinition {
                        name: "A".into(),
                        description: None,
                        alias: None,
                        fields: vec![ClassFieldDefinition {
                            name: "b".into(),
                            field_type: ty_class("B"),
                            description: None,
                            alias: None,
                            skip: false,
                        }],
                    },
                ),
                (
                    tn("B"),
                    ClassDefinition {
                        name: "B".into(),
                        description: None,
                        alias: None,
                        fields: vec![ClassFieldDefinition {
                            name: "a".into(),
                            field_type: ty_optional(ty_class("A")),
                            description: None,
                            alias: None,
                            skip: false,
                        }],
                    },
                ),
            ],
            vec![],
        );
        let content = build_output_format_content(&ty_class("A"), &ctx);
        assert!(content.recursive_classes.contains("A"), "A in cycle");
        assert!(content.recursive_classes.contains("B"), "B in cycle");
    }

    #[test]
    fn walk_non_recursive_wrapper_around_recursive_child() {
        // Wrapper { node: Node }, Node { next: Node? }
        // Only Node should be recursive, not Wrapper.
        let ctx = ctx_with(
            vec![
                (
                    tn("Wrapper"),
                    ClassDefinition {
                        name: "Wrapper".into(),
                        description: None,
                        alias: None,
                        fields: vec![ClassFieldDefinition {
                            name: "node".into(),
                            field_type: ty_class("Node"),
                            description: None,
                            alias: None,
                            skip: false,
                        }],
                    },
                ),
                (
                    tn("Node"),
                    ClassDefinition {
                        name: "Node".into(),
                        description: None,
                        alias: None,
                        fields: vec![ClassFieldDefinition {
                            name: "next".into(),
                            field_type: ty_optional(ty_class("Node")),
                            description: None,
                            alias: None,
                            skip: false,
                        }],
                    },
                ),
            ],
            vec![],
        );
        let content = build_output_format_content(&ty_class("Wrapper"), &ctx);
        assert!(
            content.recursive_classes.contains("Node"),
            "Node should be recursive"
        );
        assert!(
            !content.recursive_classes.contains("Wrapper"),
            "Wrapper should NOT be recursive"
        );
        // Both classes should be collected
        assert!(content.classes.contains_key("Wrapper"));
        assert!(content.classes.contains_key("Node"));
    }

    #[test]
    fn walk_missing_class_definition() {
        // Reference to a class not in ctx — should not panic, just skip
        let ctx = ctx_with(vec![], vec![]);
        let content = build_output_format_content(&ty_class("Missing"), &ctx);
        assert!(content.classes.is_empty());
        assert!(content.recursive_classes.is_empty());
    }
}

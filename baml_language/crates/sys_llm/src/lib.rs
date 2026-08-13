//! LLM operations, prompt specialization, and template rendering.
//!
//! This crate consolidates all LLM-related functionality:
//! - `types` - Error types and output format schema types
//! - `specialize_prompt()` - Transform a generic `PromptAst` for a specific LLM provider
//! - `execute_*` entry points for trait-based dispatch from `sys_types`

mod auth_request;
pub mod baml_std;
mod build_request;
mod model_features;
pub(crate) mod parse_response;
mod provider;
pub(crate) mod resolve_media;
mod specialize_prompt;
pub mod stream_accumulator;
pub(crate) mod types;

use std::{str::FromStr, sync::Arc};

use bex_external_types::BexExternalValue;
// --- Crate-internal re-exports (used by submodules via `crate::`) ---
pub(crate) use model_features::{AllowedMetadata, ModelFeatures};
// Used by sys_types (From<LlmOpError> for VmBamlError)
pub use provider::LlmProvider;
// --- Public API: only what sys_types and bex_engine tests actually use ---
pub use types::SapParseCache;
pub use types::{LlmOpError, OutputFormatContent};

// Selects the rustls crypto provider for the crate. No longer called directly
// now that all HTTPS flows through `RuntimeIo` (sys_native installs its own
// provider), but the `aws-crypto`/`ring-crypto` features are still part of the
// workspace-wide feature chain (e.g. `sys_ops/aws-crypto` forwards here), so
// the helper and its `rustls` dep are retained.
#[cfg(all(not(target_arch = "wasm32"), feature = "ring-crypto"))]
#[allow(dead_code)]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(feature = "ring-crypto"),
    feature = "aws-crypto"
))]
#[allow(dead_code)]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(feature = "ring-crypto"),
    not(feature = "aws-crypto")
))]
#[allow(dead_code)]
pub(crate) fn ensure_rustls_crypto_provider() {}

// ============================================================================
// Clean (owned-type) entry points for trait-based dispatch
// ============================================================================

/// Render `return_type`'s schema with default options for
/// `ctx.output_format`. Build the `OutputFormatContent`, then render with
/// `RenderOptions::default()`. An empty or `None`
/// render (e.g. a primitive return type with no schema) becomes the empty string.
pub fn render_output_format(
    return_type: &baml_type::RuntimeTy,
    ctx: &::sys_types::SysOpContext,
) -> String {
    build_output_format_content(return_type, ctx)
        .render(&types::RenderOptions::default())
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Render a prebuilt [`OutputFormatContent`] with caller-supplied options.
/// This backs the parameterized `ctx.output_format_with(...)` accessor. The content is
/// carried as an opaque handle on `Context` (built once by
/// [`build_output_format_content`]); this only re-renders. The option mapping
/// (`RenderSetting`/`RenderOptions`) stays crate-internal: a value overrides
/// that aspect (`Always`), `None` keeps the default (`Auto`).
// Options arrive by value from the sys-op glue; most are moved into
// `RenderOptions`, `map_style` is only read — taking it by ref too would just
// shift the clone to the caller.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn render_output_format_content(
    content: &types::OutputFormatContent,
    prefix: Option<String>,
    or_splitter: Option<String>,
    enum_value_prefix: Option<String>,
    hoisted_class_prefix: Option<String>,
    always_hoist_enums: Option<bool>,
    quote_class_fields: Option<bool>,
    hoist_classes: Option<Vec<String>>,
    map_style: Option<String>,
    render_null_as: Option<String>,
) -> String {
    use types::{HoistClasses, MapStyle, RenderOptions, RenderSetting};
    fn setting<V>(o: Option<V>) -> RenderSetting<V> {
        o.map_or(RenderSetting::Auto, RenderSetting::Always)
    }
    let options = RenderOptions {
        prefix: setting(prefix),
        or_splitter: setting(or_splitter),
        enum_value_prefix: setting(enum_value_prefix),
        hoisted_class_prefix: setting(hoisted_class_prefix),
        always_hoist_enums: setting(always_hoist_enums),
        quote_class_fields: setting(quote_class_fields),
        hoist_classes: hoist_classes.map_or(HoistClasses::Auto, HoistClasses::Subset),
        map_style: match map_style.as_deref() {
            // `type_parameters` is the opt-in escape hatch (`map<K, V>`); every
            // other value (including the default) renders the JSON object shape.
            Some("type_parameters") => MapStyle::TypeParameters,
            _ => MapStyle::ObjectLiteral,
        },
        render_null_as: setting(render_null_as),
    };
    content.render(&options).ok().flatten().unwrap_or_default()
}

/// Build an `OutputFormatContent` by walking a `RuntimeTy` and collecting all
/// referenced class/enum/type-alias definitions from `SysOpContext`.
pub fn build_output_format_content(
    ty: &baml_type::RuntimeTy,
    ctx: &::sys_types::SysOpContext,
) -> types::OutputFormatContent {
    use std::collections::HashSet;

    let mut content = types::OutputFormatContent::new(ty.clone());
    let mut visited = HashSet::new();
    let mut ancestry = Vec::new();

    walk_ty(ty, ctx, &mut content, &mut visited, &mut ancestry);

    content
}

fn find_class_definition<'a>(
    ctx: &'a ::sys_types::SysOpContext,
    type_name: &baml_type::TypeName,
) -> Option<&'a ::sys_types::ClassDefinition> {
    ctx.class_definitions.get(type_name).or_else(|| {
        let mut matches = ctx
            .class_definitions
            .iter()
            .filter(|(name, _)| name.display_name() == type_name.display_name())
            .map(|(_, def)| def);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

fn find_enum_definition<'a>(
    ctx: &'a ::sys_types::SysOpContext,
    type_name: &baml_type::TypeName,
) -> Option<&'a ::sys_types::EnumDefinition> {
    ctx.enum_definitions.get(type_name).or_else(|| {
        let mut matches = ctx
            .enum_definitions
            .iter()
            .filter(|(name, _)| name.display_name() == type_name.display_name())
            .map(|(_, def)| def);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

fn find_type_alias_definition<'a>(
    ctx: &'a ::sys_types::SysOpContext,
    type_name: &baml_type::TypeName,
) -> Option<&'a baml_type::RuntimeTy> {
    ctx.type_alias_definitions.get(type_name).or_else(|| {
        let mut matches = ctx
            .type_alias_definitions
            .iter()
            .filter(|(name, _)| name.display_name() == type_name.display_name())
            .map(|(_, ty)| ty);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

/// Recursive DFS walk of a type tree. `ancestry` tracks the class names
/// currently on the call stack so mutual recursion (A → B → A) is detected.
fn walk_ty(
    ty: &baml_type::RuntimeTy,
    ctx: &::sys_types::SysOpContext,
    content: &mut types::OutputFormatContent,
    visited: &mut std::collections::HashSet<baml_base::Name>,
    ancestry: &mut Vec<baml_base::Name>,
) {
    use baml_type::RuntimeTy;

    match ty {
        RuntimeTy::Class(type_name, _, _) => {
            let key = type_name.display_name();

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

            if let Some(class_def) = find_class_definition(ctx, type_name) {
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
                    type_name.display_name().to_string(),
                    types::Class {
                        name: type_name.display_name().to_string(),
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
        RuntimeTy::Enum(type_name, _) => {
            let key = type_name.display_name();
            if !visited.insert(key) {
                return;
            }
            if let Some(enum_def) = find_enum_definition(ctx, type_name) {
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
                    type_name.display_name().to_string(),
                    types::Enum {
                        name: type_name.display_name().to_string(),
                        alias: enum_def.alias.clone(),
                        description: enum_def.description.clone(),
                        values,
                    },
                );
            }
        }
        RuntimeTy::TypeAlias(type_name, _) => {
            // The `baml.json.json` recursive alias is an opaque leaf for output-format
            // rendering — it has no schema body to collect.  Record the sentinel visit so
            // any later reference is de-duped, but do *not* insert it into
            // `recursive_type_aliases` (which would trigger schema emission) and do *not*
            // recurse into the alias body (which would diverge on the self-referential
            // `json[]` / `map<string, json>` arms).
            if type_name.display_name().as_str() == ::baml_base::qualified_name::BAML_JSON_JSON {
                visited.insert(type_name.display_name());
                return;
            }
            let key = type_name.display_name();
            if !visited.insert(key) {
                return;
            }
            if let Some(target_ty) = find_type_alias_definition(ctx, type_name) {
                content
                    .recursive_type_aliases
                    .insert(type_name.display_name().to_string(), target_ty.clone());
                walk_ty(target_ty, ctx, content, visited, ancestry);
            }
        }
        RuntimeTy::List(inner, _) => {
            walk_ty(inner, ctx, content, visited, ancestry);
        }
        RuntimeTy::Map { key, value, .. } => {
            walk_ty(key, ctx, content, visited, ancestry);
            walk_ty(value, ctx, content, visited, ancestry);
        }
        RuntimeTy::Union(members, _) => {
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
    return_type: &baml_type::RuntimeTy,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
) -> Result<baml_std::HttpRequest, LlmOpError> {
    let mut request = build_request::build_request(client, prompt, io)
        .await
        .map_err(|e| LlmOpError::Other(e.to_string()))?;
    apply_output_request_features(client, return_type, &mut request)?;
    Ok(request)
}

/// Build an HTTP request with streaming enabled.
///
/// Same as `execute_build_request_from_owned` but adds `"stream": true` to the body.
pub async fn execute_build_request_stream_from_owned(
    client: &baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
    return_type: &baml_type::RuntimeTy,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
) -> Result<baml_std::HttpRequest, LlmOpError> {
    let mut request = execute_build_request_from_owned(client, prompt, return_type, io).await?;
    request.body = add_stream_flag_to_request_body(&request.body)?;
    Ok(request)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageGenerationMode {
    Disabled,
    Available,
    Required,
}

fn apply_output_request_features(
    client: &baml_std::PrimitiveClient,
    return_type: &baml_type::RuntimeTy,
    request: &mut baml_std::HttpRequest,
) -> Result<(), LlmOpError> {
    let provider = LlmProvider::from_str(&client.provider)
        .map_err(|e| LlmOpError::Other(format!("Unknown provider '{}': {e}", client.provider)))?;

    if provider == LlmProvider::OpenAiResponses {
        match image_generation_mode(return_type) {
            ImageGenerationMode::Disabled => {}
            mode => enable_openai_responses_image_generation(request, mode)?,
        }
    } else if provider == LlmProvider::OpenAiGeneric {
        match image_generation_mode(return_type) {
            ImageGenerationMode::Disabled => {}
            mode => enable_openai_generic_image_output_modalities(request, mode)?,
        }
    }

    Ok(())
}

fn enable_openai_generic_image_output_modalities(
    request: &mut baml_std::HttpRequest,
    _mode: ImageGenerationMode,
) -> Result<(), LlmOpError> {
    let mut body: serde_json::Value = serde_json::from_str(&request.body)
        .map_err(|e| LlmOpError::Other(format!("Failed to parse OpenAI Generic body: {e}")))?;
    let obj = body.as_object_mut().ok_or_else(|| {
        LlmOpError::Other("OpenAI Generic request body must be a JSON object".to_string())
    })?;

    match obj.get_mut("modalities") {
        Some(serde_json::Value::Array(modalities)) => {
            let has_image = modalities
                .iter()
                .any(|modality| modality.as_str() == Some("image"));
            if !has_image {
                modalities.push(serde_json::Value::String("image".to_string()));
            }
        }
        Some(_) => {
            return Err(LlmOpError::Other(
                "OpenAI Generic image outputs require request_body.modalities to be an array"
                    .to_string(),
            ));
        }
        None => {
            obj.insert(
                "modalities".to_string(),
                serde_json::json!(["image", "text"]),
            );
        }
    }

    request.body = serde_json::to_string(&body)
        .map_err(|e| LlmOpError::Other(format!("Failed to serialize OpenAI Generic body: {e}")))?;
    Ok(())
}

fn enable_openai_responses_image_generation(
    request: &mut baml_std::HttpRequest,
    mode: ImageGenerationMode,
) -> Result<(), LlmOpError> {
    let mut body: serde_json::Value = serde_json::from_str(&request.body)
        .map_err(|e| LlmOpError::Other(format!("Failed to parse OpenAI Responses body: {e}")))?;
    let obj = body.as_object_mut().ok_or_else(|| {
        LlmOpError::Other("OpenAI Responses request body must be a JSON object".to_string())
    })?;

    let tools = obj
        .entry("tools".to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let tools = tools.as_array_mut().ok_or_else(|| {
        LlmOpError::Other(
            "OpenAI Responses image outputs require request_body.tools to be an array".to_string(),
        )
    })?;

    let has_image_generation_tool = tools.iter().any(|tool| {
        tool.get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|tool_type| tool_type == "image_generation")
    });
    if !has_image_generation_tool {
        tools.push(serde_json::json!({ "type": "image_generation" }));
    }

    if matches!(
        mode,
        ImageGenerationMode::Required | ImageGenerationMode::Available
    ) {
        obj.insert(
            "tool_choice".to_string(),
            serde_json::json!({ "type": "image_generation" }),
        );
    }

    request.body = serde_json::to_string(&body).map_err(|e| {
        LlmOpError::Other(format!("Failed to serialize OpenAI Responses body: {e}"))
    })?;
    Ok(())
}

fn image_generation_mode(target: &baml_type::RuntimeTy) -> ImageGenerationMode {
    match target {
        baml_type::RuntimeTy::Media(baml_base::MediaKind::Image, _) => {
            ImageGenerationMode::Required
        }
        baml_type::RuntimeTy::List(inner, _) => match image_generation_mode(inner) {
            ImageGenerationMode::Required => ImageGenerationMode::Required,
            ImageGenerationMode::Available => ImageGenerationMode::Available,
            ImageGenerationMode::Disabled => ImageGenerationMode::Disabled,
        },
        baml_type::RuntimeTy::Union(_, _) if types::is_text_or_image_union(target) => {
            ImageGenerationMode::Available
        }
        baml_type::RuntimeTy::Union(members, _) => {
            let non_null_members: Vec<&baml_type::RuntimeTy> = members
                .iter()
                .filter(|member| !matches!(member, baml_type::RuntimeTy::Null { .. }))
                .collect();
            if non_null_members.is_empty() {
                return ImageGenerationMode::Disabled;
            }

            let mut has_image = false;
            for member in &non_null_members {
                match image_generation_mode(member) {
                    ImageGenerationMode::Required | ImageGenerationMode::Available => {
                        has_image = true;
                    }
                    ImageGenerationMode::Disabled => return ImageGenerationMode::Disabled,
                }
            }

            if has_image && non_null_members.len() == members.len() {
                ImageGenerationMode::Required
            } else if has_image {
                ImageGenerationMode::Available
            } else {
                ImageGenerationMode::Disabled
            }
        }
        _ => ImageGenerationMode::Disabled,
    }
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
    return_type: &baml_type::RuntimeTy,
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

    if let Some(parsed) = parse_llm_output_for_target(return_type, &response.output)? {
        return Ok(parsed);
    }

    let compiled = bex_sap::CompiledSapModel::from_sys_op_context(
        ctx,
        return_type.clone(),
        baml_type::RuntimeTy::null(), // no streaming
    )
    .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?;
    let sap = SapParseCache::new(compiled);
    execute_sap_parse_final(&response.content, &sap, ctx)
}

fn parse_llm_output_for_target(
    target: &baml_type::RuntimeTy,
    output: &parse_response::LlmOutput,
) -> Result<Option<BexExternalValue>, LlmOpError> {
    if output.parts.is_empty() {
        return Ok(None);
    }

    match target {
        baml_type::RuntimeTy::Media(kind, _) => {
            let media = media_parts(output, *kind);
            if media.is_empty() {
                return Ok(None);
            }
            reject_mixed_media_only_output(output, *kind)?;
            if media.len() != 1 {
                return Err(LlmOpError::ParseResponseError(format!(
                    "Expected exactly one {kind} output, got {}. Use {kind}[] for multiple outputs.",
                    media.len()
                )));
            }
            Ok(Some(media_to_external(media[0].clone())))
        }
        baml_type::RuntimeTy::List(inner, _)
            if is_media_type(inner, baml_base::MediaKind::Image) =>
        {
            let media = media_parts(output, baml_base::MediaKind::Image);
            if media.is_empty() {
                return Ok(None);
            }
            reject_mixed_media_only_output(output, baml_base::MediaKind::Image)?;
            Ok(Some(BexExternalValue::Array {
                element_type: *inner.clone(),
                items: media.into_iter().map(media_to_external).collect(),
            }))
        }
        baml_type::RuntimeTy::List(inner, _) if types::is_text_or_image_union(inner) => {
            let items = mixed_text_image_parts(output, inner);
            if items.is_empty() {
                return Ok(None);
            }
            Ok(Some(BexExternalValue::Array {
                element_type: *inner.clone(),
                items,
            }))
        }
        target if nullable_media_kind(target).is_some() => {
            parse_nullable_media_output(target, output)
        }
        target if nullable_list_inner(target).is_some() => {
            parse_nullable_list_output(target, output)
        }
        baml_type::RuntimeTy::Union(_, _) if types::is_text_or_image_union(target) => {
            // A nullable `(string | image)?` records its non-null members as the
            // union's declared type (optionality is tracked separately), matching
            // the former `Optional` path.
            let effective = target.strip_null();
            let items = mixed_text_image_parts(output, &effective);
            single_text_or_image_union_output(&effective, items)
        }
        _ => Ok(None),
    }
}

fn parse_nullable_media_output(
    target: &baml_type::RuntimeTy,
    output: &parse_response::LlmOutput,
) -> Result<Option<BexExternalValue>, LlmOpError> {
    let Some(kind) = nullable_media_kind(target) else {
        return Ok(None);
    };
    let media = media_parts(output, kind);
    if media.is_empty() {
        return Ok(None);
    }
    reject_mixed_media_only_output(output, kind)?;
    if media.len() != 1 {
        return Err(LlmOpError::ParseResponseError(format!(
            "Expected zero or one {kind} output for {target}, got {}. Use {kind}[] for multiple outputs.",
            media.len()
        )));
    }

    let value = media_to_external(media[0].clone());
    match target {
        baml_type::RuntimeTy::Union(members, _) => {
            let selected = members
                .iter()
                .find(|member| is_media_type(member, kind))
                .cloned()
                .unwrap_or_else(|| baml_type::RuntimeTy::Media(kind, baml_type::TyAttr::default()));
            Ok(Some(BexExternalValue::union(
                value,
                members.clone(),
                selected,
            )))
        }
        _ => Ok(Some(value)),
    }
}

fn parse_nullable_list_output(
    target: &baml_type::RuntimeTy,
    output: &parse_response::LlmOutput,
) -> Result<Option<BexExternalValue>, LlmOpError> {
    let Some(list_ty) = nullable_list_inner(target) else {
        return Ok(None);
    };
    let baml_type::RuntimeTy::List(element_type, _) = list_ty else {
        return Ok(None);
    };

    let array = if is_media_type(element_type, baml_base::MediaKind::Image) {
        let media = media_parts(output, baml_base::MediaKind::Image);
        if media.is_empty() {
            return Ok(None);
        }
        reject_mixed_media_only_output(output, baml_base::MediaKind::Image)?;
        BexExternalValue::Array {
            element_type: *element_type.clone(),
            items: media.into_iter().map(media_to_external).collect(),
        }
    } else if types::is_text_or_image_union(element_type) {
        let items = mixed_text_image_parts(output, element_type);
        if items.is_empty() {
            return Ok(None);
        }
        BexExternalValue::Array {
            element_type: *element_type.clone(),
            items,
        }
    } else {
        return Ok(None);
    };

    match target {
        baml_type::RuntimeTy::Union(members, _) => Ok(Some(BexExternalValue::union(
            array,
            members.clone(),
            list_ty.clone(),
        ))),
        _ => Ok(Some(array)),
    }
}

fn media_parts(
    output: &parse_response::LlmOutput,
    kind: baml_base::MediaKind,
) -> Vec<std::sync::Arc<baml_builtins2::MediaValue>> {
    output
        .parts
        .iter()
        .filter_map(|part| match part {
            parse_response::LlmOutputPart::Media { media, .. } if media.kind == kind => {
                Some(media.clone())
            }
            _ => None,
        })
        .collect()
}

fn reject_mixed_media_only_output(
    output: &parse_response::LlmOutput,
    kind: baml_base::MediaKind,
) -> Result<(), LlmOpError> {
    let unexpected = output
        .parts
        .iter()
        .filter(|part| match part {
            parse_response::LlmOutputPart::Media { media, .. } => media.kind != kind,
            parse_response::LlmOutputPart::Text { text } => !text.trim().is_empty(),
        })
        .count();

    if unexpected == 0 {
        return Ok(());
    }

    Err(LlmOpError::ParseResponseError(format!(
        "Expected only {kind} output parts, got {unexpected} non-{kind} part(s). Use a text/image union return type to preserve mixed outputs."
    )))
}

fn mixed_text_image_parts(
    output: &parse_response::LlmOutput,
    target: &baml_type::RuntimeTy,
) -> Vec<BexExternalValue> {
    let baml_type::RuntimeTy::Union(members, _) = target else {
        return Vec::new();
    };

    let string_ty = members
        .iter()
        .find(|member| matches!(member, baml_type::RuntimeTy::String { .. }))
        .cloned()
        .unwrap_or_else(baml_type::RuntimeTy::string);
    let image_ty = members
        .iter()
        .find(|member| {
            matches!(
                member,
                baml_type::RuntimeTy::Media(baml_base::MediaKind::Image, _)
            )
        })
        .cloned()
        .unwrap_or(baml_type::RuntimeTy::Media(
            baml_base::MediaKind::Image,
            baml_type::TyAttr::default(),
        ));

    let mut items = Vec::new();
    for part in &output.parts {
        match part {
            parse_response::LlmOutputPart::Text { text } if !text.trim().is_empty() => {
                items.push(BexExternalValue::union(
                    BexExternalValue::String(bex_external_types::BexStr::from(text.as_str())),
                    members.clone(),
                    string_ty.clone(),
                ));
            }
            parse_response::LlmOutputPart::Media { media, .. }
                if media.kind == baml_base::MediaKind::Image =>
            {
                items.push(BexExternalValue::union(
                    media_to_external(media.clone()),
                    members.clone(),
                    image_ty.clone(),
                ));
            }
            _ => {}
        }
    }

    items
}

fn single_text_or_image_union_output(
    target: &baml_type::RuntimeTy,
    items: Vec<BexExternalValue>,
) -> Result<Option<BexExternalValue>, LlmOpError> {
    match items.len() {
        0 => Ok(None),
        1 => Ok(Some(items.into_iter().next().expect("length checked"))),
        _ if all_union_items_are_text(&items) => {
            let baml_type::RuntimeTy::Union(members, _) = target else {
                return Ok(None);
            };
            let text: String = items
                .into_iter()
                .filter_map(|item| match item {
                    BexExternalValue::Union { value, .. } => match *value {
                        BexExternalValue::String(text) => Some(text.to_string()),
                        _ => None,
                    },
                    BexExternalValue::String(text) => Some(text.to_string()),
                    _ => None,
                })
                .collect();
            let string_ty = members
                .iter()
                .find(|member| matches!(member, baml_type::RuntimeTy::String { .. }))
                .cloned()
                .unwrap_or_else(baml_type::RuntimeTy::string);
            Ok(Some(BexExternalValue::union(
                BexExternalValue::String(bex_external_types::BexStr::from(text.as_str())),
                members.clone(),
                string_ty,
            )))
        }
        n => Err(LlmOpError::ParseResponseError(format!(
            "Expected one text or image output for {target}, got {n} parts. Use (image | string)[] to preserve mixed outputs."
        ))),
    }
}

fn all_union_items_are_text(items: &[BexExternalValue]) -> bool {
    items.iter().all(|item| match item {
        BexExternalValue::Union { value, .. } => {
            matches!(value.as_ref(), BexExternalValue::String(_))
        }
        BexExternalValue::String(_) => true,
        _ => false,
    })
}

fn media_to_external(media: std::sync::Arc<baml_builtins2::MediaValue>) -> BexExternalValue {
    BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(media))
}

fn is_media_type(target: &baml_type::RuntimeTy, kind: baml_base::MediaKind) -> bool {
    matches!(target, baml_type::RuntimeTy::Media(actual, _) if *actual == kind)
}

fn nullable_media_kind(target: &baml_type::RuntimeTy) -> Option<baml_base::MediaKind> {
    match target {
        baml_type::RuntimeTy::Union(members, _) => {
            let mut kind = None;
            let mut has_null = false;
            for member in members {
                match member {
                    baml_type::RuntimeTy::Media(media_kind, _) => {
                        if kind
                            .replace(*media_kind)
                            .is_some_and(|prev| prev != *media_kind)
                        {
                            return None;
                        }
                    }
                    baml_type::RuntimeTy::Null { .. } => has_null = true,
                    _ => return None,
                }
            }

            if has_null { kind } else { None }
        }
        _ => None,
    }
}

fn nullable_list_inner(target: &baml_type::RuntimeTy) -> Option<&baml_type::RuntimeTy> {
    match target {
        baml_type::RuntimeTy::Union(members, _) => {
            let mut list = None;
            let mut has_null = false;
            for member in members {
                match member {
                    baml_type::RuntimeTy::List(_, _) => {
                        if list.replace(member).is_some() {
                            return None;
                        }
                    }
                    baml_type::RuntimeTy::Null { .. } => has_null = true,
                    _ => return None,
                }
            }
            if has_null { list } else { None }
        }
        _ => None,
    }
}

pub fn execute_sap_parse_final(
    json: &str,
    sap: &SapParseCache,
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
    Ok(::bex_sap::to_external::baml_value_to_external(
        &parsed,
        sap.db(),
    ))
}

pub fn execute_sap_parse_partial(
    json: &str,
    sap: &SapParseCache,
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
            let converted = ::bex_sap::to_external::baml_value_to_external(&parsed, sap.db());
            Ok(Some(converted))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ::baml_base::TyAttr;
    use ::sys_types::{
        ClassDefinition, ClassFieldDefinition, EnumDefinition, SysOpContext,
        runtime_io::NoopRuntimeIo,
    };
    use baml_builtins2::{MediaContent, MediaValue, PromptAst};
    use bex_external_types::{BexExternalValue, RuntimeTy};

    use super::{
        build_output_format_content, execute_build_request_from_owned,
        execute_parse_response_from_owned, render_output_format,
    };
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

    fn make_openai_responses_client(
        mut options: baml_std::PrimitiveClientOptions,
    ) -> baml_std::PrimitiveClient {
        if options.model.is_none() {
            options.model = Some("gpt-4.1".to_string());
        }
        options.base_url = Some("https://api.openai.com/v1".to_string());
        baml_std::PrimitiveClient::new(
            "TestClient".to_string(),
            "openai-responses".to_string(),
            options,
        )
        .unwrap()
    }

    fn make_openai_generic_client(
        mut options: baml_std::PrimitiveClientOptions,
    ) -> baml_std::PrimitiveClient {
        if options.model.is_none() {
            options.model = Some("google/gemini-2.5-flash-image-preview".to_string());
        }
        options.base_url = Some("https://openrouter.ai/api/v1".to_string());
        baml_std::PrimitiveClient::new(
            "TestClient".to_string(),
            "openai-generic".to_string(),
            options,
        )
        .unwrap()
    }

    fn prompt_msg(text: &str) -> bex_vm_types::PromptAst {
        Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    fn body_json(request: &baml_std::HttpRequest) -> serde_json::Value {
        serde_json::from_str(&request.body).unwrap()
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
            &::baml_type::RuntimeTy::String {
                attr: TyAttr::default(),
            },
            &ctx,
        );
        assert!(allowed.is_ok());

        // "length" is rejected.
        let blocked = execute_parse_response_from_owned(
            &allow_client,
            response_length,
            &::baml_type::RuntimeTy::String {
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
            &::baml_type::RuntimeTy::String {
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
            &::baml_type::RuntimeTy::String {
                attr: TyAttr::default(),
            },
            &ctx,
        );
        assert!(
            result.is_ok(),
            "should parse Anthropic response: {result:?}"
        );
    }

    #[test]
    fn openai_responses_parses_multiple_image_outputs() {
        let client = baml_std::PrimitiveClient::new(
            "test".to_string(),
            "openai-responses".to_string(),
            baml_std::PrimitiveClientOptions {
                model: Some("gpt-4.1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let response = r#"{
            "id": "resp_img",
            "object": "response",
            "status": "completed",
            "model": "gpt-4.1",
            "output": [
                {
                    "type": "image_generation_call",
                    "id": "ig_1",
                    "status": "completed",
                    "result": "aW1hZ2Ux",
                    "output_format": "png"
                },
                {
                    "type": "image_generation_call",
                    "id": "ig_2",
                    "status": "completed",
                    "result": "aW1hZ2Uy",
                    "output_format": "jpeg"
                }
            ],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }"#;

        let result = execute_parse_response_from_owned(
            &client,
            response,
            &baml_type::RuntimeTy::List(Box::new(ty_image()), TyAttr::default()),
            &SysOpContext::empty(),
        )
        .unwrap();

        let BexExternalValue::Array { items, .. } = result else {
            panic!("expected image array");
        };
        assert_eq!(items.len(), 2);

        let BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(first)) = &items[0]
        else {
            panic!("expected first media item");
        };
        assert_eq!(first.kind, baml_base::MediaKind::Image);
        assert_eq!(first.base64(), "aW1hZ2Ux");
        assert_eq!(first.mime_type().as_deref(), Some("image/png"));

        let BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(second)) = &items[1]
        else {
            panic!("expected second media item");
        };
        assert_eq!(second.base64(), "aW1hZ2Uy");
        assert_eq!(second.mime_type().as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn openai_responses_parses_mixed_text_and_image_outputs() {
        let client = baml_std::PrimitiveClient::new(
            "test".to_string(),
            "openai-responses".to_string(),
            baml_std::PrimitiveClientOptions {
                model: Some("gpt-4.1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let response = r#"{
            "id": "resp_mixed",
            "object": "response",
            "status": "completed",
            "model": "gpt-4.1",
            "output": [
                {
                    "type": "message",
                    "id": "msg_1",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "caption"}]
                },
                {
                    "type": "image_generation_call",
                    "id": "ig_1",
                    "status": "completed",
                    "result": "aW1hZ2U=",
                    "output_format": "webp"
                }
            ],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }"#;

        let result = execute_parse_response_from_owned(
            &client,
            response,
            &baml_type::RuntimeTy::List(Box::new(ty_text_image_union()), TyAttr::default()),
            &SysOpContext::empty(),
        )
        .unwrap();

        let BexExternalValue::Array { items, .. } = result else {
            panic!("expected mixed array");
        };
        assert_eq!(items.len(), 2);

        let BexExternalValue::Union { value, .. } = &items[0] else {
            panic!("expected first union item");
        };
        assert_eq!(value.as_ref(), &BexExternalValue::String("caption".into()));

        let BexExternalValue::Union { value, .. } = &items[1] else {
            panic!("expected second union item");
        };
        let BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(media)) =
            value.as_ref()
        else {
            panic!("expected media union value");
        };
        assert_eq!(media.kind, baml_base::MediaKind::Image);
        assert_eq!(media.base64(), "aW1hZ2U=");
        assert_eq!(media.mime_type().as_deref(), Some("image/webp"));
    }

    #[test]
    fn text_image_union_collapses_multiple_text_parts() {
        let mut output = super::parse_response::LlmOutput::default();
        output.push_text("hello ".into());
        output.push_text("world".into());

        let result = super::parse_llm_output_for_target(&ty_text_image_union(), &output)
            .unwrap()
            .expect("expected parsed union");

        let BexExternalValue::Union { value, .. } = result else {
            panic!("expected union");
        };
        assert_eq!(
            value.as_ref(),
            &BexExternalValue::String("hello world".into())
        );
    }

    #[test]
    fn optional_text_image_union_parses_native_media_part() {
        let mut output = super::parse_response::LlmOutput::default();
        output.push_media(
            Arc::new(MediaValue::new(
                baml_base::MediaKind::Image,
                MediaContent::Base64 {
                    base64_data: "aW1hZ2U=".into(),
                },
                Some("image/png".into()),
            )),
            None,
            serde_json::Value::Null,
        );

        let inner = ty_text_image_union();
        let result = super::parse_llm_output_for_target(&ty_optional(inner.clone()), &output)
            .unwrap()
            .expect("expected optional union media");
        let BexExternalValue::Union { value, metadata } = result else {
            panic!("expected text|image union");
        };
        assert_eq!(metadata.union_type, inner);
        assert_eq!(
            metadata.selected_option,
            baml_type::RuntimeTy::Media(baml_base::MediaKind::Image, TyAttr::default())
        );
        let BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(media)) =
            value.as_ref()
        else {
            panic!("expected media union value");
        };
        assert_eq!(media.base64(), "aW1hZ2U=");
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
    }

    #[tokio::test]
    async fn openai_responses_image_return_enables_image_generation_tool() {
        let client = make_openai_responses_client(baml_std::PrimitiveClientOptions::default());
        let request = execute_build_request_from_owned(
            &client,
            prompt_msg("Generate a product photo of a brass desk lamp."),
            &ty_image(),
            Arc::new(NoopRuntimeIo),
        )
        .await
        .unwrap();

        let body = body_json(&request);
        assert_eq!(
            body["tools"],
            serde_json::json!([{ "type": "image_generation" }])
        );
        assert_eq!(
            body["tool_choice"],
            serde_json::json!({ "type": "image_generation" })
        );
    }

    #[test]
    fn openai_responses_required_image_overrides_conflicting_tool_choice() {
        let mut request = baml_std::HttpRequest {
            method: "POST".into(),
            url: "https://api.openai.com/v1/responses".into(),
            headers: indexmap::IndexMap::new(),
            body: serde_json::json!({
                "tools": [],
                "tool_choice": "none"
            })
            .to_string(),
        };

        super::enable_openai_responses_image_generation(
            &mut request,
            super::ImageGenerationMode::Required,
        )
        .unwrap();

        let body = body_json(&request);
        assert_eq!(
            body["tool_choice"],
            serde_json::json!({ "type": "image_generation" })
        );
    }

    #[tokio::test]
    async fn openai_responses_mixed_text_image_return_enables_tool_and_choice() {
        let client = make_openai_responses_client(baml_std::PrimitiveClientOptions::default());
        let request = execute_build_request_from_owned(
            &client,
            prompt_msg("Return a caption and generate an illustration."),
            &baml_type::RuntimeTy::List(Box::new(ty_text_image_union()), TyAttr::default()),
            Arc::new(NoopRuntimeIo),
        )
        .await
        .unwrap();

        let body = body_json(&request);
        assert_eq!(
            body["tools"],
            serde_json::json!([{ "type": "image_generation" }])
        );
        assert_eq!(
            body["tool_choice"],
            serde_json::json!({ "type": "image_generation" })
        );
    }

    #[tokio::test]
    async fn openai_generic_image_return_enables_image_modality() {
        let client = make_openai_generic_client(baml_std::PrimitiveClientOptions::default());
        let request = execute_build_request_from_owned(
            &client,
            prompt_msg("Generate a product photo of a brass desk lamp."),
            &ty_image(),
            Arc::new(NoopRuntimeIo),
        )
        .await
        .unwrap();

        let body = body_json(&request);
        assert_eq!(body["modalities"], serde_json::json!(["image", "text"]));
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
    }

    #[tokio::test]
    async fn openai_generic_image_return_preserves_existing_modalities_and_adds_image() {
        let client = make_openai_generic_client(baml_std::PrimitiveClientOptions {
            request_body: indexmap::IndexMap::from([(
                "modalities".to_string(),
                BexExternalValue::Array {
                    element_type: RuntimeTy::String {
                        attr: TyAttr::default(),
                    },
                    items: vec![BexExternalValue::String("text".into())],
                },
            )]),
            ..Default::default()
        });
        let request = execute_build_request_from_owned(
            &client,
            prompt_msg("Generate a product photo of a brass desk lamp."),
            &ty_image(),
            Arc::new(NoopRuntimeIo),
        )
        .await
        .unwrap();

        let body = body_json(&request);
        assert_eq!(body["modalities"], serde_json::json!(["text", "image"]));
    }

    #[test]
    fn openai_generic_parses_message_images_for_image_return() {
        let client = make_openai_generic_client(baml_std::PrimitiveClientOptions::default());
        let response = r#"{
            "model": "google/gemini-2.5-flash-image-preview",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "",
                    "images": [{
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/png;base64,aW1hZ2U="
                        }
                    }]
                },
                "finish_reason": "stop"
            }]
        }"#;

        let result = execute_parse_response_from_owned(
            &client,
            response,
            &ty_image(),
            &SysOpContext::empty(),
        )
        .unwrap();

        let BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(media)) = result else {
            panic!("expected image media");
        };
        assert_eq!(media.kind, baml_base::MediaKind::Image);
        assert_eq!(media.base64(), "aW1hZ2U=");
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
    }

    #[test]
    fn nullable_image_targets_parse_native_media_parts() {
        let mut output = super::parse_response::LlmOutput::default();
        output.push_media(
            Arc::new(MediaValue::new(
                baml_base::MediaKind::Image,
                MediaContent::Base64 {
                    base64_data: "aW1hZ2U=".into(),
                },
                Some("image/png".into()),
            )),
            None,
            serde_json::Value::Null,
        );

        let result = super::parse_llm_output_for_target(&ty_optional(ty_image()), &output)
            .unwrap()
            .expect("expected optional media");
        let BexExternalValue::Union { value, .. } = result else {
            panic!("expected optional union");
        };
        let BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(media)) =
            value.as_ref()
        else {
            panic!("expected media");
        };
        assert_eq!(media.base64(), "aW1hZ2U=");

        let image_null_union = baml_type::RuntimeTy::Union(
            vec![ty_image(), baml_type::RuntimeTy::null()],
            TyAttr::default(),
        );
        let result = super::parse_llm_output_for_target(&image_null_union, &output)
            .unwrap()
            .expect("expected union media");
        let BexExternalValue::Union { value, .. } = result else {
            panic!("expected image|null union");
        };
        assert!(matches!(
            value.as_ref(),
            BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(_))
        ));
    }

    #[test]
    fn nullable_image_list_targets_parse_native_media_parts() {
        let mut output = super::parse_response::LlmOutput::default();
        output.push_media(
            Arc::new(MediaValue::new(
                baml_base::MediaKind::Image,
                MediaContent::Base64 {
                    base64_data: "aW1hZ2U=".into(),
                },
                Some("image/png".into()),
            )),
            None,
            serde_json::Value::Null,
        );

        let image_list = baml_type::RuntimeTy::List(Box::new(ty_image()), TyAttr::default());
        let result = super::parse_llm_output_for_target(&ty_optional(image_list.clone()), &output)
            .unwrap()
            .expect("expected optional image list");
        let BexExternalValue::Union { value, metadata } = result else {
            panic!("expected optional union");
        };
        assert_eq!(metadata.selected_option, image_list);
        let BexExternalValue::Array { items, .. } = value.as_ref() else {
            panic!("expected image array");
        };
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn media_only_targets_reject_mixed_output_parts() {
        let mut output = super::parse_response::LlmOutput::default();
        output.push_text("caption".into());
        output.push_media(
            Arc::new(MediaValue::new(
                baml_base::MediaKind::Image,
                MediaContent::Base64 {
                    base64_data: "aW1hZ2U=".into(),
                },
                Some("image/png".into()),
            )),
            None,
            serde_json::Value::Null,
        );

        let error = super::parse_llm_output_for_target(&ty_image(), &output).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Expected only image output parts")
        );
    }

    // ========================================================================
    // build_output_format_content / walk_ty tests
    // ========================================================================

    fn ty_class(name: &str) -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Class(
            baml_type::TypeName::local(name.into()),
            Vec::new(),
            TyAttr::default(),
        )
    }
    fn ty_enum(name: &str) -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Enum(baml_type::TypeName::local(name.into()), TyAttr::default())
    }
    fn ty_string() -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_image() -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Media(baml_base::MediaKind::Image, TyAttr::default())
    }
    fn ty_text_image_union() -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::Union(vec![ty_string(), ty_image()], TyAttr::default())
    }
    fn ty_optional(inner: baml_type::RuntimeTy) -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::optional(inner)
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
    fn render_output_format_renders_class_schema() {
        // For a class return type, `ctx.output_format` must emit the schema.
        let ctx = ctx_with(
            vec![(
                tn("User"),
                ClassDefinition {
                    name: "User".into(),
                    description: None,
                    alias: None,
                    fields: vec![
                        ClassFieldDefinition {
                            name: "name".into(),
                            field_type: ty_string(),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                        ClassFieldDefinition {
                            name: "age".into(),
                            field_type: baml_type::RuntimeTy::Int {
                                attr: TyAttr::default(),
                            },
                            description: None,
                            alias: None,
                            skip: false,
                        },
                    ],
                },
            )],
            vec![],
        );
        let rendered = render_output_format(&ty_class("User"), &ctx);
        assert!(
            rendered.contains("name"),
            "schema must list fields: {rendered}"
        );
        assert!(
            rendered.contains("age"),
            "schema must list fields: {rendered}"
        );
        // Parity check: identical to a default-options render of the same content.
        let direct = build_output_format_content(&ty_class("User"), &ctx)
            .render(&crate::types::RenderOptions::default())
            .unwrap()
            .unwrap_or_default();
        assert_eq!(rendered, direct);
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

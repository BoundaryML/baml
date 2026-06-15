//! Vercel AI Gateway image-model body builder.
//!
//! Mirrors the Vercel AI SDK gateway image model endpoint at `/v4/ai/image-model`.

use baml_builtins2::{PromptAst, PromptAstSimple};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ImagesRequestBody {
    prompt: String,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

pub(crate) fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: &bex_vm_types::PromptAst,
) -> Result<crate::baml_std::HttpRequest, crate::build_request::BuildRequestError> {
    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.insert(
        "ai-gateway-protocol-version".to_string(),
        "0.0.1".to_string(),
    );
    headers.insert(
        "ai-image-model-specification-version".to_string(),
        "4".to_string(),
    );
    headers.insert("ai-model-id".to_string(), client.model.clone());

    let mut extra = client.extra_body.clone();
    extra
        .entry("n".to_string())
        .or_insert_with(|| serde_json::json!(1));

    let body = ImagesRequestBody {
        prompt: prompt_to_text(prompt)?,
        extra,
    };
    let body_str = serde_json::to_string(&body)?;

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url: format!(
            "{}/image-model",
            client
                .options
                .base_url
                .as_deref()
                .unwrap_or_default()
                .trim_end_matches('/')
        ),
        headers,
        body: body_str,
    })
}

fn prompt_to_text(
    prompt: &bex_vm_types::PromptAst,
) -> Result<String, crate::build_request::BuildRequestError> {
    let mut parts = Vec::new();
    collect_prompt_text(prompt, &mut parts)?;
    let prompt = parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if prompt.is_empty() {
        return Err(crate::build_request::BuildRequestError::Other(
            "Images API request requires a text prompt".to_string(),
        ));
    }

    Ok(prompt)
}

fn collect_prompt_text(
    node: &bex_vm_types::PromptAst,
    parts: &mut Vec<String>,
) -> Result<(), crate::build_request::BuildRequestError> {
    match node.as_ref() {
        PromptAst::Vec(items) => {
            for item in items {
                collect_prompt_text(item, parts)?;
            }
        }
        PromptAst::Message { content, .. } => collect_simple_text(content.as_ref(), parts)?,
        PromptAst::Simple(content) => collect_simple_text(content.as_ref(), parts)?,
    }
    Ok(())
}

fn collect_simple_text(
    content: &PromptAstSimple,
    parts: &mut Vec<String>,
) -> Result<(), crate::build_request::BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => {
            parts.push(s.clone());
            Ok(())
        }
        PromptAstSimple::Media(media) => Err(unsupported_media(media.kind)),
        PromptAstSimple::Multiple(items) => {
            for item in items {
                collect_simple_text(item, parts)?;
            }
            Ok(())
        }
    }
}

fn unsupported_media(kind: baml_base::MediaKind) -> crate::build_request::BuildRequestError {
    crate::build_request::BuildRequestError::UnsupportedMedia(format!(
        "Images API only supports text prompts; found {kind} input"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(
        extra_body: serde_json::Map<String, serde_json::Value>,
    ) -> crate::baml_std::PrimitiveClient {
        let options = crate::baml_std::PrimitiveClientOptions {
            model: Some("bfl/flux-2-pro".to_string()),
            api_key: Some("test-key".to_string()),
            request_body: extra_body
                .into_iter()
                .map(|(k, v)| (k, json_to_bex(v)))
                .collect(),
            ..Default::default()
        };

        crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "ai-gateway-images".to_string(),
            options,
        )
        .unwrap()
    }

    fn json_to_bex(value: serde_json::Value) -> bex_external_types::BexExternalValue {
        match value {
            serde_json::Value::Null => bex_external_types::BexExternalValue::Null,
            serde_json::Value::Bool(v) => bex_external_types::BexExternalValue::Bool(v),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    bex_external_types::BexExternalValue::Int(i)
                } else {
                    bex_external_types::BexExternalValue::Float(n.as_f64().unwrap())
                }
            }
            serde_json::Value::String(v) => bex_external_types::BexExternalValue::String(v.into()),
            serde_json::Value::Array(items) => bex_external_types::BexExternalValue::Array {
                element_type: baml_type::RuntimeTy::unknown(),
                items: items.into_iter().map(json_to_bex).collect(),
            },
            serde_json::Value::Object(map) => bex_external_types::BexExternalValue::Map {
                key_type: baml_type::RuntimeTy::string(),
                value_type: baml_type::RuntimeTy::unknown(),
                entries: map.into_iter().map(|(k, v)| (k, json_to_bex(v))).collect(),
            },
        }
    }

    #[test]
    fn builds_gateway_image_model_request() {
        let client = client(serde_json::Map::from_iter([
            ("n".to_string(), serde_json::json!(2)),
            (
                "providerOptions".to_string(),
                serde_json::json!({ "blackForestLabs": { "outputFormat": "jpeg" } }),
            ),
        ]));
        let prompt = std::sync::Arc::new(baml_builtins2::PromptAst::Simple(std::sync::Arc::new(
            baml_builtins2::PromptAstSimple::String("Draw a brass desk lamp".to_string()),
        )));

        let request = build_request(&client, &prompt).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.url,
            "https://ai-gateway.vercel.sh/v4/ai/image-model"
        );
        assert_eq!(
            request.headers.get("ai-image-model-specification-version"),
            Some(&"4".to_string())
        );
        assert_eq!(
            request.headers.get("ai-model-id"),
            Some(&"bfl/flux-2-pro".to_string())
        );

        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["prompt"], "Draw a brass desk lamp");
        assert_eq!(body["n"], 2);
        assert_eq!(
            body["providerOptions"]["blackForestLabs"]["outputFormat"],
            "jpeg"
        );
    }

    #[test]
    fn defaults_gateway_image_count_to_one() {
        let client = client(serde_json::Map::new());
        let prompt = std::sync::Arc::new(baml_builtins2::PromptAst::Simple(std::sync::Arc::new(
            baml_builtins2::PromptAstSimple::String("Draw a brass desk lamp".to_string()),
        )));

        let request = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["n"], 1);
    }
}

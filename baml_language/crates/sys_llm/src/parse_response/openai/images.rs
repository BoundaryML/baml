use serde::Deserialize;

use crate::parse_response::{
    FinishReason, LlmOutput, LlmProviderResponse, ParseResponseError, TokenUsage,
};

#[derive(Debug, Deserialize)]
struct ImagesApiResponse {
    #[serde(default)]
    images: Vec<String>,
    #[serde(rename = "providerMetadata")]
    provider_metadata: Option<serde_json::Value>,
}

pub(in crate::parse_response) fn parse_openai_images_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ImagesApiResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "ai-gateway-images",
            source: e,
            content: body.to_string(),
        })?;

    let mut output = LlmOutput::default();

    for base64 in &response.images {
        output.push_media(
            baml_builtins2::MediaValue::from_base64(
                baml_base::MediaKind::Image,
                base64,
                Some("image/png"),
            ),
            None,
            serde_json::json!({
                "provider": "ai-gateway-images",
                "providerMetadata": response.provider_metadata.clone(),
            }),
        );
    }

    if output.parts.is_empty() {
        return Err(ParseResponseError::NoContent {
            provider: "ai-gateway-images",
            detail: "response contained no images in images[]".to_string(),
        });
    }

    Ok(LlmProviderResponse {
        output,
        content: String::new(),
        model: None,
        finish_reason: FinishReason::Stop,
        finish_reason_raw: Some("stop".to_string()),
        usage: TokenUsage::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gateway_images_response() {
        let body = r#"{
          "images": ["abc123", "def456"],
          "providerMetadata": {
            "gateway": {
              "routing": { "provider": "black-forest-labs" }
            }
          }
        }"#;

        let response = parse_openai_images_response(body).unwrap();
        assert_eq!(response.output.parts.len(), 2);

        let crate::parse_response::LlmOutputPart::Media { media, .. } = &response.output.parts[0]
        else {
            panic!("expected media");
        };
        assert_eq!(media.kind, baml_base::MediaKind::Image);
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
    }
}

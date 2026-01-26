use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use super::types::{GoogleResponse, Part};
use crate::internal::llm_client::{
    primitive::request::RequestBuilder, traits::WithClient, ErrorCode, LLMCompleteResponse,
    LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
};

fn to_prompt(
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
) -> internal_baml_jinja::RenderedPrompt {
    match prompt {
        either::Left(prompt) => internal_baml_jinja::RenderedPrompt::Completion(prompt.clone()),
        either::Right(prompt) => internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
    }
}

pub fn parse_google_response<C: WithClient + RequestBuilder>(
    client: &C,
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
    response_body: serde_json::Value,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    model_name: Option<String>,
) -> LLMResponse {
    // baml_log::info!("Parsing Google response: {:#?}", response_body);
    let response = match GoogleResponse::deserialize(&response_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<GoogleResponse>(),
            response_body
        ))
        .map_err(|e| LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!("{e:?}"),
            code: ErrorCode::UnsupportedResponse(2),
            raw_response: Some(response_body.to_string()),
        }) {
        Ok(response) => response,
        Err(e) => return LLMResponse::LLMFailure(e),
    };

    if response.candidates.len() != 1 {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!(
                "Expected exactly one content block, got {}",
                response.candidates.len()
            ),
            code: ErrorCode::Other(200),
            raw_response: Some(response_body.to_string()),
        });
    }

    let candidate = &response.candidates[0];

    let Some(content) = candidate.content.as_ref() else {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: "No content returned".to_string(),
            code: ErrorCode::Other(200),
            raw_response: Some(response_body.to_string()),
        });
    };

    let model_name = model_name.unwrap_or("<unknown>".to_string());
    let text_content = text_content_part(&content.parts);
    LLMResponse::Success(LLMCompleteResponse {
        client: client.context().name.to_string(),
        prompt: to_prompt(prompt),
        content: text_content.unwrap_or_default(),
        start_time: system_now,
        latency: instant_now.elapsed(),
        request_options: client.request_options().clone(),
        model: model_name,
        metadata: LLMCompleteResponseMetadata {
            baml_is_complete: candidate
                .finish_reason
                .as_ref()
                .map(|r| r == "STOP")
                .unwrap_or(false),
            finish_reason: candidate.finish_reason.clone(),
            prompt_tokens: response.usage_metadata.prompt_token_count,
            output_tokens: response.usage_metadata.candidates_token_count,
            total_tokens: response.usage_metadata.total_token_count,
            cached_input_tokens: response.usage_metadata.cached_content_token_count,
        },
    })
}

fn text_content_part(parts: &[Part]) -> Option<String> {
    let non_thought_parts: Vec<&str> = parts
        .iter()
        .filter(|part| !part.thought.unwrap_or(false))
        .map(|part| part.text.as_str())
        .collect();

    if non_thought_parts.is_empty() {
        None
    } else {
        Some(non_thought_parts.join(""))
    }
}

pub fn scan_google_response_stream(
    client_name: &str,
    request_options: &baml_types::BamlMap<String, serde_json::Value>,
    prompt: &internal_baml_jinja::RenderedPrompt,
    system_now: &web_time::SystemTime,
    instant_now: &web_time::Instant,
    model_name: &Option<String>,
    accumulated: &mut Result<LLMCompleteResponse>,
    event_body: serde_json::Value,
) -> Result<(), LLMErrorResponse> {
    let inner = match accumulated {
        Ok(accumulated) => accumulated,
        // We'll just keep the first error and return it
        Err(e) => return Ok(()),
    };

    let event = GoogleResponse::deserialize(&event_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<GoogleResponse>(),
            event_body
        ))
        .map_err(|e| LLMErrorResponse {
            client: client_name.to_string(),
            model: model_name.clone(),
            prompt: prompt.clone(),
            start_time: *system_now,
            request_options: request_options.clone(),
            latency: instant_now.elapsed(),
            message: format!("{e:?}"),
            code: ErrorCode::UnsupportedResponse(2),
            raw_response: Some(event_body.to_string()),
        })?;

    if let Some(choice) = event.candidates.first() {
        let text_content = &choice
            .content
            .as_ref()
            .and_then(|c| text_content_part(&c.parts));

        if let Some(text_content) = text_content {
            inner.content += text_content;
        }
        inner.metadata.finish_reason = choice.finish_reason.as_ref().map(|r| r.to_string());
        if choice
            .finish_reason
            .as_ref()
            .map(|r| r == "STOP")
            .unwrap_or(false)
        {
            inner.metadata.baml_is_complete = true;
        }
    }

    inner.metadata.prompt_tokens = event.usage_metadata.prompt_token_count;
    inner.metadata.output_tokens = event.usage_metadata.candidates_token_count;
    inner.metadata.total_tokens = event.usage_metadata.total_token_count;
    inner.metadata.cached_input_tokens = event.usage_metadata.cached_content_token_count;

    inner.latency = instant_now.elapsed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use web_time::Duration;

    use super::*;
    use crate::internal::llm_client::primitive::{
        google::types::{Candidate, Content, Part, UsageMetaData},
        tests::MockClient,
    };

    const RESPONSE: &str = r#"
{
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "```json\n{\n  name: {\n    first: null,\n    last: null\n  },\n  email: null,\n  experience: []\n}\n```\n"
          }
        ],
        "role": "model"
      },
      "finishReason": "STOP",
      "avgLogprobs": -0.0090854397186866179
    }
  ],
  "usageMetadata": {
    "promptTokenCount": 166,
    "candidatesTokenCount": 39,
    "totalTokenCount": 205,
    "promptTokensDetails": [
      {
        "modality": "TEXT",
        "tokenCount": 166
      }
    ],
    "candidatesTokensDetails": [
      {
        "modality": "TEXT",
        "tokenCount": 39
      }
    ]
  },
  "modelVersion": "gemini-1.5-flash"
}
        "#;

    const FLASH_25_RESPONSE_STREAMING_RESPONSE: &str = r#"
  {
    "candidates": [
      {
        "content": {
          "parts": [
            {
              "text": "I need the grounding documents to summarize them. Please provide the documents related to the query \"why not?\"."
            }
          ],
          "role": "model"
        },
        "finishReason": "STOP",
        "index": 0
      }
    ],
    "usageMetadata": {
      "promptTokenCount": 135,
      "candidatesTokenCount": 21,
      "totalTokenCount": 404,
      "promptTokensDetails": [
        {
          "modality": "TEXT",
          "tokenCount": 135
        }
      ],
      "thoughtsTokenCount": 248
    },
    "modelVersion": "gemini-2.5-flash",
    "responseId": "THqbaJaMOuCajMcP54G4qAE"
    }
    "#;

    #[test]
    fn test_json_deserialization() {
        let response: GoogleResponse = serde_json::from_str(RESPONSE).unwrap();
        let expected = GoogleResponse {
            candidates: vec![Candidate {
                index: None,
                content: Some(Content {
                    parts: vec![Part {
                        text: "```json\n{\n  name: {\n    first: null,\n    last: null\n  },\n  email: null,\n  experience: []\n}\n```\n".to_string(),
                        inline_data: None,
                        file_data: None,
                        function_call: None,
                        function_response: None,
                        video_metadata: None,
                        thought: None,
                    }],
                    role: Some("model".to_string()),
                }),
                finish_reason: Some("STOP".to_string()),
                safety_ratings: None,
                grounding_metadata: None,
                finish_message: None,
            }],
            prompt_feedback: None,
            usage_metadata: UsageMetaData {
                prompt_token_count: Some(166),
                candidates_token_count: Some(39),
                total_token_count: Some(205),
                cached_content_token_count: None,
            },
        };

        let response_json = serde_json::to_string(&response).unwrap();
        let expected_json = serde_json::to_string(&expected).unwrap();
        assert_eq!(response_json, expected_json);
    }

    #[test]

    fn test_flash25_streaming_deserialization() {
        let _: GoogleResponse = serde_json::from_str(FLASH_25_RESPONSE_STREAMING_RESPONSE).unwrap();
    }

    #[test]
    fn test_parse_google_response() {
        let client = MockClient::new();
        let prompt = vec![];
        let response_body = serde_json::from_str(RESPONSE.trim()).unwrap();
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();
        let model_name = Some("gemini-1.5-flash".to_string());

        let result = parse_google_response(
            &client,
            either::Right(prompt.as_slice()),
            response_body,
            system_now,
            instant_now,
            model_name,
        );

        let expected = LLMCompleteResponse {
            client: "mock".to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(vec![]),
            content: "```json\n{\n  name: {\n    first: null,\n    last: null\n  },\n  email: null,\n  experience: []\n}\n```\n".to_string(),
            start_time: system_now,
            latency: Duration::ZERO,
            model: "gemini-1.5-flash".to_string(),
            request_options: client.request_options().clone(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: true,
                finish_reason: Some("STOP".to_string()),
                prompt_tokens: Some(166),
                output_tokens: Some(39),
                total_tokens: Some(205),
                cached_input_tokens: None,
            },
        };

        if let LLMResponse::Success(mut actual_result) = result {
            actual_result.latency = Duration::ZERO;
            assert_eq!(actual_result, expected);
        } else {
            panic!("Expected LLMResponse::Success, got {result:?}");
        }
    }

    #[test]
    fn test_parse_google_response_with_thinking() {
        // Test case that reproduces the bug with thinking responses
        let response_json = r#"
{
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "Analyzing the image to determine what type of product this is. It appears to be some kind of car accessory. Looking at the shape and structure...",
            "thought": true
          },
          {
            "text": "Based on the mesh pattern and the mounting points, this looks like it's designed to fit in a vehicle's cargo area.",
            "thought": true
          },
          {
            "text": "This product is a \"dog cargo guard\" or a barrier designed to separate the trunk/boot from the passenger area of a car. Its main purpose is to safely contain a pet or cargo."
          }
        ],
        "role": "model"
      },
      "finishReason": "STOP"
    }
  ],
  "usageMetadata": {
    "promptTokenCount": 100,
    "candidatesTokenCount": 50,
    "totalTokenCount": 150
  }
}
        "#;

        let client = MockClient::new();
        let prompt = vec![];
        let response_body: serde_json::Value = serde_json::from_str(response_json.trim()).unwrap();
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();
        let model_name = Some("gemini-2.5-pro".to_string());

        let result = parse_google_response(
            &client,
            either::Right(prompt.as_slice()),
            response_body,
            system_now,
            instant_now,
            model_name,
        );

        if let LLMResponse::Success(actual_result) = result {
            // Should contain only the non-thought content
            assert_eq!(
                actual_result.content,
                "This product is a \"dog cargo guard\" or a barrier designed to separate the trunk/boot from the passenger area of a car. Its main purpose is to safely contain a pet or cargo."
            );
        } else {
            panic!("Expected LLMResponse::Success, got {result:?}");
        }
    }

    #[test]
    fn test_parse_google_response_with_multiple_non_thought_parts() {
        // Test case showing the bug: when there are multiple non-thought parts
        let response_json = r#"
{
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "Let me analyze this image...",
            "thought": true
          },
          {
            "text": "I can see it's related to vehicles.",
            "thought": true
          },
          {
            "text": "This product is a "
          },
          {
            "text": "\"dog cargo guard\""
          },
          {
            "text": " or a barrier designed to separate the trunk/boot from the passenger area of a car."
          }
        ],
        "role": "model"
      },
      "finishReason": "STOP"
    }
  ],
  "usageMetadata": {
    "promptTokenCount": 100,
    "candidatesTokenCount": 50,
    "totalTokenCount": 150
  }
}
        "#;

        let client = MockClient::new();
        let prompt = vec![];
        let response_body: serde_json::Value = serde_json::from_str(response_json.trim()).unwrap();
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();
        let model_name = Some("gemini-2.5-pro".to_string());

        let result = parse_google_response(
            &client,
            either::Right(prompt.as_slice()),
            response_body,
            system_now,
            instant_now,
            model_name,
        );

        if let LLMResponse::Success(actual_result) = result {
            // Currently this will only return "This product is a " (the first non-thought part)
            // But it should concatenate all non-thought parts
            assert_eq!(
                actual_result.content,
                "This product is a \"dog cargo guard\" or a barrier designed to separate the trunk/boot from the passenger area of a car."
            );
        } else {
            panic!("Expected LLMResponse::Success, got {result:?}");
        }
    }
}

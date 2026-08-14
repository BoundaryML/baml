#![cfg(feature = "internal")]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use baml_ids::{FunctionCallId, HttpRequestId};
use baml_runtime::{
    client_registry::{ClientProperty, ClientProvider},
    internal::llm_client::{
        primitive::LLMPrimitiveProvider,
        traits::{HttpContext, WithSingleCallable, WithStreamable},
        LLMResponse,
    },
    RuntimeContext,
};
use baml_types::{BamlMap, BamlMedia, BamlMediaType, BamlValue};
use base64::{prelude::BASE64_STANDARD, Engine};
use futures::StreamExt;
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::TypeIR;
use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage, RenderedPrompt};
use internal_llm_client::OpenAIClientProviderVariant;
use serde_json::json;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, ResponseTemplate,
};

const MODEL: &str = "gpt-transcribe";
const EXPECTED_TRANSCRIPT: &str = "the transcript";
const AUDIO_MIME: &str = "audio/mpeg";
const EXPECTED_FILENAME: &str = "audio.mp3";

#[tokio::test]
async fn openai_transcriptions_audio_returns_transcript_through_real_http() {
    let audio_bytes = b"fake mp3 bytes for the transcription closure";
    let request_observation = Arc::new(Mutex::new(None));
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .and(TranscriptionMultipartMatcher {
            expected_file_bytes: audio_bytes.to_vec(),
            expected_model: MODEL.to_string(),
            expected_filename: EXPECTED_FILENAME.to_string(),
            expected_mime: AUDIO_MIME.to_string(),
            expected_stream: false,
            observation: request_observation.clone(),
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "text": EXPECTED_TRANSCRIPT,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let runtime_context = runtime_context();
    let client_property = transcription_client_property(server.uri());
    let client = LLMPrimitiveProvider::try_from((&client_property, &runtime_context))
        .expect("openai-transcriptions test client should construct");
    let ctx = TestHttpContext::new(runtime_context);
    let prompt = vec![RenderedChatMessage {
        role: "user".to_string(),
        allow_duplicate_role: false,
        parts: vec![ChatMessagePart::Media(BamlMedia::base64(
            BamlMediaType::Audio,
            BASE64_STANDARD.encode(audio_bytes),
            Some(AUDIO_MIME.to_string()),
        ))],
    }];
    let prompt = RenderedPrompt::Chat(prompt);

    let response = client.single_call(&ctx, &prompt).await;

    server.verify().await;

    match response {
        LLMResponse::Success(response) => {
            assert_eq!(response.content, EXPECTED_TRANSCRIPT);
            assert_eq!(response.model, MODEL);
        }
        other => panic!("expected successful transcription response, got {other:?}"),
    }

    let observed = request_observation
        .lock()
        .expect("request observation lock poisoned")
        .clone()
        .expect("multipart matcher should have observed the request");
    assert!(observed.saw_multipart_content_type);
    assert_eq!(observed.file_count, 1);
    assert_eq!(observed.model, Some(MODEL.to_string()));
    assert!(!observed.part_names.iter().any(|name| name == "messages"));
    assert_eq!(observed.stream, None);
}

#[tokio::test]
async fn openai_transcriptions_streams_deltas_through_real_http() {
    let audio_bytes = b"fake mp3 bytes for the streaming transcription closure";
    let request_observation = Arc::new(Mutex::new(None));
    let server = MockServer::start().await;
    let sse_body = concat!(
        "event: transcript.text.delta\n",
        "data: {\"type\":\"transcript.text.delta\",\"delta\":\"the \"}\n\n",
        "event: transcript.text.delta\n",
        "data: {\"type\":\"transcript.text.delta\",\"delta\":\"transcript\"}\n\n",
        "event: transcript.text.done\n",
        "data: {\"type\":\"transcript.text.done\",\"text\":\"the transcript\",\"usage\":{\"type\":\"tokens\",\"input_tokens\":12,\"output_tokens\":2,\"total_tokens\":14}}\n\n"
    );

    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .and(TranscriptionMultipartMatcher {
            expected_file_bytes: audio_bytes.to_vec(),
            expected_model: MODEL.to_string(),
            expected_filename: EXPECTED_FILENAME.to_string(),
            expected_mime: AUDIO_MIME.to_string(),
            expected_stream: true,
            observation: request_observation.clone(),
        })
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;

    let runtime_context = runtime_context();
    let client_property = transcription_client_property(server.uri());
    let client = LLMPrimitiveProvider::try_from((&client_property, &runtime_context))
        .expect("openai-transcriptions test client should construct");
    let ctx = TestHttpContext::new(runtime_context);
    let prompt = RenderedPrompt::Chat(vec![RenderedChatMessage {
        role: "user".to_string(),
        allow_duplicate_role: false,
        parts: vec![ChatMessagePart::Media(BamlMedia::base64(
            BamlMediaType::Audio,
            BASE64_STANDARD.encode(audio_bytes),
            Some(AUDIO_MIME.to_string()),
        ))],
    }]);

    let mut stream = client
        .stream(&ctx, &prompt)
        .await
        .expect("transcription stream should start");
    let mut responses = Vec::new();
    while let Some(response) = stream.next().await {
        match response {
            LLMResponse::Success(response) => responses.push(response),
            other => panic!("expected successful transcription stream event, got {other:?}"),
        }
    }

    server.verify().await;

    assert_eq!(responses.len(), 3);
    let final_response = responses
        .last()
        .expect("stream should emit a final response");
    assert_eq!(final_response.content, EXPECTED_TRANSCRIPT);
    assert_eq!(final_response.model, MODEL);
    assert!(final_response.metadata.baml_is_complete);
    assert_eq!(final_response.metadata.prompt_tokens, Some(12));
    assert_eq!(final_response.metadata.output_tokens, Some(2));
    assert_eq!(final_response.metadata.total_tokens, Some(14));

    let observed = request_observation
        .lock()
        .expect("request observation lock poisoned")
        .clone()
        .expect("multipart matcher should have observed the request");
    assert_eq!(observed.stream.as_deref(), Some("true"));
}

fn transcription_client_property(base_url: String) -> ClientProperty {
    let options: BamlMap<String, BamlValue> = [
        ("base_url".to_string(), BamlValue::String(base_url)),
        (
            "api_key".to_string(),
            BamlValue::String("test-key".to_string()),
        ),
        ("model".to_string(), BamlValue::String(MODEL.to_string())),
    ]
    .into_iter()
    .collect();

    ClientProperty::new(
        "TranscriptionsTestClient".to_string(),
        ClientProvider::OpenAI(OpenAIClientProviderVariant::Transcriptions),
        None,
        options,
    )
}

fn runtime_context() -> RuntimeContext {
    RuntimeContext::new(
        Arc::new(None),
        HashMap::new(),
        HashMap::new(),
        None,
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::<String, TypeIR>::new(),
        Vec::<IndexSet<String>>::new(),
        Vec::<IndexMap<String, TypeIR>>::new(),
        vec![FunctionCallId::new()],
    )
}

struct TestHttpContext {
    http_request_id: HttpRequestId,
    runtime_context: RuntimeContext,
}

impl TestHttpContext {
    fn new(runtime_context: RuntimeContext) -> Self {
        Self {
            http_request_id: HttpRequestId::new(),
            runtime_context,
        }
    }
}

impl HttpContext for TestHttpContext {
    fn http_request_id(&self) -> &HttpRequestId {
        &self.http_request_id
    }

    fn runtime_context(&self) -> &RuntimeContext {
        &self.runtime_context
    }
}

#[derive(Clone)]
struct TranscriptionMultipartMatcher {
    expected_file_bytes: Vec<u8>,
    expected_model: String,
    expected_filename: String,
    expected_mime: String,
    expected_stream: bool,
    observation: Arc<Mutex<Option<ObservedMultipartRequest>>>,
}

impl wiremock::Match for TranscriptionMultipartMatcher {
    fn matches(&self, request: &Request) -> bool {
        let content_type = match request.headers.get("content-type") {
            Some(value) => match value.to_str() {
                Ok(value) => value,
                Err(_) => return false,
            },
            None => return false,
        };

        let parts = match parse_multipart(content_type, &request.body) {
            Ok(parts) => parts,
            Err(_) => return false,
        };

        let part_names = parts
            .iter()
            .filter_map(|part| part.name.clone())
            .collect::<Vec<_>>();
        let file_parts = parts
            .iter()
            .filter(|part| part.name.as_deref() == Some("file"))
            .collect::<Vec<_>>();
        let model = parts
            .iter()
            .find(|part| part.name.as_deref() == Some("model"))
            .map(|part| String::from_utf8_lossy(&part.body).to_string());
        let stream = parts
            .iter()
            .find(|part| part.name.as_deref() == Some("stream"))
            .map(|part| String::from_utf8_lossy(&part.body).to_string());

        let observed = ObservedMultipartRequest {
            saw_multipart_content_type: content_type.starts_with("multipart/form-data"),
            file_count: file_parts.len(),
            model: model.clone(),
            stream: stream.clone(),
            part_names,
        };
        *self
            .observation
            .lock()
            .expect("request observation lock poisoned") = Some(observed);

        if file_parts.len() != 1 {
            return false;
        }

        let file_part = file_parts[0];
        file_part.filename.as_deref() == Some(self.expected_filename.as_str())
            && file_part.content_type.as_deref() == Some(self.expected_mime.as_str())
            && file_part.body == self.expected_file_bytes
            && model.as_deref() == Some(self.expected_model.as_str())
            && stream.as_deref() == self.expected_stream.then_some("true")
            && !parts
                .iter()
                .any(|part| part.name.as_deref() == Some("messages"))
    }
}

#[derive(Clone, Debug)]
struct ObservedMultipartRequest {
    saw_multipart_content_type: bool,
    file_count: usize,
    model: Option<String>,
    stream: Option<String>,
    part_names: Vec<String>,
}

#[derive(Debug)]
struct MultipartPart {
    name: Option<String>,
    filename: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

fn parse_multipart(content_type: &str, body: &[u8]) -> Result<Vec<MultipartPart>, String> {
    let boundary = multipart_boundary(content_type)?;
    let delimiter = format!("--{boundary}");
    let delimiter = delimiter.as_bytes();
    let mut parts = Vec::new();

    for segment in split_on_subslice(body, delimiter) {
        let mut segment = trim_ascii_crlf(segment);
        if segment.is_empty() || segment == b"--" {
            continue;
        }
        if segment.ends_with(b"--") {
            segment = &segment[..segment.len() - 2];
            segment = trim_ascii_crlf(segment);
        }

        let header_end = find_subslice(segment, b"\r\n\r\n")
            .ok_or_else(|| "multipart part missing header/body separator".to_string())?;
        let headers = std::str::from_utf8(&segment[..header_end])
            .map_err(|_| "multipart headers are not utf-8".to_string())?;
        let body = trim_ascii_crlf(&segment[header_end + 4..]).to_vec();

        let disposition = header_value(headers, "content-disposition");
        let content_type = header_value(headers, "content-type");

        parts.push(MultipartPart {
            name: disposition
                .as_deref()
                .and_then(|value| quoted_parameter(value, "name")),
            filename: disposition
                .as_deref()
                .and_then(|value| quoted_parameter(value, "filename")),
            content_type,
            body,
        });
    }

    Ok(parts)
}

fn multipart_boundary(content_type: &str) -> Result<String, String> {
    if !content_type.starts_with("multipart/form-data") {
        return Err("request content type is not multipart/form-data".to_string());
    }

    content_type
        .split(';')
        .map(str::trim)
        .find_map(|parameter| parameter.strip_prefix("boundary="))
        .map(|boundary| boundary.trim_matches('"').to_string())
        .filter(|boundary| !boundary.is_empty())
        .ok_or_else(|| "multipart content type missing boundary".to_string())
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

fn quoted_parameter(header_value: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=\"");
    header_value
        .split(';')
        .map(str::trim)
        .find_map(|parameter| {
            parameter
                .strip_prefix(prefix.as_str())
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_string)
        })
}

fn split_on_subslice<'a>(input: &'a [u8], delimiter: &[u8]) -> Vec<&'a [u8]> {
    let mut segments = Vec::new();
    let mut start = 0;

    while let Some(relative_index) = find_subslice(&input[start..], delimiter) {
        let index = start + relative_index;
        segments.push(&input[start..index]);
        start = index + delimiter.len();
    }

    segments.push(&input[start..]);
    segments
}

fn find_subslice(input: &[u8], needle: &[u8]) -> Option<usize> {
    input
        .windows(needle.len())
        .position(|window| window == needle)
}

fn trim_ascii_crlf(mut input: &[u8]) -> &[u8] {
    while matches!(input.first(), Some(b'\r' | b'\n')) {
        input = &input[1..];
    }
    while matches!(input.last(), Some(b'\r' | b'\n')) {
        input = &input[..input.len() - 1];
    }
    input
}

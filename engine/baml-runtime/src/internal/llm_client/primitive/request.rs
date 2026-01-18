use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
use baml_types::{
    tracing::events::{ClientDetails, HTTPBody, HTTPRequest, HTTPResponse, TraceEvent},
    BamlMap,
};
use bytes::Bytes;
use http::Response as HttpResponse;
use internal_baml_jinja::{RenderContext_Client, RenderedChatMessage, RenderedPrompt};
pub use internal_llm_client::ResponseType;
use reqwest::{header::HeaderMap, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::{
    internal::llm_client::{
        traits::{HttpContext, WithClient},
        ErrorCode, LLMErrorResponse, LLMResponse,
    },
    tracingv2::storage::storage::BAML_TRACER,
};

#[derive(Debug)]
pub struct LoggedHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub url: String,
    pub body: Bytes,
}

impl LoggedHttpResponse {
    pub async fn new_from_reqwest(resp: reqwest::Response) -> Result<Self, reqwest::Error> {
        let status = resp.status();
        let url = resp.url().to_string();
        let headers = resp.headers().clone();
        let body = resp.bytes().await?;

        Ok(Self {
            status,
            headers,
            url,
            body,
        })
    }

    pub fn into_http_response(self) -> HttpResponse<Bytes> {
        let mut builder = http::response::Builder::new().status(self.status);
        for (key, value) in self.headers.iter() {
            builder = builder.header(key, value);
        }
        builder
            .body(self.body)
            .expect("Building HttpResponse failed")
    }
}

pub trait RequestBuilder {
    #[allow(async_fn_in_trait)]
    async fn build_request(
        &self,
        prompt: either::Either<&String, &[RenderedChatMessage]>,
        allow_proxy: bool,
        stream: bool,
        expose_secrets: bool,
    ) -> Result<reqwest::RequestBuilder>;

    fn request_options(&self) -> &BamlMap<String, serde_json::Value>;
    fn http_client(&self) -> &reqwest::Client;
    fn http_config(&self) -> &internal_llm_client::HttpConfig;
}

pub(crate) fn to_prompt(
    prompt: either::Either<&String, &[RenderedChatMessage]>,
) -> internal_baml_jinja::RenderedPrompt {
    match prompt {
        either::Left(p) => RenderedPrompt::Completion(p.clone()),
        either::Right(p) => RenderedPrompt::Chat(p.to_vec()),
    }
}

pub enum JsonBodyInput<'a> {
    ReqwestBody(Option<&'a reqwest::Body>),
    Bytes(&'a [u8]),
    String(String),
}

pub(crate) fn json_body(input: JsonBodyInput) -> Result<serde_json::Value> {
    let string_to_parse = match input {
        JsonBodyInput::ReqwestBody(maybe_body) => {
            if let Some(b) = maybe_body {
                std::str::from_utf8(b.as_bytes().context("Failed to convert body to string")?)?
                    .to_string()
            } else {
                return Ok(serde_json::Value::Null);
            }
        }
        JsonBodyInput::Bytes(b) => std::str::from_utf8(b)?.to_string(),
        JsonBodyInput::String(s) => s,
    };

    // Try to parse as JSON object first
    if let Ok(json) = serde_json::from_str(&string_to_parse) {
        return Ok(json);
    }
    // Try to parse as JSON array
    if let Ok(json) = serde_json::from_str(&format!("[{string_to_parse}]")) {
        return Ok(json);
    }
    // Fall back to string if not valid JSON object or array
    Ok(serde_json::Value::String(string_to_parse))
}

pub(crate) fn json_headers(headers: &HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (key, value) in headers.iter() {
        let value_str = value.to_str().unwrap_or_default().to_string();
        map.insert(key.to_string(), value_str);
    }
    map
}

async fn log_http_response(
    runtime_context: &impl HttpContext,
    status: u16,
    headers: Option<HashMap<String, String>>,
    body: HTTPBody,
    client: &RenderContext_Client,
) {
    let event = TraceEvent::new_raw_llm_response(
        runtime_context.runtime_context().call_id_stack.clone(),
        Arc::new(HTTPResponse::new(
            runtime_context.http_request_id().clone(),
            status,
            headers,
            body,
            ClientDetails {
                name: client.name.clone(),
                provider: client.provider.clone(),
                options: client.options.clone(),
            },
        )),
    );
    BAML_TRACER.lock().unwrap().put(Arc::new(event));
}

pub(crate) async fn build_and_log_outbound_request(
    client: &(impl WithClient + RequestBuilder),
    prompt: either::Either<&String, &[RenderedChatMessage]>,
    allow_proxy: bool,
    stream: bool,
    runtime_context: &impl HttpContext,
) -> Result<(web_time::SystemTime, web_time::Instant, reqwest::Request), LLMResponse> {
    let system_now = web_time::SystemTime::now();
    let instant_now = web_time::Instant::now();

    let req_builder = client
        .build_request(prompt, allow_proxy, stream, true)
        .await
        .context("Failed to build request")
        .map_err(|e| {
            LLMResponse::LLMFailure(LLMErrorResponse {
                client: client.context().name.to_string(),
                model: None,
                prompt: to_prompt(prompt),
                start_time: system_now,
                request_options: client.request_options().clone(),
                latency: instant_now.elapsed(),
                message: format!("Failed to create request builder: {e:#?}"),
                code: ErrorCode::Other(2),
                raw_response: None,
            })
        })?;

    let built_req = match req_builder.build() {
        Ok(req) => req,
        Err(e) => {
            return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                client: client.context().name.to_string(),
                model: None,
                prompt: to_prompt(prompt),
                start_time: system_now,
                request_options: client.request_options().clone(),
                latency: instant_now.elapsed(),
                message: format!("Failed to build request: {e:#?}"),
                code: ErrorCode::Other(2),
                raw_response: None,
            }));
        }
    };

    {
        let event = TraceEvent::new_raw_llm_request(
            runtime_context.runtime_context().call_id_stack.clone(),
            Arc::new(HTTPRequest::new(
                runtime_context.http_request_id().clone(),
                built_req.url().to_string(),
                built_req.method().to_string(),
                json_headers(built_req.headers()),
                HTTPBody::new(
                    built_req
                        .body()
                        .and_then(reqwest::Body::as_bytes)
                        .unwrap_or_default()
                        .into(),
                ),
                ClientDetails {
                    name: client.context().name.clone(),
                    provider: client.context().provider.clone(),
                    options: client.context().options.clone(),
                },
            )),
        );
        BAML_TRACER.lock().unwrap().put(Arc::new(event));
    }

    Ok((system_now, instant_now, built_req))
}

pub async fn execute_request(
    client: &(impl WithClient + RequestBuilder),
    built_req: reqwest::Request,
    prompt: either::Either<&String, &[RenderedChatMessage]>,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    runtime_context: &impl HttpContext,
    consume_body: bool,
) -> Result<(EitherResponse, web_time::SystemTime, web_time::Instant), LLMResponse> {
    let response = match client.http_client().execute(built_req).await {
        Ok(resp) => resp,
        Err(e) => {
            // Detect timeout errors
            let (message, code) = if e.is_timeout() {
                ("Request timed out".to_string(), ErrorCode::Timeout)
            } else {
                (
                    {
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            format!("{e:?}")
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            // Note, Wasm can't use :? for some reason (it makes it so the error looks like garbage). But only doing to_string also makes it so that the full error is not shown. E.g. DNS errors only say "error sending request for url".
                            format!(
                                "{e}\n\nIf you haven't yet, try enabling the proxy (See API Keys button)"
                            )
                        }
                    },
                    e.status()
                        .map_or(ErrorCode::Other(2), ErrorCode::from_status),
                )
            };

            log_http_response(
                runtime_context,
                e.status()
                    .unwrap_or(reqwest::StatusCode::INTERNAL_SERVER_ERROR)
                    .as_u16(),
                None,
                HTTPBody::new(format!("No response. Error: {message}").into_bytes()),
                client.context(),
            )
            .await;

            return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                client: client.context().name.to_string(),
                model: None,
                prompt: to_prompt(prompt),
                start_time: system_now,
                request_options: client.request_options().clone(),
                latency: instant_now.elapsed(),
                message,
                code,
                raw_response: None,
            }));
        }
    };

    if !response.status().is_success() && !consume_body {
        let logged_res = match LoggedHttpResponse::new_from_reqwest(response).await {
            Ok(lr) => lr,
            Err(e) => {
                log_http_response(
                    runtime_context,
                    0,
                    None,
                    HTTPBody::new(format!("Could not read response body: {e:?}").into_bytes()),
                    client.context(),
                )
                .await;
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client: client.context().name.to_string(),
                    model: None,
                    prompt: to_prompt(prompt),
                    start_time: system_now,
                    request_options: client.request_options().clone(),
                    latency: instant_now.elapsed(),
                    message: format!("Could not read response body: {e:?}"),
                    code: e
                        .status()
                        .map_or(ErrorCode::Other(2), ErrorCode::from_status),
                    raw_response: None,
                }));
            }
        };

        let resp_body = match std::str::from_utf8(&logged_res.body) {
            Ok(s) if !s.is_empty() => s.to_string(),
            _ => "<no response or invalid utf-8>".to_string(),
        };

        log_http_response(
            runtime_context,
            logged_res.status.as_u16(),
            Some(json_headers(&logged_res.headers)),
            HTTPBody::new(resp_body.clone().into_bytes()),
            client.context(),
        )
        .await;

        return Err(LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!(
                "Request failed with status code: {}, \n{}",
                logged_res.status, resp_body
            ),
            code: ErrorCode::from_status(logged_res.status),
            raw_response: Some(resp_body),
        }));
    }

    if consume_body {
        let logged_response = match LoggedHttpResponse::new_from_reqwest(response).await {
            Ok(lr) => lr,
            Err(e) => {
                log_http_response(
                    runtime_context,
                    0,
                    None,
                    HTTPBody::new(format!("Could not read response body: {e:?}").into_bytes()),
                    client.context(),
                )
                .await;
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client: client.context().name.to_string(),
                    model: None,
                    prompt: to_prompt(prompt),
                    start_time: system_now,
                    request_options: client.request_options().clone(),
                    latency: instant_now.elapsed(),
                    message: format!("Could not read response body: {e:?}"),
                    code: e
                        .status()
                        .map_or(ErrorCode::Other(2), ErrorCode::from_status),
                    raw_response: None,
                }));
            }
        };

        let resp_body = match std::str::from_utf8(&logged_response.body) {
            Ok(b) => b.to_string(),
            Err(_) => "<invalid utf-8>".to_string(),
        };
        log_http_response(
            runtime_context,
            logged_response.status.as_u16(),
            Some(json_headers(&logged_response.headers)),
            HTTPBody::new(resp_body.into_bytes()),
            client.context(),
        )
        .await;

        Ok((
            EitherResponse::Consumed(logged_response),
            system_now,
            instant_now,
        ))
    } else {
        Ok((EitherResponse::Raw(response), system_now, instant_now))
    }
}

pub(crate) enum EitherResponse {
    Raw(Response),
    Consumed(LoggedHttpResponse),
}

pub async fn make_request(
    client: &(impl WithClient + RequestBuilder),
    prompt: either::Either<&String, &[RenderedChatMessage]>,
    stream: bool,
    runtime_context: &impl HttpContext,
) -> Result<(LoggedHttpResponse, web_time::SystemTime, web_time::Instant), LLMResponse> {
    let (system_now, instant_now, built_req) =
        build_and_log_outbound_request(client, prompt, true, stream, runtime_context).await?;

    match execute_request(
        client,
        built_req,
        prompt,
        system_now,
        instant_now,
        runtime_context,
        true,
    )
    .await?
    {
        (EitherResponse::Consumed(logged_res), sys, inst) => Ok((logged_res, sys, inst)),
        (EitherResponse::Raw(_), _, _) => unreachable!("We always consume the body here."),
    }
}

pub async fn make_parsed_request(
    client: &(impl WithClient + RequestBuilder),
    model_name: Option<String>,
    prompt: either::Either<&String, &[RenderedChatMessage]>,
    stream: bool,
    response_type: ResponseType,
    runtime_context: &impl HttpContext,
) -> LLMResponse {
    let (response, system_now, instant_now) =
        match make_request(client, prompt, stream, runtime_context).await {
            Ok((response, system_now, instant_now)) => (response, system_now, instant_now),
            Err(e) => return e,
        };

    // Capture raw response body as string before parsing
    let raw_body_str = std::str::from_utf8(&response.body)
        .ok()
        .map(|s| s.to_string());

    let response_body = serde_json::from_slice::<serde_json::Value>(&response.body).map_err(|e| {
        LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!("Failed to parse JSON: {e}"),
            code: ErrorCode::from_status(response.status),
            raw_response: raw_body_str.clone(),
        })
    });

    let response_body = match response_body {
        Ok(response) => response,
        Err(e) => {
            return LLMResponse::LLMFailure(LLMErrorResponse {
                client: client.context().name.to_string(),
                model: None,
                prompt: to_prompt(prompt),
                start_time: system_now,
                request_options: client.request_options().clone(),
                latency: instant_now.elapsed(),
                message: e.to_string(),
                code: ErrorCode::from_status(response.status),
                raw_response: raw_body_str,
            })
        }
    };

    if response.status != StatusCode::OK {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!(
                "Request failed with status code: {}. {}",
                response.status, response_body
            ),
            code: ErrorCode::from_status(response.status),
            raw_response: Some(response_body.to_string()),
        });
    }

    match response_type {
        ResponseType::OpenAI => super::openai::response_handler::parse_openai_response(
            client,
            prompt,
            response_body,
            system_now,
            instant_now,
            model_name,
        ),
        ResponseType::Anthropic => super::anthropic::response_handler::parse_anthropic_response(
            client,
            prompt,
            response_body,
            system_now,
            instant_now,
            model_name,
        ),
        ResponseType::Google => super::google::response_handler::parse_google_response(
            client,
            prompt,
            response_body,
            system_now,
            instant_now,
            model_name,
        ),
        ResponseType::Vertex => super::vertex::response_handler::parse_vertex_response(
            client,
            prompt,
            response_body,
            system_now,
            instant_now,
            model_name,
        ),
        ResponseType::OpenAIResponses => {
            super::openai::response_handler::parse_openai_responses_response(
                client,
                prompt,
                response_body,
                system_now,
                instant_now,
                model_name,
            )
        }
    }
}

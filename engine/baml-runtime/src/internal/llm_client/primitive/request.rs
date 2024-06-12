use std::{collections::HashMap, fmt::format};

use anyhow::{Context, Result};
use internal_baml_jinja::RenderedChatMessage;
use reqwest::Response;
use serde::de::DeserializeOwned;

use crate::internal::llm_client::{traits::WithClient, ErrorCode, LLMErrorResponse, LLMResponse};

pub trait RequestBuilder {
    fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        stream: bool,
    ) -> reqwest::RequestBuilder;

    fn invocation_params(&self) -> &HashMap<String, serde_json::Value>;

    fn http_client(&self) -> &reqwest::Client;
}

fn to_prompt(
    prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
) -> internal_baml_jinja::RenderedPrompt {
    match prompt {
        either::Left(prompt) => internal_baml_jinja::RenderedPrompt::Completion(prompt.clone()),
        either::Right(prompt) => internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
    }
}

pub async fn make_request(
    client: &(impl WithClient + RequestBuilder),
    prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
    stream: bool,
) -> Result<(Response, web_time::SystemTime, web_time::Instant), LLMResponse> {
    let (system_now, instant_now) = (web_time::SystemTime::now(), web_time::Instant::now());
    log::info!("Making request using client {}", client.context().name);

    let req = match client.build_request(prompt, stream).build() {
        Ok(req) => req,
        Err(e) => {
            return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                client: client.context().name.to_string(),
                model: None,
                prompt: to_prompt(prompt),
                start_time: system_now,
                invocation_params: client.invocation_params().clone(),
                latency: instant_now.elapsed(),
                message: e.to_string(),
                code: ErrorCode::Other(2),
            }));
        }
    };

    let response = match client.http_client().execute(req).await {
        Ok(response) => response,
        Err(e) => {
            return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                client: client.context().name.to_string(),
                model: None,
                prompt: to_prompt(prompt),
                start_time: system_now,
                invocation_params: client.invocation_params().clone(),
                latency: instant_now.elapsed(),
                message: e.to_string(),
                code: ErrorCode::Other(2),
            }));
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Err(LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            invocation_params: client.invocation_params().clone(),
            latency: instant_now.elapsed(),
            message: format!(
                "Request failed: {}",
                response.text().await.unwrap_or("<no response>".into())
            ),
            code: ErrorCode::from_status(status),
        }));
    }

    Ok((response, system_now, instant_now))
}

pub async fn make_parsed_request<T: DeserializeOwned>(
    client: &(impl WithClient + RequestBuilder),
    prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
    stream: bool,
) -> Result<(T, web_time::SystemTime, web_time::Instant), LLMResponse> {
    let (response, system_now, instant_now) = make_request(client, prompt, stream).await?;

    let raw_response = response
        .text()
        .await
        .context("Failed to read response text")
        .unwrap_or_default();

    // Attempt to parse the response JSON
    match serde_json::from_str::<T>(&raw_response).with_context(|| {
        format!(
            "Failed to parse into a response accepted by {}. Response: {}",
            std::any::type_name::<T>(),
            raw_response
        )
    }) {
        Ok(response) => Ok((response, system_now, instant_now)),
        Err(e) => Err(LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            invocation_params: client.invocation_params().clone(),
            latency: instant_now.elapsed(),
            message: e.to_string(),
            code: ErrorCode::Other(2),
        })),
    }
}

use std::collections::HashMap;

use aws_config::{identity::IdentityCache, retry::RetryConfig, BehaviorVersion, ConfigLoader};
use aws_sdk_bedrockruntime::{self as bedrock, operation::converse::ConverseOutput};

use anyhow::{Context, Result};
use aws_smithy_json::serialize::JsonObjectWriter;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::Blob;
use baml_types::BamlMediaContent;
use baml_types::{BamlMedia, BamlMediaType};
use futures::stream;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use serde::Deserialize;
use serde_json::Map;
use web_time::Instant;
use web_time::SystemTime;

use crate::internal::llm_client::traits::{ToProviderMessageExt, WithClientProperties};
use crate::internal::llm_client::AllowedMetadata;
use crate::internal::llm_client::{
    primitive::request::RequestBuilder,
    traits::{
        StreamResponse, WithChat, WithClient, WithNoCompletion, WithRenderRawCurl, WithRetryPolicy,
        WithStreamChat,
    },
    ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
    ModelFeatures, ResolveMediaUrls,
};

use crate::{RenderCurlSettings, RuntimeContext};

// stores properties required for making a post request to the API
struct RequestProperties {
    model_id: String,

    default_role: String,
    inference_config: Option<bedrock::types::InferenceConfiguration>,
    allowed_metadata: AllowedMetadata,

    request_options: HashMap<String, serde_json::Value>,
    ctx_env: HashMap<String, String>,
}

// represents client that interacts with the Anthropic API
pub struct AwsClient {
    pub name: String,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: RequestProperties,
}

fn resolve_properties(client: &ClientWalker, ctx: &RuntimeContext) -> Result<RequestProperties> {
    let mut properties = (&client.item.elem.options)
        .iter()
        .map(|(k, v)| {
            Ok((
                k.into(),
                ctx.resolve_expression::<serde_json::Value>(v)
                    .context(format!(
                        "client {} could not resolve options.{}",
                        client.name(),
                        k
                    ))?,
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;

    let model_id = properties
        .remove("model_id")
        .context("model_id is required")?
        .as_str()
        .context("model_id should be a string")?
        .to_string();

    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "user".to_string());
    let allowed_metadata = match properties.remove("allowed_role_metadata") {
        Some(allowed_metadata) => serde_json::from_value(allowed_metadata).context(
            "allowed_role_metadata must be an array of keys. For example: ['key1', 'key2']",
        )?,
        None => AllowedMetadata::None,
    };
    let inference_config = match properties.remove("inference_configuration") {
        Some(v) => Some(
            super::types::InferenceConfiguration::deserialize(v)
                .context("Failed to parse inference_configuration")?
                .into(),
        ),
        None => None,
    };

    Ok(RequestProperties {
        model_id,
        default_role,
        inference_config,
        allowed_metadata,
        request_options: properties,
        ctx_env: ctx.env.clone(),
    })
}

impl AwsClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<AwsClient> {
        let post_properties = resolve_properties(client, ctx)?;
        let default_role = post_properties.default_role.clone(); // clone before moving

        Ok(Self {
            name: client.name().into(),
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
                default_role: default_role,
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: true,
                resolve_media_urls: ResolveMediaUrls::Always,
                allowed_metadata: post_properties.allowed_metadata.clone(),
            },
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            properties: post_properties,
        })
    }

    pub fn request_options(&self) -> &std::collections::HashMap<String, serde_json::Value> {
        &self.properties.request_options
    }

    // TODO: this should be memoized on client construction, but because config loading is async,
    // we can't do this in AwsClient::new (which is called from LLMPRimitiveProvider::try_from)
    async fn client_anyhow(&self) -> Result<bedrock::Client> {
        let loader: ConfigLoader = {
            cfg_if::cfg_if! {
                if #[cfg(target_arch = "wasm32")] {
                    use aws_config::Region;
                    use aws_credential_types::Credentials;

                    let (aws_region, aws_access_key_id, aws_secret_access_key) = match (
                        self.properties.ctx_env.get("AWS_REGION"),
                        self.properties.ctx_env.get("AWS_ACCESS_KEY_ID"),
                        self.properties.ctx_env.get("AWS_SECRET_ACCESS_KEY"),
                    ) {
                        (Some(aws_region), Some(aws_access_key_id), Some(aws_secret_access_key)) => {
                            (aws_region, aws_access_key_id, aws_secret_access_key)
                        }
                        _ => {
                            anyhow::bail!(
                                "AWS_REGION, AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY must be set in the environment"
                            )
                        }
                    };

                    let loader = super::wasm::load_aws_config()
                        .region(Region::new(aws_region.clone()))
                        .credentials_provider(Credentials::new(
                            aws_access_key_id.clone(),
                            aws_secret_access_key.clone(),
                            None,
                            None,
                            "baml-runtime/wasm",
                        ));

                    loader
                } else {
                    aws_config::defaults(BehaviorVersion::latest())
                }
            }
        };

        let config = loader
            .retry_config(RetryConfig::disabled())
            .identity_cache(IdentityCache::no_cache())
            .load()
            .await;

        Ok(bedrock::Client::new(&config))
    }

    async fn chat_anyhow<'r>(&self, response: &'r ConverseOutput) -> Result<&'r String> {
        let Some(bedrock::types::ConverseOutput::Message(ref message)) = response.output else {
            anyhow::bail!(
                "Expected message output in response, but is type {}",
                "unknown"
            );
        };
        let content = message
            .content
            .get(0)
            .context("Expected message output to have content")?;
        let bedrock::types::ContentBlock::Text(ref content) = content else {
            anyhow::bail!(
                "Expected message output to be text, got {}",
                match content {
                    bedrock::types::ContentBlock::Image(_) => "image",
                    bedrock::types::ContentBlock::GuardContent(_) => "guardContent",
                    bedrock::types::ContentBlock::ToolResult(_) => "toolResult",
                    bedrock::types::ContentBlock::ToolUse(_) => "toolUse",
                    bedrock::types::ContentBlock::Text(_) => "text",
                    _ => "unknown",
                }
            );
        };

        Ok(content)
    }

    fn build_request(
        &self,
        ctx: &RuntimeContext,
        chat_messages: &Vec<RenderedChatMessage>,
    ) -> Result<bedrock::operation::converse::ConverseInput> {
        let mut system_message = None;
        let mut chat_slice = chat_messages.as_slice();

        if let Some((first, remainder_slice)) = chat_slice.split_first() {
            if first.role == "system" {
                system_message = Some(
                    first
                        .parts
                        .iter()
                        .map(|part| self.part_to_system_message(part))
                        .collect::<Result<_>>()?,
                );
                chat_slice = remainder_slice;
            }
        }

        let converse_messages = chat_slice
            .iter()
            .map(|m| self.role_to_message(m))
            .collect::<Result<Vec<_>>>()?;

        bedrock::operation::converse::ConverseInput::builder()
            .set_inference_config(self.properties.inference_config.clone())
            .set_model_id(Some(self.properties.model_id.clone()))
            .set_system(system_message)
            .set_messages(Some(converse_messages))
            .build()
            .context("Failed to convert BAML prompt to AWS Bedrock request")
    }
}

fn try_to_json<
    Ser: Fn(
        &mut JsonObjectWriter,
        &T,
    ) -> Result<(), ::aws_smithy_types::error::operation::SerializationError>,
    T,
>(
    shape: Ser,
    input: &T,
) -> Result<String> {
    let mut out = String::new();
    let mut object = JsonObjectWriter::new(&mut out);
    shape(&mut object, input)?;
    object.finish();

    Ok(out)
}

impl WithRenderRawCurl for AwsClient {
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<internal_baml_jinja::RenderedChatMessage>,
        _render_settings: RenderCurlSettings,
    ) -> Result<String> {
        let converse_input = self.build_request(ctx, prompt)?;

        // TODO(sam): this is fucked up. The SDK actually hides all the serializers inside the crate and doesn't let the user access them.

        Ok(format!(
            "Note, this is not yet complete!\n\nSee: https://docs.aws.amazon.com/cli/latest/reference/bedrock-runtime/converse.html\n\naws bedrock converse --model-id {} --messages {} {}",
            converse_input.model_id.unwrap_or("<model_id>".to_string()),
            "<messages>",
            "TODO"
        ))
    }
}

// getters for client info
impl WithRetryPolicy for AwsClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClientProperties for AwsClient {
    fn client_properties(&self) -> &HashMap<String, serde_json::Value> {
        &self.properties.request_options
    }
    fn allowed_metadata(&self) -> &crate::internal::llm_client::AllowedMetadata {
        &self.properties.allowed_metadata
    }
}

impl WithClient for AwsClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for AwsClient {}

impl WithStreamChat for AwsClient {
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        chat_messages: &Vec<RenderedChatMessage>,
    ) -> StreamResponse {
        let client = self.context.name.to_string();
        let model = Some(self.properties.model_id.clone());
        let request_options = self.properties.request_options.clone();
        let prompt = internal_baml_jinja::RenderedPrompt::Chat(chat_messages.clone());

        let aws_client = match self.client_anyhow().await {
            Ok(c) => c,
            Err(e) => {
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{:#?}", e),
                    code: ErrorCode::Other(2),
                }));
            }
        };

        let request = match self.build_request(ctx, chat_messages) {
            Ok(r) => r,
            Err(e) => {
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{:#?}", e),
                    code: ErrorCode::Other(2),
                }))
            }
        };

        let request = aws_client
            .converse_stream()
            .set_model_id(request.model_id)
            .set_inference_config(request.inference_config)
            .set_system(request.system)
            .set_messages(request.messages);

        let system_start = SystemTime::now();
        let instant_start = Instant::now();

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: system_start,
                    request_options,
                    latency: instant_start.elapsed(),
                    message: format!("{:#?}", e),
                    code: match e {
                        SdkError::ConstructionFailure(_) => ErrorCode::Other(2),
                        SdkError::TimeoutError(_) => ErrorCode::Other(2),
                        SdkError::DispatchFailure(_) => ErrorCode::Other(2),
                        SdkError::ResponseError(e) => {
                            ErrorCode::UnsupportedResponse(e.raw().status().as_u16())
                        }
                        SdkError::ServiceError(e) => {
                            let status = e.raw().status();
                            match status.as_u16() {
                                400 => ErrorCode::InvalidAuthentication,
                                403 => ErrorCode::NotSupported,
                                429 => ErrorCode::RateLimited,
                                500 => ErrorCode::ServerError,
                                503 => ErrorCode::ServiceUnavailable,
                                _ => {
                                    if status.is_server_error() {
                                        ErrorCode::ServerError
                                    } else {
                                        ErrorCode::Other(status.as_u16())
                                    }
                                }
                            }
                        }
                        _ => ErrorCode::Other(2),
                    },
                }));
            }
        };

        let stream = stream::unfold(
            (
                Some(LLMCompleteResponse {
                    client,
                    prompt,
                    content: "".to_string(),
                    start_time: system_start,
                    latency: instant_start.elapsed(),
                    model: self.properties.model_id.clone(),
                    request_options,
                    metadata: LLMCompleteResponseMetadata {
                        baml_is_complete: false,
                        finish_reason: None,
                        prompt_tokens: None,
                        output_tokens: None,
                        total_tokens: None,
                    },
                }),
                response,
            ),
            move |(initial_state, mut response)| {
                async move {
                    let Some(mut new_state) = initial_state else {
                        return None;
                    };
                    match response.stream.recv().await {
                        Ok(Some(message)) => {
                            log::trace!("Received message: {:#?}", message);
                            match message {
                                bedrock::types::ConverseStreamOutput::ContentBlockDelta(
                                    content_block_delta,
                                ) => {
                                    if let Some(bedrock::types::ContentBlockDelta::Text(
                                        ref delta,
                                    )) = content_block_delta.delta
                                    {
                                        new_state.content += delta;
                                        // TODO- handle
                                    }
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::ContentBlockStart(_) => {
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::ContentBlockStop(_) => {
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::MessageStart(_) => {
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::MessageStop(stop) => {
                                    new_state.metadata.baml_is_complete = match stop.stop_reason {
                                        bedrock::types::StopReason::StopSequence
                                        | bedrock::types::StopReason::EndTurn => true,
                                        _ => false,
                                    };
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::Metadata(metadata) => {
                                    if let Some(usage) = metadata.usage() {
                                        new_state.metadata.prompt_tokens =
                                            Some(usage.input_tokens() as u64);
                                        new_state.metadata.output_tokens =
                                            Some(usage.output_tokens() as u64);
                                        new_state.metadata.total_tokens =
                                            Some((usage.total_tokens()) as u64);
                                    }
                                }
                                _ => {
                                    // TODO- handle
                                }
                            }
                            new_state.latency = instant_start.elapsed();
                            Some((
                                LLMResponse::Success(new_state.clone()),
                                (Some(new_state), response),
                            ))
                        }
                        Ok(None) => None,
                        Err(e) => Some((
                            LLMResponse::LLMFailure(LLMErrorResponse {
                                client: new_state.client,
                                model: Some(new_state.model),
                                prompt: new_state.prompt,
                                start_time: new_state.start_time,
                                request_options: new_state.request_options,
                                latency: instant_start.elapsed(),
                                message: format!("Failed to parse event: {:#?}", e),
                                code: ErrorCode::Other(2),
                            }),
                            (None, response),
                        )),
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }
}

impl AwsClient {
    fn to_chat_message(&self, text: &str) -> Result<bedrock::types::ContentBlock> {
        Ok(bedrock::types::ContentBlock::Text(text.to_string()))
    }

    fn to_media_message(
        &self,
        media: &baml_types::BamlMedia,
    ) -> Result<bedrock::types::ContentBlock> {
        if media.media_type != BamlMediaType::Image {
            anyhow::bail!(
                "AWS supports images, but does not support this media type: {:#?}",
                media
            )
        }
        match &media.content {
            BamlMediaContent::File(_) => {
                anyhow::bail!(
                    "BAML internal error (AWSBedrock): file should have been resolved to base64"
                )
            }
            BamlMediaContent::Url(_) => {
                anyhow::bail!(
                    "BAML internal error (AWSBedrock): media URL should have been resolved to base64"
                )
            }
            BamlMediaContent::Base64(b64_media) => Ok(bedrock::types::ContentBlock::Image(
                bedrock::types::ImageBlock::builder()
                    .set_format(Some(bedrock::types::ImageFormat::from(
                        {
                            let mime_type = media.mime_type_as_ok()?;
                            match mime_type.strip_prefix("image/") {
                                Some(s) => s.to_string(),
                                None => mime_type,
                            }
                        }
                        .as_str(),
                    )))
                    .set_source(Some(bedrock::types::ImageSource::Bytes(Blob::new(
                        aws_smithy_types::base64::decode(b64_media.base64.clone())?,
                    ))))
                    .build()
                    .context("Failed to build image block")?,
            )),
        }
    }

    fn role_to_message(&self, msg: &RenderedChatMessage) -> Result<bedrock::types::Message> {
        let content = msg
            .parts
            .iter()
            .map(|part| self.part_to_message(part))
            .collect::<Result<Vec<_>>>()?;

        bedrock::types::Message::builder()
            .set_role(Some(msg.role.as_str().into()))
            .set_content(Some(content))
            .build()
            .map_err(|e: bedrock::error::BuildError| e.into())
    }

    fn part_to_system_message(
        &self,
        part: &ChatMessagePart,
    ) -> Result<bedrock::types::SystemContentBlock> {
        match part {
            ChatMessagePart::Text(t) => Ok(bedrock::types::SystemContentBlock::Text(t.clone())),
            ChatMessagePart::Media(_) => anyhow::bail!(
                "AWS Bedrock only supports text blocks for system messages, but got {:#?}",
                part
            ),
            ChatMessagePart::WithMeta(p, _) => self.part_to_system_message(p),
        }
    }

    fn part_to_message(&self, part: &ChatMessagePart) -> Result<bedrock::types::ContentBlock> {
        match part {
            ChatMessagePart::Text(t) => self.to_chat_message(t),
            ChatMessagePart::Media(m) => self.to_media_message(m),
            ChatMessagePart::WithMeta(p, _) => {
                // All metadata is dropped as AWS does not support it
                // this means caching, etc.
                self.part_to_message(&p)
            }
        }
    }

    fn parts_to_message(
        &self,
        parts: &Vec<ChatMessagePart>,
    ) -> Result<Vec<bedrock::types::ContentBlock>> {
        Ok(parts
            .iter()
            .map(|p| self.part_to_message(p))
            .collect::<Result<Vec<_>>>()?)
    }
}

impl WithChat for AwsClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    async fn chat(
        &self,
        _ctx: &RuntimeContext,
        chat_messages: &Vec<RenderedChatMessage>,
    ) -> LLMResponse {
        let client = self.context.name.to_string();
        let model = Some(self.properties.model_id.clone());
        let request_options = self.properties.request_options.clone();
        let prompt = internal_baml_jinja::RenderedPrompt::Chat(chat_messages.clone());

        let aws_client = match self.client_anyhow().await {
            Ok(c) => c,
            Err(e) => {
                return LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{:#?}", e),
                    code: ErrorCode::Other(2),
                })
            }
        };

        let request = match self.build_request(_ctx, chat_messages) {
            Ok(r) => r,
            Err(e) => {
                return LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{:#?}", e),
                    code: ErrorCode::Other(2),
                })
            }
        };
        let request = aws_client
            .converse()
            .set_model_id(request.model_id)
            .set_inference_config(request.inference_config)
            .set_system(request.system)
            .set_messages(request.messages);

        let system_start = SystemTime::now();
        let instant_start = Instant::now();

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                return LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: system_start,
                    request_options,
                    latency: instant_start.elapsed(),
                    message: format!("{:#?}", e),
                    // TODO: derive this from the aws-returned error
                    code: ErrorCode::Other(2),
                });
            }
        };

        match self.chat_anyhow(&response).await {
            Ok(content) => LLMResponse::Success(LLMCompleteResponse {
                client,
                prompt,
                content: content.clone(),
                start_time: system_start.clone(),
                latency: instant_start.elapsed(),
                request_options,
                model: self.properties.model_id.clone(),
                metadata: LLMCompleteResponseMetadata {
                    baml_is_complete: match response.stop_reason {
                        bedrock::types::StopReason::StopSequence
                        | bedrock::types::StopReason::EndTurn => true,
                        _ => false,
                    },
                    finish_reason: Some(response.stop_reason().as_str().into()),
                    prompt_tokens: response
                        .usage
                        .as_ref()
                        .map(|i| i.input_tokens.try_into().ok())
                        .flatten(),
                    output_tokens: response
                        .usage
                        .as_ref()
                        .map(|i| i.output_tokens.try_into().ok())
                        .flatten(),
                    total_tokens: response
                        .usage
                        .as_ref()
                        .map(|i| i.total_tokens.try_into().ok())
                        .flatten(),
                },
            }),
            Err(e) => LLMResponse::LLMFailure(LLMErrorResponse {
                client,
                model,
                prompt,
                start_time: system_start,
                request_options,
                latency: instant_start.elapsed(),
                message: format!("{:#?}", e),
                code: ErrorCode::Other(200),
            }),
        }
    }
}

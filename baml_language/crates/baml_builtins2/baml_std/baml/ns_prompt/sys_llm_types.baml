// Internal compatibility schemas for the Rust-only `sys_llm` implementation.
// They are generated into Rust data types but are not part of the `ai.Agent`
// execution architecture. New provider implementations expose `ai.Client`
// values and lower `ai.Prompt` directly.

class MediaUrlHandler {
    image: string | null,
    audio: string | null,
    video: string | null,
    pdf: string | null,
}

class PrimitiveClientOptions {
    model: string | null,
    base_url: string | null,
    allowed_role_metadata: unknown | null,
    finish_reason_allow_list: string[] | null,
    finish_reason_deny_list: string[] | null,
    supports_streaming: bool | null,
    default_role: string | null,
    allowed_roles: string[] | null,
    remap_roles: map<string, string> | null,
    api_key: string | null,
    provider_options: AnthropicOptions
        | AzureOpenAiOptions
        | BedrockOptions
        | GoogleAiOptions
        | VertexAiOptions
        | null,
    media_url_handler: MediaUrlHandler | null,
    headers: map<string, string>,
    query_params: map<string, string>,
    request_body: map<string, unknown>,
}

class AzureOpenAiOptions {
    resource_name: string | null,
    deployment_id: string | null,
    api_version: string,
    max_tokens: int | null,
}

class AnthropicOptions {
    max_tokens: int | null,
}

class GoogleAiOptions {
    enterprise: bool | null,
    credentials: string | null,
    credentials_content: string | null,
    location: string | null,
    project_id: string | null,
}

class VertexAiOptions {
    credentials: string | null,
    credentials_content: string | null,
    location: string | null,
    project_id: string | null,
}

class BedrockOptions {
    region: string | null,
    endpoint_url: string | null,
    access_key_id: string | null,
    secret_access_key: string | null,
    session_token: string | null,
    profile: string | null,
    stop_sequences: string[] | null,
    max_tokens: int | null,
    temperature: float | null,
    top_p: float | null,
}

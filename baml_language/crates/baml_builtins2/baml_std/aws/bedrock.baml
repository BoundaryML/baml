// `aws.BedrockClient` — one model turn against the Amazon Bedrock Converse
// API (`POST /model/{modelId}/converse`).
//
// The wire format, the URL and the response parse are pure BAML
// (`root.internal`, ns_internal/bedrock.baml); only the SigV4 signature is
// Rust, because the stdlib has no HMAC-SHA256 (`root.internal.sign_request`,
// ns_internal/auth.baml).
//
// Credentials and region are NEVER read at construction time. `new()` is pure,
// so declaring a client is always safe; the AWS provider chain (environment,
// shared config/credentials files, `credential_process`, SSO cache, container
// endpoint, EC2 IMDS) runs when a request is signed.

class BedrockClient {
    /// A foundation-model id (`anthropic.claude-3-5-haiku-20241022-v1:0`), a
    /// cross-region inference-profile id (`us.anthropic.claude-haiku-4-5-…`),
    /// or a full model/inference-profile ARN. It is a URL path label, not a
    /// body field: `/` and `:` are percent-encoded so an ARN stays one path
    /// segment. There is no default — model availability is per-account and
    /// per-region, so there is no id that is right everywhere.
    model: string,

    // --- credentials and endpoint ------------------------------------------
    /// Explicit region. `null` resolves `AWS_REGION`, then
    /// `AWS_DEFAULT_REGION`, then the active profile's `region`; if none of
    /// those produce one, the request fails with a message naming all of them
    /// (unless `endpoint_url` is set, which needs no region for the URL —
    /// signing still scopes to the resolved region).
    region: string?,
    /// Named profile in the shared config/credentials files. `null` uses
    /// `AWS_PROFILE`, else `default`.
    profile: string?,
    /// Explicit static credentials. Both `access_key_id` and
    /// `secret_access_key` must be set to short-circuit the provider chain; a
    /// lone key id falls through rather than signing with half an identity.
    access_key_id: string?,
    secret_access_key: string?,
    /// Session token for temporary (STS/SSO) credentials.
    session_token: string?,
    /// Replaces `https://bedrock-runtime.<region>.amazonaws.com` — a VPC
    /// endpoint, a LocalStack instance, or a test server. A trailing slash is
    /// tolerated. Setting it bypasses region resolution for the URL.
    ///
    /// A literal string, or `env.NAME` (a late-bound `baml.env.Ref` read at
    /// request time) so a `client Name = ...` declaration — evaluated during
    /// `$init`, which cannot run io sysops — stays pure while a deployment
    /// still points the client somewhere else.
    endpoint_url: ai.Credential?,

    // --- inference parameters (Converse `inferenceConfig`) ------------------
    /// Maximum tokens to generate. Wire type is a 32-bit int.
    max_tokens: int?,
    temperature: float?,
    top_p: float?,
    stop_sequences: string[]?,

    // --- tools --------------------------------------------------------------
    /// `toolConfig.toolChoice`: `"auto"`, `"any"` (alias `"required"`), or a
    /// tool name to force. `null` omits the field, which is the API default
    /// and the only shape every Bedrock model family accepts.
    ///
    /// NOTE on replayed tool results: `toolResult.status` ("success"/"error")
    /// is only supported by Amazon Nova and Anthropic Claude 3/4, so it is sent
    /// only when `model` names one of those families. Other families still get
    /// the result text, which carries the tool's error message.
    tool_choice: string?,

    // --- prompt caching -----------------------------------------------------
    /// Append a `cachePoint` after the system content, making the system
    /// prompt reusable across calls. Per-message checkpoints come from the
    /// `cache_point` key of `ai.PromptMessage.metadata` instead.
    cache_system_prompt: bool,
    /// Append a `cachePoint` after the tool list, making the tool definitions
    /// reusable across calls.
    cache_tools: bool,

    // --- passthrough (PLAN P0.3) --------------------------------------------
    /// `additionalModelRequestFields`: model-specific parameters Converse has
    /// no first-class field for, e.g. `{"top_k": 50}` or
    /// `{"reasoning_config": {"type": "enabled", "budget_tokens": 1024}}`.
    /// This is the engine's `additional_model_request_fields` option.
    additional_model_request_fields: baml.json.json?,
    /// Deep-merged into the serialized request as the last step before it is
    /// signed: maps merge recursively, a `null` value deletes that key, and
    /// everything else replaces. Use it to reach any Converse field the typed
    /// request does not model (`guardrailConfig`, `promptVariables`, …).
    request_body: baml.json.json?,
    /// Extra request headers, applied BEFORE signing so they are covered by
    /// the signature. Names are lowercased; a name the client already sets is
    /// replaced, not duplicated.
    headers: map<string, string>?,
    /// Extra query parameters, appended (and percent-encoded) BEFORE signing.
    query_params: map<string, string>?,
    /// Record request/response bodies on the LLMCall wire records delivered
    /// through on_event. Status, headers, and timing are always recorded; this
    /// flag only gates the bodies.
    capture_wire: bool,

    function new(
        model: string,
        region: string? = null,
        profile: string? = null,
        access_key_id: string? = null,
        secret_access_key: string? = null,
        session_token: string? = null,
        endpoint_url: ai.Credential? = null,
        max_tokens: int? = null,
        temperature: float? = null,
        top_p: float? = null,
        stop_sequences: string[]? = null,
        tool_choice: string? = null,
        cache_system_prompt: bool = false,
        cache_tools: bool = false,
        additional_model_request_fields: baml.json.json? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        capture_wire: bool = true,
    ) -> BedrockClient {
        BedrockClient {
            model: model,
            region: region,
            profile: profile,
            access_key_id: access_key_id,
            secret_access_key: secret_access_key,
            session_token: session_token,
            endpoint_url: endpoint_url,
            max_tokens: max_tokens,
            temperature: temperature,
            top_p: top_p,
            stop_sequences: stop_sequences,
            tool_choice: tool_choice,
            cache_system_prompt: cache_system_prompt,
            cache_tools: cache_tools,
            additional_model_request_fields: additional_model_request_fields,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
            capture_wire: capture_wire,
        }
    }

    /// The request-time endpoint override: the explicit value (a literal, or
    /// the value of the variable an `env.NAME` ref points at), else null
    /// (build the regional host). Bedrock has no canonical endpoint variable.
    function resolved_endpoint_url(self) -> string? {
        ai.wire.resolve_credential(self.endpoint_url, null)
    }

    implements ai.Client {
        function id(self) -> string {
            `bedrock/${self.model}`
        }

        // This is deliberately the unsigned Converse request. Signing is an
        // invocation concern and would read credentials during a snapshot.
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            aws.internal.build_request(self, input, preview = true) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    // NOTE: deliberately no `implements ai.stream.StreamingClient`.
    // ConverseStream frames its response as binary
    // `application/vnd.amazon.eventstream` (length-prefixed frames with a
    // header block and a CRC), which `baml.http.fetch_sse` cannot consume —
    // decoding it needs a new Rust transport. Absent the interface,
    // `ai.stream.from_spec` raises `ai.errors.StreamingUnsupported` naming
    // this client, which is exactly right. sys_llm never streamed Bedrock
    // either, so nothing regresses (PLAN §6).
}

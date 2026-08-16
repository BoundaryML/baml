// The Anthropic Messages client. Construction is pure: credentials and base
// URLs resolve from the environment at request time, never in `new()`.
//
// The wire model (typed request / response / SSE-event classes) lives in
// `ns_internal/messages.baml`; this file is the user-facing option surface.

/// Extended-thinking configuration (`thinking` on the Messages request).
///
/// `budget_tokens` must be smaller than the request's `max_tokens`, and
/// Anthropic rejects `temperature` / `top_p` / `top_k` alongside enabled
/// thinking — the client sends what it is given and lets the API say so.
class ThinkingConfig {
    /// "enabled" or "disabled".
    type: string,
    budget_tokens: int?,

    function enabled(budget_tokens: int) -> ThinkingConfig {
        let budget = budget_tokens;
        ThinkingConfig { type: "enabled", budget_tokens: budget }
    }

    function disabled() -> ThinkingConfig {
        let kind = "disabled";
        ThinkingConfig { type: kind, budget_tokens: null }
    }
}

/// How the model may use the tools in the run's toolbox (`tool_choice`).
/// Only sent when the toolbox is non-empty; the default is `auto`.
class ToolChoice {
    /// "auto", "any", "tool", or "none".
    type: string,
    /// The tool the model must call. Only read when `type` is "tool".
    name: string?,
    disable_parallel_tool_use: bool?,

    /// The model decides whether to call a tool.
    function auto(disable_parallel_tool_use: bool? = null) -> ToolChoice {
        let parallel = disable_parallel_tool_use;
        ToolChoice { type: "auto", name: null, disable_parallel_tool_use: parallel }
    }

    /// The model must call one of the tools, its choice which.
    function any(disable_parallel_tool_use: bool? = null) -> ToolChoice {
        let parallel = disable_parallel_tool_use;
        ToolChoice { type: "any", name: null, disable_parallel_tool_use: parallel }
    }

    /// The model must call exactly this tool.
    function tool(name: string, disable_parallel_tool_use: bool? = null) -> ToolChoice {
        let chosen = name;
        ToolChoice {
            type: "tool",
            name: chosen,
            disable_parallel_tool_use: disable_parallel_tool_use,
        }
    }

    /// The model may not call a tool this turn.
    function none() -> ToolChoice {
        let kind = "none";
        ToolChoice { type: kind, name: null, disable_parallel_tool_use: null }
    }
}

class AnthropicClient {
    model: string,
    // A literal string, `env.NAME` (a late-bound `baml.env.Ref` read at request
    // time), or null to resolve ANTHROPIC_API_KEY at request time. Construction
    // never touches env, so declaring/constructing a client is always safe —
    // and a `Ref` keeps the secret out of the client value entirely.
    api_key: ai.Credential?,
    base_url: ai.Credential?,

    // First-class typed parameters. Each is lowered into the typed request
    // ONLY when non-null, so an unset option never reaches the wire.
    max_tokens: int,
    temperature: float?,
    top_p: float?,
    top_k: int?,
    stop_sequences: string[]?,
    thinking: ThinkingConfig?,
    tool_choice: ToolChoice?,

    // Escape hatches (PLAN P0.3). `request_body` is deep-merged into the
    // serialized typed request as the LAST step of body construction, with the
    // engine's semantics: maps merge recursively, `null` DELETES a key
    // (`{"max_tokens": null}` drops the default), everything else replaces.
    request_body: baml.json.json?,
    // Extra request headers, applied BEFORE the client's own auth header.
    // This is how `anthropic-beta` (extended cache TTL, 1M context, code
    // execution, …) and `anthropic-dangerous-direct-browser-access` are
    // reached; the client never sets either on its own.
    headers: map<string, string>?,
    // Extra URL query parameters, appended verbatim (no percent-encoding —
    // there is no encoder in the stdlib yet; keep values URL-safe).
    query_params: map<string, string>?,

    // NOTE: the task spec named "claude-3-5-haiku-latest" as the default, but
    // that model was retired (the API 404s on it); claude-haiku-4-5 is the
    // current Haiku.
    //
    // max_tokens defaults to 8192, not the engine's 4096 (PLAN §5): every
    // model this client can address supports at least that, and 4096 silently
    // truncates ordinary structured answers.
    function new(
        model: string = "claude-haiku-4-5",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        max_tokens: int = 8192,
        temperature: float? = null,
        top_p: float? = null,
        top_k: int? = null,
        stop_sequences: string[]? = null,
        thinking: ThinkingConfig? = null,
        tool_choice: ToolChoice? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
    ) -> AnthropicClient {
        AnthropicClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            max_tokens: max_tokens,
            temperature: temperature,
            top_p: top_p,
            top_k: top_k,
            stop_sequences: stop_sequences,
            thinking: thinking,
            tool_choice: tool_choice,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
        }
    }

    // The request-time credential: an explicit value, else the provider's
    // canonical one.
    //
    // A missing credential is a CATCHABLE configuration error, thrown the same
    // way the chat family reports one (`_chat_headers`): `baml.env.get_or_panic`
    // used to raise an uncatchable panic that escaped `catch_all` — so it
    // bypassed this client's own `invoke` normalize wrapper and turned the one
    // misconfiguration every deployment hits first into a process-level failure.
    function resolved_api_key(self) -> string {
        if let k: string = ai.wire.resolve_credential(self.api_key, "ANTHROPIC_API_KEY") {
            return k;
        }
        throw root.internal._reject(
            `anthropic needs an API key: pass api_key (a literal or env.NAME), or set one of the variables it consulted — ${ai.wire.credential_sources(self.api_key, "ANTHROPIC_API_KEY")}`,
        )
    }

    // The request-time base URL, or null for the provider default.
    function resolved_base_url(self) -> string? {
        ai.wire.resolve_credential(self.base_url, null)
    }

    implements ai.Client {
        function id(self) -> string {
            `anthropic/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            anthropic.internal.anthropic_preview(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.invoke_stream(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }
}

// LLM clients, one per model tier.
//
//   LLM_BASE_URL : the proxy base (e.g. http://bammy-2-proxy.internal:9090).
//                  Unset -> BAML calls api.anthropic.com directly (then LLM_API_KEY
//                  must be a real Anthropic key).
//   LLM_API_KEY  : sent as x-api-key. In proxy mode this is the proxy token.

retry_policy LlmRetry {
    max_retries: 3,
    initial_delay_ms: 1000,
    multiplier: 2.0,
    max_delay_ms: 60000,
}

client<llm> Opus {
    provider: anthropic,
    retry_policy: LlmRetry,
    options: {
        model: "claude-opus-4-8",
        base_url: env.LLM_BASE_URL,
        api_key: env.LLM_API_KEY,
        supports_streaming: false,
        // The proxy fulfils /v1/messages by running `claude -p` (up to 300s), so a
        // long generation can take ~30s+. BAML's default request timeout is shorter
        // than that, which made slow redrafts/dedups fail with "All orchestration
        // steps failed". 600s is comfortably above the proxy's claude budget.
        http: {
            request_timeout_ms: 600000,
        },
    },
}

client<llm> Sonnet {
    provider: anthropic,
    retry_policy: LlmRetry,
    options: {
        model: "claude-sonnet-4-6",
        base_url: env.LLM_BASE_URL,
        api_key: env.LLM_API_KEY,
        supports_streaming: false,
        // The proxy fulfils /v1/messages by running `claude -p` (up to 300s), so a
        // long generation can take ~30s+. BAML's default request timeout is shorter
        // than that, which made slow redrafts/dedups fail with "All orchestration
        // steps failed". 600s is comfortably above the proxy's claude budget.
        http: {
            request_timeout_ms: 600000,
        },
    },
}

client<llm> Haiku {
    provider: anthropic,
    retry_policy: LlmRetry,
    options: {
        model: "claude-haiku-4-5",
        base_url: env.LLM_BASE_URL,
        api_key: env.LLM_API_KEY,
        supports_streaming: false,
        // The proxy fulfils /v1/messages by running `claude -p` (up to 300s), so a
        // long generation can take ~30s+. BAML's default request timeout is shorter
        // than that, which made slow redrafts/dedups fail with "All orchestration
        // steps failed". 600s is comfortably above the proxy's claude budget.
        http: {
            request_timeout_ms: 600000,
        },
    },
}

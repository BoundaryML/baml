// One grader client. Same env contract as agent-tries-baml:
//
//   LLM_BASE_URL : claude-proxy base (fulfils /v1/messages by running `claude -p`).
//                  Unset -> BAML calls api.anthropic.com directly (LLM_API_KEY must
//                  then be a real Anthropic key).
//   LLM_API_KEY  : sent as x-api-key. In proxy mode this is the proxy token.
//
// temperature 0 is requested for determinism, but the real reproducibility
// guarantee is the content-addressed cache in cache.baml: identical inputs never
// reach the model twice, so a completed run re-plays bit-for-bit.

retry_policy LlmRetry {
    max_retries: 3,
    initial_delay_ms: 1000,
    multiplier: 2.0,
    max_delay_ms: 60000,
}

// Judgment-heavy stages: grading, refutation, omission scan.
client<llm> Grader {
    provider: anthropic,
    retry_policy: LlmRetry,
    options: {
        model: "claude-sonnet-4-6",
        base_url: env.LLM_BASE_URL,
        api_key: env.LLM_API_KEY,
        supports_streaming: false,
        temperature: 0.0,
        // Proxy runs `claude -p` (up to 300s); default request timeout is shorter.
        http: {
            request_timeout_ms: 600000,
        },
    },
}

// High-volume cheap stages: per-chunk intention inference and card routing —
// these run once per chunk (hundreds of calls on a real project) and are
// summarization/matching tasks, not judgment calls.
client<llm> GraderFast {
    provider: anthropic,
    retry_policy: LlmRetry,
    options: {
        model: "claude-haiku-4-5",
        base_url: env.LLM_BASE_URL,
        api_key: env.LLM_API_KEY,
        supports_streaming: false,
        temperature: 0.0,
        http: {
            request_timeout_ms: 600000,
        },
    },
}

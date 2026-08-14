// LLM clients for bamlcode.
//
// Default brain is Anthropic Claude (set ANTHROPIC_API_KEY).
// To use OpenAI instead, swap `client: Brain` for `client: BrainOpenAI`
// in baml_src/ns_agent/agent.baml (and set OPENAI_API_KEY).

client<llm> Brain {
    provider: anthropic,
    options: {
        model: "claude-sonnet-4-6",
        api_key: env.ANTHROPIC_API_KEY,
        max_tokens: 4096,
    },
}

client<llm> BrainOpenAI {
    provider: openai,
    options: {
        model: "gpt-4o",
        api_key: env.OPENAI_API_KEY,
        max_tokens: 4096,
    },
}

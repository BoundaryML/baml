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

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// ns_agent_clients.baml:1:1 (comment) len=28 "// LLM clients for bamlcode."
// ns_agent_clients.baml:2:1 (comment) len=2 "//"
// ns_agent_clients.baml:3:1 (comment) len=61 "// Default brain is Anthropic Claude (set ANTHROPIC_API_KEY)."
// ns_agent_clients.baml:4:1 (comment) len=72 "// To use OpenAI instead, swap `client: Brain` for `client: BrainOpenAI`"
// ns_agent_clients.baml:5:1 (comment) len=60 "// in baml_src/ns_agent/agent.baml (and set OPENAI_API_KEY)."
// ns_agent_clients.baml:7:1 (keyword) len=6 "client"
// ns_agent_clients.baml:7:7 (operator) len=1 "<"
// ns_agent_clients.baml:7:8 (type) len=3 "llm"
// ns_agent_clients.baml:7:11 (operator) len=1 ">"
// ns_agent_clients.baml:7:13 (struct) [declaration] len=5 "Brain"
// ns_agent_clients.baml:8:5 (property) len=8 "provider"
// ns_agent_clients.baml:9:5 (property) len=7 "options"
// ns_agent_clients.baml:10:9 (property) len=5 "model"
// ns_agent_clients.baml:10:16 (string) len=19 "\"claude-sonnet-4-6\""
// ns_agent_clients.baml:11:9 (property) len=7 "api_key"
// ns_agent_clients.baml:12:9 (property) len=10 "max_tokens"
// ns_agent_clients.baml:12:21 (number) len=4 "4096"
// ns_agent_clients.baml:16:1 (keyword) len=6 "client"
// ns_agent_clients.baml:16:7 (operator) len=1 "<"
// ns_agent_clients.baml:16:8 (type) len=3 "llm"
// ns_agent_clients.baml:16:11 (operator) len=1 ">"
// ns_agent_clients.baml:16:13 (struct) [declaration] len=11 "BrainOpenAI"
// ns_agent_clients.baml:17:5 (property) len=8 "provider"
// ns_agent_clients.baml:18:5 (property) len=7 "options"
// ns_agent_clients.baml:19:9 (property) len=5 "model"
// ns_agent_clients.baml:19:16 (string) len=8 "\"gpt-4o\""
// ns_agent_clients.baml:20:9 (property) len=7 "api_key"
// ns_agent_clients.baml:21:9 (property) len=10 "max_tokens"
// ns_agent_clients.baml:21:21 (number) len=4 "4096"

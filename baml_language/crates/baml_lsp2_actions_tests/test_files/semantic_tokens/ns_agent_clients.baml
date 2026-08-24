// LLM clients for bamlcode.
//
// Default brain is Anthropic Claude (set ANTHROPIC_API_KEY).
// To use OpenAI instead, swap `client Brain` for `client BrainOpenAI`
// in baml_src/ns_agent/agent.baml (and set OPENAI_API_KEY).

client Brain = anthropic.AnthropicClient.new(
    model = "claude-sonnet-4-6",
    api_key = env.ANTHROPIC_API_KEY,
    max_tokens = 4096,
);

client BrainOpenAI = openai.ResponsesClient.new(
    model = "gpt-4o",
    api_key = env.OPENAI_API_KEY,
);

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// ns_agent_clients.baml:1:1 (comment) len=28 "// LLM clients for bamlcode."
// ns_agent_clients.baml:2:1 (comment) len=2 "//"
// ns_agent_clients.baml:3:1 (comment) len=61 "// Default brain is Anthropic Claude (set ANTHROPIC_API_KEY)."
// ns_agent_clients.baml:4:1 (comment) len=70 "// To use OpenAI instead, swap `client Brain` for `client BrainOpenAI`"
// ns_agent_clients.baml:5:1 (comment) len=60 "// in baml_src/ns_agent/agent.baml (and set OPENAI_API_KEY)."
// ns_agent_clients.baml:7:1 (keyword) len=6 "client"
// ns_agent_clients.baml:7:14 (operator) len=1 "="
// ns_agent_clients.baml:7:16 (namespace) [defaultLibrary] len=9 "anthropic"
// ns_agent_clients.baml:7:25 (namespace) [defaultLibrary] len=1 "."
// ns_agent_clients.baml:7:26 (class) [defaultLibrary] len=15 "AnthropicClient"
// ns_agent_clients.baml:7:42 (method) len=3 "new"
// ns_agent_clients.baml:8:5 (parameter) len=5 "model"
// ns_agent_clients.baml:8:11 (operator) len=1 "="
// ns_agent_clients.baml:8:13 (string) len=19 "\"claude-sonnet-4-6\""
// ns_agent_clients.baml:9:5 (parameter) len=7 "api_key"
// ns_agent_clients.baml:9:13 (operator) len=1 "="
// ns_agent_clients.baml:10:5 (parameter) len=10 "max_tokens"
// ns_agent_clients.baml:10:16 (operator) len=1 "="
// ns_agent_clients.baml:10:18 (number) len=4 "4096"
// ns_agent_clients.baml:13:1 (keyword) len=6 "client"
// ns_agent_clients.baml:13:20 (operator) len=1 "="
// ns_agent_clients.baml:13:22 (namespace) [defaultLibrary] len=6 "openai"
// ns_agent_clients.baml:13:28 (namespace) [defaultLibrary] len=1 "."
// ns_agent_clients.baml:13:29 (class) [defaultLibrary] len=15 "ResponsesClient"
// ns_agent_clients.baml:13:45 (method) len=3 "new"
// ns_agent_clients.baml:14:5 (parameter) len=5 "model"
// ns_agent_clients.baml:14:11 (operator) len=1 "="
// ns_agent_clients.baml:14:13 (string) len=8 "\"gpt-4o\""
// ns_agent_clients.baml:15:5 (parameter) len=7 "api_key"
// ns_agent_clients.baml:15:13 (operator) len=1 "="

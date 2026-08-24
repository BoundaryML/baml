// Learn more about clients at https://docs.boundaryml.com/docs/snippets/clients/overview

function add(a: int, b: int) -> int {
    a + b
}

client CustomGPT5 = openai.ResponsesClient.new(
    model = "gpt-5",
    api_key = env.TEMP_KEY,
);

// Retries compose by wrapping the inner client value.
client CustomGPT5Mini = ai.clients.Retry.new(
    inner = openai.ResponsesClient.new(model = "gpt-5-mini", api_key = env.OPENAI_API_KEY),
    max_attempts = 2,
);

// Openai with chat completion
client CustomGPT5Chat = openai.ResponsesClient.new(
    model = "gpt-5",
    api_key = env.OPENAI_API_KEY,
);

// Latest Anthropic Claude 4 models
client CustomOpus4 = anthropic.AnthropicClient.new(
    model = "claude-opus-4-1-20250805",
    api_key = env.ANTHROPIC_API_KEY,
);

client CustomSonnet4 = anthropic.AnthropicClient.new(
    model = "claude-sonnet-4-20250514",
    api_key = env.ANTHROPIC_API_KEY,
);

client CustomHaiku = anthropic.AnthropicClient.new(
    model = "claude-haiku-4-5",
    api_key = env.ANTHROPIC_API_KEY,
);

// Example Google AI client (uncomment to use)
// client CustomGemini = google.GoogleClient.new(
//     model = "gemini-2.5-pro",
//     api_key = env.GOOGLE_API_KEY,
// );

// This will alternate between the two clients
client CustomFast = ai.clients.RoundRobin.new([CustomGPT5Mini, CustomHaiku]);

// This will try the clients in order until one succeeds
client OpenaiFallback = ai.clients.Fallback { members: [CustomGPT5Mini, CustomGPT5] };

function will_double(x: int) -> int {
    x * 2
}

function double() -> void {
    let doubled = [1, 2, 3].map(will_double);
    log.info({ "doubled": doubled });
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// ns_game_clients.baml:1:1 (comment) len=89 "// Learn more about clients at https://docs.boundaryml.com/docs/snippets/clients/overview"
// ns_game_clients.baml:3:1 (keyword) len=8 "function"
// ns_game_clients.baml:3:10 (function) [declaration] len=3 "add"
// ns_game_clients.baml:3:14 (parameter) [declaration] len=1 "a"
// ns_game_clients.baml:3:17 (type) [defaultLibrary] len=3 "int"
// ns_game_clients.baml:3:22 (parameter) [declaration] len=1 "b"
// ns_game_clients.baml:3:25 (type) [defaultLibrary] len=3 "int"
// ns_game_clients.baml:3:33 (type) [defaultLibrary] len=3 "int"
// ns_game_clients.baml:4:5 (parameter) len=1 "a"
// ns_game_clients.baml:4:7 (operator) len=1 "+"
// ns_game_clients.baml:4:9 (parameter) len=1 "b"
// ns_game_clients.baml:7:1 (keyword) len=6 "client"
// ns_game_clients.baml:7:19 (operator) len=1 "="
// ns_game_clients.baml:7:21 (namespace) [defaultLibrary] len=6 "openai"
// ns_game_clients.baml:7:27 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:7:28 (class) [defaultLibrary] len=15 "ResponsesClient"
// ns_game_clients.baml:7:44 (method) len=3 "new"
// ns_game_clients.baml:8:5 (parameter) len=5 "model"
// ns_game_clients.baml:8:11 (operator) len=1 "="
// ns_game_clients.baml:8:13 (string) len=7 "\"gpt-5\""
// ns_game_clients.baml:9:5 (parameter) len=7 "api_key"
// ns_game_clients.baml:9:13 (operator) len=1 "="
// ns_game_clients.baml:12:1 (comment) len=54 "// Retries compose by wrapping the inner client value."
// ns_game_clients.baml:13:1 (keyword) len=6 "client"
// ns_game_clients.baml:13:23 (operator) len=1 "="
// ns_game_clients.baml:13:25 (namespace) [defaultLibrary] len=2 "ai"
// ns_game_clients.baml:13:27 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:13:28 (namespace) [defaultLibrary] len=7 "clients"
// ns_game_clients.baml:13:35 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:13:36 (class) [defaultLibrary] len=5 "Retry"
// ns_game_clients.baml:13:42 (method) len=3 "new"
// ns_game_clients.baml:14:5 (parameter) len=5 "inner"
// ns_game_clients.baml:14:11 (operator) len=1 "="
// ns_game_clients.baml:14:13 (namespace) [defaultLibrary] len=6 "openai"
// ns_game_clients.baml:14:19 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:14:20 (class) [defaultLibrary] len=15 "ResponsesClient"
// ns_game_clients.baml:14:36 (method) len=3 "new"
// ns_game_clients.baml:14:40 (parameter) len=5 "model"
// ns_game_clients.baml:14:46 (operator) len=1 "="
// ns_game_clients.baml:14:48 (string) len=12 "\"gpt-5-mini\""
// ns_game_clients.baml:14:62 (parameter) len=7 "api_key"
// ns_game_clients.baml:14:70 (operator) len=1 "="
// ns_game_clients.baml:15:5 (parameter) len=12 "max_attempts"
// ns_game_clients.baml:15:18 (operator) len=1 "="
// ns_game_clients.baml:15:20 (number) len=1 "2"
// ns_game_clients.baml:18:1 (comment) len=30 "// Openai with chat completion"
// ns_game_clients.baml:19:1 (keyword) len=6 "client"
// ns_game_clients.baml:19:23 (operator) len=1 "="
// ns_game_clients.baml:19:25 (namespace) [defaultLibrary] len=6 "openai"
// ns_game_clients.baml:19:31 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:19:32 (class) [defaultLibrary] len=15 "ResponsesClient"
// ns_game_clients.baml:19:48 (method) len=3 "new"
// ns_game_clients.baml:20:5 (parameter) len=5 "model"
// ns_game_clients.baml:20:11 (operator) len=1 "="
// ns_game_clients.baml:20:13 (string) len=7 "\"gpt-5\""
// ns_game_clients.baml:21:5 (parameter) len=7 "api_key"
// ns_game_clients.baml:21:13 (operator) len=1 "="
// ns_game_clients.baml:24:1 (comment) len=35 "// Latest Anthropic Claude 4 models"
// ns_game_clients.baml:25:1 (keyword) len=6 "client"
// ns_game_clients.baml:25:20 (operator) len=1 "="
// ns_game_clients.baml:25:22 (namespace) [defaultLibrary] len=9 "anthropic"
// ns_game_clients.baml:25:31 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:25:32 (class) [defaultLibrary] len=15 "AnthropicClient"
// ns_game_clients.baml:25:48 (method) len=3 "new"
// ns_game_clients.baml:26:5 (parameter) len=5 "model"
// ns_game_clients.baml:26:11 (operator) len=1 "="
// ns_game_clients.baml:26:13 (string) len=26 "\"claude-opus-4-1-20250805\""
// ns_game_clients.baml:27:5 (parameter) len=7 "api_key"
// ns_game_clients.baml:27:13 (operator) len=1 "="
// ns_game_clients.baml:30:1 (keyword) len=6 "client"
// ns_game_clients.baml:30:22 (operator) len=1 "="
// ns_game_clients.baml:30:24 (namespace) [defaultLibrary] len=9 "anthropic"
// ns_game_clients.baml:30:33 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:30:34 (class) [defaultLibrary] len=15 "AnthropicClient"
// ns_game_clients.baml:30:50 (method) len=3 "new"
// ns_game_clients.baml:31:5 (parameter) len=5 "model"
// ns_game_clients.baml:31:11 (operator) len=1 "="
// ns_game_clients.baml:31:13 (string) len=26 "\"claude-sonnet-4-20250514\""
// ns_game_clients.baml:32:5 (parameter) len=7 "api_key"
// ns_game_clients.baml:32:13 (operator) len=1 "="
// ns_game_clients.baml:35:1 (keyword) len=6 "client"
// ns_game_clients.baml:35:20 (operator) len=1 "="
// ns_game_clients.baml:35:22 (namespace) [defaultLibrary] len=9 "anthropic"
// ns_game_clients.baml:35:31 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:35:32 (class) [defaultLibrary] len=15 "AnthropicClient"
// ns_game_clients.baml:35:48 (method) len=3 "new"
// ns_game_clients.baml:36:5 (parameter) len=5 "model"
// ns_game_clients.baml:36:11 (operator) len=1 "="
// ns_game_clients.baml:36:13 (string) len=18 "\"claude-haiku-4-5\""
// ns_game_clients.baml:37:5 (parameter) len=7 "api_key"
// ns_game_clients.baml:37:13 (operator) len=1 "="
// ns_game_clients.baml:40:1 (comment) len=46 "// Example Google AI client (uncomment to use)"
// ns_game_clients.baml:41:1 (comment) len=49 "// client CustomGemini = google.GoogleClient.new("
// ns_game_clients.baml:42:1 (comment) len=32 "//     model = \"gemini-2.5-pro\","
// ns_game_clients.baml:43:1 (comment) len=36 "//     api_key = env.GOOGLE_API_KEY,"
// ns_game_clients.baml:44:1 (comment) len=5 "// );"
// ns_game_clients.baml:46:1 (comment) len=46 "// This will alternate between the two clients"
// ns_game_clients.baml:47:1 (keyword) len=6 "client"
// ns_game_clients.baml:47:19 (operator) len=1 "="
// ns_game_clients.baml:47:21 (namespace) [defaultLibrary] len=2 "ai"
// ns_game_clients.baml:47:23 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:47:24 (namespace) [defaultLibrary] len=7 "clients"
// ns_game_clients.baml:47:31 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:47:32 (class) [defaultLibrary] len=10 "RoundRobin"
// ns_game_clients.baml:47:43 (method) len=3 "new"
// ns_game_clients.baml:47:48 (variable) len=14 "CustomGPT5Mini"
// ns_game_clients.baml:47:64 (variable) len=11 "CustomHaiku"
// ns_game_clients.baml:49:1 (comment) len=56 "// This will try the clients in order until one succeeds"
// ns_game_clients.baml:50:1 (keyword) len=6 "client"
// ns_game_clients.baml:50:23 (operator) len=1 "="
// ns_game_clients.baml:50:25 (class) len=2 "ai"
// ns_game_clients.baml:50:27 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:50:35 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:50:47 (property) len=7 "members"
// ns_game_clients.baml:50:57 (variable) len=14 "CustomGPT5Mini"
// ns_game_clients.baml:50:73 (variable) len=10 "CustomGPT5"
// ns_game_clients.baml:52:1 (keyword) len=8 "function"
// ns_game_clients.baml:52:10 (function) [declaration] len=11 "will_double"
// ns_game_clients.baml:52:22 (parameter) [declaration] len=1 "x"
// ns_game_clients.baml:52:25 (type) [defaultLibrary] len=3 "int"
// ns_game_clients.baml:52:33 (type) [defaultLibrary] len=3 "int"
// ns_game_clients.baml:53:5 (parameter) len=1 "x"
// ns_game_clients.baml:53:7 (operator) len=1 "*"
// ns_game_clients.baml:53:9 (number) len=1 "2"
// ns_game_clients.baml:56:1 (keyword) len=8 "function"
// ns_game_clients.baml:56:10 (function) [declaration] len=6 "double"
// ns_game_clients.baml:56:22 (type) [defaultLibrary] len=4 "void"
// ns_game_clients.baml:57:5 (keyword) len=3 "let"
// ns_game_clients.baml:57:9 (variable) [declaration] len=7 "doubled"
// ns_game_clients.baml:57:17 (operator) len=1 "="
// ns_game_clients.baml:57:20 (number) len=1 "1"
// ns_game_clients.baml:57:23 (number) len=1 "2"
// ns_game_clients.baml:57:26 (number) len=1 "3"
// ns_game_clients.baml:57:29 (method) len=3 "map"
// ns_game_clients.baml:57:33 (function) len=11 "will_double"
// ns_game_clients.baml:58:5 (namespace) [defaultLibrary] len=3 "log"
// ns_game_clients.baml:58:8 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:58:9 (function) len=4 "info"
// ns_game_clients.baml:58:16 (string) len=9 "\"doubled\""
// ns_game_clients.baml:58:27 (variable) len=7 "doubled"

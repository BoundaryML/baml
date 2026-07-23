// Learn more about clients at https://docs.boundaryml.com/docs/snippets/clients/overview

function add(a: int, b: int) -> int {
    a + b
}

// Using the new OpenAI Responses API for enhanced formatting
client<llm> CustomGPT5 {
    provider: openai-responses,
    options: {
        model: "gpt-5",
        api_key: env.TEMP_KEY,
    },
}

client<llm> CustomGPT5Mini {
    provider: openai-responses,
    retry_policy: Exponential,
    options: {
        model: "gpt-5-mini",
        api_key: env.OPENAI_API_KEY,
    },
}

// Openai with chat completion
client<llm> CustomGPT5Chat {
    provider: openai,
    options: {
        model: "gpt-5",
        api_key: env.OPENAI_API_KEY,
    },
}

// Latest Anthropic Claude 4 models
client<llm> CustomOpus4 {
    provider: anthropic,
    options: {
        model: "claude-opus-4-1-20250805",
        api_key: env.ANTHROPIC_API_KEY,
    },
}

client<llm> CustomSonnet4 {
    provider: anthropic,
    options: {
        model: "claude-sonnet-4-20250514",
        api_key: env.ANTHROPIC_API_KEY,
    },
}

client<llm> CustomHaiku {
    provider: anthropic,
    options: {
        model: "claude-haiku-4-5",
        api_key: env.ANTHROPIC_API_KEY,
    },
}

// Example Google AI client (uncomment to use)
// client<llm> CustomGemini {
//   provider google-ai
//   options {
//     model "gemini-2.5-pro"
//     api_key env.GOOGLE_API_KEY
//   }
// }

// Example AWS Bedrock client (uncomment to use)
// client<llm> CustomBedrock {
//   provider aws-bedrock
//   options {
//     model "anthropic.claude-sonnet-4-20250514-v1:0"
//     region "us-east-1"
//     // AWS credentials are auto-detected from env vars
//   }
// }

// Example Azure OpenAI client (uncomment to use)
// client<llm> CustomAzure {
//   provider azure-openai
//   options {
//     model "gpt-5"
//     api_key env.AZURE_OPENAI_API_KEY
//     base_url "https://MY_RESOURCE_NAME.openai.azure.com/openai/deployments/MY_DEPLOYMENT_ID"
//     api_version "2024-10-01-preview"
//   }
// }

// Example Vertex AI client (uncomment to use)
// client<llm> CustomVertex {
//   provider vertex-ai
//   options {
//     model "gemini-2.5-pro"
//     location "us-central1"
//     // Uses Google Cloud Application Default Credentials
//   }
// }

// Example Ollama client for local models (uncomment to use)
// client<llm> CustomOllama {
//   provider openai-generic
//   options {
//     base_url "http://localhost:11434/v1"
//     model "llama4"
//     default_role "user" // Most local models prefer the user role
//     // No API key needed for local Ollama
//   }
// }

// https://docs.boundaryml.com/docs/snippets/clients/round-robin
client<llm> CustomFast {
    provider: round-robin,
    options: {
        // This will alternate between the two clients
        strategy: [CustomGPT5Mini, CustomHaiku],
    },
}

// https://docs.boundaryml.com/docs/snippets/clients/fallback
client<llm> OpenaiFallback {
    provider: fallback,
    options: {
        // This will try the clients in order until one succeeds
        strategy: [CustomGPT5Mini, CustomGPT5],
    },
}

// https://docs.boundaryml.com/docs/snippets/clients/retry
retry_policy Constant {
    max_retries: 3,
    strategy: {
        type: constant_delay,
        delay_ms: 200,
    },
}

retry_policy Exponential {
    max_retries: 2,
    strategy: {
        type: exponential_backoff,
        delay_ms: 300,
        multiplier: 1.5,
        max_delay_ms: 10000,
    },
}

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
// ns_game_clients.baml:7:1 (comment) len=61 "// Using the new OpenAI Responses API for enhanced formatting"
// ns_game_clients.baml:8:1 (keyword) len=6 "client"
// ns_game_clients.baml:8:7 (operator) len=1 "<"
// ns_game_clients.baml:8:8 (type) len=3 "llm"
// ns_game_clients.baml:8:11 (operator) len=1 ">"
// ns_game_clients.baml:8:13 (struct) [declaration] len=10 "CustomGPT5"
// ns_game_clients.baml:9:5 (property) len=8 "provider"
// ns_game_clients.baml:10:5 (property) len=7 "options"
// ns_game_clients.baml:11:9 (property) len=5 "model"
// ns_game_clients.baml:11:16 (string) len=7 "\"gpt-5\""
// ns_game_clients.baml:12:9 (property) len=7 "api_key"
// ns_game_clients.baml:16:1 (keyword) len=6 "client"
// ns_game_clients.baml:16:7 (operator) len=1 "<"
// ns_game_clients.baml:16:8 (type) len=3 "llm"
// ns_game_clients.baml:16:11 (operator) len=1 ">"
// ns_game_clients.baml:16:13 (struct) [declaration] len=14 "CustomGPT5Mini"
// ns_game_clients.baml:17:5 (property) len=8 "provider"
// ns_game_clients.baml:18:5 (property) len=12 "retry_policy"
// ns_game_clients.baml:19:5 (property) len=7 "options"
// ns_game_clients.baml:20:9 (property) len=5 "model"
// ns_game_clients.baml:20:16 (string) len=12 "\"gpt-5-mini\""
// ns_game_clients.baml:21:9 (property) len=7 "api_key"
// ns_game_clients.baml:25:1 (comment) len=30 "// Openai with chat completion"
// ns_game_clients.baml:26:1 (keyword) len=6 "client"
// ns_game_clients.baml:26:7 (operator) len=1 "<"
// ns_game_clients.baml:26:8 (type) len=3 "llm"
// ns_game_clients.baml:26:11 (operator) len=1 ">"
// ns_game_clients.baml:26:13 (struct) [declaration] len=14 "CustomGPT5Chat"
// ns_game_clients.baml:27:5 (property) len=8 "provider"
// ns_game_clients.baml:28:5 (property) len=7 "options"
// ns_game_clients.baml:29:9 (property) len=5 "model"
// ns_game_clients.baml:29:16 (string) len=7 "\"gpt-5\""
// ns_game_clients.baml:30:9 (property) len=7 "api_key"
// ns_game_clients.baml:34:1 (comment) len=35 "// Latest Anthropic Claude 4 models"
// ns_game_clients.baml:35:1 (keyword) len=6 "client"
// ns_game_clients.baml:35:7 (operator) len=1 "<"
// ns_game_clients.baml:35:8 (type) len=3 "llm"
// ns_game_clients.baml:35:11 (operator) len=1 ">"
// ns_game_clients.baml:35:13 (struct) [declaration] len=11 "CustomOpus4"
// ns_game_clients.baml:36:5 (property) len=8 "provider"
// ns_game_clients.baml:37:5 (property) len=7 "options"
// ns_game_clients.baml:38:9 (property) len=5 "model"
// ns_game_clients.baml:38:16 (string) len=26 "\"claude-opus-4-1-20250805\""
// ns_game_clients.baml:39:9 (property) len=7 "api_key"
// ns_game_clients.baml:43:1 (keyword) len=6 "client"
// ns_game_clients.baml:43:7 (operator) len=1 "<"
// ns_game_clients.baml:43:8 (type) len=3 "llm"
// ns_game_clients.baml:43:11 (operator) len=1 ">"
// ns_game_clients.baml:43:13 (struct) [declaration] len=13 "CustomSonnet4"
// ns_game_clients.baml:44:5 (property) len=8 "provider"
// ns_game_clients.baml:45:5 (property) len=7 "options"
// ns_game_clients.baml:46:9 (property) len=5 "model"
// ns_game_clients.baml:46:16 (string) len=26 "\"claude-sonnet-4-20250514\""
// ns_game_clients.baml:47:9 (property) len=7 "api_key"
// ns_game_clients.baml:51:1 (keyword) len=6 "client"
// ns_game_clients.baml:51:7 (operator) len=1 "<"
// ns_game_clients.baml:51:8 (type) len=3 "llm"
// ns_game_clients.baml:51:11 (operator) len=1 ">"
// ns_game_clients.baml:51:13 (struct) [declaration] len=11 "CustomHaiku"
// ns_game_clients.baml:52:5 (property) len=8 "provider"
// ns_game_clients.baml:53:5 (property) len=7 "options"
// ns_game_clients.baml:54:9 (property) len=5 "model"
// ns_game_clients.baml:54:16 (string) len=18 "\"claude-haiku-4-5\""
// ns_game_clients.baml:55:9 (property) len=7 "api_key"
// ns_game_clients.baml:59:1 (comment) len=46 "// Example Google AI client (uncomment to use)"
// ns_game_clients.baml:60:1 (comment) len=29 "// client<llm> CustomGemini {"
// ns_game_clients.baml:61:1 (comment) len=23 "//   provider google-ai"
// ns_game_clients.baml:62:1 (comment) len=14 "//   options {"
// ns_game_clients.baml:63:1 (comment) len=29 "//     model \"gemini-2.5-pro\""
// ns_game_clients.baml:64:1 (comment) len=33 "//     api_key env.GOOGLE_API_KEY"
// ns_game_clients.baml:65:1 (comment) len=6 "//   }"
// ns_game_clients.baml:66:1 (comment) len=4 "// }"
// ns_game_clients.baml:68:1 (comment) len=48 "// Example AWS Bedrock client (uncomment to use)"
// ns_game_clients.baml:69:1 (comment) len=30 "// client<llm> CustomBedrock {"
// ns_game_clients.baml:70:1 (comment) len=25 "//   provider aws-bedrock"
// ns_game_clients.baml:71:1 (comment) len=14 "//   options {"
// ns_game_clients.baml:72:1 (comment) len=54 "//     model \"anthropic.claude-sonnet-4-20250514-v1:0\""
// ns_game_clients.baml:73:1 (comment) len=25 "//     region \"us-east-1\""
// ns_game_clients.baml:74:1 (comment) len=57 "//     // AWS credentials are auto-detected from env vars"
// ns_game_clients.baml:75:1 (comment) len=6 "//   }"
// ns_game_clients.baml:76:1 (comment) len=4 "// }"
// ns_game_clients.baml:78:1 (comment) len=49 "// Example Azure OpenAI client (uncomment to use)"
// ns_game_clients.baml:79:1 (comment) len=28 "// client<llm> CustomAzure {"
// ns_game_clients.baml:80:1 (comment) len=26 "//   provider azure-openai"
// ns_game_clients.baml:81:1 (comment) len=14 "//   options {"
// ns_game_clients.baml:82:1 (comment) len=20 "//     model \"gpt-5\""
// ns_game_clients.baml:83:1 (comment) len=39 "//     api_key env.AZURE_OPENAI_API_KEY"
// ns_game_clients.baml:84:1 (comment) len=95 "//     base_url \"https://MY_RESOURCE_NAME.openai.azure.com/openai/deployments/MY_DEPLOYMENT_ID\""
// ns_game_clients.baml:85:1 (comment) len=39 "//     api_version \"2024-10-01-preview\""
// ns_game_clients.baml:86:1 (comment) len=6 "//   }"
// ns_game_clients.baml:87:1 (comment) len=4 "// }"
// ns_game_clients.baml:89:1 (comment) len=46 "// Example Vertex AI client (uncomment to use)"
// ns_game_clients.baml:90:1 (comment) len=29 "// client<llm> CustomVertex {"
// ns_game_clients.baml:91:1 (comment) len=23 "//   provider vertex-ai"
// ns_game_clients.baml:92:1 (comment) len=14 "//   options {"
// ns_game_clients.baml:93:1 (comment) len=29 "//     model \"gemini-2.5-pro\""
// ns_game_clients.baml:94:1 (comment) len=29 "//     location \"us-central1\""
// ns_game_clients.baml:95:1 (comment) len=59 "//     // Uses Google Cloud Application Default Credentials"
// ns_game_clients.baml:96:1 (comment) len=6 "//   }"
// ns_game_clients.baml:97:1 (comment) len=4 "// }"
// ns_game_clients.baml:99:1 (comment) len=60 "// Example Ollama client for local models (uncomment to use)"
// ns_game_clients.baml:100:1 (comment) len=29 "// client<llm> CustomOllama {"
// ns_game_clients.baml:101:1 (comment) len=28 "//   provider openai-generic"
// ns_game_clients.baml:102:1 (comment) len=14 "//   options {"
// ns_game_clients.baml:103:1 (comment) len=43 "//     base_url \"http://localhost:11434/v1\""
// ns_game_clients.baml:104:1 (comment) len=21 "//     model \"llama4\""
// ns_game_clients.baml:105:1 (comment) len=68 "//     default_role \"user\" // Most local models prefer the user role"
// ns_game_clients.baml:106:1 (comment) len=44 "//     // No API key needed for local Ollama"
// ns_game_clients.baml:107:1 (comment) len=6 "//   }"
// ns_game_clients.baml:108:1 (comment) len=4 "// }"
// ns_game_clients.baml:110:1 (comment) len=64 "// https://docs.boundaryml.com/docs/snippets/clients/round-robin"
// ns_game_clients.baml:111:1 (keyword) len=6 "client"
// ns_game_clients.baml:111:7 (operator) len=1 "<"
// ns_game_clients.baml:111:8 (type) len=3 "llm"
// ns_game_clients.baml:111:11 (operator) len=1 ">"
// ns_game_clients.baml:111:13 (struct) [declaration] len=10 "CustomFast"
// ns_game_clients.baml:112:5 (property) len=8 "provider"
// ns_game_clients.baml:113:5 (property) len=7 "options"
// ns_game_clients.baml:114:9 (comment) len=46 "// This will alternate between the two clients"
// ns_game_clients.baml:115:9 (property) len=8 "strategy"
// ns_game_clients.baml:119:1 (comment) len=61 "// https://docs.boundaryml.com/docs/snippets/clients/fallback"
// ns_game_clients.baml:120:1 (keyword) len=6 "client"
// ns_game_clients.baml:120:7 (operator) len=1 "<"
// ns_game_clients.baml:120:8 (type) len=3 "llm"
// ns_game_clients.baml:120:11 (operator) len=1 ">"
// ns_game_clients.baml:120:13 (struct) [declaration] len=14 "OpenaiFallback"
// ns_game_clients.baml:121:5 (property) len=8 "provider"
// ns_game_clients.baml:122:5 (property) len=7 "options"
// ns_game_clients.baml:123:9 (comment) len=56 "// This will try the clients in order until one succeeds"
// ns_game_clients.baml:124:9 (property) len=8 "strategy"
// ns_game_clients.baml:128:1 (comment) len=58 "// https://docs.boundaryml.com/docs/snippets/clients/retry"
// ns_game_clients.baml:129:1 (keyword) len=12 "retry_policy"
// ns_game_clients.baml:129:14 (struct) [declaration] len=8 "Constant"
// ns_game_clients.baml:130:5 (property) len=11 "max_retries"
// ns_game_clients.baml:130:18 (number) len=1 "3"
// ns_game_clients.baml:131:5 (property) len=8 "strategy"
// ns_game_clients.baml:132:9 (property) len=4 "type"
// ns_game_clients.baml:133:9 (property) len=8 "delay_ms"
// ns_game_clients.baml:133:19 (number) len=3 "200"
// ns_game_clients.baml:137:1 (keyword) len=12 "retry_policy"
// ns_game_clients.baml:137:14 (struct) [declaration] len=11 "Exponential"
// ns_game_clients.baml:138:5 (property) len=11 "max_retries"
// ns_game_clients.baml:138:18 (number) len=1 "2"
// ns_game_clients.baml:139:5 (property) len=8 "strategy"
// ns_game_clients.baml:140:9 (property) len=4 "type"
// ns_game_clients.baml:141:9 (property) len=8 "delay_ms"
// ns_game_clients.baml:141:19 (number) len=3 "300"
// ns_game_clients.baml:142:9 (property) len=10 "multiplier"
// ns_game_clients.baml:142:21 (number) len=3 "1.5"
// ns_game_clients.baml:143:9 (property) len=12 "max_delay_ms"
// ns_game_clients.baml:143:23 (number) len=5 "10000"
// ns_game_clients.baml:147:1 (keyword) len=8 "function"
// ns_game_clients.baml:147:10 (function) [declaration] len=11 "will_double"
// ns_game_clients.baml:147:22 (parameter) [declaration] len=1 "x"
// ns_game_clients.baml:147:25 (type) [defaultLibrary] len=3 "int"
// ns_game_clients.baml:147:33 (type) [defaultLibrary] len=3 "int"
// ns_game_clients.baml:148:5 (parameter) len=1 "x"
// ns_game_clients.baml:148:7 (operator) len=1 "*"
// ns_game_clients.baml:148:9 (number) len=1 "2"
// ns_game_clients.baml:151:1 (keyword) len=8 "function"
// ns_game_clients.baml:151:10 (function) [declaration] len=6 "double"
// ns_game_clients.baml:151:22 (type) [defaultLibrary] len=4 "void"
// ns_game_clients.baml:152:5 (keyword) len=3 "let"
// ns_game_clients.baml:152:9 (variable) [declaration] len=7 "doubled"
// ns_game_clients.baml:152:17 (operator) len=1 "="
// ns_game_clients.baml:152:20 (number) len=1 "1"
// ns_game_clients.baml:152:23 (number) len=1 "2"
// ns_game_clients.baml:152:26 (number) len=1 "3"
// ns_game_clients.baml:152:29 (method) len=3 "map"
// ns_game_clients.baml:152:33 (function) len=11 "will_double"
// ns_game_clients.baml:153:5 (namespace) [defaultLibrary] len=3 "log"
// ns_game_clients.baml:153:8 (namespace) [defaultLibrary] len=1 "."
// ns_game_clients.baml:153:9 (function) len=4 "info"
// ns_game_clients.baml:153:16 (string) len=9 "\"doubled\""
// ns_game_clients.baml:153:27 (variable) len=7 "doubled"

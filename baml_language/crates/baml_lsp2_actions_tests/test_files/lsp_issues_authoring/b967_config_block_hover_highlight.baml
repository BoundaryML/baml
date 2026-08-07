client<llm> ConfiguredClient {
  prov<[CURSOR]ider openai
  opt<[CURSOR]ions {
    mod<[CURSOR]el "gpt-4o-mini"
    ht<[CURSOR]tp {
      request_time<[CURSOR]out_ms 30000
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b967_config_block_hover_highlight.baml:2:7
// ```baml
// provider <name>
// ```
// Selects the provider implementation used by this LLM client.
// hover at b967_config_block_hover_highlight.baml:3:6
// ```baml
// options { ... }
// ```
// Contains provider-specific client configuration such as the model, credentials, and HTTP settings.
// hover at b967_config_block_hover_highlight.baml:4:8
// ```baml
// model <name>
// ```
// A provider configuration property that selects the model used for requests.
// hover at b967_config_block_hover_highlight.baml:5:7
// ```baml
// http { ... }
// ```
// A provider configuration property that groups HTTP transport and timeout settings.
// hover at b967_config_block_hover_highlight.baml:6:19
// ```baml
// request_timeout_ms <milliseconds>
// ```
// A provider configuration property that sets the request timeout in milliseconds. Use `0` for no timeout.
//
//- semantic_tokens
// b967_config_block_hover_highlight.baml:1:1 (keyword) len=6 "client"
// b967_config_block_hover_highlight.baml:1:7 (operator) len=1 "<"
// b967_config_block_hover_highlight.baml:1:8 (type) len=3 "llm"
// b967_config_block_hover_highlight.baml:1:11 (operator) len=1 ">"
// b967_config_block_hover_highlight.baml:1:13 (struct) [declaration] len=16 "ConfiguredClient"
// b967_config_block_hover_highlight.baml:2:3 (property) len=8 "provider"
// b967_config_block_hover_highlight.baml:3:3 (property) len=7 "options"
// b967_config_block_hover_highlight.baml:4:5 (property) len=5 "model"
// b967_config_block_hover_highlight.baml:4:11 (string) len=13 "\"gpt-4o-mini\""
// b967_config_block_hover_highlight.baml:5:5 (property) len=4 "http"
// b967_config_block_hover_highlight.baml:6:7 (property) len=18 "request_timeout_ms"
// b967_config_block_hover_highlight.baml:6:26 (number) len=5 "30000"

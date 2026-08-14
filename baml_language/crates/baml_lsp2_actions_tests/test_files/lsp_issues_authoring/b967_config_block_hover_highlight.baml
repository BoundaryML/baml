// Config blocks are gone; hover/highlight now targets the constructed client
// value: the builtin client class, its constructor, and named arguments.
client ConfiguredClient = openai.OpenAiClient.new(
  mod<[CURSOR]el = "gpt-4o-mini",
  base_url = "http://localhost:1234",
);

client OtherClient = openai.OpenAiCl<[CURSOR]ient.n<[CURSOR]ew(model = "gpt-4o");

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b967_config_block_hover_highlight.baml:4:6
// No hover content
// hover at b967_config_block_hover_highlight.baml:8:37
// ```baml
// class OpenAiClient {
//     model: string,
//     api_key: string | null,
//     base_url: string | null,
//     api_key_env: string | null,
//     base_url_env: string | null,
//     implements ai.Client {}
//     implements ai.stream.StreamingClient {}
// }
// ```
// Run `baml describe openai.OpenAiClient` for methods and details.
// hover at b967_config_block_hover_highlight.baml:8:43
// No hover content
//
//- semantic_tokens
// b967_config_block_hover_highlight.baml:1:1 (comment) len=77 "// Config blocks are gone; hover/highlight now targets the constructed client"
// b967_config_block_hover_highlight.baml:2:1 (comment) len=73 "// value: the builtin client class, its constructor, and named arguments."
// b967_config_block_hover_highlight.baml:3:1 (keyword) len=6 "client"
// b967_config_block_hover_highlight.baml:3:25 (operator) len=1 "="
// b967_config_block_hover_highlight.baml:3:27 (namespace) [defaultLibrary] len=6 "openai"
// b967_config_block_hover_highlight.baml:3:33 (namespace) [defaultLibrary] len=1 "."
// b967_config_block_hover_highlight.baml:3:34 (class) [defaultLibrary] len=12 "OpenAiClient"
// b967_config_block_hover_highlight.baml:3:47 (method) len=3 "new"
// b967_config_block_hover_highlight.baml:4:3 (parameter) len=5 "model"
// b967_config_block_hover_highlight.baml:4:9 (operator) len=1 "="
// b967_config_block_hover_highlight.baml:4:11 (string) len=13 "\"gpt-4o-mini\""
// b967_config_block_hover_highlight.baml:5:3 (parameter) len=8 "base_url"
// b967_config_block_hover_highlight.baml:5:12 (operator) len=1 "="
// b967_config_block_hover_highlight.baml:5:14 (string) len=23 "\"http://localhost:1234\""
// b967_config_block_hover_highlight.baml:8:1 (keyword) len=6 "client"
// b967_config_block_hover_highlight.baml:8:20 (operator) len=1 "="
// b967_config_block_hover_highlight.baml:8:22 (namespace) [defaultLibrary] len=6 "openai"
// b967_config_block_hover_highlight.baml:8:28 (namespace) [defaultLibrary] len=1 "."
// b967_config_block_hover_highlight.baml:8:29 (class) [defaultLibrary] len=12 "OpenAiClient"
// b967_config_block_hover_highlight.baml:8:42 (method) len=3 "new"
// b967_config_block_hover_highlight.baml:8:46 (parameter) len=5 "model"
// b967_config_block_hover_highlight.baml:8:52 (operator) len=1 "="
// b967_config_block_hover_highlight.baml:8:54 (string) len=8 "\"gpt-4o\""

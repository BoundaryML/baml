# Live LLM provider tests

This BAML test group checks that six provider clients can parse a bare `hello world` into a string, using both a normal call and a streamed final result, and that `${cache()}` works on five clients (seven cases). It runs directly in the BAML CLI, without an SDK bridge. These are live API calls that require credentials and incur provider usage; missing credentials fail the tests.

From the repository root, build the local CLI and run under Infisical's `dev-llm-provider-tests` environment:

```sh
cd baml_language
cargo build -p baml_cli
infisical run --env=dev-llm-provider-tests -- target/debug/baml-cli test --from crates/baml_tests/baml_src --profile live -i root.llm_providers::live::
```

To run one provider, replace `-i root.llm_providers::live::` with `-i root.llm_providers::live::BedrockHaiku45` (or another test name below). To check compilation without credentials or network calls, run `target/debug/baml-cli check --from crates/baml_tests/baml_src`.

| Test | Client | Model | Required environment variables |
| --- | --- | --- | --- |
| `BedrockHaiku45` | `aws.BedrockClient` | Claude Haiku 4.5 | `BEDROCK_AWS_REGION`, `BEDROCK_AWS_ACCESS_KEY_ID`, `BEDROCK_AWS_SECRET_ACCESS_KEY` |
| `BedrockHaiku45Cache` | `aws.BedrockClient` | Claude Haiku 4.5 | the same as `BedrockHaiku45` |
| `AnthropicHaiku45Cache` | `anthropic.Client` | `claude-haiku-4-5` | `ANTHROPIC_API_KEY` |
| `OpenAIChatGpt56Cache` | `openai.ChatClient` | `gpt-5.6-luna` | `OPENAI_API_KEY` |
| `OpenAIResponsesGpt56Cache` | `openai.ResponsesClient` | `gpt-5.6-luna` | `OPENAI_API_KEY` |
| `AzureChatGpt56Cache` | `openai.AzureClient` | the `gpt-5.6-luna` deployment | `AZURE_OPENAI_RESPONSES_BASE_URL`, `AZURE_OPENAI_API_KEY` |
| `AzureChatGpt56CacheKey` | `openai.AzureClient` | the `gpt-5.6-luna` deployment, with `prompt_cache_key` | the same as `AzureChatGpt56Cache` |
| `AzureChatGpt5MiniCache` | `openai.AzureClient` | the `gpt-5-mini` deployment | `AZURE_OPENAI_RESPONSES_BASE_URL`, `AZURE_OPENAI_API_KEY` |
| `BedrockGptOss20b` | `openai.ResponsesClient` | `openai.gpt-oss-20b`, low reasoning | `BEDROCK_MANTLE_API_KEY` |
| `AzureGpt5MiniLow` | `openai.ResponsesClient` | `gpt-5-mini`, low reasoning | `AZURE_OPENAI_RESPONSES_BASE_URL`, `AZURE_OPENAI_API_KEY` |
| `Gemini31FlashLiteMinimal` | `google.GeminiClient` | `gemini-3.1-flash-lite`, minimal thinking | `GOOGLE_API_KEY` |
| `VertexGeminiFlashLiteLow` | `google.VertexClient` | `gemini-3.5-flash-lite`, low thinking | `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` |
| `VertexLlama4ScoutRouter` | `openai.GenericClient` | `meta/llama-4-scout-17b-16e-instruct-maas` | `VERTEX_LLAMA_OPENAI_BASE_URL`, `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` or `VERTEX_ACCESS_TOKEN` |

Vertex Gemini resolves the project from the service account document or `GOOGLE_CLOUD_PROJECT`. Vertex Model Garden uses `VERTEX_ACCESS_TOKEN` when set; otherwise it mints an OAuth token from the service account. The Llama endpoint is in `us-east5` and requires `max_tokens`. Mantle uses its `/v1` endpoint for gpt-oss.

The `*Cache` tests check `${cache()}` end to end. Each sends a prompt with a fresh prefix and a cache delimiter with no args twice, so the client supplies its own default marker:

- `BedrockHaiku45Cache` and `AnthropicHaiku45Cache` (~12,000-token prefix) assert that the first call reports the prefix as written to the cache and the second reports the same number of tokens read from it.
- `OpenAIChatGpt56Cache` and `OpenAIResponsesGpt56Cache` (~12,000-token prefix) assert that the default is `{"mode": "explicit"}`, that GPT-5.6 accepts it, and that the second call reads the prefix from the cache. OpenAI also caches unmarked prefixes, so the read alone does not prove the marker worked.
- `AzureChatGpt56Cache` and `AzureChatGpt56CacheKey` (~12,000-token prefix) assert that GPT-5.6 on Azure accepts the default marker and that the first call reports the prefix as written to the cache. They do not assert a read: Azure takes a few seconds to make a write readable, so a second call made right after the first almost always misses. `AzureChatGpt56CacheKey` builds the client from `resource_name` and `deployment_id`, sets `prompt_cache_key` through `request_body`, and asserts the request carries it.
- `AzureChatGpt5MiniCache` (~12,000-token prefix) asserts that the default for a `gpt-5-mini` model is null and that both calls succeed. `openai.AzureClient` judges its `model` by the same rules as the OpenAI clients. The Azure cache tests call a deployment named after its model on the resource behind `AZURE_OPENAI_RESPONSES_BASE_URL` (currently `boundarydev-resource`), with `/openai/v1` swapped for `/openai/deployments/<model>`. Nothing is asserted about cache reads, because Azure's implicit cache misses often enough to flake. Provisioned (PTU-M) Azure deployments are not supported: they reject `prompt_cache_breakpoint`, which a GPT-5.6 `model` sends.

The default client on `HelloWorld` supplies the function spec only: every test overrides it with the corresponding provider client, so only the OpenAI cache tests need an OpenAI API key. The group lives in `baml_tests` under `testset "live" with testing.Sequential()`. The default `offline` profile excludes it before any credentials are read; the command above selects only this group. Missing credentials fail a selected test rather than silently reducing coverage.

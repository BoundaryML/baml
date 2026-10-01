# Live LLM provider tests

This standalone BAML fixture checks that six provider clients can parse a bare `hello world` into a string, using both a normal call and a streamed final result. It runs directly in the BAML CLI, without an SDK bridge. These are live API calls that require credentials and incur provider usage; missing credentials fail the tests.

From the repository root, build the local CLI and run under Infisical's `dev-llm-provider-tests` environment:

```sh
cd baml_language
cargo build -p baml_cli
infisical run --env=dev-llm-provider-tests -- target/debug/baml-cli test --from tests/llm_providers
```

To run one provider, append `-i BedrockHaiku45` (or another test name below). To check compilation without credentials or network calls, run `target/debug/baml-cli check --from tests/llm_providers`.

| Test | Client | Model | Required environment variables |
| --- | --- | --- | --- |
| `BedrockHaiku45` | `aws.BedrockClient` | Claude Haiku 4.5 | `BEDROCK_AWS_REGION`, `BEDROCK_AWS_ACCESS_KEY_ID`, `BEDROCK_AWS_SECRET_ACCESS_KEY` |
| `BedrockGptOss20b` | `openai.ResponsesClient` | `openai.gpt-oss-20b`, low reasoning | `BEDROCK_MANTLE_API_KEY` |
| `AzureGpt5MiniLow` | `openai.ResponsesClient` | `gpt-5-mini`, low reasoning | `AZURE_OPENAI_RESPONSES_BASE_URL`, `AZURE_OPENAI_API_KEY` |
| `Gemini31FlashLiteMinimal` | `google.GeminiClient` | `gemini-3.1-flash-lite`, minimal thinking | `GOOGLE_API_KEY` |
| `VertexGeminiFlashLiteLow` | `google.VertexClient` | `gemini-3.5-flash-lite`, low thinking | `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` |
| `VertexLlama4ScoutRouter` | `openai.GenericClient` | `meta/llama-4-scout-17b-16e-instruct-maas` | `VERTEX_LLAMA_OPENAI_BASE_URL`, `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` or `VERTEX_ACCESS_TOKEN` |

Vertex Gemini resolves the project from the service account document or `GOOGLE_CLOUD_PROJECT`. Vertex Model Garden uses `VERTEX_ACCESS_TOKEN` when set; otherwise it mints an OAuth token from the service account. The Llama endpoint is in `us-east5` and requires `max_tokens`. Mantle uses its `/v1` endpoint for gpt-oss.

The default client on `HelloWorld` supplies the function spec only: every test overrides it with the corresponding provider client, so no OpenAI API key is needed. This fixture is separate from the keyless compiler corpus and SDK test matrix; run it explicitly with the command above.

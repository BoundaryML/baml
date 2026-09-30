# Python streaming UI

A Streamlit app (plus a headless `bench.py`) for checking that structured-output streaming through `baml_sdk` into a Python UI performs well end to end. For each (provider, v1 client) scenario in the migration test matrix, it streams the same extraction twice: once through BAML (A) and once through the provider's native Python SDK (B). It then compares what a UI consuming each stream would see.

## Task

`ExtractPullRequest -> PullRequest` asks the model to rebuild the `gh pr view --json` object for [BoundaryML/baml#5041](https://github.com/BoundaryML/baml/pull/5041) from a plaintext rendering of it. `PullRequest` in `baml_src/pull_request.prompt.baml` mirrors the gh JSON field for field, so each final result is scored against `data/pr_5041.json` ("field accuracy"). The reply is about 29K characters of JSON.

- **A (baml):** `baml_sdk.ExtractPullRequest_stream_async(pr_text, scenario)`. The schema goes in the prompt via `${ctx.output_format()}`, a TS-style object literal rather than JSON Schema, and BAML parses partials engine-side.
- **B (native SDK):** the same instructions through the provider's own SDK, using structured outputs with a Pydantic mirror of `PullRequest` (`stream_ui/models.py`). Partials come from `jiter` parsing the text so far.

## Scenarios

| # | Provider | A: v1 client (`baml_src/scenarios.baml`) | Model | B: native SDK |
| --- | --- | --- | --- | --- |
| 1 | AWS Bedrock | `aws.BedrockClient` | Claude Haiku 4.5 | `anthropic[bedrock]` `AsyncAnthropicBedrock().messages.stream(output_format=...)` |
| 2 | Bedrock Mantle | `openai.ResponsesClient` | `openai.gpt-oss-20b` | `openai` `AsyncOpenAI(base_url=<Mantle /v1>).responses.stream(text_format=...)` |
| 3 | Azure AI Foundry | `openai.ResponsesClient` | `gpt-5-mini`, effort low | `openai` `AsyncOpenAI(base_url=<Foundry>).responses.stream(text_format=...)` |
| 4 | Google AI Studio | `google.GeminiClient` | Gemini 3.1 Flash-Lite, thinking minimal | `google-genai` `generate_content_stream(response_schema=...)` |
| 5 | Vertex AI | `google.VertexClient` | Gemini 3.5 Flash-Lite, thinking low | `google-genai` `Client(vertexai=True)`, same call |
| 6 | Vertex Model Garden | `openai.GenericClient` | Llama 4 Scout (MaaS) | `openai` `AsyncOpenAI(base_url=<Vertex MaaS>).chat.completions.stream(response_format=...)` |

The client settings mirror the representatives in `sdk_tests/fixtures/llm_providers` (PR #5041). Scenario 6 raises Llama's `max_tokens` from 4096 to the endpoint's 8192 ceiling, because the output here is much longer.

## Metrics

- **first partial**: request start → first yielded value.
- **partials, partials/s, chars/s**: how often the UI gets something new, and how fast content arrives.
- **gap p50/p95/max**: time between consecutive partials; big gaps read as stalls.
- **duplicates**: partials identical to the previous one (wasted renders).
- **shrinks**: partials that render smaller than the previous one (visible flicker).
- **convert / render**: time spent turning partials into plain data and in Streamlit calls.
- **cpu**: process CPU time for the run.
- **loop lag p95/max**: how late a 5 ms asyncio timer fires while streaming. If the stream blocks the event loop, the UI freezes.
- **field accuracy**: share of ground-truth JSON leaves reproduced exactly.

Run A and B one after the other (the default) when comparing CPU and loop lag. The concurrent mode is for watching them race.

## Setup

The app builds `baml_bridge` from this checkout in release mode, and generates `baml_sdk` with a matching `baml-cli`. Scenario 1 needs Bedrock streaming from PR #5047.

```bash
cd baml_language
cargo build -p baml_cli --bin baml-cli
cd examples/python_streaming_ui
BAML_VERSION=../../target/debug/baml-cli baml generate   # writes ./baml_sdk (gitignored)
uv sync                                                  # builds baml_bridge (release) from ../../sdks/python
```

After changing Rust under `baml_language/`, rebuild the bridge with `uv sync --reinstall-package baml_bridge`.

## Run

The credentials for all six scenarios are in the `dev-llm-provider-tests` Infisical environment. The Scenarios tab lists any that are missing.

```bash
infisical run --env=dev-llm-provider-tests -- uv run streamlit run app.py
infisical run --env=dev-llm-provider-tests -- uv run python bench.py --scenario 1 4
```

## Seed data

`data/pr_5041.json` and `data/pr_5041.txt` are checked in. To refresh them or seed another PR:

```bash
uv run python seed.py          # BoundaryML/baml#5041
uv run python seed.py 1234     # another PR
```

## Known issues this surfaced (2026-09-30)

### Native SDK side (B)

- **Scenario 2 (Mantle `gpt-oss-20b`):** the model does not honor the structured-output schema. It returned fenced, malformed JSON, and in one run took 405s. The openai SDK's `responses.stream()` helper also crashes on Mantle's event stream, so B reads the raw `responses.create(stream=True)` events.
- **Scenario 6 (Vertex MaaS Llama 4 Scout):** `response_format` isn't enforced, and the reply doesn't match `PullRequest`.
- **anthropic 1.x:** `messages.stream()` no longer takes `temperature`, so B passes it via `extra_body`.

### BAML side (A)

- **A structured stream fails if the reply starts with anything but JSON.** The non-streaming call parses these replies fine, but `ExtractPullRequest_stream` fails on its first partial with `LlmClient: <root>: Expected user.PullRequest, got String("…", Incomplete)` instead of holding the partial back until the JSON starts. In the A vs B runs this blocked side A for:
  - scenario 1 (Bedrock Haiku 4.5, `` ```json `` fence);
  - scenario 4 (Gemini 3.1 Flash-Lite, fence);
  - scenario 5 (Vertex Gemini, fence);
  - scenario 6 (Llama 4 Scout, which opens with "Here…").
- **A required `int`/`bool` field holds back every partial until it arrives.** A class like `{ a: string, n: int }` yields 1 partial where `{ a: string, b: string }` yields ~40, because the class has no partial value until `n` exists. Put scalar fields first, or make them optional, if the UI should fill in progressively.
- **A client handle can't cross into an `ai.Client` parameter.** Passing a client returned by a BAML function back into `Fn(..., llm=client)` fails with `host value type anthropic.Client is not a member of declared union ai.Client | null`. So `ExtractPullRequest` takes a scenario number and builds the client in BAML (`baml_src/scenarios.baml`).

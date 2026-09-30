# Python streaming UI

A Streamlit app (plus a headless `bench.py`) for checking that streaming LLM output through `baml_sdk` into a Python UI performs well end to end. It runs the same task through three stacks and measures what a UI consuming the stream would feel:

| Backend | How it streams | Where partials come from |
| --- | --- | --- |
| `baml → anthropic` / `baml → openai` | `baml_sdk.<Fn>_stream_async(...)`, schema in the prompt via `${ctx.output_format()}` (a TS-style object literal, not JSON Schema) | BAML's parser, engine-side |
| `anthropic sdk` | `AsyncAnthropic().messages.stream(..., output_format=PydanticModel)` (structured outputs) | `jiter` partial JSON parse of the text so far |
| `openai sdk` | `AsyncOpenAI().responses.stream(..., text_format=PydanticModel)` (structured outputs) | `jiter` partial JSON parse of the text so far |

## Stream shapes

- **Structured output** (`ExtractPullRequest -> PullRequest`): the model rebuilds the `gh pr view --json` object for [BoundaryML/baml#5041](https://github.com/BoundaryML/baml/pull/5041) from a plaintext rendering of it. `PullRequest` in `baml_src/pull_request.prompt.baml` mirrors the gh JSON field for field, so the final result is scored against `data/pr_5041.json` ("field accuracy").
- **Single long string** (`WriteNarrative -> string`): a ~1500-word narrative of the PR.
- **Object with one long string** (`WriteNarrativeObject -> Narrative { text }`): the same narrative wrapped in a one-field class.

## Metrics

- **first partial**: request start → first yielded value.
- **partials, partials/s, chars/s**: how often the UI gets something new, and how fast content arrives.
- **gap p50/p95/max**: time between consecutive partials; big gaps read as stalls.
- **duplicates**: partials identical to the previous one (wasted renders).
- **shrinks**: partials that render smaller than the previous one (visible flicker).
- **convert / render**: time spent turning partials into plain data and in Streamlit calls.
- **cpu**: process CPU time for the run.
- **loop lag p95/max**: how late a 5 ms asyncio timer fires while streaming. If the stream blocks the event loop, the UI freezes.
- **field accuracy** (structured only): share of ground-truth JSON leaves reproduced exactly.

Run backends sequentially (the default) when comparing CPU and loop lag; concurrent mode is for eyeballing them side by side.

## Setup

The app builds `baml_bridge` from this checkout in release mode, and generates `baml_sdk` with a matching `baml-cli`:

```bash
cd baml_language
cargo build -p baml_cli --bin baml-cli
cd examples/python_streaming_ui
BAML_VERSION=../../target/debug/baml-cli baml generate   # writes ./baml_sdk (gitignored)
uv sync                                                  # builds baml_bridge (release) from ../../sdks/python
```

After changing Rust under `baml_language/`, rebuild the bridge with `uv sync --reinstall-package baml_bridge`.

## Run

Needs `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` (both are in the `dev-llm-provider-tests` Infisical environment):

```bash
infisical run --env=dev-llm-provider-tests -- uv run streamlit run app.py
infisical run --env=dev-llm-provider-tests -- uv run python bench.py --shape structured
```

The default models are `claude-haiku-4-5` and `gpt-4.1-mini`; both are editable in the sidebar.

## Seed data

`data/pr_5041.json` and `data/pr_5041.txt` are checked in. To refresh them or seed another PR:

```bash
uv run python seed.py          # BoundaryML/baml#5041
uv run python seed.py 1234     # another PR
```

## Known issues this surfaced (2026-09-30)

- **Structured streams fail when the reply opens with a code fence.** `gpt-4.1-mini` answers `ExtractPullRequest` with `` ```json `` first. The non-streaming call parses that fine, but `ExtractPullRequest_stream` fails on its first partial with `LlmClient: <root>: Expected user.PullRequest, got String("", Incomplete)` instead of withholding the partial until the JSON starts. It depends on how the model opens its reply, so the same run fails some of the time and passes otherwise.
- **A required `int`/`bool` field holds back every partial until it arrives.** A class like `{ a: string, n: int }` yields 1 partial where `{ a: string, b: string }` yields ~40, because the class has no partial value until `n` exists. Put scalar fields first, or make them optional, if the UI should fill in progressively.
- **A client handle can't cross into an `ai.Client` parameter.** Passing a client returned by a BAML function back into `Fn(..., llm=client)` fails with `host value type anthropic.Client is not a member of declared union ai.Client | null`. So the functions here take `provider`/`model` strings and build the client in BAML (`baml_src/clients.baml`).

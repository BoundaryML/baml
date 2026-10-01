"""A vs B streams for the six (provider, v1 client) scenarios in 02-stream-test-matrix.md.

A: baml_sdk `ExtractPullRequest_stream_async`, through the scenario's v1 client (`baml_src/scenarios.baml`).
   The schema goes in the prompt via `${ctx.output_format()}`; BAML parses partials engine-side.
B: the provider's native Python SDK with structured outputs where the provider has them. Partials come
   from `jiter` parsing the text so far, which is what a hand-written integration would do.
C: BAML v0 (baml-py) `b.stream.ExtractPullRequest`, through the scenario's v0 `client<llm>` (`v0/baml_src`).

All sides get the same instructions and the same PR text, and yield `Partial`s then one `Final`.

Workloads:
  structured - `ExtractPullRequest -> PullRequest`, the gh JSON rebuilt from prose.
  string     - `WriteNarrative -> string`, a long narrative of the same PR. B streams plain text, with no
               structured outputs, and its partials are the text so far.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Any, AsyncIterator, Literal

import jiter

from . import models

Side = Literal["baml", "native", "v0"]
SIDES: tuple[Side, ...] = ("baml", "native", "v0")
SIDE_LETTER: dict[str, str] = {"baml": "A", "native": "B", "v0": "C"}

Workload = Literal["structured", "string"]
WORKLOADS: tuple[Workload, ...] = ("structured", "string")

# Same instructions as baml_src/pull_request.prompt.baml, minus `${ctx.output_format()}`.
EXTRACT_INSTRUCTIONS = """\
Convert this GitHub pull request, written out as prose, back into structured data.
Copy every text field (bodies, message bodies, headlines) verbatim, including markdown and HTML.
Use null for timestamps given as "-". Enum-like values (state, changeType, authorAssociation) are upper-case.
"""

# Same instructions as baml_src/narrative.prompt.baml.
NARRATIVE_INSTRUCTIONS = """\
Write a detailed, roughly 1500-word engineering narrative of this pull request for a changelog:
what it adds, why, how it is tested, the gaps it found, and what follows up. Plain prose with markdown headings.
"""

MANTLE_GPT_OSS_BASE_URL = "https://bedrock-mantle.us-east-1.api.aws/v1"
CLOUD_PLATFORM_SCOPE = "https://www.googleapis.com/auth/cloud-platform"


@dataclass(frozen=True)
class Scenario:
    n: int
    provider: str
    v1_client: str
    representative: str
    model: str
    native_sdk: str
    env: tuple[str, ...]

    def missing_env(self) -> list[str]:
        return [name for name in self.env if not os.environ.get(name)]


SCENARIOS: dict[int, Scenario] = {
    s.n: s
    for s in [
        Scenario(
            1, "AWS Bedrock", "aws.BedrockClient", "BedrockHaiku45",
            "global.anthropic.claude-haiku-4-5-20251001-v1:0", "anthropic[bedrock] AnthropicBedrock",
            ("BEDROCK_AWS_REGION", "BEDROCK_AWS_ACCESS_KEY_ID", "BEDROCK_AWS_SECRET_ACCESS_KEY"),
        ),
        Scenario(
            2, "Bedrock Mantle", "openai.ResponsesClient", "BedrockGptOss20b",
            "openai.gpt-oss-20b", "openai[bedrock] OpenAI(provider=bedrock())", ("BEDROCK_MANTLE_API_KEY",),
        ),
        Scenario(
            3, "Azure AI Foundry", "openai.ResponsesClient", "AzureGpt5MiniLow",
            "gpt-5-mini", "openai OpenAI (Foundry)", ("AZURE_OPENAI_API_KEY", "AZURE_OPENAI_RESPONSES_BASE_URL"),
        ),
        Scenario(
            4, "Google AI Studio", "google.GeminiClient", "Gemini31FlashLiteMinimal",
            "gemini-3.1-flash-lite", "google-genai Client", ("GOOGLE_API_KEY",),
        ),
        Scenario(
            5, "Vertex AI", "google.VertexClient", "VertexGeminiFlashLiteLow",
            "gemini-3.5-flash-lite", "google-genai Client(vertexai=True)", ("GOOGLE_APPLICATION_CREDENTIALS_CONTENT",),
        ),
        Scenario(
            6, "Vertex Model Garden", "openai.GenericClient", "VertexLlama4ScoutRouter",
            "meta/llama-4-scout-17b-16e-instruct-maas", "openai OpenAI (Vertex MaaS)",
            ("VERTEX_LLAMA_OPENAI_BASE_URL", "GOOGLE_APPLICATION_CREDENTIALS_CONTENT"),
        ),
    ]
}  # fmt: skip


@dataclass(frozen=True)
class Partial:
    value: Any


@dataclass(frozen=True)
class Final:
    value: Any


@dataclass(frozen=True)
class RunConfig:
    scenario: int
    side: Side
    pr_text: str
    workload: Workload = "structured"

    @property
    def label(self) -> str:
        s = SCENARIOS[self.scenario]
        if self.side == "baml":
            return f"{s.n}A baml v1 ({s.v1_client})"
        if self.side == "v0":
            return f"{s.n}C baml v0 ({s.representative})"
        return f"{s.n}B {s.native_sdk}"


def native_prompt(pr_text: str, workload: Workload = "structured") -> str:
    instructions = EXTRACT_INSTRUCTIONS if workload == "structured" else NARRATIVE_INSTRUCTIONS
    return f"{instructions}\n{pr_text}"


def baml_prompt(pr_text: str, workload: Workload = "structured") -> str:
    """The prompt BAML renders; for `structured`, with the TS-literal schema from `ctx.output_format()`.

    It is the same for every scenario. Scenario 4 renders it because its client resolves credentials only
    when a request is sent, so this works without keys.
    """
    import baml_sdk

    spec_fn = baml_sdk.ExtractPullRequest_spec if workload == "structured" else baml_sdk.WriteNarrative_spec
    return spec_fn(pr_text, 4).prompt().text()


def stream(cfg: RunConfig) -> AsyncIterator[Partial | Final]:
    """Yields a `Partial` as each one arrives, then one `Final`."""
    if cfg.side == "baml":
        return _stream_baml(cfg)
    if cfg.side == "v0":
        return _stream_v0(cfg)
    structured = cfg.workload == "structured"
    deltas = _NATIVE_TEXT[cfg.scenario](native_prompt(cfg.pr_text, cfg.workload), structured)
    return _accumulate(deltas, structured)


# --------------------------------------------------------------------------- A: baml


async def _stream_baml(cfg: RunConfig) -> AsyncIterator[Partial | Final]:
    import baml_sdk
    from baml_sdk.ai.stream import Done

    stream_fn = (
        baml_sdk.ExtractPullRequest_stream_async
        if cfg.workload == "structured"
        else baml_sdk.WriteNarrative_stream_async
    )
    stream = await stream_fn(cfg.pr_text, cfg.scenario)
    while True:
        v = await stream.next_async()
        if isinstance(v, Done):
            break
        yield Partial(v)
    yield Final(await stream.final_async())


# --------------------------------------------------------------------------- C: baml v0


async def _stream_v0(cfg: RunConfig) -> AsyncIterator[Partial | Final]:
    from baml_client.async_client import b

    options: dict[str, Any] = {"client": SCENARIOS[cfg.scenario].representative}
    if cfg.scenario == 6:
        # v0 reads VERTEX_ACCESS_TOKEN like the user's config; mint a fresh one per call.
        options["env"] = {**os.environ, "VERTEX_ACCESS_TOKEN": _vertex_access_token()}
    stream_fn = b.stream.ExtractPullRequest if cfg.workload == "structured" else b.stream.WriteNarrative
    stream = stream_fn(cfg.pr_text, baml_options=options)
    async for partial in stream:
        yield Partial(partial)
    yield Final(await stream.get_final_response())


# --------------------------------------------------------------------------- B: native SDKs
#
# Each `_text_N(prompt, structured)` yields the reply's text deltas, asking for structured outputs only when
# `structured`; `_accumulate` turns the deltas into partials.


async def _accumulate(deltas: AsyncIterator[str], structured: bool) -> AsyncIterator[Partial | Final]:
    buf = ""
    async for delta in deltas:
        if not delta:
            continue
        buf += delta
        if not structured:
            yield Partial(buf)
            continue
        try:
            partial = jiter.from_json(buf.encode(), partial_mode="trailing-strings")
        except ValueError:
            continue
        yield Partial(partial)
    yield Final(models.PullRequest.model_validate_json(buf) if structured else buf)


async def _openai_responses(client: Any, structured: bool, **kwargs: Any) -> AsyncIterator[str]:
    if structured:
        kwargs["text_format"] = models.PullRequest
    async with client.responses.stream(store=False, **kwargs) as s:
        async for event in s:
            if event.type == "response.output_text.delta":
                yield event.delta


async def _text_1(prompt: str, structured: bool) -> AsyncIterator[str]:
    from anthropic import AsyncAnthropicBedrock

    client = AsyncAnthropicBedrock(
        aws_access_key=os.environ["BEDROCK_AWS_ACCESS_KEY_ID"],
        aws_secret_key=os.environ["BEDROCK_AWS_SECRET_ACCESS_KEY"],
        aws_region=os.environ["BEDROCK_AWS_REGION"],
    )
    kwargs: dict[str, Any] = {"output_format": models.PullRequest} if structured else {}
    async with client.messages.stream(
        model=SCENARIOS[1].model,
        max_tokens=64000,
        messages=[{"role": "user", "content": prompt}],
        # anthropic 1.x dropped the `temperature` argument; match side A's 0.0.
        extra_body={"temperature": 0.0},
        **kwargs,
    ) as s:
        async for text in s.text_stream:
            yield text


async def _text_2(prompt: str, structured: bool) -> AsyncIterator[str]:
    from openai import AsyncOpenAI
    from openai.lib._parsing._responses import type_to_text_format_param
    from openai.providers import bedrock

    # openai[bedrock]'s provider defaults to Mantle's `/openai/v1`, which doesn't serve gpt-oss.
    client = AsyncOpenAI(
        provider=bedrock(
            endpoint="mantle", base_url=MANTLE_GPT_OSS_BASE_URL, api_key=os.environ["BEDROCK_MANTLE_API_KEY"]
        )
    )
    # `responses.stream()` crashes on Mantle's `response.reasoning_part.added` / `reasoning_text.delta` events
    # (its snapshot never sees the part), so read the raw event stream instead; the request is the same.
    kwargs: dict[str, Any] = {"text": {"format": type_to_text_format_param(models.PullRequest)}} if structured else {}
    stream = await client.responses.create(
        model=SCENARIOS[2].model,
        input=prompt,
        reasoning={"effort": "low"},
        store=False,
        stream=True,
        **kwargs,
    )
    async for event in stream:
        if event.type == "response.output_text.delta":
            yield event.delta


async def _text_3(prompt: str, structured: bool) -> AsyncIterator[str]:
    from openai import AsyncOpenAI

    key = os.environ["AZURE_OPENAI_API_KEY"]
    client = AsyncOpenAI(
        base_url=os.environ["AZURE_OPENAI_RESPONSES_BASE_URL"], api_key=key, default_headers={"api-key": key}
    )
    async for delta in _openai_responses(
        client, structured, model=SCENARIOS[3].model, input=prompt, reasoning={"effort": "low"}
    ):
        yield delta


async def _gemini(
    client: Any, model: str, prompt: str, structured: bool, thinking_level: str, temperature: float | None
) -> AsyncIterator[str]:
    from google.genai import types

    schema: dict[str, Any] = (
        {"response_mime_type": "application/json", "response_schema": models.PullRequest} if structured else {}
    )
    config = types.GenerateContentConfig(
        temperature=temperature,
        thinking_config=types.ThinkingConfig(thinking_level=thinking_level),
        **schema,
    )
    async for chunk in await client.aio.models.generate_content_stream(model=model, contents=prompt, config=config):
        if chunk.text:
            yield chunk.text


async def _text_4(prompt: str, structured: bool) -> AsyncIterator[str]:
    from google import genai

    client = genai.Client(api_key=os.environ["GOOGLE_API_KEY"])
    async for delta in _gemini(client, SCENARIOS[4].model, prompt, structured, "MINIMAL", None):
        yield delta


def _service_account_credentials() -> Any:
    from google.oauth2 import service_account

    info = json.loads(os.environ["GOOGLE_APPLICATION_CREDENTIALS_CONTENT"])
    return service_account.Credentials.from_service_account_info(info, scopes=[CLOUD_PLATFORM_SCOPE])


async def _text_5(prompt: str, structured: bool) -> AsyncIterator[str]:
    from google import genai

    creds = _service_account_credentials()
    client = genai.Client(vertexai=True, project=creds.project_id, location="global", credentials=creds)
    async for delta in _gemini(client, SCENARIOS[5].model, prompt, structured, "LOW", 0.0):
        yield delta


def _vertex_access_token() -> str:
    if token := os.environ.get("VERTEX_ACCESS_TOKEN"):
        return token
    import google.auth.transport.requests

    creds = _service_account_credentials()
    creds.refresh(google.auth.transport.requests.Request())
    return creds.token


async def _text_6(prompt: str, structured: bool) -> AsyncIterator[str]:
    from openai import AsyncOpenAI

    client = AsyncOpenAI(base_url=os.environ["VERTEX_LLAMA_OPENAI_BASE_URL"], api_key=_vertex_access_token())
    kwargs: dict[str, Any] = {"response_format": models.PullRequest} if structured else {}
    async with client.chat.completions.stream(
        model=SCENARIOS[6].model,
        messages=[{"role": "user", "content": prompt}],
        max_tokens=8192,
        temperature=0.0,
        **kwargs,
    ) as s:
        async for event in s:
            if event.type == "content.delta":
                yield event.delta


_NATIVE_TEXT = {1: _text_1, 2: _text_2, 3: _text_3, 4: _text_4, 5: _text_5, 6: _text_6}

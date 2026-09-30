"""One async generator per (backend, shape): each yields the latest partial value as it streams.

Backends:
  baml       - baml_sdk `<Fn>_stream_async`; BAML parses partials engine-side.
  openai     - openai SDK Responses API with structured outputs; partials parsed with jiter.
  anthropic  - anthropic SDK Messages API with structured outputs; partials parsed with jiter.

Shapes:
  structured - the whole PullRequest object (data/pr_*.json)
  text       - one long string
  object     - {"text": <one long string>}
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, AsyncIterator, Callable, Literal

import jiter

from . import models

Backend = Literal["baml", "openai", "anthropic"]
Shape = Literal["structured", "text", "object"]
Provider = Literal["anthropic", "openai"]

BACKENDS: tuple[Backend, ...] = ("baml", "openai", "anthropic")
SHAPES: tuple[Shape, ...] = ("structured", "text", "object")

MAX_TOKENS = 32000

# Same instructions as baml_src/*.prompt.baml, minus `${ctx.output_format()}`:
# the SDK paths send the schema as structured-output JSON Schema instead.
EXTRACT_INSTRUCTIONS = """\
Convert this GitHub pull request, written out as prose, back into structured data.
Copy every text field (bodies, message bodies, headlines) verbatim, including markdown and HTML.
Use null for timestamps given as "-". Enum-like values (state, changeType, authorAssociation) are upper-case.
"""

NARRATIVE_INSTRUCTIONS = """\
Write a detailed, roughly 1500-word engineering narrative of this pull request for a changelog:
what it adds, why, how it is tested, the gaps it found, and what follows up. Plain prose with markdown headings.
"""


@dataclass(frozen=True)
class RunConfig:
    backend: Backend
    shape: Shape
    # Which provider BAML talks to; the SDK backends are their own provider.
    provider: Provider
    model: str
    pr_text: str

    @property
    def label(self) -> str:
        if self.backend == "baml":
            return f"baml → {self.provider} ({self.model})"
        return f"{self.backend} sdk ({self.model})"


def sdk_prompt(shape: Shape, pr_text: str) -> str:
    if shape == "structured":
        return f"{EXTRACT_INSTRUCTIONS}\n{pr_text}"
    if shape == "object":
        return f"{NARRATIVE_INSTRUCTIONS}Put the whole narrative in the text field.\n\n{pr_text}"
    return f"{NARRATIVE_INSTRUCTIONS}\n{pr_text}"


def _output_model(shape: Shape) -> type[models.BaseModel] | None:
    return {"structured": models.PullRequest, "object": models.Narrative, "text": None}[shape]


def _partial_json(buf: str) -> Any:
    """What a hand-rolled SDK integration would show: tolerant parse of the text so far."""
    if not buf:
        return None
    try:
        return jiter.from_json(buf.encode(), partial_mode="trailing-strings")
    except ValueError:
        return None


# --------------------------------------------------------------------------- baml


def baml_prompt(shape: Shape, pr_text: str) -> str:
    """The prompt BAML renders, including the TS-literal schema from `ctx.output_format()`."""
    import baml_sdk

    spec_fn = {
        "structured": baml_sdk.ExtractPullRequest_spec,
        "text": baml_sdk.WriteNarrative_spec,
        "object": baml_sdk.WriteNarrativeObject_spec,
    }[shape]
    return spec_fn(pr_text).prompt().text()


async def _stream_baml(cfg: RunConfig) -> AsyncIterator[Any]:
    import baml_sdk
    from baml_sdk.ai.stream import Done

    stream_fn: Callable[..., Any] = {
        "structured": baml_sdk.ExtractPullRequest_stream_async,
        "text": baml_sdk.WriteNarrative_stream_async,
        "object": baml_sdk.WriteNarrativeObject_stream_async,
    }[cfg.shape]
    stream = await stream_fn(cfg.pr_text, provider=cfg.provider, model=cfg.model)
    while True:
        v = await stream.next_async()
        if isinstance(v, Done):
            break
        yield v
    yield await stream.final_async()


# --------------------------------------------------------------------------- openai


async def _stream_openai(cfg: RunConfig) -> AsyncIterator[Any]:
    from openai import AsyncOpenAI

    client = AsyncOpenAI()
    kwargs: dict[str, Any] = dict(
        model=cfg.model, input=sdk_prompt(cfg.shape, cfg.pr_text), max_output_tokens=MAX_TOKENS
    )
    output_model = _output_model(cfg.shape)
    if output_model is not None:
        kwargs["text_format"] = output_model

    buf = ""
    async with client.responses.stream(**kwargs) as stream:
        async for event in stream:
            if event.type != "response.output_text.delta":
                continue
            buf += event.delta
            partial = buf if output_model is None else _partial_json(buf)
            if partial is not None:
                yield partial
        final = await stream.get_final_response()
    yield final.output_parsed if output_model is not None else final.output_text


# --------------------------------------------------------------------------- anthropic


async def _stream_anthropic(cfg: RunConfig) -> AsyncIterator[Any]:
    from anthropic import AsyncAnthropic

    client = AsyncAnthropic()
    kwargs: dict[str, Any] = dict(
        model=cfg.model,
        max_tokens=MAX_TOKENS,
        messages=[{"role": "user", "content": sdk_prompt(cfg.shape, cfg.pr_text)}],
    )
    output_model = _output_model(cfg.shape)
    if output_model is not None:
        kwargs["output_format"] = output_model

    buf = ""
    async with client.messages.stream(**kwargs) as stream:
        async for text in stream.text_stream:
            buf += text
            partial = buf if output_model is None else _partial_json(buf)
            if partial is not None:
                yield partial
        final = await stream.get_final_message()
    if output_model is None:
        yield "".join(b.text for b in final.content if b.type == "text")
    else:
        yield output_model.model_validate_json(buf)


def stream(cfg: RunConfig) -> AsyncIterator[Any]:
    """Yields each partial, then the final value last."""
    return {"baml": _stream_baml, "openai": _stream_openai, "anthropic": _stream_anthropic}[cfg.backend](cfg)

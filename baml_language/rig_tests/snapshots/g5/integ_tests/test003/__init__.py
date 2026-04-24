from __future__ import annotations

import typing
import pydantic

from baml.baml_core import define_function as __define_function


AnalyzeSentiment       = __define_function("root.test003.AnalyzeSentiment", "sync",  ["text"])
AnalyzeSentiment_async = __define_function("root.test003.AnalyzeSentiment", "async", ["text"])
AnalyzeSentiment__render_prompt       = __define_function("root.test003.AnalyzeSentiment$render_prompt", "sync",  ["text"])
AnalyzeSentiment__render_prompt_async = __define_function("root.test003.AnalyzeSentiment$render_prompt", "async", ["text"])
AnalyzeSentiment__build_request       = __define_function("root.test003.AnalyzeSentiment$build_request", "sync",  ["text"])
AnalyzeSentiment__build_request_async = __define_function("root.test003.AnalyzeSentiment$build_request", "async", ["text"])
AnalyzeSentiment__parse_stream       = __define_function("root.test003.AnalyzeSentiment$parse_stream", "sync",  ["sse"])
AnalyzeSentiment__parse_stream_async = __define_function("root.test003.AnalyzeSentiment$parse_stream", "async", ["sse"])
AnalyzeSentiment__parse       = __define_function("root.test003.AnalyzeSentiment$parse", "sync",  ["json"])
AnalyzeSentiment__parse_async = __define_function("root.test003.AnalyzeSentiment$parse", "async", ["json"])


class Sentiment(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    label: typing.Union[typing.Literal["positive"], typing.Literal["negative"], typing.Literal["neutral"]]
    confidence: float


__all__ = [
    "AnalyzeSentiment",
    "AnalyzeSentiment_async",
    "AnalyzeSentiment__render_prompt",
    "AnalyzeSentiment__render_prompt_async",
    "AnalyzeSentiment__build_request",
    "AnalyzeSentiment__build_request_async",
    "AnalyzeSentiment__parse_stream",
    "AnalyzeSentiment__parse_stream_async",
    "AnalyzeSentiment__parse",
    "AnalyzeSentiment__parse_async",
    "Sentiment",
]

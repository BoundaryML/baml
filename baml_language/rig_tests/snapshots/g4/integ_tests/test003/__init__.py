from __future__ import annotations

import typing
import pydantic


AnalyzeSentiment = None
AnalyzeSentiment_async = None
AnalyzeSentiment__render_prompt = None
AnalyzeSentiment__render_prompt_async = None
AnalyzeSentiment__build_request = None
AnalyzeSentiment__build_request_async = None
AnalyzeSentiment__parse_stream = None
AnalyzeSentiment__parse_stream_async = None
AnalyzeSentiment__parse = None
AnalyzeSentiment__parse_async = None


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

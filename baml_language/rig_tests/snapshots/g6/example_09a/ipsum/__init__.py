from __future__ import annotations

import enum

from baml.baml_core import define_function as __define_function


ClassifySentiment       = __define_function("root.ipsum.ClassifySentiment", "sync",  ["text"])
ClassifySentiment_async = __define_function("root.ipsum.ClassifySentiment", "async", ["text"])
ClassifySentiment__parse_stream       = __define_function("root.ipsum.ClassifySentiment$parse_stream", "sync",  ["sse"])
ClassifySentiment__parse_stream_async = __define_function("root.ipsum.ClassifySentiment$parse_stream", "async", ["sse"])
ClassifySentiment__build_request       = __define_function("root.ipsum.ClassifySentiment$build_request", "sync",  ["text"])
ClassifySentiment__build_request_async = __define_function("root.ipsum.ClassifySentiment$build_request", "async", ["text"])
ClassifySentiment__render_prompt       = __define_function("root.ipsum.ClassifySentiment$render_prompt", "sync",  ["text"])
ClassifySentiment__render_prompt_async = __define_function("root.ipsum.ClassifySentiment$render_prompt", "async", ["text"])
ClassifySentiment__parse       = __define_function("root.ipsum.ClassifySentiment$parse", "sync",  ["json"])
ClassifySentiment__parse_async = __define_function("root.ipsum.ClassifySentiment$parse", "async", ["json"])


class Sentiment(str, enum.Enum):
    POSITIVE = "POSITIVE"
    NEGATIVE = "NEGATIVE"
    NEUTRAL = "NEUTRAL"


__all__ = [
    "ClassifySentiment",
    "ClassifySentiment_async",
    "ClassifySentiment__parse_stream",
    "ClassifySentiment__parse_stream_async",
    "ClassifySentiment__build_request",
    "ClassifySentiment__build_request_async",
    "ClassifySentiment__render_prompt",
    "ClassifySentiment__render_prompt_async",
    "ClassifySentiment__parse",
    "ClassifySentiment__parse_async",
    "Sentiment",
]

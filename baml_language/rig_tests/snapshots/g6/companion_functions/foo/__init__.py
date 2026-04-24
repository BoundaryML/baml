from __future__ import annotations

import typing
import pydantic

from baml.baml_core import define_function as __define_function


class Sentiment(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    label: str


ClassifySentiment       = __define_function("root.foo.ClassifySentiment", "sync",  ["input"])
ClassifySentiment_async = __define_function("root.foo.ClassifySentiment", "async", ["input"])
ClassifySentiment__build_request       = __define_function("root.foo.ClassifySentiment$build_request", "sync",  ["input"])
ClassifySentiment__build_request_async = __define_function("root.foo.ClassifySentiment$build_request", "async", ["input"])


__all__ = [
    "Sentiment",
    "ClassifySentiment",
    "ClassifySentiment_async",
    "ClassifySentiment__build_request",
    "ClassifySentiment__build_request_async",
]

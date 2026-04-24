from __future__ import annotations

import typing
import pydantic

from baml.baml_core import define_function as __define_function


ExtractResume       = __define_function("root.lorem.ExtractResume", "sync",  ["text"])
ExtractResume_async = __define_function("root.lorem.ExtractResume", "async", ["text"])
ExtractResume__parse_stream       = __define_function("root.lorem.ExtractResume$parse_stream", "sync",  ["sse"])
ExtractResume__parse_stream_async = __define_function("root.lorem.ExtractResume$parse_stream", "async", ["sse"])
ExtractResume__build_request       = __define_function("root.lorem.ExtractResume$build_request", "sync",  ["text"])
ExtractResume__build_request_async = __define_function("root.lorem.ExtractResume$build_request", "async", ["text"])
ExtractResume__parse       = __define_function("root.lorem.ExtractResume$parse", "sync",  ["json"])
ExtractResume__parse_async = __define_function("root.lorem.ExtractResume$parse", "async", ["json"])
ExtractResume__render_prompt       = __define_function("root.lorem.ExtractResume$render_prompt", "sync",  ["text"])
ExtractResume__render_prompt_async = __define_function("root.lorem.ExtractResume$render_prompt", "async", ["text"])


class Resume(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    email: typing.Union[str, None]


__all__ = [
    "ExtractResume",
    "ExtractResume_async",
    "ExtractResume__parse_stream",
    "ExtractResume__parse_stream_async",
    "ExtractResume__build_request",
    "ExtractResume__build_request_async",
    "ExtractResume__parse",
    "ExtractResume__parse_async",
    "ExtractResume__render_prompt",
    "ExtractResume__render_prompt_async",
    "Resume",
]

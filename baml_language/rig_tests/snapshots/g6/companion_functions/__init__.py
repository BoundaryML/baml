from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml, foo

import typing
import pydantic

from baml.baml_core import define_function as __define_function


class Resume(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str


ExtractResume       = __define_function("root.ExtractResume", "sync",  ["resume"])
ExtractResume_async = __define_function("root.ExtractResume", "async", ["resume"])
ExtractResume__build_request       = __define_function("root.ExtractResume$build_request", "sync",  ["resume"])
ExtractResume__build_request_async = __define_function("root.ExtractResume$build_request", "async", ["resume"])
ExtractResume__render_prompt       = __define_function("root.ExtractResume$render_prompt", "sync",  ["resume"])
ExtractResume__render_prompt_async = __define_function("root.ExtractResume$render_prompt", "async", ["resume"])
ExtractResume__parse       = __define_function("root.ExtractResume$parse", "sync",  ["json"])
ExtractResume__parse_async = __define_function("root.ExtractResume$parse", "async", ["json"])


__all__ = [
    "Resume",
    "ExtractResume",
    "ExtractResume_async",
    "ExtractResume__build_request",
    "ExtractResume__build_request_async",
    "ExtractResume__render_prompt",
    "ExtractResume__render_prompt_async",
    "ExtractResume__parse",
    "ExtractResume__parse_async",
]

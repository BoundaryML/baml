from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml, stream_types, test003

import typing
import pydantic

from baml.baml_core import define_function as __define_function


class Test001Basic(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    skills: typing.List[str]


class Inner001(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    value: str


class StreamDoneNested(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    done_class: Inner001
    done_list: typing.List[Inner001]
    done_map: typing.Dict[str, Inner001]
    regular: Inner001


ExtractTest001Basic       = __define_function("root.ExtractTest001Basic", "sync",  ["text"])
ExtractTest001Basic_async = __define_function("root.ExtractTest001Basic", "async", ["text"])
ExtractTest001Basic__render_prompt       = __define_function("root.ExtractTest001Basic$render_prompt", "sync",  ["text"])
ExtractTest001Basic__render_prompt_async = __define_function("root.ExtractTest001Basic$render_prompt", "async", ["text"])
ExtractTest001Basic__parse       = __define_function("root.ExtractTest001Basic$parse", "sync",  ["json"])
ExtractTest001Basic__parse_async = __define_function("root.ExtractTest001Basic$parse", "async", ["json"])
ExtractTest001Basic__parse_stream       = __define_function("root.ExtractTest001Basic$parse_stream", "sync",  ["sse"])
ExtractTest001Basic__parse_stream_async = __define_function("root.ExtractTest001Basic$parse_stream", "async", ["sse"])
ExtractTest001Basic__build_request       = __define_function("root.ExtractTest001Basic$build_request", "sync",  ["text"])
ExtractTest001Basic__build_request_async = __define_function("root.ExtractTest001Basic$build_request", "async", ["text"])


ExtractContact       = __define_function("root.ExtractContact", "sync",  ["text"])
ExtractContact_async = __define_function("root.ExtractContact", "async", ["text"])
ExtractContact__parse_stream       = __define_function("root.ExtractContact$parse_stream", "sync",  ["sse"])
ExtractContact__parse_stream_async = __define_function("root.ExtractContact$parse_stream", "async", ["sse"])
ExtractContact__build_request       = __define_function("root.ExtractContact$build_request", "sync",  ["text"])
ExtractContact__build_request_async = __define_function("root.ExtractContact$build_request", "async", ["text"])
ExtractContact__parse       = __define_function("root.ExtractContact$parse", "sync",  ["json"])
ExtractContact__parse_async = __define_function("root.ExtractContact$parse", "async", ["json"])
ExtractContact__render_prompt       = __define_function("root.ExtractContact$render_prompt", "sync",  ["text"])
ExtractContact__render_prompt_async = __define_function("root.ExtractContact$render_prompt", "async", ["text"])


class ContactInfo(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    email: str
    phone: typing.Union[str, None]


ExtractKeywords       = __define_function("root.ExtractKeywords", "sync",  ["text"])
ExtractKeywords_async = __define_function("root.ExtractKeywords", "async", ["text"])
ExtractKeywords__build_request       = __define_function("root.ExtractKeywords$build_request", "sync",  ["text"])
ExtractKeywords__build_request_async = __define_function("root.ExtractKeywords$build_request", "async", ["text"])
ExtractKeywords__parse_stream       = __define_function("root.ExtractKeywords$parse_stream", "sync",  ["sse"])
ExtractKeywords__parse_stream_async = __define_function("root.ExtractKeywords$parse_stream", "async", ["sse"])
ExtractKeywords__render_prompt       = __define_function("root.ExtractKeywords$render_prompt", "sync",  ["text"])
ExtractKeywords__render_prompt_async = __define_function("root.ExtractKeywords$render_prompt", "async", ["text"])
ExtractKeywords__parse       = __define_function("root.ExtractKeywords$parse", "sync",  ["json"])
ExtractKeywords__parse_async = __define_function("root.ExtractKeywords$parse", "async", ["json"])


CountWords       = __define_function("root.CountWords", "sync",  ["text"])
CountWords_async = __define_function("root.CountWords", "async", ["text"])
CountWords__render_prompt       = __define_function("root.CountWords$render_prompt", "sync",  ["text"])
CountWords__render_prompt_async = __define_function("root.CountWords$render_prompt", "async", ["text"])
CountWords__build_request       = __define_function("root.CountWords$build_request", "sync",  ["text"])
CountWords__build_request_async = __define_function("root.CountWords$build_request", "async", ["text"])
CountWords__parse_stream       = __define_function("root.CountWords$parse_stream", "sync",  ["sse"])
CountWords__parse_stream_async = __define_function("root.CountWords$parse_stream", "async", ["sse"])
CountWords__parse       = __define_function("root.CountWords$parse", "sync",  ["json"])
CountWords__parse_async = __define_function("root.CountWords$parse", "async", ["json"])


ExtractMultipleResumes       = __define_function("root.ExtractMultipleResumes", "sync",  ["text"])
ExtractMultipleResumes_async = __define_function("root.ExtractMultipleResumes", "async", ["text"])
ExtractMultipleResumes__build_request       = __define_function("root.ExtractMultipleResumes$build_request", "sync",  ["text"])
ExtractMultipleResumes__build_request_async = __define_function("root.ExtractMultipleResumes$build_request", "async", ["text"])
ExtractMultipleResumes__parse_stream       = __define_function("root.ExtractMultipleResumes$parse_stream", "sync",  ["sse"])
ExtractMultipleResumes__parse_stream_async = __define_function("root.ExtractMultipleResumes$parse_stream", "async", ["sse"])
ExtractMultipleResumes__parse       = __define_function("root.ExtractMultipleResumes$parse", "sync",  ["json"])
ExtractMultipleResumes__parse_async = __define_function("root.ExtractMultipleResumes$parse", "async", ["json"])
ExtractMultipleResumes__render_prompt       = __define_function("root.ExtractMultipleResumes$render_prompt", "sync",  ["text"])
ExtractMultipleResumes__render_prompt_async = __define_function("root.ExtractMultipleResumes$render_prompt", "async", ["text"])


Summarize       = __define_function("root.Summarize", "sync",  ["text"])
Summarize_async = __define_function("root.Summarize", "async", ["text"])
Summarize__parse_stream       = __define_function("root.Summarize$parse_stream", "sync",  ["sse"])
Summarize__parse_stream_async = __define_function("root.Summarize$parse_stream", "async", ["sse"])
Summarize__parse       = __define_function("root.Summarize$parse", "sync",  ["json"])
Summarize__parse_async = __define_function("root.Summarize$parse", "async", ["json"])
Summarize__build_request       = __define_function("root.Summarize$build_request", "sync",  ["text"])
Summarize__build_request_async = __define_function("root.Summarize$build_request", "async", ["text"])
Summarize__render_prompt       = __define_function("root.Summarize$render_prompt", "sync",  ["text"])
Summarize__render_prompt_async = __define_function("root.Summarize$render_prompt", "async", ["text"])


__all__ = [
    "Test001Basic",
    "Inner001",
    "StreamDoneNested",
    "ExtractTest001Basic",
    "ExtractTest001Basic_async",
    "ExtractTest001Basic__render_prompt",
    "ExtractTest001Basic__render_prompt_async",
    "ExtractTest001Basic__parse",
    "ExtractTest001Basic__parse_async",
    "ExtractTest001Basic__parse_stream",
    "ExtractTest001Basic__parse_stream_async",
    "ExtractTest001Basic__build_request",
    "ExtractTest001Basic__build_request_async",
    "ExtractContact",
    "ExtractContact_async",
    "ExtractContact__parse_stream",
    "ExtractContact__parse_stream_async",
    "ExtractContact__build_request",
    "ExtractContact__build_request_async",
    "ExtractContact__parse",
    "ExtractContact__parse_async",
    "ExtractContact__render_prompt",
    "ExtractContact__render_prompt_async",
    "ContactInfo",
    "ExtractKeywords",
    "ExtractKeywords_async",
    "ExtractKeywords__build_request",
    "ExtractKeywords__build_request_async",
    "ExtractKeywords__parse_stream",
    "ExtractKeywords__parse_stream_async",
    "ExtractKeywords__render_prompt",
    "ExtractKeywords__render_prompt_async",
    "ExtractKeywords__parse",
    "ExtractKeywords__parse_async",
    "CountWords",
    "CountWords_async",
    "CountWords__render_prompt",
    "CountWords__render_prompt_async",
    "CountWords__build_request",
    "CountWords__build_request_async",
    "CountWords__parse_stream",
    "CountWords__parse_stream_async",
    "CountWords__parse",
    "CountWords__parse_async",
    "ExtractMultipleResumes",
    "ExtractMultipleResumes_async",
    "ExtractMultipleResumes__build_request",
    "ExtractMultipleResumes__build_request_async",
    "ExtractMultipleResumes__parse_stream",
    "ExtractMultipleResumes__parse_stream_async",
    "ExtractMultipleResumes__parse",
    "ExtractMultipleResumes__parse_async",
    "ExtractMultipleResumes__render_prompt",
    "ExtractMultipleResumes__render_prompt_async",
    "Summarize",
    "Summarize_async",
    "Summarize__parse_stream",
    "Summarize__parse_stream_async",
    "Summarize__parse",
    "Summarize__parse_async",
    "Summarize__build_request",
    "Summarize__build_request_async",
    "Summarize__render_prompt",
    "Summarize__render_prompt_async",
]

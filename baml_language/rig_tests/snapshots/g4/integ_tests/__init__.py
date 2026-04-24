from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml, stream_types, test003

import typing
import pydantic


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


ExtractTest001Basic = None
ExtractTest001Basic_async = None
ExtractTest001Basic__render_prompt = None
ExtractTest001Basic__render_prompt_async = None
ExtractTest001Basic__parse = None
ExtractTest001Basic__parse_async = None
ExtractTest001Basic__parse_stream = None
ExtractTest001Basic__parse_stream_async = None
ExtractTest001Basic__build_request = None
ExtractTest001Basic__build_request_async = None


ExtractContact = None
ExtractContact_async = None
ExtractContact__parse_stream = None
ExtractContact__parse_stream_async = None
ExtractContact__build_request = None
ExtractContact__build_request_async = None
ExtractContact__parse = None
ExtractContact__parse_async = None
ExtractContact__render_prompt = None
ExtractContact__render_prompt_async = None


class ContactInfo(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    email: str
    phone: typing.Union[str, None]


ExtractKeywords = None
ExtractKeywords_async = None
ExtractKeywords__build_request = None
ExtractKeywords__build_request_async = None
ExtractKeywords__parse_stream = None
ExtractKeywords__parse_stream_async = None
ExtractKeywords__render_prompt = None
ExtractKeywords__render_prompt_async = None
ExtractKeywords__parse = None
ExtractKeywords__parse_async = None


CountWords = None
CountWords_async = None
CountWords__render_prompt = None
CountWords__render_prompt_async = None
CountWords__build_request = None
CountWords__build_request_async = None
CountWords__parse_stream = None
CountWords__parse_stream_async = None
CountWords__parse = None
CountWords__parse_async = None


ExtractMultipleResumes = None
ExtractMultipleResumes_async = None
ExtractMultipleResumes__build_request = None
ExtractMultipleResumes__build_request_async = None
ExtractMultipleResumes__parse_stream = None
ExtractMultipleResumes__parse_stream_async = None
ExtractMultipleResumes__parse = None
ExtractMultipleResumes__parse_async = None
ExtractMultipleResumes__render_prompt = None
ExtractMultipleResumes__render_prompt_async = None


Summarize = None
Summarize_async = None
Summarize__parse_stream = None
Summarize__parse_stream_async = None
Summarize__parse = None
Summarize__parse_async = None
Summarize__build_request = None
Summarize__build_request_async = None
Summarize__render_prompt = None
Summarize__render_prompt_async = None


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

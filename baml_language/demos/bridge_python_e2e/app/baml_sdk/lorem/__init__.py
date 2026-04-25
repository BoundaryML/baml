from __future__ import annotations

import enum
import typing
import pydantic

if typing.TYPE_CHECKING:
    from .. import baml

from baml.baml_core import (
    define_function as _define_function,
    define_instance_method as _define_instance_method,
)


ExtractResume       = _define_function("user.lorem.ExtractResume", "sync",  ["text"])
ExtractResume_async = _define_function("user.lorem.ExtractResume", "async", ["text"])
ExtractResume__parse_stream       = _define_function("user.lorem.ExtractResume$parse_stream", "sync",  ["sse"])
ExtractResume__parse_stream_async = _define_function("user.lorem.ExtractResume$parse_stream", "async", ["sse"])
ExtractResume__build_request       = _define_function("user.lorem.ExtractResume$build_request", "sync",  ["text"])
ExtractResume__build_request_async = _define_function("user.lorem.ExtractResume$build_request", "async", ["text"])
ExtractResume__parse       = _define_function("user.lorem.ExtractResume$parse", "sync",  ["json"])
ExtractResume__parse_async = _define_function("user.lorem.ExtractResume$parse", "async", ["json"])
ExtractResume__render_prompt       = _define_function("user.lorem.ExtractResume$render_prompt", "sync",  ["text"])
ExtractResume__render_prompt_async = _define_function("user.lorem.ExtractResume$render_prompt", "async", ["text"])


class Address(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    street: str
    city: str
    zip: typing.Union[str, None]


class Sentiment(str, enum.Enum):
    POSITIVE = "POSITIVE"
    NEGATIVE = "NEGATIVE"
    NEUTRAL = "NEUTRAL"


class PhoneNumber(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    country_code: int
    digits: str


class Resume(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    email: typing.Union[str, None]
    addresses: typing.List[Address]
    scores: typing.Dict[str, int]
    sentiment: Sentiment
    contact: PhoneNumber

    transform       = _define_instance_method("user.lorem.Resume.transform", "sync",  ["self"])
    transform_async = _define_instance_method("user.lorem.Resume.transform", "async", ["self"])


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
    "Address",
    "Sentiment",
    "PhoneNumber",
    "Resume",
]

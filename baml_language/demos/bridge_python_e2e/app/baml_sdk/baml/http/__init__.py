from __future__ import annotations

import typing
import pydantic

from baml.baml_core import define_instance_method as _define_instance_method


class Request(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    method: str
    url: str
    headers: typing.Dict[str, str]
    body: str


class Response(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    status_code: int
    headers: typing.Dict[str, str]
    url: str
    _body: None

    text       = _define_instance_method("baml.http.Response.text", "sync",  ["self"])
    text_async = _define_instance_method("baml.http.Response.text", "async", ["self"])

    bytes       = _define_instance_method("baml.http.Response.bytes", "sync",  ["self"])
    bytes_async = _define_instance_method("baml.http.Response.bytes", "async", ["self"])

    ok       = _define_instance_method("baml.http.Response.ok", "sync",  ["self"])
    ok_async = _define_instance_method("baml.http.Response.ok", "async", ["self"])


class SseStream(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    url: str
    _handle: None

    next       = _define_instance_method("baml.http.SseStream.next", "sync",  ["self"])
    next_async = _define_instance_method("baml.http.SseStream.next", "async", ["self"])

    close       = _define_instance_method("baml.http.SseStream.close", "sync",  ["self"])
    close_async = _define_instance_method("baml.http.SseStream.close", "async", ["self"])


__all__ = [
    "Request",
    "Response",
    "SseStream",
]

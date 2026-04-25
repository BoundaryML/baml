from __future__ import annotations

import enum
import typing
import pydantic

if typing.TYPE_CHECKING:
    from .. import baml


def ExtractResume(text: str) -> Resume: ...
async def ExtractResume_async(text: str) -> Resume: ...
def ExtractResume__parse_stream(sse: baml.http.SseStream) -> baml.llm.Stream: ...
async def ExtractResume__parse_stream_async(sse: baml.http.SseStream) -> baml.llm.Stream: ...
def ExtractResume__build_request(text: str) -> baml.http.Request: ...
async def ExtractResume__build_request_async(text: str) -> baml.http.Request: ...
def ExtractResume__parse(json: str) -> Resume: ...
async def ExtractResume__parse_async(json: str) -> Resume: ...
def ExtractResume__render_prompt(text: str) -> baml.llm.PromptAst: ...
async def ExtractResume__render_prompt_async(text: str) -> baml.llm.PromptAst: ...


class Address(pydantic.BaseModel): ...


class Sentiment(str, enum.Enum): ...


class PhoneNumber(pydantic.BaseModel): ...


class Resume(pydantic.BaseModel):
    def transform(self) -> Resume: ...
    async def transform_async(self) -> Resume: ...


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

from __future__ import annotations

import pydantic


class Request(pydantic.BaseModel): ...


class Response(pydantic.BaseModel): ...


class SseStream(pydantic.BaseModel): ...


__all__ = [
    "Request",
    "Response",
    "SseStream",
]

from __future__ import annotations

import typing
import pydantic


class Request(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    url: str


__all__ = [
    "Request",
]

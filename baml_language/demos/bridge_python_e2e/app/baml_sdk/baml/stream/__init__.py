from __future__ import annotations

import typing
import pydantic


class StreamFinished(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")


class StreamNoYield(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")


__all__ = [
    "StreamFinished",
    "StreamNoYield",
]

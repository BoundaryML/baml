from __future__ import annotations

import pydantic


class StreamFinished(pydantic.BaseModel): ...


class StreamNoYield(pydantic.BaseModel): ...


__all__ = [
    "StreamFinished",
    "StreamNoYield",
]

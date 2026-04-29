from __future__ import annotations

import typing
import pydantic


class File(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    _handle: None


__all__ = [
    "File",
]

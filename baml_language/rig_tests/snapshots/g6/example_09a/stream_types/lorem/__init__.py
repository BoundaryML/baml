from __future__ import annotations

import typing
import pydantic


class Resume(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: typing.Union[str, None]
    email: typing.Union[str, None]


__all__ = [
    "Resume",
]

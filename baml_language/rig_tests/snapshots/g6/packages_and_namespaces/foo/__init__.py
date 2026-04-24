from __future__ import annotations

import typing
import pydantic


class Sentiment(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    label: str


__all__ = [
    "Sentiment",
]

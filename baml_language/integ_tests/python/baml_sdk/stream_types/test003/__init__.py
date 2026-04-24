from __future__ import annotations

import typing
import pydantic


class Sentiment(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    label: typing.Union[typing.Literal["positive"], typing.Literal["negative"], typing.Literal["neutral"]]
    confidence: typing.Union[float, None]


__all__ = [
    "Sentiment",
]

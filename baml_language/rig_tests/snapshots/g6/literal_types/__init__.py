from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml

import typing
import pydantic


class Literals(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    priority_1: typing.Literal["1"]
    priority_2: typing.Literal["2"]
    priority_3: typing.Literal["3"]
    status_draft: typing.Literal["draft"]
    status_published: typing.Literal["published"]
    count: typing.Literal[42]
    enabled: typing.Literal[True]
    disabled: typing.Literal[False]


__all__ = [
    "Literals",
]

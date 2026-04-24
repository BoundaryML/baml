from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml, foo, vendor

import typing
import pydantic


class Resume(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str


__all__ = [
    "Resume",
]

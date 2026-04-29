from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml
from . import lorem

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

__all__ = ["lorem"]

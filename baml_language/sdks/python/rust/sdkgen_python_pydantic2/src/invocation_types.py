from __future__ import annotations

from typing import TYPE_CHECKING, TypedDict

if TYPE_CHECKING:
    from .vendor import trace
    from .baml.spawn import CancelToken


class BamlOptions(TypedDict, total=False):
    trace: trace.Options | trace.ReservedSpan | None
    cancel: CancelToken | None
    timeout_ms: int | None

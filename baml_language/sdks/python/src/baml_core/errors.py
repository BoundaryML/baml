# BAML Python error types.
#
# A thrown BAML value surfaces in Python as one of these *wrappers* carrying
# the decoded value via `.value` (a plain pydantic model / enum / alias,
# codegen'd by the normal rules) — see 31a-spec. The wrappers are raised by
# `decode_call_result` (proto.py) from the `BamlOutboundResult` envelope.
#
# They are deliberately plain Python classes: a BAML error type cannot itself
# subclass `BaseException` (it is a `pydantic.BaseModel`, and the two layouts
# conflict), and Python has no checked-exception typing, so the value rides
# inside a wrapper rather than being raised directly.

from __future__ import annotations

from typing import Any, List, Optional


def _format_message(class_name: Optional[str], value: Any) -> str:
    """A non-empty message for `str(e)` (the `@trace` / telemetry path records
    it). `class_name` is the thrown value's BAML FQN when known (e.g.
    `baml.json.JsonParseError`); `{value!r}` works for arbitrary user-thrown
    types that have no `message` field."""
    name = class_name or type(value).__name__
    return f"{name}: {value!r}"


class BamlError(Exception):
    """Raised when a BAML function surfaces a thrown error value.

    `.value` is the decoded thrown value; `.baml_trace` is the list of
    pre-rendered ``File "...", line N, in fn`` strings from the BAML stack
    (turned into a real Python traceback in 31g-phase6).
    """

    def __init__(
        self,
        value: Any,
        baml_trace: Optional[List[str]] = None,
        class_name: Optional[str] = None,
    ) -> None:
        self._value = value
        self._baml_trace: List[str] = list(baml_trace) if baml_trace else []
        super().__init__(_format_message(class_name, value))

    @property
    def value(self) -> Any:
        return self._value

    @property
    def baml_trace(self) -> List[str]:
        return self._baml_trace


class BamlPanic(BaseException):
    """Raised for a BAML panic (incl. cancellation).

    Subclasses `BaseException`, not `Exception` — like `asyncio.CancelledError`
    and `SystemExit` — so a bare `except Exception` does not swallow it.
    """

    def __init__(
        self,
        value: Any,
        baml_trace: Optional[List[str]] = None,
        class_name: Optional[str] = None,
    ) -> None:
        self._value = value
        self._baml_trace: List[str] = list(baml_trace) if baml_trace else []
        super().__init__(_format_message(class_name, value))

    @property
    def value(self) -> Any:
        return self._value

    @property
    def baml_trace(self) -> List[str]:
        return self._baml_trace


__all__ = [
    "BamlError",
    "BamlPanic",
]

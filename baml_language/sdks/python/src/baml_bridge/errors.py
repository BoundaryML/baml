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

import re
import sys
import types
from typing import Any, List, Optional, TypeVar

# Wire trace line shape, e.g. `File "resume.baml", line 12, in user.extract`.
_TRACE_LINE = re.compile(r'File "(?P<file>.*)", line (?P<line>\d+), in (?P<func>.*)')

_E = TypeVar("_E", bound=BaseException)


def _capture_frame(filename: str, lineno: int, func: str) -> Optional[types.FrameType]:
    """Build a real `frame` object whose displayed location is
    `(filename, lineno, func)` — the Jinja2 technique (31g-phase6).

    `compile`/`exec` a throwaway one-line `def` with the frame's `co_filename`
    (padded so `lineno` is valid), rename its `co_name` to `func`, then raise
    inside it to capture the frame off the traceback.
    """
    lineno = max(int(lineno), 1)
    src = ("\n" * (lineno - 1)) + "def __baml(): raise ValueError()\n"
    code = compile(src, filename, "exec")
    ns: dict = {}
    exec(code, ns)  # noqa: S102 — throwaway frame factory, not user input
    fn = ns["__baml"]
    try:
        fn.__code__ = fn.__code__.replace(co_name=func)
    except (ValueError, TypeError):
        pass  # exotic name; keep the default co_name rather than fail
    try:
        fn()
    except ValueError:
        tb = sys.exc_info()[2]
        if tb is not None and tb.tb_next is not None:
            return tb.tb_next.tb_frame
    return None


def _synthesize_traceback(lines: List[str]) -> Optional[types.TracebackType]:
    """Turn the pre-rendered BAML frame lines into a real `TracebackType`
    chain so they render as ordinary traceback lines (one continuous
    traceback ending in `.baml` source) rather than a detached blob.

    The wire is most-recent-call-last (oldest first); a Python tb is linked
    head=outermost with `tb_next` walking inward, and `TracebackType` is
    immutable (built tail-first), so we iterate `reversed(lines)` — wrapping
    the innermost frame first and ending with the outermost as the head.
    """
    tb_next: Optional[types.TracebackType] = None
    for line in reversed(lines):
        m = _TRACE_LINE.match(line.strip())
        if m is None:
            continue
        frame = _capture_frame(m.group("file"), int(m.group("line")), m.group("func"))
        if frame is None:
            continue
        tb_next = types.TracebackType(
            tb_next, frame, frame.f_lasti, max(int(m.group("line")), 1)
        )
    return tb_next


def attach_baml_traceback(exc: _E) -> _E:
    """Splice `exc.baml_trace` into `exc`'s Python traceback, if any. Best
    effort — on any failure the exception is returned untouched, so error
    delivery never depends on the cosmetic splice."""
    trace = getattr(exc, "baml_trace", None)
    if not trace:
        trace = getattr(getattr(exc, "reason", None), "baml_trace", None)
    if not trace:
        return exc
    try:
        synth = _synthesize_traceback(trace)
    except Exception:
        synth = None
    if synth is None:
        return exc
    return exc.with_traceback(synth)


def _format_message(class_name: Optional[str], value: Any) -> str:
    """A non-empty message for `str(e)` (the `@trace` / telemetry path records
    it). `class_name` is the thrown value's BAML FQN when known (e.g.
    `baml.json.ParseError`); `{value!r}` works for arbitrary user-thrown
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
        self._class_name = class_name
        super().__init__(_format_message(class_name, value))

    @property
    def value(self) -> Any:
        return self._value

    @property
    def baml_trace(self) -> List[str]:
        return self._baml_trace

    @property
    def class_name(self) -> Optional[str]:
        return self._class_name


class BamlCancelledError(BamlError):
    """Structured BAML cancellation reason carried by host cancellation."""


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
        self._class_name = class_name
        super().__init__(_format_message(class_name, value))

    @property
    def value(self) -> Any:
        return self._value

    @property
    def baml_trace(self) -> List[str]:
        return self._baml_trace

    @property
    def class_name(self) -> Optional[str]:
        return self._class_name


def make_sdk_panic(message: str) -> BamlPanic:
    """Build a `BamlPanic` wrapping a `baml.panics.SdkPanic` value.

    Used by the Rust pre-call *handle-returning* sites (`get_runtime` /
    `initialize_runtime`) — SDK-internal *setup* failures, which are
    panic-shaped, not recoverable `baml.errors.*` (32c). When the runtime
    isn't initialized the typemap may be unavailable, so we fall back to the
    plain string as `.value` rather than letting construction fail.
    """
    try:
        from .typemap import get_type_map  # local import: avoid circular load

        value: Any = get_type_map().get_class("baml.panics.SdkPanic")(message=message)
    except Exception:
        value = message
    return BamlPanic(value, class_name="baml.panics.SdkPanic")


__all__ = [
    "BamlError",
    "BamlCancelledError",
    "BamlPanic",
    "make_sdk_panic",
]

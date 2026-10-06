# BAML Python error types.
#
# A thrown BAML value surfaces in Python as one of these *wrappers* carrying
# the decoded value via `.value` (a plain pydantic model / enum / alias,
# codegen'd by the normal rules) — see 31a-spec. The wrappers are raised by
# `decode_call_result` (proto.py) from the `BamlOutboundResult` envelope.
# `BamlError` is the base of all of them.
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
    """Raised for every failure that comes from BAML.

    `except BamlError` catches a thrown error value, a panic (`BamlPanic`), a
    cancelled sync call (`BamlCancelledError`) and a rejected argument
    (`BamlTypeError`). Two exceptions of a call are not a `BamlError`: the
    exception that a Python callback raised, which comes back as the same
    object, and the `asyncio.CancelledError` of a cancelled async call.

    `.value` is the decoded thrown value; `.baml_trace` is the list of
    pre-rendered ``File "...", line N, in fn`` strings from the BAML stack
    (turned into a real Python traceback in 31g-phase6); `.class_name` is the
    name of the BAML class of the value, when it is an instance of a class.
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
        super().__init__(self._message())

    def _message(self) -> str:
        return _format_message(self._class_name, self._value)

    @property
    def value(self) -> Any:
        return self._value

    @property
    def baml_trace(self) -> List[str]:
        return self._baml_trace

    @property
    def class_name(self) -> Optional[str]:
        return self._class_name


class BamlPanic(BamlError):
    """Raised for a BAML panic (incl. cancellation): a failure that BAML code
    cannot `catch`, such as a failed assertion or `baml.sys.panic`.

    A `BamlError`, so the handler of a host for BAML failures also gets the
    panics. Catch `BamlPanic` first to treat them in another way.
    """


class BamlCancelledError(BamlPanic):
    """Structured BAML cancellation surfaced by sync calls and carried as the
    ``reason`` on native ``asyncio.CancelledError`` for async calls."""


class BamlTypeError(BamlError, TypeError):
    """Raised for a value that does not inhabit the type that BAML declares
    for it (`baml.errors.TypeMismatch`): an argument of another kind, a string
    that names no variant of an enum, a `TypeVar` that no argument binds.

    Also a `TypeError`, which is what Python raises for an argument of a wrong
    type. `str()` is the message alone.
    """

    def _message(self) -> str:
        message = getattr(self._value, "message", None)
        if message is None and isinstance(self._value, dict):
            message = self._value.get("message")
        return message if message is not None else str(self._value)


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
    "BamlTypeError",
    "make_sdk_panic",
]

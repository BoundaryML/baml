"""RED tests for the BamlError / BamlPanic delivery contract (31b-phase1).

Pins the target behavior from 31a-spec before any of it is implemented: a
thrown BAML value must surface in Python as a `BamlError` (or `BamlPanic`)
*wrapper* carrying the decoded value via `.value` — a plain pydantic model,
codegen'd by the normal rules — rather than as a stringified hardcoded pyo3
exception (`BamlClientError` / `BamlInvalidArgumentError` / `BamlCancelledError`),
which is how errors come back today.

These fail until phases 2-7 land:
  - `from baml_sdk.baml import BamlError, BamlPanic` does not resolve yet
    (the wrappers are added in 31f-phase5), so the whole module is red.
  - Once it resolves, each case below asserts the per-arm contract.

Exit (`baml.sys.exit`) can't be observed with `pytest.raises` because the
decode path calls `os._exit`, which is uncatchable. Per 31b the intent is to
assert the *process* exit code; we do that by running a standalone snippet in
a subprocess (a child `os._exit` can't take pytest down) and asserting its
return code — covering both a non-zero code and `exit(0)`, the case the
`is_exit_panic` discriminator exists to protect.
"""

from __future__ import annotations

import asyncio
import re
import subprocess
import sys
import textwrap
import traceback

import pytest

import baml_sdk  # noqa: F401  — importing initializes the BAML runtime
from baml_bridge import BamlCallContext, BamlCancelledError, call_function, get_runtime
from baml_sdk import hello_world
from baml_sdk.baml import BamlError, BamlPanic
from baml_sdk.baml.errors import InvalidArgument
from baml_sdk.baml.json import JsonParseError
from baml_sdk.baml.panics import UserPanic
from baml_sdk.raises_test import LoadDoc, ParseError, Reparse
from baml_sdk.throws_test import MyError, ParseJson, SleepMs, ThrowMyError

# stdlib native builtins (`baml.json.parse`, `baml.sys.*`) can't be called as
# top-level entry points, so the fixture wraps each in a bytecode function
# (ParseJson / DoPanic / DoExit / SleepMs) that the host calls.

_BAD_JSON = "{not valid json"


def test_errors_stdlib_error_surfaces_as_baml_error():
    """`baml.json.parse` on bad input → `BamlError` whose `.value` is a
    `JsonParseError` (a plain pydantic model). Proves stdlib error classes
    surface structured, independent of any `throws` clause."""
    with pytest.raises(BamlError) as exc_info:
        ParseJson(_BAD_JSON)
    assert isinstance(exc_info.value.value, JsonParseError)


def test_errors_user_throw_surfaces_declared_instance():
    """A user `throw` of a declared error → `BamlError` whose `.value` is
    the declared user error instance itself (not a `.code` sub-field)."""
    with pytest.raises(BamlError) as exc_info:
        ThrowMyError()
    assert isinstance(exc_info.value.value, MyError)


def test_errors_union_throws_preserves_class_name():
    """A throw escaping a *multi-member* `throws` union must carry the thrown
    value's class FQN in `class_name`, exactly like a single-member throws.

    Regression for the bridge-dogfood bug: the engine wraps a thrown value in
    `union_variant_value` for a multi-member `throws`, and the FQN reader only
    unwrapped a top-level `class_value` — so `class_name` came back `None` for
    union throws while the value still decoded fine. `Reparse` declares
    `throws ParseError` (single) and `LoadDoc` declares
    `throws ParseError | TimeoutError` (union); both throw `ParseError`, so
    their `class_name` must agree.
    """
    with pytest.raises(BamlError) as single:
        Reparse("x")
    with pytest.raises(BamlError) as union:
        LoadDoc("x")

    assert single.value.class_name == "user.raises_test.ParseError"
    assert union.value.class_name == single.value.class_name
    assert isinstance(union.value.value, ParseError)


def test_errors_host_invalid_argument_wraps_baml_errors_invalid_argument():
    """A host-side invalid argument (an extra kwarg the function doesn't
    declare) → `BamlError` wrapping `baml.errors.InvalidArgument`,
    synthesized host-side rather than thrown from the VM."""
    with pytest.raises(BamlError) as exc_info:
        hello_world(not_a_param=2)  # type: ignore[call-arg]
    assert isinstance(exc_info.value.value, InvalidArgument)


def test_errors_user_panic_surfaces_as_baml_panic():
    """A user-initiated panic via `baml.sys.panic` → `BamlPanic` whose
    `.value` is a `baml.panics.UserPanic` (routed by the namespace check,
    distinct from a host-synthesized `SdkPanic`)."""
    from baml_sdk.throws_test import DoPanic

    with pytest.raises(BamlPanic) as exc_info:
        DoPanic("user-initiated boom")
    assert isinstance(exc_info.value.value, UserPanic)


async def test_errors_cancellation_surfaces_as_baml_panic():
    """Async cancellation maps to `asyncio.CancelledError` with BAML reason."""
    rt = get_runtime()
    ctx = BamlCallContext()

    async def _abort_soon():
        await asyncio.sleep(0.1)
        ctx.abort()

    with pytest.raises(asyncio.CancelledError) as exc_info:
        await asyncio.gather(
            call_function(rt, "user.throws_test.SleepMs", {"ms": 2000}, _ctx=ctx),
            _abort_soon(),
        )
    assert isinstance(exc_info.value.reason, BamlCancelledError)


def test_errors_str_is_non_empty():
    """`str(e)` is non-empty — guards the `@trace` / telemetry path, which
    records `str(e)`."""
    with pytest.raises(BamlError) as exc_info:
        ParseJson(_BAD_JSON)
    assert str(exc_info.value)


# ---------------------------------------------------------------------------
# BAML traceback (31g-phase6). The thrown error carries the BAML stack as a
# list of pre-rendered `File "<src>", line N, in <fn>` strings on
# `.baml_trace`, which are also spliced into the exception's real Python
# traceback so `traceback.format_exception` renders the `.baml` source frame
# as an ordinary traceback line.
# ---------------------------------------------------------------------------

# `File "<src>", line N, in <fn>` — the wire trace-line shape.
_TRACE_LINE = r'File "(?P<file>[^"]*)", line (?P<line>\d+), in (?P<func>[^"]+)'


def _python_traceback_line(line: str) -> str:
    """Python tracebacks are 1-indexed, even for synthetic builtin frames."""
    m = re.fullmatch(_TRACE_LINE, line)
    if m is None:
        return line
    lineno = max(int(m["line"]), 1)
    return f'File "{m["file"]}", line {lineno}, in {m["func"]}'


def test_errors_baml_error_carries_baml_trace():
    """`.baml_trace` is the list of rendered BAML stack frames (one per
    frame), pointing into the throwing function's `.baml` source."""
    with pytest.raises(BamlError) as exc_info:
        ThrowMyError()
    trace = exc_info.value.baml_trace
    assert isinstance(trace, list) and trace, f"expected a non-empty list, got {trace!r}"
    # Most-recent-call-last: the throwing function is the last frame.
    m = re.fullmatch(_TRACE_LINE, trace[-1])
    assert m is not None, f"trace line not in `File ..., line N, in fn` form: {trace[-1]!r}"
    assert m["file"].endswith("types.baml"), m["file"]
    assert m["func"] == "user.throws_test.ThrowMyError", m["func"]
    assert int(m["line"]) >= 1


def test_errors_baml_trace_spliced_into_python_traceback():
    """The BAML frames are spliced into the exception's Python traceback, so
    `traceback.format_exception` renders the `.baml` source frame inline (not
    as a detached blob)."""
    try:
        ParseJson(_BAD_JSON)
    except BamlError as e:
        # Bind inside the handler — Python unbinds `e` after the except block.
        rendered = "".join(traceback.format_exception(type(e), e, e.__traceback__))
        wire_trace = list(e.baml_trace)
    else:
        pytest.fail("ParseJson did not raise BamlError")

    # Every wire trace line must appear in the rendered traceback. Builtin
    # frames may carry line 0 on the wire, but Python traceback objects render
    # locations as 1-indexed lines.
    for line in wire_trace:
        expected = _python_traceback_line(line)
        assert expected in rendered, f"{expected!r} not spliced into:\n{rendered}"
    # ...and the splice must name the throwing BAML function + its source.
    assert re.search(
        r'File "[^"]*types\.baml", line \d+, in user\.throws_test\.ParseJson',
        rendered,
    ), rendered


# ---------------------------------------------------------------------------
# Clean exit — subprocess, outside pytest's caught-exception machinery.
# `baml.sys.exit(code)` must terminate the process with `code` (via
# `os._exit`), NOT raise a catchable `BamlPanic`. Both a non-zero code and
# `exit(0)` are covered: zero is exactly what the `is_exit_panic` bool
# discriminator protects (proto3 can't tell exit-code-0 from "no exit").
# ---------------------------------------------------------------------------

_EXIT_SNIPPET = textwrap.dedent(
    """
    import baml_sdk  # initializes the runtime
    from baml_sdk.throws_test import DoExit
    DoExit({code})
    print("UNREACHABLE")  # os._exit must fire before this
    """
)


def _assert_clean_exit_terminates_process_with_code(code: int) -> None:
    result = subprocess.run(
        [sys.executable, "-c", _EXIT_SNIPPET.format(code=code)],
        capture_output=True,
    )
    assert result.returncode == code, (
        f"expected exit code {code}, got {result.returncode}; "
        f"stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert b"UNREACHABLE" not in result.stdout


def test_errors_clean_exit_terminates_process_with_code_0():
    _assert_clean_exit_terminates_process_with_code(0)


def test_errors_clean_exit_terminates_process_with_code_7():
    _assert_clean_exit_terminates_process_with_code(7)

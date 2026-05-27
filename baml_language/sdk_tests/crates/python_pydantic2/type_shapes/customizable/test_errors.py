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
import subprocess
import sys
import textwrap

import pytest

import baml_sdk  # noqa: F401  — importing initializes the BAML runtime
import baml_sdk.baml.json as baml_json
import baml_sdk.baml.sys as baml_sys
from baml_core import AbortController, call_function, get_runtime
from baml_sdk import MakeFoo
from baml_sdk.baml import BamlError, BamlPanic
from baml_sdk.baml.errors import InvalidArgument
from baml_sdk.baml.json import JsonParseError
from baml_sdk.throws import MyError, ThrowMyError

_BAD_JSON = "{not valid json"


def test_stdlib_error_surfaces_as_baml_error():
    """`baml.json.parse` on bad input → `BamlError` whose `.value` is a
    `JsonParseError` (a plain pydantic model). Proves stdlib error classes
    surface structured, independent of any `throws` clause."""
    with pytest.raises(BamlError) as exc_info:
        baml_json.parse(_BAD_JSON)
    assert isinstance(exc_info.value.value, JsonParseError)


def test_user_throw_surfaces_declared_instance():
    """A user `throw` of a declared error → `BamlError` whose `.value` is
    the declared user error instance itself (not a `.code` sub-field)."""
    with pytest.raises(BamlError) as exc_info:
        ThrowMyError()
    assert isinstance(exc_info.value.value, MyError)


def test_host_invalid_argument_wraps_baml_errors_invalid_argument():
    """A host-side invalid argument (an extra kwarg the function doesn't
    declare) → `BamlError` wrapping `baml.errors.InvalidArgument`,
    synthesized host-side rather than thrown from the VM."""
    with pytest.raises(BamlError) as exc_info:
        MakeFoo(v=1, not_a_param=2)  # type: ignore[call-arg]
    assert isinstance(exc_info.value.value, InvalidArgument)


def test_user_panic_surfaces_as_baml_panic():
    """A user-initiated panic via `baml.sys.panic` → `BamlPanic` (the panic
    enters the VM as a `baml.panics.*` value, routed by the namespace check)."""
    with pytest.raises(BamlPanic):
        baml_sys.panic("user-initiated boom")


async def test_cancellation_surfaces_as_baml_panic():
    """Cancelling an in-flight call → `BamlPanic` (cancellation is the
    `baml.panics.Cancelled` class tag; panics subclass `BaseException`)."""
    rt = get_runtime()
    controller = AbortController()

    async def _abort_soon():
        await asyncio.sleep(0.1)
        controller.abort()

    with pytest.raises(BamlPanic):
        await asyncio.gather(
            call_function(
                rt, "baml.sys.sleep", {"ms": 2000}, abort_controller=controller
            ),
            _abort_soon(),
        )


def test_str_is_non_empty():
    """`str(e)` is non-empty — guards the `@trace` / telemetry path, which
    records `str(e)`."""
    with pytest.raises(BamlError) as exc_info:
        baml_json.parse(_BAD_JSON)
    assert str(exc_info.value)


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
    import baml_sdk.baml.sys as baml_sys
    baml_sys.exit({code})
    print("UNREACHABLE")  # os._exit must fire before this
    """
)


@pytest.mark.parametrize("code", [0, 7])
def test_clean_exit_terminates_process_with_code(code):
    result = subprocess.run(
        [sys.executable, "-c", _EXIT_SNIPPET.format(code=code)],
        capture_output=True,
    )
    assert result.returncode == code, (
        f"expected exit code {code}, got {result.returncode}; "
        f"stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert b"UNREACHABLE" not in result.stdout

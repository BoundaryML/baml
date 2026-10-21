"""Smoke tests for plain (non-LLM) expression functions."""

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import (
    hello_world,
    int_plus_one,
    single_required_arg,
)


def test_main_hello_world_returns_literal():
    assert hello_world() == "hello world"


def test_main_single_required_arg_round_trips():
    # The next step up from the nullary case: one required positional
    # argument round-trips through the engine unchanged.
    assert single_required_arg("hi") == "hi"


# SDK_PARITY_LINT(skip): statically typed SDKs cannot pass a mistyped argument
def test_main_rejects_mistyped_int_argument():
    # A `str` for an `int` parameter is a catchable `TypeError` naming the
    # function, not a crash in the VM's integer arithmetic (#5332).
    assert int_plus_one(41) == 42
    with pytest.raises(TypeError, match="int_plus_one"):
        int_plus_one("bad")  # type: ignore[arg-type]

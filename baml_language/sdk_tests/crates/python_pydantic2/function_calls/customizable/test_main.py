"""Smoke tests for plain (non-LLM) expression functions."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import (
    hello_world,
    single_required_arg,
)


def test_main_hello_world_returns_literal():
    assert hello_world() == "hello world"


def test_main_single_required_arg_round_trips():
    # The next step up from the nullary case: one required positional
    # argument round-trips through the engine unchanged.
    assert single_required_arg("hi") == "hi"

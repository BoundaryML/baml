"""Smoke test for a minimal nullary expression function.

`main.baml` declares `hello_world() -> "hello world"`, whose body
returns the string literal. Calling the generated Python binding should
round-trip that literal back through the engine unchanged.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import hello_world, shout


def test_hello_world_returns_literal():
    assert hello_world() == "hello world"


def test_shout_round_trips_single_arg():
    # The next step up from the nullary case: one required positional
    # argument round-trips through the engine.
    assert shout("hi") == "hi!"

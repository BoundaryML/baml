"""Acceptance probe for the pending generated concrete facade/input roles.

Check against an SDK generated from sdk_tests/fixtures/interfaces. Unlike
runtime pass-back alone, this requires the factory, methods and interface
argument to agree in the native type system. Keep failures visible until
concrete codegen is complete; do not repair them with casts.
"""

from typing_extensions import assert_type

from baml_sdk import FriendlyGreeter, welcome_async


async def check_concrete_callers() -> None:
    greeter = await FriendlyGreeter.new_async(prefix="Hello")
    assert_type(greeter, FriendlyGreeter)
    assert_type(await welcome_async(greeter, "Ada"), str)
    assert_type(await greeter.greet("Ada"), str)
    assert_type(await greeter.label(), str)

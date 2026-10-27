"""Static contract for host callbacks: a plain function or an `async def`.

The bridge runs the coroutine of an `async def` callback to completion, so a
generated callback type accepts both. A callable that BAML returns is not a
host callback: a call returns the value.
"""

import typing

from typing_extensions import assert_type

from baml_sdk.host_callable_tests import (
    Person,
    call_callback_with_nullable_optional_states,
    call_void_callback,
    call_with_callback,
    call_with_callback_async,
    call_with_class_callback,
    make_adder,
)


def int_to_str(value: int) -> str:
    return str(value)


async def int_to_str_async(value: int) -> str:
    return str(value)


def person_name(person: Person) -> str:
    return person.name


async def person_name_async(person: Person) -> str:
    return person.name


def record(value: int) -> None:
    return None


async def record_async(value: int) -> None:
    return None


def add_optional(x: int, value: typing.Optional[int] = None) -> int:
    return x + (value or 0)


async def add_optional_async(x: int, value: typing.Optional[int] = None) -> int:
    return x + (value or 0)


def int_to_int(value: int) -> int:
    return value


async def int_to_int_async(value: int) -> int:
    return value


async def async_entry_points() -> None:
    assert_type(await call_with_callback_async(callback=int_to_str, x=1), str)
    assert_type(await call_with_callback_async(callback=int_to_str_async, x=1), str)


# A `(int) -> string` callback: both forms, and the result type does not change.
assert_type(call_with_callback(callback=int_to_str, x=1), str)
assert_type(call_with_callback(callback=int_to_str_async, x=1), str)

# A class argument and a void return.
call_with_class_callback(callback=person_name, p=Person(name="Ada", age=36))
call_with_class_callback(callback=person_name_async, p=Person(name="Ada", age=36))
call_void_callback(callback=record, value=1)
call_void_callback(callback=record_async, value=1)

# A callback with an optional parameter is a Protocol; it accepts both forms too.
call_callback_with_nullable_optional_states(callback=add_optional, x=1)
call_callback_with_nullable_optional_states(callback=add_optional_async, x=1)

# The return type is still checked, for both forms.
call_with_callback(callback=int_to_int, x=1)  # pyright: ignore[reportArgumentType]
call_with_callback(callback=int_to_int_async, x=1)  # pyright: ignore[reportArgumentType]

# A callable that BAML returns gives its value from a plain call.
assert_type(make_adder(offset=1)(2), int)

"""Common host values exercised through real generated functions and capture."""

import asyncio
from datetime import date, datetime, timezone

from pydantic import BaseModel, Field, field_serializer

from baml_bridge import shutdown_runtime
from baml_sdk import trace
from baml_sdk.methods_on_classes import Greeter
from baml_sdk.generic_tests import GenericBox
from baml_sdk import host_callable_tests as callbacks


class Model(BaseModel):
    number: int = Field(serialization_alias="value")
    when: datetime


class HostileModel(Model):
    def model_dump(self, *args, **kwargs):
        raise AssertionError("capture must not invoke model_dump")

    def __iter__(self):
        raise AssertionError("capture must not invoke model iteration")

    @field_serializer("number")
    def serialize_number(self, value):
        raise AssertionError("capture must not invoke field serializers")


def options(case):
    return trace.span(inputs=True, output=True, error=True).context(
        metadata={"probe": "common_types", "case": case}
    )


stamp = datetime(2026, 10, 6, 12, 30, 45, tzinfo=timezone.utc)
native = Greeter(name="hello")
model = Model(number=7, when=stamp)
values = [
    ("native", native),
    ("native_enum", callbacks.CallbackMood.HAPPY),
    ("generic_native", GenericBox[int](value=7)),
    ("pydantic", model),
    ("hostile_model", HostileModel(number=7, when=stamp)),
    ("date", date(2026, 10, 6)),
    ("datetime", stamp),
    ("naive_datetime", datetime(2026, 10, 6, 12, 30, 45)),
    ("exception_value", ValueError("problem")),
    ("nested", {"models": [native, model], "error": ValueError("problem")}),
    ("value_budget", [None] * 600),
    ("byte_budget", "x" * (64 * 1024 + 1)),
]
for case, value in values:

    @trace.instrument(options(case))
    def identity(value):
        return value

    assert identity(value) is value


@trace.instrument(options("raised"))
def fail():
    raise failure


failure = ValueError("problem")
try:
    fail()
except ValueError as caught:
    assert caught is failure
else:
    raise AssertionError("application failure disappeared")

assert native.who(_baml={"trace": options("baml_native")}) == "hello"
assert GenericBox[int](value=7).get(_baml={"trace": options("baml_generic")}) == "int"


@trace.instrument(options("callback_enum"))
def enum_callback(mood):
    return mood


assert (
    callbacks.call_enum_roundtrip_callback(
        enum_callback,
        callbacks.CallbackMood.HAPPY,
        _baml={"trace": options("baml_enum")},
    )
    is callbacks.CallbackMood.HAPPY
)


@trace.instrument(options("async_native"))
async def asynchronous(value):
    await asyncio.sleep(0)
    return value


assert asyncio.run(asynchronous(native)) is native


@trace.instrument(options("callback_native"))
def callback(person):
    return person


person = callbacks.Person(name="hello", age=7)
assert (
    callbacks.call_class_roundtrip_callback(
        callback, person, _baml={"trace": options("baml_callback")}
    )
    == person
)


class Custom:
    def __init__(self, value):
        self.value = value


class Derived(Custom):
    pass


class CustomFailure(ValueError):
    pass


@trace.capture_for(Custom)
def custom_capture(value):
    if value.value == "failure":
        raise ValueError("projection failure")
    if value.value == "cycle":
        return {"safe": 7, "cycle": value}
    return {"custom": value.value}


trace.register_capture(
    Greeter,
    lambda value: (_ for _ in ()).throw(
        AssertionError("native BAML capture must take precedence")
    ),
)
trace.register_capture(
    dict,
    lambda value: (_ for _ in ()).throw(AssertionError("builtin maps take precedence")),
)
trace.register_capture(CustomFailure, lambda value: {"code": 7})
for case, value in [
    ("custom", Custom(7)),
    ("custom_subclass", Derived(7)),
    ("custom_failure", Custom("failure")),
    ("custom_cycle", Custom("cycle")),
    ("native_precedence", native),
    ("builtin_precedence", {"safe": 7}),
]:

    @trace.instrument(options(case))
    def projected(value):
        return value

    assert projected(value) is value


@trace.instrument(options("custom_exception"))
def custom_fail():
    raise custom_failure


custom_failure = CustomFailure("private message")
try:
    custom_fail()
except CustomFailure as caught:
    assert caught is custom_failure
else:
    raise AssertionError("custom application failure disappeared")

try:
    trace.register_capture(Custom, asynchronous)
except TypeError:
    pass
else:
    raise AssertionError("async capture handlers must be rejected")

shutdown_runtime()

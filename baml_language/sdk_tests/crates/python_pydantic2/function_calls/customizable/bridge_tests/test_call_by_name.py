"""A call through the bridge alone: by the name of the function, with no
generated wrapper and, where a test asks for it, with no generated class.

A host that embeds BAML with `baml_bridge` only (no `baml generate`) calls
this way. The error that the function threw must reach it as it is.
"""

from __future__ import annotations

import pytest

import baml_sdk  # noqa: F401  — importing initializes the BAML runtime
from baml_bridge import BamlError, BamlPanic, call_function_sync, get_runtime
from baml_sdk import Person


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_call_by_name_returns_the_result():
    assert call_function_sync(get_runtime(), "user.round_trip_int", {"value": 7}).result() == 7


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_call_by_name_returns_a_generated_class():
    person = {"person": "p", "name": "Ada", "age": 36}
    result = call_function_sync(get_runtime(), "user.round_trip_person", {"person": person}).result()
    assert result == Person(person="p", name="Ada", age=36)


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_missing_argument_raises_invalid_argument():
    with pytest.raises(BamlError, match="Invalid argument: value") as exc_info:
        call_function_sync(get_runtime(), "user.round_trip_int", {})
    assert exc_info.value.class_name == "baml.errors.InvalidArgument"


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_function_not_found_is_a_panic():
    """The program has no such function: no BAML code ran that could catch it."""
    with pytest.raises(BamlPanic, match="no_such_function") as exc_info:
        call_function_sync(get_runtime(), "user.no_such_function", {})
    assert exc_info.value.class_name == "baml.panics.SdkPanic"


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_thrown_class_without_generated_class_is_its_fields(no_generated_classes):
    with pytest.raises(BamlError, match="boom") as exc_info:
        call_function_sync(get_runtime(), "user.throws_test.ThrowMyError", {})
    assert exc_info.value.class_name == "user.throws_test.MyError"
    assert exc_info.value.value == {"code": 42, "detail": "boom"}


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_stdlib_error_without_generated_class_is_its_fields(no_generated_classes):
    with pytest.raises(BamlError) as exc_info:
        call_function_sync(get_runtime(), "user.throws_test.ParseJson", {"s": "{"})
    assert exc_info.value.class_name == "baml.json.ParseError"
    assert isinstance(exc_info.value.value["message"], str)


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_missing_argument_without_generated_class_keeps_its_message(no_generated_classes):
    with pytest.raises(BamlError, match="Invalid argument: value") as exc_info:
        call_function_sync(get_runtime(), "user.round_trip_int", {})
    assert exc_info.value.class_name == "baml.errors.InvalidArgument"
    assert exc_info.value.value == {"message": "Invalid argument: value"}


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_panic_without_generated_class_is_its_fields(no_generated_classes):
    with pytest.raises(BamlPanic, match="stop") as exc_info:
        call_function_sync(get_runtime(), "user.throws_test.DoPanic", {"message": "stop"})
    assert exc_info.value.class_name == "baml.panics.UserPanic"
    assert exc_info.value.value["message"] == "stop"


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_function_not_found_without_generated_class_is_a_panic(no_generated_classes):
    with pytest.raises(BamlPanic, match="no_such_function") as exc_info:
        call_function_sync(get_runtime(), "user.no_such_function", {})
    assert exc_info.value.class_name == "baml.panics.SdkPanic"


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_argument_of_another_kind_without_generated_class_is_a_type_error(no_generated_classes):
    with pytest.raises(TypeError, match="argument 1: Value of type 'string' does not match the declared type `int`"):
        call_function_sync(get_runtime(), "user.round_trip_int", {"value": "7"})


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_callback_exception_without_generated_class_is_the_original(no_generated_classes):
    raised = ValueError("nope")

    def callback(_x: int) -> str:
        raise raised

    with pytest.raises(ValueError) as exc_info:
        call_function_sync(get_runtime(), "user.host_callable_tests.call_with_callback", {"callback": callback, "x": 1})
    assert exc_info.value is raised


# SDK_PARITY_LINT(skip): calls the Python bridge by function name, without the generated wrappers
def test_returned_class_without_generated_class_is_an_error(no_generated_classes):
    """A return value has a declared type that a typed caller relies on, so a
    class that codegen did not emit stays an error there."""
    person = {"person": "p", "name": "Ada", "age": 36}
    with pytest.raises(BamlError, match="Unknown class FQN 'user.Person'"):
        call_function_sync(get_runtime(), "user.round_trip_person", {"person": person})

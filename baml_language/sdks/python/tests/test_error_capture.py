import asyncio

import pytest
from pydantic import BaseModel, computed_field

from baml_bridge._host_capture import capture, register_capture
import json
from baml_bridge.errors import BamlError


def decode(observation):
    kind, *parts = observation
    if kind == "map":
        return {key: decode(item) for key, item in parts[0]}
    if kind == "list":
        return [decode(item) for item in parts[0]]
    if kind == "null":
        return None
    if kind in ("unavailable", "bytes", "depth", "values"):
        return {"unavailable": kind}
    return parts[0]


def recorded(value):
    return decode(json.loads(capture(value)))


def test_exception_fields_preserve_message_module_chains_and_attributes():
    class Failure(Exception):
        __slots__ = ("code",)

        def __str__(self):
            return "formatted message"

    cause = ValueError("cause")
    context = KeyError("context")
    error = Failure("argument")
    error.code = 42
    error.details = {"retry": False}
    error.__cause__ = cause
    error.__context__ = context
    try:
        raise error
    except Failure:
        captured = recorded(error)
    assert captured["type"].endswith("Failure")
    assert captured["module"] == __name__
    assert captured["message"] == "formatted message"
    assert captured["args"] == ["argument"]
    assert captured["cause"]["message"] == "cause"
    assert captured["context"]["message"] == "'context'"
    assert captured["suppress_context"] is True
    assert captured["attributes"] == {"details": {"retry": False}, "code": 42}
    assert (
        captured["traceback"][-1]["function"]
        == test_exception_fields_preserve_message_module_chains_and_attributes.__name__
    )


def test_broken_display_and_hostile_getters_do_not_erase_error_details():
    class Failure(Exception):
        def __str__(self):
            raise KeyboardInterrupt()

        def __getattribute__(self, name):
            raise AssertionError("attribute hook must not run")

        @property
        def __cause__(self):
            raise AssertionError("property must not run")

    error = Failure("still recorded")
    captured = recorded(error)
    assert captured["message"] == {"$opaque": "exception_message"}
    assert captured["args"] == ["still recorded"]
    assert captured["cause"] is None


def test_builtin_exception_attributes_and_group_members():
    error = FileNotFoundError(2, "missing", "example.txt")
    assert recorded(error)["attributes"]["filename"] == "example.txt"
    try:
        group_type = ExceptionGroup
    except NameError:
        pytest.skip("ExceptionGroup requires Python 3.11")
    group = group_type("failures", [error, ValueError("bad")])
    assert recorded(group)["exceptions"][1]["message"] == "bad"


def test_baml_error_retains_structured_value_without_model_serializers():
    class Detail(BaseModel):
        message: str

        @computed_field(repr=False)
        @property
        def dangerous(self) -> str:
            raise AssertionError("computed property must not run")

        def model_dump(self, *args, **kwargs):
            raise AssertionError("serializer must not run")

    value = Detail(message="bad response")
    error = BamlError(value, baml_trace=["frame"], class_name="ai.errors.ParseFailed")
    captured = recorded(error)
    assert captured["attributes"]["_value"] == {"message": "bad response"}
    assert captured["attributes"]["_class_name"] == "ai.errors.ParseFailed"
    assert captured["attributes"]["_baml_trace"] == ["frame"]


@pytest.mark.parametrize("asynchronous", [False, True])
def test_instrument_defers_error_capture_and_preserves_exception_identity(
    monkeypatch, asynchronous
):
    import baml_bridge._instrumentation as instrumentation

    class Failure(Exception):
        def __str__(self):
            raise AssertionError("formatting must wait for a capture request")

    error = Failure("failure")
    observed = []

    class Execution:
        def finish(self, outcome, value):
            observed.append((outcome, value))

    monkeypatch.setattr(
        instrumentation, "_validate_host_options", lambda options: (False, False, False)
    )
    monkeypatch.setattr(instrumentation, "_define_host_marker", lambda *args: object())
    monkeypatch.setattr(instrumentation, "_enter", lambda *args: (Execution(), None))

    def synchronous():
        raise error

    async def coroutine():
        raise error

    wrapped = instrumentation.instrument(coroutine if asynchronous else synchronous)
    try:
        if asynchronous:
            asyncio.run(wrapped())
        else:
            wrapped()
    except Failure as caught:
        assert caught is error
    else:
        pytest.fail("expected application failure")
    assert observed == [("error", error)]


def test_registered_projection_precedence_cycles_and_budgets():
    class Failure(Exception):
        pass

    register_capture(Failure, lambda error: {"custom": error.args[0]})
    assert recorded(Failure("value")) == {"custom": "value"}
    error = ValueError("cycle")
    error.__cause__ = error
    assert recorded(error)["cause"] == {"unavailable": "unavailable"}
    assert recorded(ValueError("x" * 65537))["message"] == {"unavailable": "bytes"}


def test_common_scalars_dataclasses_and_validation_errors():
    from dataclasses import dataclass
    from decimal import Decimal
    from enum import Enum
    from uuid import UUID
    from pydantic import ValidationError

    class Choice(Enum):
        First = "first"

    @dataclass
    class Data:
        number: int

    assert recorded(Decimal("1.230")) == "1.230"
    assert recorded(UUID(int=0)) == "00000000-0000-0000-0000-000000000000"
    assert recorded(Choice.First) == "first"
    assert recorded(Data(7)) == {"number": 7}
    assert recorded(b"abc") == {"encoding": "hex", "data": "616263"}
    assert sorted(recorded({1, 2})) == [1, 2]

    class Model(BaseModel):
        number: int

    try:
        Model(number="bad")
    except ValidationError as error:
        result = recorded(error)
    assert result["validation_errors"][0]["loc"] == ["number"]
    assert "input" not in result["validation_errors"][0]


def test_http_summaries_do_not_read_streams_or_record_credentials():
    httpx = pytest.importorskip("httpx")
    requests = pytest.importorskip("requests")
    url = "https://user:password@example.com/path?secret=value#fragment"
    request = requests.Request("GET", url).prepare()
    response = requests.Response()
    response.status_code = 403
    response.url = url
    response.request = request
    error = requests.HTTPError("failed", request=request, response=response)
    result = recorded(error)["attributes"]
    assert result["request"] == {"method": "GET", "url": "https://example.com/path"}
    assert result["response"]["status_code"] == 403

    class Stream(httpx.SyncByteStream):
        def __iter__(self):
            raise AssertionError("stream must not be consumed")

    request = httpx.Request("POST", url)
    response = httpx.Response(500, request=request, stream=Stream())
    error = httpx.HTTPStatusError("failed", request=request, response=response)
    result = recorded(error)["attributes"]
    assert result["_request"] == {"method": "POST", "url": "https://example.com/path"}
    assert result["response"]["status_code"] == 500

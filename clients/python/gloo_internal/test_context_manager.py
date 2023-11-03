import asyncio
import random
import pytest
import typing

from . import api_types
from .tracer import trace, update_trace_tags
from api import API

from mock import patch, AsyncMock


class APIWrapperLogMocker:
    def __init__(self, logs_list: typing.List[api_types.LogSchema]) -> None:
        self.logs_list = logs_list

    async def __aenter__(self) -> AsyncMock:
        self.patcher = patch.object(API, "log", new_callable=AsyncMock)
        self.mock_log = self.patcher.start()
        self.mock_log.side_effect = lambda payload: self.logs_list.append(payload)
        return self.mock_log

    async def __aexit__(
        self, exc_type: typing.Any, exc_val: typing.Any, exc_tb: typing.Any
    ) -> None:
        self.patcher.stop()


def validate_log(log: api_types.LogSchema, chain: typing.List[str]) -> None:
    assert log.event_type == "func_code"
    assert len(log.context.event_chain) == len(chain)
    for i, func_name in enumerate(chain):
        assert log.context.event_chain[i].function_name == func_name
        assert log.context.event_chain[i].variant_name is None


@pytest.mark.asyncio
async def test_single_function() -> None:
    # Add a mock for APIWrapper.log

    @trace()
    async def foo(x: int) -> int:
        await asyncio.sleep(0.1)
        return x

    logs: typing.List[api_types.LogSchema] = []
    async with APIWrapperLogMocker(logs) as mock_log:
        assert await foo(100) == 100
        mock_log.assert_called()

    assert len(logs) == 1
    log = logs[0]
    validate_log(log, ["foo"])


@pytest.mark.asyncio
async def test_chained() -> None:
    @trace()
    async def foo(x: int) -> int:
        await asyncio.sleep(0.1 + random.random() / 10)
        return x

    @trace()
    async def bar(x: typing.List[int]) -> int:
        res = await asyncio.gather(*map(foo, x))
        return sum(res)

    logs: typing.List[api_types.LogSchema] = []
    async with APIWrapperLogMocker(logs) as mock_log:
        assert await bar([100, 90, 80, 70, 60, 50, 40, 30, 20, 10]) == 550
        mock_log.assert_called()

    assert len(logs) == 11
    values = []
    for i in range(10):
        log = logs[i]
        validate_log(log, ["bar", "foo"])
        assert log.io.input is not None
        assert log.io.output is not None

        values.append(log.io.input.value)
        assert log.io.input.type.name == "int"

        assert log.io.output.value == values[-1]
        assert log.io.output.type.name == "int"

    assert set(values) == {100, 90, 80, 70, 60, 50, 40, 30, 20, 10}

    last_event = logs[-1]
    validate_log(last_event, ["bar"])
    assert last_event.io.input is not None
    assert last_event.io.output is not None

    assert set(last_event.io.input.value) == set(values)
    assert last_event.io.input.type.name == "list"
    assert last_event.io.output.value == 550


@pytest.mark.asyncio
async def test_chained_tags() -> None:
    @trace()
    async def foo(x: int) -> int:
        update_trace_tags(second=str(x))
        if x == 50:
            update_trace_tags(first="100")
        if x == 40:
            update_trace_tags(first=None)
        await asyncio.sleep(0.1 + random.random() / 10)
        return x

    @trace()
    async def bar(x: typing.List[int]) -> int:
        update_trace_tags(first=str(len(x)))
        res = await asyncio.gather(*map(foo, x))
        return sum(res)

    logs: typing.List[api_types.LogSchema] = []
    async with APIWrapperLogMocker(logs) as mock_log:
        assert await bar([100, 90, 80, 70, 60, 50, 40, 30, 20, 10]) == 550
        mock_log.assert_called()

    assert len(logs) == 11
    values = []
    for i in range(10):
        log = logs[i]
        validate_log(log, ["bar", "foo"])
        assert log.io.input is not None
        assert log.io.output is not None

        values.append(log.io.input.value)
        assert log.io.input.type.name == "int"

        assert log.io.output.value == values[-1]
        assert log.io.output.type.name == "int"

        if values[-1] == 50:
            assert log.context.tags == {"first": "100", "second": str(values[-1])}
        elif values[-1] == 40:
            assert log.context.tags == {"second": str(values[-1])}
        else:
            assert log.context.tags == {"first": "10", "second": str(values[-1])}

    assert set(values) == {100, 90, 80, 70, 60, 50, 40, 30, 20, 10}

    last_event = logs[-1]
    validate_log(last_event, ["bar"])
    assert last_event.io.input is not None
    assert last_event.io.output is not None

    assert set(last_event.io.input.value) == set(values)
    assert last_event.io.input.type.name == "list"
    assert last_event.io.output.value == 550
    assert last_event.context.tags == {"first": "10"}

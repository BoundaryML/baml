"""Unhandled spawn errors use real native owned delivery, including shutdown."""

import asyncio
import copy
import gc
import weakref

import pytest
import baml_bridge
from baml_bridge import proto
from baml_bridge.baml_py import (
    BamlRuntime,
    BamlEncodedResult,
    _live_handle_count,
    new_function_call,
    register_host_callable,
    lookup_host_value,
    shutdown_runtime,
)
from baml_bridge.cffi.v1 import baml_handle_pb2, baml_inbound_pb2, baml_outbound_pb2
from baml_bridge.errors import BamlError
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map

SOURCE = """
class Failure<T> { first: T, second: T }
function main<T>(value: T) -> int {
    spawn { throw value; };
    1
}
function boxed<T>(value: T) -> int {
    spawn { throw Failure<T> { first: value, second: value }; };
    1
}
interface ReportedCounter {
    function add(self, amount: int) -> int throws never
}
class Counter {
    count: int,
    implements ReportedCounter {
        function add(self, amount: int) -> int throws never {
            self.count += amount;
            self.count
        }
    }
}
function report_counter() -> int {
    let value: ReportedCounter = Counter { count: 10 };
    spawn { throw value; };
    1
}
"""


class Payload:
    pass


@pytest.fixture
def received(monkeypatch):
    results = []
    saved = get_type_map()
    # The installed default reporter calls this function. Retain its native
    # envelope instead of applying the application's process-exit policy.
    monkeypatch.setattr(baml_bridge, "decode_call_result", results.append)
    yield results
    for result in results:
        result._discard()
    shutdown_runtime()
    set_type_map(saved)
    gc.collect()


def spawn_error(received, name="main"):
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    original = Payload()
    weak = weakref.ref(original)
    request = baml_inbound_pb2.CallFunctionArgs.FromString(
        proto.encode_call_args(
            {},
            new_function_call(),
            function_name=name,
        )
    )
    arg = request.kwargs.add()
    arg.string_key = "value"
    arg.value.handle.key = register_host_callable(original)
    arg.value.handle.handle_type = baml_handle_pb2.HOST_VALUE_OPAQUE
    del original  # diagnostic tracebacks may retain this helper frame
    assert (
        proto.decode_call_result(
            runtime.call_function_sync(request.SerializeToString())
        )
        == 1
    )
    shutdown_runtime()
    assert len(received) == 1
    assert isinstance(received[0], BamlEncodedResult)
    return weak(), weak, received[0]


def test_unread_spawned_error_retains_until_discard(received):
    baseline = _live_handle_count()
    original, weak, result = spawn_error(received)
    del original
    gc.collect()
    assert weak() is not None
    assert _live_handle_count() == baseline + 1
    result._discard()
    gc.collect()
    assert weak() is None
    assert _live_handle_count() == baseline


def test_spawned_error_adopts_host_reference_before_raising(received):
    baseline = _live_handle_count()
    original, weak, result = spawn_error(received)
    with pytest.raises(BamlError) as raised:
        proto.decode_call_result(result)
    handle = raised.value.value
    cloned = copy.copy(handle)
    result._discard()
    assert lookup_host_value(cloned) is original
    del original, raised, handle
    gc.collect()
    assert weak() is not None
    del cloned
    gc.collect()
    assert weak() is None
    assert _live_handle_count() == baseline


def test_rejected_spawned_error_releases_escaped_provisional_children(
    received, monkeypatch
):
    baseline = _live_handle_count()
    original, weak, result = spawn_error(received, "boxed")
    escaped = []
    decode = proto._decode_handle

    def reject_second(handle, type_map):
        if escaped:
            raise ValueError("rejected error child")
        escaped.append(decode(handle, type_map))
        return escaped[-1]

    name = baml_outbound_pb2.BamlOutboundResult.FromString(
        result.payload
    ).error.value.class_value.name
    type_map = BamlTypeMap()
    type_map._class_cache[name] = dict
    set_type_map(type_map)
    monkeypatch.setattr(proto, "_decode_handle", reject_second)
    with pytest.raises(ValueError, match="rejected error child"):
        proto.decode_call_result(result)
    with pytest.raises(RuntimeError, match="not been adopted"):
        copy.copy(escaped[0])
    del original
    gc.collect()
    assert weak() is None
    assert _live_handle_count() == baseline


@pytest.mark.asyncio
@pytest.mark.parametrize("issuer", ["open", "shutdown", "replace"])
async def test_delivered_interface_keeps_original_invocation_authority(received, issuer):
    baseline = _live_handle_count()
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    assert proto.decode_call_result(
        runtime.call_function_sync(
            proto.encode_call_args({}, new_function_call(), function_name="report_counter")
        )
    ) == 1
    # A failed spawn is reported when its unobserved future is collected.
    # Exercise that path while the runtime is still open, rather than using
    # shutdown as the only way to force delivery.
    for _ in range(100):
        if received:
            break
        await asyncio.sleep(0.01)
        proto.decode_call_result(await runtime.call_function(proto.encode_call_args(
            {}, new_function_call(), function_name="baml.sys.collect_garbage",
        )))
    assert len(received) == 1
    if issuer == "shutdown":
        shutdown_runtime()
    elif issuer == "replace":
        BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})

    # Reporting/adoption must still work after shutdown. It must not reopen
    # the original runtime or attach the reference to a replacement runtime.
    with pytest.raises(BamlError) as raised:
        proto.decode_call_result(received[0])
    counter = raised.value.value
    received[0]._discard()
    if issuer == "open":
        assert await counter._invoke("add", {"amount": 2}) == 12
        assert await counter._invoke("add", {"amount": 3}) == 15
    else:
        with pytest.raises(BamlError, match="closed or replaced"):
            await counter._invoke("add", {"amount": 2})
    counter.close()
    assert _live_handle_count() == baseline


@pytest.mark.parametrize("adopt", [False, True])
def test_broken_reporter_discards_only_unadopted_ownership(
    received, monkeypatch, adopt
):
    import sys

    retained = []
    unraisable = []

    def decode_then_fail(result):
        received.append(result)
        if adopt:
            try:
                proto.decode_call_result(result)
            except BamlError as error:
                retained.append(error.value)
        raise RuntimeError("report delivery failed")

    def broken_reporter(_error):
        raise RuntimeError("reporter itself failed")

    monkeypatch.setattr(baml_bridge, "decode_call_result", decode_then_fail)
    monkeypatch.setattr(baml_bridge.traceback, "print_exception", broken_reporter)
    monkeypatch.setattr(sys, "unraisablehook", unraisable.append)
    original, weak, result = spawn_error(received)
    assert len(unraisable) == 1
    if adopt:
        assert lookup_host_value(copy.copy(retained[0])) is original
    del original
    # The diagnostic record retains the reporter's traceback and envelope.
    # It must not retain any unadopted native leases through that envelope.
    gc.collect()
    assert (weak() is not None) == adopt
    with pytest.raises(RuntimeError):
        result._adopt()
    retained.clear()
    unraisable.clear()
    gc.collect()
    assert weak() is None


def test_unhandled_spawn_error_uses_default_reporter():
    import subprocess
    import sys

    child = subprocess.run(
        [
            sys.executable,
            "-c",
            '''
from baml_bridge.baml_py import BamlRuntime, new_function_call, shutdown_runtime
from baml_bridge.proto import encode_call_args, decode_call_result
runtime = BamlRuntime.initialize_runtime(".", {"main.baml": """
    function main() -> int { spawn { throw "spawn failure"; }; 1 }
"""})
assert decode_call_result(runtime.call_function_sync(
    encode_call_args({}, new_function_call(), function_name="main")
)) == 1
shutdown_runtime()
''',
        ],
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert child.returncode == 1
    assert "BamlError" in child.stderr
    assert "spawn failure" in child.stderr

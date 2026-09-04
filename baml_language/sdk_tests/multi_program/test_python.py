"""Two separately generated SDKs, including nominal types with identical FQNs."""
import asyncio
import importlib.util
from pathlib import Path
import sys

import pytest
from baml_bridge import BamlRuntime, call_function_sync, decode_call_result, encode_call_args, new_function_call, release_function_call, cancel_function_call
from baml_bridge.errors import BamlPanic

def invoke(runtime, name):
    return decode_call_result(runtime.call_function_sync(encode_call_args({}, new_function_call(), function_name=name)))

ROOT = Path(__file__).parent


def load_sdk(name, program):
    spec = importlib.util.spec_from_file_location(name, ROOT / program / "python/baml_sdk/__init__.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def test_generated_programs_and_registration():
    a = load_sdk("sdk_a", "a")
    closure = a.closure()
    stream = a.stream()
    b = load_sdk("sdk_b", "b")
    assert a._RUNTIME.runtime_key > 2**53
    assert b._RUNTIME.runtime_key > 2**53
    assert a._RUNTIME.runtime_key != b._RUNTIME.runtime_key
    assert a.value() == 11
    assert b.value() == 22
    assert closure() == 11
    assert stream.next() == 11
    assert b.stream().final() == 22
    assert stream.final() == 11
    av, bv = a.result(), b.result()
    assert isinstance(av, a.Result) and isinstance(bv, b.Result)
    assert type(av) is not type(bv)
    assert av.read() == 11 and bv.read() == 22
    assert isinstance(a.roundtrip(av), a.Result)
    assert isinstance(call_function_sync(a._RUNTIME, "user.result", {}).result(), a.Result)
    assert isinstance(call_function_sync(b._RUNTIME, "user.result", {}).result(), b.Result)
    again = load_sdk("sdk_a_again", "a")
    assert again._RUNTIME.runtime_key == a._RUNTIME.runtime_key
    assert isinstance(again.result(), again.Result)
    assert isinstance(a.result(), a.Result)
    with pytest.raises(Exception, match="Conflicting BAML program"):
        BamlRuntime.initialize_runtime_from_bytecode(b._inlinedbaml.BYTECODE, None, a._RUNTIME.runtime_key)
    assert a.value() == 11 and b.value() == 22
    high = BamlRuntime.initialize_runtime_from_bytecode(a._inlinedbaml.BYTECODE, None, 2**64 - 1)
    assert high.runtime_key == 2**64 - 1
    assert invoke(high, "user.value") == 11

    async def concurrent():
        async def callback_a(value):
            assert isinstance(value, a.Result)
            await asyncio.sleep(0.01)
            return value
        async def callback_b(value):
            assert isinstance(value, b.Result)
            await asyncio.sleep(0)
            return value
        streams = await asyncio.gather(a.stream_async(), b.stream_async())
        assert await asyncio.gather(*(s.final_async() for s in streams)) == [11, 22]
        values = await asyncio.gather(a.result_async(), b.result_async(), a.callback_async(callback_a), b.callback_async(callback_b))
        assert [value.value for value in values] == [11, 22, 11, 22]
        assert all(isinstance(value, cls) for value, cls in zip(values, [a.Result, b.Result, a.Result, b.Result]))
    asyncio.run(concurrent())


def test_dynamic_instances_are_independent():
    a = BamlRuntime.initialize_runtime(".", {"main.baml": "function value() -> int { 33 }"})
    b = BamlRuntime.initialize_runtime(".", {"main.baml": "function value() -> int { 44 }"})
    same = BamlRuntime.initialize_runtime(".", {"main.baml": "function value() -> int { 33 }"})
    assert len({a.runtime_key, b.runtime_key, same.runtime_key}) == 3
    assert invoke(a, "value") == 33
    assert invoke(b, "value") == 44
    a.close()
    with pytest.raises(Exception):
        invoke(a, "value")
    assert invoke(b, "value") == 44
    b.close()
    same.close()


def test_inflight_close_and_cancellation_keep_origin():
    from baml_bridge import cancel_function_call
    source = {"main.baml": "function wait(cb: () -> int throws never) -> int { cb() }"}
    a = BamlRuntime.initialize_runtime(".", source)
    b = BamlRuntime.initialize_runtime(".", source)

    async def run():
        import threading
        started_a, started_b = threading.Event(), threading.Event()
        release_a, release_b = threading.Event(), threading.Event()
        def callback(started, release, value):
            started.set()
            assert release.wait(10)
            return value
        id_a, id_b = new_function_call(), new_function_call()
        call_a = a.call_function(encode_call_args({"cb": lambda: callback(started_a, release_a, 33)}, id_a, function_name="wait"))
        call_b = b.call_function(encode_call_args({"cb": lambda: callback(started_b, release_b, 44)}, id_b, function_name="wait"))
        assert await asyncio.to_thread(started_a.wait, 10)
        assert await asyncio.to_thread(started_b.wait, 10)
        a.close()
        assert cancel_function_call(id_b)
        release_a.set(); release_b.set()
        assert decode_call_result(await call_a) == 33
        with pytest.raises(Exception, match="[Cc]ancel"):
            decode_call_result(await call_b)
        b.close()
    asyncio.run(run())


def test_generated_source_registration_is_idempotent():
    key = 2**64 - 7
    source = {"main.baml": "function value() -> int { 66 }"}
    a = BamlRuntime.initialize_runtime(".", source, key)
    again = BamlRuntime.initialize_runtime(".", dict(source), key)
    assert a.runtime_key == again.runtime_key == key
    assert invoke(a, "value") == invoke(again, "value") == 66
    with pytest.raises(BamlPanic, match="Conflicting BAML program"):
        BamlRuntime.initialize_runtime(".", {"main.baml": "function value() -> int { 77 }"}, key)
    assert invoke(a, "value") == 66


def test_identical_dynamic_programs_have_independent_closure_state():
    from baml_bridge import call_function_sync
    source = {"main.baml": "function counter() -> () -> int throws never { let current = 0; () => { current += 1; current } }"}
    a = BamlRuntime.initialize_runtime(".", source)
    b = BamlRuntime.initialize_runtime(".", source)
    first = call_function_sync(a, "counter", {}).result()
    second = call_function_sync(b, "counter", {}).result()
    assert [first(), first(), second(), first(), second()] == [1, 2, 1, 3, 2]
    a.close(); b.close()


def test_generated_source_payload_imports():
    a = load_sdk("source_sdk_a", "source_a")
    b = load_sdk("source_sdk_b", "source_b")
    again = load_sdk("source_sdk_a_again", "source_a")
    assert a._RUNTIME.runtime_key == again._RUNTIME.runtime_key
    assert a._RUNTIME.runtime_key != b._RUNTIME.runtime_key
    assert invoke(a._RUNTIME, "user.value") == 11
    assert invoke(b._RUNTIME, "user.value") == 22
    assert invoke(again._RUNTIME, "user.value") == 11
    assert "main.baml" in a.get_baml_source_files()


def test_abandoned_and_failed_encoding_releases_call_ids():
    abandoned = new_function_call()
    release_function_call(abandoned)
    assert not cancel_function_call(abandoned)
    failed = new_function_call()
    with pytest.raises((TypeError, ValueError)):
        encode_call_args({"invalid": object()}, failed, function_name="user.value")
    assert not cancel_function_call(failed)

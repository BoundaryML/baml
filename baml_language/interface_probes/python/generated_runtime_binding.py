"""Run against freshly generated shared interface fixtures on PYTHONPATH.

Checks actual generated imports/factories, with the native bridge. This is a
Python runtime-binding probe, not an all-bridge or generated-concrete-API claim.
"""

from __future__ import annotations

import asyncio
import gc
import weakref

import baml_sdk
from baml_sdk import _inlinedbaml

from baml_bridge import define_function
from baml_bridge.baml_py import BamlRuntime, _pending_transfer_count, shutdown_runtime
from baml_bridge.errors import BamlError
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


async def main() -> None:
    sdk_map = get_type_map()
    counter = await baml_sdk.make_counter_async(10)
    unrelated = BamlTypeMap()
    set_type_map(unrelated)
    try:
        record = await baml_sdk.counter_record_async(counter)
        assert type(record) is baml_sdk.CounterRecord
        assert record.title == "counter"
        assert get_type_map() is unrelated
        record.counter.close()
        counter.close()

        # Even reinstalling the identical artifact creates another session;
        # old generated functions/factories must not silently retarget it.
        replacement = BamlRuntime.initialize_runtime_from_bytecode(
            _inlinedbaml.BYTECODE, _inlinedbaml.EMBEDDED_BAML_TOML
        )
        for call, kwargs in [
            (baml_sdk.make_greeter, {"prefix": "Hello"}),
            (baml_sdk.FriendlyGreeter.new, {"prefix": "Hello"}),
        ]:
            try:
                call(**kwargs)
            except BamlError as error:
                assert "closed or replaced" in str(error)
            else:
                raise AssertionError("generated sync call retargeted replacement")

        for call in [baml_sdk.make_greeter_async, baml_sdk.FriendlyGreeter.new_async]:
            try:
                await call(prefix="Hello")
            except BamlError as error:
                assert "closed or replaced" in str(error)
            else:
                raise AssertionError("generated async call retargeted replacement")

        class Callback:
            def __call__(self, value: str) -> str:
                raise AssertionError("rejected call invoked its callback")

        callback = Callback()
        weak = weakref.ref(callback)
        try:
            await baml_sdk.pass_callback_async(callback)
        except BamlError as error:
            assert "closed or replaced" in str(error)
        else:
            raise AssertionError("generated callback call retargeted replacement")
        del callback
        gc.collect()
        assert weak() is None
        assert _pending_transfer_count() == 0

        # Rejection is specific to the old binding, not a broken new engine.
        fresh = define_function(
            "user.make_greeter",
            "async",
            ["prefix"],
            runtime=replacement,
            type_map=sdk_map,
        )
        greeter = await fresh("Fresh")
        assert await greeter.greet("Ada") == "Fresh, Ada!"
        greeter.close()
        assert get_type_map() is unrelated
    finally:
        set_type_map(sdk_map)
        shutdown_runtime()


if __name__ == "__main__":
    asyncio.run(main())
    print("Generated SDK runtime-binding probe passed")

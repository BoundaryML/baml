"""Public invocation signatures, checked against the actual generated SDK."""

from typing_extensions import assert_type

from baml_sdk import BamlOptions, OptBox, hello_world_sync, optional_args_probe_sync, trace
from baml_sdk import host_callable_tests as baml
from baml_sdk.baml.spawn import CancelToken

token = CancelToken.new_sync()
options: BamlOptions = {"trace": trace.span_sync(), "cancel": token, "timeout_ms": 1000}
hello_world_sync(_baml=options)
hello_world_sync(_baml=None)
optional_args_probe_sync(1, opt1=None, _baml={})
box = OptBox.make_sync(1, _baml=options)
box.probe_sync(2, _baml=options)
add = baml.make_adder_sync(3, _baml=options)
assert_type(add.call_sync(4, _baml=options), int)
hello_world_sync(_baml={"unknown": True})  # type: ignore[typeddict-unknown-key]
hello_world_sync(_baml={"cancel": object()})  # type: ignore[typeddict-item]
hello_world_sync(_ctx=None)  # type: ignore[call-arg]
add(4, _types={"T": int})  # type: ignore[call-arg]


def ordinary_callback(x: int, y: int = 8, z: int = 9) -> int:
    return x + y + z


baml.call_callback_with_optional_args_all_unset_sync(ordinary_callback, 5, _baml=options)


async def async_surfaces() -> None:
    assert_type(await add(4, _baml=options), int)
    active = trace.current_cancel_token()
    if active is not None:
        assert_type(active, CancelToken)
        active.cancel_sync()

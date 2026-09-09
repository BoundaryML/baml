"""Native facade-selection probe against the generated shared interface fixture.

Run with that SDK on PYTHONPATH. The small facade classes below isolate checked
selection; they do not stand in for the still-pending generated method API.
"""

from __future__ import annotations

import copy
import gc
import hashlib
from unittest.mock import patch

from baml_sdk import _inlinedbaml, _typemap
from baml_bridge import proto
from baml_bridge._concrete import BamlConcreteRef
from baml_bridge.baml_py import (
    BamlRuntime,
    _live_handle_count,
    _pending_transfer_count,
    new_function_call,
    shutdown_runtime,
)
from baml_bridge.errors import BamlError
from baml_bridge.typemap import BamlTypeMap


class GreeterFacade(BamlConcreteRef):
    pass


class WrongFacade:
    pass


captured: list[BamlConcreteRef] = []


class BrokenFacade(BamlConcreteRef):
    @classmethod
    def _from_handle(cls, handle, type_map):
        ref = super()._from_handle(handle, type_map)
        captured.append(ref)
        try:
            copy.copy(ref)
        except RuntimeError as error:
            assert "not been adopted" in str(error)
        else:
            raise AssertionError("provisional facade was activated early")
        raise RuntimeError("failed facade construction")


def mapping(bundle_id: bytes, name: str = "GreeterFacade") -> BamlTypeMap:
    return BamlTypeMap.from_lazy_entries(
        classes={},
        enums={},
        type_aliases={},
        concrete_refs={"user.FriendlyGreeter": (__name__, name)},
        sdk_bundle_id=bundle_id,
    )


def main() -> None:
    bundle = _typemap._SDK_BUNDLE_ID
    assert (
        bundle
        == hashlib.sha256(b"baml.sdk.bundle.v1\0" + _inlinedbaml.BYTECODE).digest()
    )
    runtime = BamlRuntime.initialize_runtime_from_bytecode(
        _inlinedbaml.BYTECODE, _inlinedbaml.EMBEDDED_BAML_TOML
    )
    good = mapping(bundle)

    def result():
        args = proto.encode_call_args(
            {"prefix": "Hello"},
            new_function_call(),
            function_name="FriendlyGreeter.new",
        )
        return runtime.call_function_sync(args)

    gc.collect()
    baseline = _live_handle_count()
    receiver = proto.decode_call_result(result(), type_map=good)
    assert type(receiver) is GreeterFacade
    sibling = copy.copy(receiver)
    receiver.close()
    assert type(sibling) is GreeterFacade
    assert sibling.to_data() == {"prefix": "Hello"}
    sibling.close()
    assert _live_handle_count() == baseline

    # Mutating the diagnostic wire name cannot select a different declaration.
    decode_handle = proto._decode_handle

    def misleading_name(handle, type_map):
        if handle.handle_type == proto.baml_handle_pb2.CONCRETE_OBJECT:
            handle.ty.class_ty.name = "user.StoredCounter"
        return decode_handle(handle, type_map)

    with patch.object(proto, "_decode_handle", misleading_name):
        receiver = proto.decode_call_result(result(), type_map=good)
    assert type(receiver) is GreeterFacade
    receiver.close()

    for type_map, message in [
        (mapping(bytes(32)), "SDK bundle does not match"),
        (mapping(bundle, "WrongFacade"), "not an SDK reference class"),
        (mapping(bundle, "MissingFacade"), "Could not resolve concrete ref"),
        (mapping(bundle, "BrokenFacade"), "failed facade construction"),
    ]:
        try:
            proto.decode_call_result(result(), type_map=type_map)
        except (BamlError, RuntimeError) as error:
            assert message in str(error), str(error)
        else:
            raise AssertionError("invalid facade selection succeeded")
        gc.collect()
        assert _pending_transfer_count() == 0
        assert _live_handle_count() == baseline

    for ref in captured:
        try:
            ref._to_pyhandle()._sdk_concrete_name(bundle)
        except RuntimeError as error:
            assert "discarded" in str(error)
        else:
            raise AssertionError("discarded facade retained usable metadata access")
        ref.close()
    captured.clear()

    # No generated entry preserves the exact receiver with a generic wrapper.
    unknown = BamlTypeMap.from_lazy_entries({}, {}, {}, sdk_bundle_id=bundle)
    receiver = proto.decode_call_result(result(), type_map=unknown)
    assert type(receiver) is BamlConcreteRef
    receiver.close()

    # A name collision in a source-created engine supplies no bundle evidence.
    runtime = BamlRuntime.initialize_runtime(
        ".",
        {
            "main.baml": """
class FriendlyGreeter {
    prefix: string,
    function new(prefix: string) -> FriendlyGreeter throws never { FriendlyGreeter { prefix } }
    function local(self) -> string throws never { self.prefix }
}
"""
        },
    )
    receiver = proto.decode_call_result(result(), type_map=good)
    assert type(receiver) is BamlConcreteRef
    receiver.close()
    assert _pending_transfer_count() == 0
    shutdown_runtime()
    print("concrete facade identity: passed")


if __name__ == "__main__":
    main()

"""Real private native operations; generated user-facing binders come later."""

import gc
import weakref

import pytest

from baml_bridge import proto
from baml_bridge.baml_py import (
    BamlRuntime,
    _pending_transfer_count,
    new_function_call,
    register_host_callable,
    shutdown_runtime,
)
from baml_bridge.cffi.v1 import baml_handle_pb2 as tags
from baml_bridge.cffi.v1 import baml_inbound_pb2 as wire
from baml_bridge.errors import BamlPanic


SOURCE = """
interface Echo {
    type Item
    function echo(self, value: Self.Item) -> Self.Item throws never
    function label(self) -> string throws never { "default" }
}
interface OtherEcho {
    function echo(self, value: string) -> int throws never
}
interface EmptyWithItem {
    type Item
    function label(self) -> string throws never { "empty" }
}
"""


@pytest.fixture
def runtime():
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    yield runtime
    assert _pending_transfer_count() == 0
    shutdown_runtime()
    gc.collect()


def registration():
    request = wire.HostOperationRequest()
    request.register.name = "PythonEcho"
    impl = request.register.implementations.add()
    impl.interface_template.interface.name = "user.Echo"
    binding = impl.interface_template.interface.bindings.add(name="Item")
    binding.ty.type_var.index = 1
    binding.ty.type_var.name = "Item"
    impl.methods.append("echo")
    request.register.type_args.add().type_value.CopyFrom(
        proto.python_type_to_wire_ty(str)
    )
    return request


def adopted(result):
    """Test-only decoder retains all handles from the same aggregate receipt."""
    try:
        output = wire.HostOperationResult.FromString(result.payload)
        kind = output.WhichOneof("result")
        if kind == "registered":
            handles = [output.registered.adapter_type, output.registered.class_type]
            handles.extend(output.registered.interface_types)
        elif kind == "value":
            handles = [output.value.handle_value]
        else:
            raise AssertionError(output)
        owned = [result._wrap_handle(h.key, h.handle_type) for h in handles]
        result._adopt()
        return output, owned
    except BaseException:
        result._discard()
        raise


class PythonEcho:
    def __init__(self):
        self.seen = []
        self.broken = False

    def echo(self, value):
        self.seen.append(value)
        return 7 if self.broken else value


def create(adapter, receiver):
    request = wire.HostOperationRequest()
    request.create.adapter_type = adapter
    # Same registry owns both arbitrary Python receivers and callable bodies.
    request.create.receiver.handle.key = register_host_callable(receiver)
    request.create.receiver.handle.handle_type = tags.HOST_VALUE_OPAQUE
    callback = request.create.callbacks.add()
    callback.handle.key = register_host_callable(receiver.echo)
    callback.handle.handle_type = tags.HOST_VALUE_CALLABLE
    return request


@pytest.mark.asyncio
@pytest.mark.parametrize("mode", ["sync", "async"])
async def test_native_registration_projection_and_python_method(runtime, mode):
    async def execute(request):
        if mode == "sync":
            return runtime._host_operation_sync(request.SerializeToString())
        return await runtime._host_operation(request.SerializeToString())

    output, type_handles = adopted(await execute(registration()))
    registered = output.registered
    assert registered.adapter_type.handle_type == tags.HOST_ADAPTER_TYPE
    receiver = PythonEcho()
    instance, instance_handles = adopted(
        await execute(create(registered.adapter_type.key, receiver))
    )
    project = wire.HostOperationRequest()
    project.project.receiver = instance.value.handle_value.key
    project.project.interface_type.type_reference = registered.interface_types[0].key
    view, view_handles = adopted(await execute(project))
    assert view.value.handle_value.handle_type == tags.ADT_INTERFACE
    del type_handles, instance_handles
    gc.collect()
    for member, arguments, expected in [
        ("echo", {"value": "Ada"}, "Ada"),
        ("label", {}, "default"),
    ]:
        request = wire.CallFunctionArgs.FromString(
            proto.encode_call_args(arguments, new_function_call())
        )
        request.interface_method.view = view.value.handle_value.key
        request.interface_method.member = member
        assert (
            proto.decode_call_result(
                await runtime.call_function(request.SerializeToString())
            )
            == expected
        )
    assert receiver.seen == ["Ada"]
    # Native state belongs to Python. A later invalid completion is still
    # checked against the retained Item=string contract on every crossing.
    receiver.broken = True
    request = wire.CallFunctionArgs.FromString(
        proto.encode_call_args({"value": "Ada"}, new_function_call())
    )
    request.interface_method.view = view.value.handle_value.key
    request.interface_method.member = "echo"
    with pytest.raises(BamlPanic, match="HostContractViolation"):
        proto.decode_call_result(
            await runtime.call_function(request.SerializeToString())
        )
    del view_handles


def test_unobserved_registration_discards_provisional_handles(runtime):
    result = runtime._host_operation_sync(registration().SerializeToString())
    assert _pending_transfer_count() == 1
    message = wire.HostOperationResult.FromString(result.payload)
    handle = result._wrap_handle(
        message.registered.adapter_type.key, tags.HOST_ADAPTER_TYPE
    )
    result._discard()
    with pytest.raises(RuntimeError, match="discarded"):
        handle._clone_key_for_wire()
    assert _pending_transfer_count() == 0


@pytest.mark.parametrize("replaced", [False, True])
def test_rejected_creation_releases_python_receiver(runtime, replaced):
    output, handles = adopted(
        runtime._host_operation_sync(registration().SerializeToString())
    )
    receiver = PythonEcho()
    weak = weakref.ref(receiver)
    request = create(output.registered.adapter_type.key, receiver)
    if replaced:
        BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    else:
        # Wrong callback count; the engine must reject the entire aggregate.
        request.create.callbacks.add().CopyFrom(request.create.callbacks[0])
    result = runtime._host_operation_sync(request.SerializeToString())
    failure = wire.HostOperationResult.FromString(result.payload).failure
    assert failure.WhichOneof("result") == "error"
    result._adopt()
    del receiver
    gc.collect()
    assert weak() is None
    del handles


def test_failed_async_scheduling_releases_prepared_receiver(runtime):
    output, handles = adopted(
        runtime._host_operation_sync(registration().SerializeToString())
    )
    receiver = PythonEcho()
    weak = weakref.ref(receiver)
    request = create(output.registered.adapter_type.key, receiver)
    with pytest.raises(RuntimeError, match="no running event loop"):
        runtime._host_operation(request.SerializeToString())
    del receiver
    gc.collect()
    assert weak() is None
    del handles


@pytest.mark.asyncio
async def test_binding_helper_uses_checked_indices_for_same_named_methods(runtime):
    from baml_bridge._host import HostInterface, HostMethod, bind_host
    from baml_bridge.cffi.v1 import baml_type_pb2
    from baml_bridge.typemap import BamlTypeMap

    class Receiver:
        def echo_text(self, value):
            return value

        def echo_length(self, value):
            return len(value)

    echo = registration().register
    other = baml_type_pb2.BamlTy()
    other.interface.name = "user.OtherEcho"
    interfaces = (
        HostInterface(
            echo.implementations[0].interface_template.SerializeToString(),
            (HostMethod("echo", "echo_text", True, 1),),
        ),
        HostInterface(
            other.SerializeToString(), (HostMethod("echo", "echo_length", True, 1),)
        ),
    )
    for index, expected in [(0, "Ada"), (1, 3)]:
        view = await bind_host(
            Receiver(),
            interfaces,
            runtime=runtime,
            type_map=BamlTypeMap(),
            type_arguments=echo.type_args,
            interface_index=index,
        )
        assert await view._invoke("echo", {"value": "Ada"}) == expected
        view.close()


@pytest.mark.asyncio
async def test_binding_all_default_interface_preserves_fresh_associated_identity(
    runtime,
):
    from baml_bridge._host import HostInterface, HostMethod, bind_host
    from baml_bridge.cffi.v1 import baml_type_pb2
    from baml_bridge.typemap import BamlTypeMap

    ty = baml_type_pb2.BamlTy()
    ty.interface.name = "user.EmptyWithItem"
    ty.interface.bindings.add(name="Item").ty.type_var.index = 1
    evidence = wire.BamlTyArg()
    evidence.type_definition.root.class_ty.name = "FreshItem"
    evidence.type_definition.classes.add(name="FreshItem")
    view = await bind_host(
        object(),
        (
            HostInterface(
                ty.SerializeToString(), (HostMethod("label", "label", False, 0),)
            ),
        ),
        runtime=runtime,
        type_map=BamlTypeMap(),
        type_arguments=[evidence],
    )
    assert await view._invoke("label", {}) == "empty"
    view.close()


@pytest.mark.asyncio
async def test_binding_checks_methods_without_executing_properties_or_bodies(runtime):
    from baml_bridge._host import HostInterface, HostMethod, bind_host
    from baml_bridge.typemap import BamlTypeMap

    calls = []

    class Property:
        @property
        def echo(self):
            calls.append("property")
            return lambda value: value

    class WrongConvention:
        def echo(self, *, value):
            calls.append("body")
            return value

    echo = registration().register
    specs = (
        HostInterface(
            echo.implementations[0].interface_template.SerializeToString(),
            (HostMethod("echo", "echo", True, 1),),
        ),
    )
    for value in (object(), Property(), WrongConvention()):
        with pytest.raises(TypeError):
            await bind_host(
                value,
                specs,
                runtime=runtime,
                type_map=BamlTypeMap(),
                type_arguments=echo.type_args,
            )
    assert calls == []

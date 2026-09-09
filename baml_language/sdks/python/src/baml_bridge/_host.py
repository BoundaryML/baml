"""Private registration machinery for generated interface implementation binders.

The compiler supplies operation metadata; native objects supply method bodies.
This module never infers BAML membership from Python method names.
"""

from __future__ import annotations

from dataclasses import dataclass
import inspect
from typing import Any, Callable, Sequence, TypeVar

from . import proto
from ._interface import BamlInterfaceRef
from .baml_py import BamlEncodedResult, BamlPyHandle, BamlRuntime
from .baml_py import register_host_callable, release_host_callable
from .cffi.v1 import baml_handle_pb2 as tags
from .cffi.v1 import baml_inbound_pb2 as wire
from .cffi.v1 import baml_type_pb2
from .typemap import BamlTypeMap, _using_type_map


@dataclass(frozen=True)
class HostMethod:
    name: str
    attribute: str
    required: bool
    # Required args are positional. Optional args retain their authored wire
    # names and are renamed only when entering the native body.
    required_count: int
    optional_names: tuple[tuple[str, str], ...] = ()


@dataclass(frozen=True)
class HostInterface:
    # Serialized BamlTy template, with the registration's explicit type frame.
    template: bytes
    methods: tuple[HostMethod, ...]


@dataclass
class _Registration:
    adapter: tuple[int, BamlPyHandle]
    interfaces: list[tuple[int, BamlPyHandle]]
    callbacks: tuple[tuple[int, str], ...]


_Result = TypeVar("_Result")


def _receive(
    result: BamlEncodedResult,
    type_map: BamlTypeMap,
    decode: Callable[[wire.HostOperationResult, BamlEncodedResult], _Result],
) -> _Result:
    token = proto._active_result.set(result)
    failed = False
    try:
        envelope = wire.HostOperationResult.FromString(result.payload)
        if envelope.WhichOneof("result") == "failure":
            failed, value = proto._decode_call_outcome(
                envelope.failure.SerializeToString(), type_map=type_map
            )
            if not failed:
                raise ValueError(
                    "host operation returned a successful failure envelope"
                )
        else:
            value = decode(envelope, result)
        result._adopt()
    except BaseException:
        result._discard()
        raise
    finally:
        proto._active_result.reset(token)
    if failed:
        raise value
    return value


def _registration(envelope, result) -> _Registration:
    if envelope.WhichOneof("result") != "registered":
        raise ValueError("host operation did not return registration metadata")
    value = envelope.registered
    if (
        not value.HasField("adapter_type")
        or value.adapter_type.handle_type != tags.HOST_ADAPTER_TYPE
    ):
        raise ValueError("host registration has no adapter capability")
    if not value.interface_types:
        raise ValueError("host registration has no checked interface types")
    callbacks = tuple(
        (slot.implementation_index, slot.method) for slot in value.callbacks
    )
    if any(index >= len(value.interface_types) for index, _ in callbacks):
        raise ValueError("host callback selects an unknown implementation")

    def owned(handle):
        return handle.key, result._wrap_handle(handle.key, handle.handle_type)

    # The class type is unnecessary for binding and is intentionally unclaimed.
    # Its provisional table lease is released by this aggregate's adoption.
    return _Registration(
        owned(value.adapter_type),
        [owned(ty) for ty in value.interface_types],
        callbacks,
    )


def _instance(envelope, result) -> tuple[int, BamlPyHandle]:
    if (
        envelope.WhichOneof("result") != "value"
        or envelope.value.WhichOneof("value") != "handle_value"
    ):
        raise ValueError("host creation did not return an instance handle")
    handle = envelope.value.handle_value
    if handle.handle_type != tags.UNTAGGED_BEX_HEAP:
        raise ValueError("host creation returned the wrong handle kind")
    return handle.key, result._wrap_handle(handle.key, handle.handle_type)


def _method(receiver: object, spec: HostMethod) -> Callable[..., Any] | None:
    # Properties are application data/accessors, not method implementations.
    missing = object()
    declared = inspect.getattr_static(receiver, spec.attribute, missing)
    if declared is missing:
        if spec.required:
            raise TypeError(f"host implementation is missing method {spec.attribute!r}")
        return None
    if isinstance(declared, property):
        raise TypeError(f"host method {spec.attribute!r} cannot be a property")
    method = getattr(receiver, spec.attribute)
    if not callable(method):
        raise TypeError(f"host member {spec.attribute!r} is not callable")
    names = dict(spec.optional_names)
    try:
        signature = inspect.signature(method)
    except (TypeError, ValueError):
        # Some native extension methods have no inspectable signature. The
        # generated host contract and actual crossing checks still apply.
        signature = None
    if signature is not None:
        positional = [object()] * spec.required_count
        try:
            signature.bind(*positional)
            signature.bind(*positional, **{name: object() for name in names.values()})
        except TypeError as error:
            raise TypeError(
                f"host method {spec.attribute!r} has an incompatible calling convention: {error}"
            ) from error

    def dispatch(*args: Any, **kwargs: Any) -> Any:
        return method(*args, **{names[name]: value for name, value in kwargs.items()})

    return dispatch


async def bind_host(
    implementation: object,
    interfaces: Sequence[HostInterface],
    *,
    runtime: BamlRuntime,
    type_map: BamlTypeMap,
    type_arguments: Sequence[wire.BamlTyArg] = (),
    interface_index: int = 0,
) -> BamlInterfaceRef:
    """Register explicitly selected obligations and return an exact owned view.

    Generated binders supply the schema, SDK runtime/map and typed arguments.
    No global cache owns the receiver; no method body is run during binding.
    """
    if not 0 <= interface_index < len(interfaces):
        raise ValueError("binding requires a selected interface")
    request = wire.HostOperationRequest()
    request.register.name = type(implementation).__qualname__
    request.register.type_args.extend(type_arguments)
    callbacks: dict[tuple[int, str], Callable[..., Any]] = {}
    for index, interface in enumerate(interfaces):
        target = request.register.implementations.add()
        target.interface_template.CopyFrom(
            baml_type_pb2.BamlTy.FromString(interface.template)
        )
        for method in interface.methods:
            callback = _method(implementation, method)
            if callback is not None:
                key = (index, method.name)
                if key in callbacks:
                    raise ValueError("duplicate host method in generated metadata")
                callbacks[key] = callback
                target.methods.append(method.name)
    registration = _receive(
        await runtime._host_operation(request.SerializeToString()),
        type_map,
        _registration,
    )
    # Resolve slots before acquiring any host registry leases. Even obligations
    # sharing a method name have distinct, checked callback indices.
    ordered = [callbacks[key] for key in registration.callbacks]
    create = wire.HostOperationRequest()
    create.create.adapter_type = registration.adapter[0]
    acquired: list[int] = []
    try:
        with _using_type_map(type_map):
            receiver_key = register_host_callable(implementation)
            acquired.append(receiver_key)
            create.create.receiver.handle.key = receiver_key
            create.create.receiver.handle.handle_type = tags.HOST_VALUE_OPAQUE
            for callback in ordered:
                key = register_host_callable(callback)
                acquired.append(key)
                value = create.create.callbacks.add()
                value.handle.key = key
                value.handle.handle_type = tags.HOST_VALUE_CALLABLE
        encoded = create.SerializeToString()
        try:
            pending = runtime._host_operation(encoded)
        finally:
            # Synchronous native preparation consumes this complete valid
            # aggregate, including admission/scheduling failure.
            acquired.clear()
    finally:
        for key in acquired:
            release_host_callable(key)
    instance = _receive(await pending, type_map, _instance)
    project = wire.HostOperationRequest()
    project.project.receiver = instance[0]
    project.project.interface_type.type_reference = registration.interfaces[
        interface_index
    ][0]

    def decode_view(envelope, _result):
        if envelope.WhichOneof("result") != "value":
            raise ValueError("host projection did not return a value")
        value = proto.decode_value(envelope.value, type_map)
        if not isinstance(value, BamlInterfaceRef):
            raise ValueError("host projection did not return an interface reference")
        return value

    return _receive(
        await runtime._host_operation(project.SerializeToString()),
        type_map,
        decode_view,
    )

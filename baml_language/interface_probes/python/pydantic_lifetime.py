"""Pydantic integration and ownership models; not the production interface ABI.

Run using sdks/python/.venv/bin/python interface_probes/python/pydantic_lifetime.py.
"""

from __future__ import annotations

import asyncio
import gc
import weakref
from abc import ABC, abstractmethod
from typing import ClassVar

import pydantic
from pydantic import BaseModel
from pydantic_core import PydanticSerializationError, core_schema


def reject_interface_json(value: object) -> object:
    raise ValueError("A live interface is not portable configuration")


class GreeterInput:
    @classmethod
    def __get_pydantic_core_schema__(cls, source_type, handler):
        return core_schema.json_or_python_schema(
            json_schema=core_schema.no_info_plain_validator_function(reject_interface_json),
            python_schema=core_schema.is_instance_schema(cls),
            serialization=core_schema.plain_serializer_function_ser_schema(
                reject_interface_json, when_used="json"
            ),
        )


class GreeterImplementation(GreeterInput, ABC):
    # Generated opt-in metadata, not an engine-validated implementation witness.
    baml_contract: ClassVar[str] = "Greeter"

    @abstractmethod
    async def greet(self, name: str) -> str: ...


class PythonGreeter(BaseModel, GreeterImplementation):
    prefix: str

    async def greet(self, name: str) -> str:
        return f"{self.prefix}, {name}!"


class IncompleteGreeter(BaseModel, GreeterImplementation):
    prefix: str


class Envelope(BaseModel):
    greeter: GreeterInput


class Registry:
    """Stand-in for external roots invisible to a host-only cycle collector."""

    def __init__(self) -> None:
        self.entries: dict[int, list] = {}
        self.next_key = 1
        self.pending: list[int] = []

    def create(self, host: object) -> int:
        key = self.next_key
        self.next_key += 1
        self.entries[key] = [host, 1]
        return key

    def retain(self, key: int) -> None:
        self.entries[key][1] += 1

    def release(self, key: int) -> None:
        if key not in self.entries:
            return  # A completed shutdown already invalidated this generation.
        self.entries[key][1] -= 1
        if self.entries[key][1] == 0:
            del self.entries[key]

    def drain(self) -> None:
        while self.pending:
            self.release(self.pending.pop())


def enqueue_release(registry_ref: weakref.ReferenceType[Registry], key: int) -> None:
    registry = registry_ref()
    if registry is not None:
        registry.pending.append(key)


class Proxy:
    def __init__(self, registry: Registry, key: int) -> None:
        # Neither callback nor arguments retain this proxy or the host object.
        self.finalizer = weakref.finalize(self, enqueue_release, weakref.ref(registry), key)

    def close(self) -> None:
        self.finalizer()


def main() -> None:
    host = PythonGreeter(prefix="Hello")
    assert asyncio.run(host.greet("Ada")) == "Hello, Ada!"
    assert host.model_dump() == {"prefix": "Hello"}
    assert host.model_dump_json() == '{"prefix":"Hello"}'
    assert set(PythonGreeter.model_fields) == {"prefix"}
    copy = host.model_copy(deep=True)
    assert copy == host and copy is not host
    assert weakref.ref(host)() is host
    envelope = Envelope(greeter=host)
    assert envelope.greeter is host
    try:
        envelope.model_dump_json()
    except PydanticSerializationError as error:
        assert "not portable" in str(error)
    else:
        raise AssertionError("Interface field serialized as host configuration")
    try:
        Envelope.model_validate_json('{"greeter":{"prefix":"Hello"}}')
    except pydantic.ValidationError:
        pass
    else:
        raise AssertionError("JSON constructed a live interface")
    del envelope
    try:
        IncompleteGreeter(prefix="Hello")
    except TypeError as error:
        assert "abstract" in str(error)
    else:
        raise AssertionError("Missing required implementation was accepted")
    try:
        weakref.WeakKeyDictionary()[host] = 1
    except TypeError as error:
        assert "unhashable" in str(error)
    else:
        raise AssertionError("Mutable Pydantic model unexpectedly hashable")
    del copy

    registry = Registry()
    key = registry.create(host)
    proxy = Proxy(registry, key)
    registry.retain(key)  # BAML retained a separate reference.
    weak_host, weak_proxy = weakref.ref(host), weakref.ref(proxy)
    del host, proxy
    gc.collect()
    registry.drain()
    assert weak_proxy() is None and weak_host() is not None
    assert registry.entries[key][1] == 1
    registry.release(key)
    gc.collect()
    assert weak_host() is None

    key = registry.create(PythonGreeter(prefix="Once"))
    proxy = Proxy(registry, key)
    proxy.close()
    proxy.close()
    del proxy
    gc.collect()
    assert registry.pending == [key]
    registry.drain()
    assert not registry.entries

    host = PythonGreeter(prefix="Cycle")
    key = registry.create(host)
    proxy = Proxy(registry, key)
    # Deliberately create a host -> proxy -> external-root cycle.
    # This bypasses Pydantic storage only to model arbitrary application state.
    object.__setattr__(host, "retained_proxy", proxy)
    weak_host, weak_proxy = weakref.ref(host), weakref.ref(proxy)
    del host, proxy
    gc.collect()
    registry.drain()
    assert weak_host() is not None and weak_proxy() is not None
    registry.entries.clear()  # Model explicit scope/runtime teardown.
    gc.collect()
    registry.drain()
    assert weak_host() is None and weak_proxy() is None

    print(f"Pydantic {pydantic.__version__}: mixin, required method, data-only dump/copy passed")
    print("Interface input field preserves native instance; interface JSON input/output rejected")
    print("Mutable model is weak-referenceable but unhashable: identity cache must not use WeakKeyDictionary")
    print("Ownership model: proxy GC releases only its lease; retained host survives; final release frees host")
    print("Explicit close is idempotent; cross-runtime cycle requires scope/runtime teardown")


if __name__ == "__main__":
    main()

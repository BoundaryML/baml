---
date: 2026-07-08
repository: baml4
source_paths:
  - baml_language/sdks/python/src/baml_bridge/__init__.py
  - baml_language/sdks/python/src/baml_bridge/proto.py
  - baml_language/sdks/python/src/baml_bridge/typemap.py
  - baml_language/sdks/python/src/baml_bridge/_stream.py
  - baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/baml_inbound.proto
  - baml_language/crates/bridge_ctypes/src/value_decode.rs
  - baml_language/crates/bridge_cffi/src/lib.rs
---

# Python Inbound Argument Encoding

This file records how generated Python SDK calls encode Python arguments before
they cross into the BAML engine. It complements
`00a-prior-art-python-type-mappings.md` and
`00a-prior-art-python-examples.md`: those documents describe the generated
Python type surface, while this one describes the inbound runtime value path.

The important implementation fact is that the runtime bridge is implemented in
`baml_language/sdks/python/src/baml_bridge` (the distributed package name is
`baml_bridge`). Generated SDK packages import the runtime with absolute imports
(`from baml_bridge import define_function`) and bind functions with
`define_function(...)`; the generated package itself does not implement protobuf
encoding or runtime lookup.

## Call Path Overview

Generated runtime leaves bind functions, static methods, and instance methods
like this:

```python
from baml_bridge import define_function as _define_function

make_foo       = _define_function("user.make_foo", "sync",  ["v"])
make_foo_async = _define_function("user.make_foo", "async", ["v"])

class Box(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    value: int
    get_value       = _define_function("user.Box.get_value", "sync",  ["self"])
    get_value_async = _define_function("user.Box.get_value", "async", ["self"])
```

The typed callable surface lives in `.pyi` files. Runtime `.py` files install
plain Python callables returned by `define_function`; those callables do not
inspect the generated type annotations.

At runtime, a generated callable does this on the inbound side:

1. `_build_kwargs(...)` zips positional args against the generated
   `required_param_names`. The reserved kwargs `_ctx` and `_types` are popped
   first: `_types` binds explicit generic type args, `_ctx` carries a call
   context.
2. Keyword args are merged on top; `baml.UNSET` values are skipped.
3. A `call_id` is minted (`new_function_call()`) and, if `_types` was supplied,
   a `BamlTyArg` list is built via `_build_type_args`. `encode_call_args(merged,
   call_id, type_args)` serializes the merged kwargs to `CallFunctionArgs`.
4. `_attach_call_ctx` runs, then `get_runtime().call_function_sync(...)` or
   `await get_runtime().call_function(...)` sends the function FQN and encoded
   args to the PyO3/Rust runtime, and `_detach_call_ctx` runs afterward.

Sync and async functions share the same encoder. The only difference is whether
the Rust runtime call is `call_function_sync` or `call_function`.

## Argument Collection

`baml_bridge.define_function(...)` captures only these runtime facts:

```python
def define_function(
    baml_fqn: str,
    mode: Literal["sync", "async"],
    required_param_names: List[str],
    optional_param_names: Optional[List[str]] = None,
    *,
    type_params: Optional[List[str]] = None,
    class_type_params: Optional[List[str]] = None,
) -> Callable[..., Any]: ...
```

`type_params` / `class_type_params` name the generic parameters a callable can
bind via the `_types=` kwarg (or subscript). They are how the runtime knows
which `BamlTyArg`s to emit into `CallFunctionArgs.type_args`.

Required names are used for positional zipping. Optional names are captured but
are not used for runtime keyword validation in `_build_kwargs`; unknown keyword
names can still enter the protobuf payload and are rejected later by the engine
call boundary. Type correctness is likewise not checked against annotations on
the Python side.

`UNSET` is an omission sentinel:

```python
optional_args_probe(1, opt1=baml.UNSET)
```

omits `opt1` entirely from the encoded kwargs, so the engine can evaluate the
BAML default. Passing `None` is different: it encodes an explicit BAML `null`.

Instance methods work because Python's descriptor protocol supplies the
receiver as positional arg 0. Generated instance method bindings therefore have
`"self"` as their first required parameter name. Generated static methods wrap
the returned callable in `staticmethod(...)`, so no receiver is injected.

## Inbound Proto Shape

Python arguments encode to `baml_inbound.proto`:

```proto
message CallFunctionArgs {
  repeated InboundMapEntry kwargs = 1;
  uint64 call_id = 2;            // mandatory, must be nonzero
  repeated BamlTyArg type_args = 3;  // explicit generic bindings from `_types=` / subscript
}

message InboundMapEntry {
  oneof key {
    string string_key = 1;
    int64 int_key = 2;
    bool bool_key = 3;
    InboundEnumValue enum_key = 5;
  }
  InboundValue value = 6;
}

message InboundValue {
  oneof value {
    string string_value = 2;
    int64 int_value = 3;
    double float_value = 4;
    bool bool_value = 5;
    InboundListValue list_value = 6;
    InboundMapValue map_value = 7;
    InboundClassValue class_value = 8;
    InboundEnumValue enum_value = 9;
    BamlHandle handle = 10;
    bytes uint8array_value = 11;
    string bigint_value = 12;
    BamlTy ty_value = 13;  // decoded by Rust, but no Python encoder branch emits it (no-op from Python)
  }
  // Absent oneof = null value.
}
```

`InboundClassValue` no longer has a flat `name` field (field 1 is `reserved`);
the class FQN and any reified generics live on `class_ty` (`class_ty.name` +
`class_ty.type_args`).

Inbound values intentionally do not carry declared BAML parameter types. The
Python encoder dispatches on Python runtime shape, Rust decodes to
`BexExternalValue`, and the engine re-runs BAML signature/type validation after
deserialization.

## Python Encoding Rules

`baml_bridge.proto.encode_call_args(kwargs, call_id, type_args)` creates a
`CallFunctionArgs` message and populates each kwarg with
`_set_inbound_map_entry(...)` / `_set_inbound_value(...)`. It raises
`ValueError` if `call_id == 0`.

The encoder order matters:

| Python runtime value | Inbound proto field | Notes |
| --- | --- | --- |
| `None` | absent oneof | Encodes BAML `null`. |
| `bool` | `bool_value` | Checked before `int`, because `bool` is an `int` subclass in Python. |
| `int` within signed i64 | `int_value` | Python `int` is arbitrary precision, but this field is `int64`. |
| `int` outside signed i64 | `bigint_value` | Hex/base-16 string via `format(value, "x")`; negative values keep a leading `-`. |
| `float` | `float_value` | No declared-type check on the Python side. |
| `str` | `string_value` |  |
| `bytes` / `bytearray` | `uint8array_value` | `bytearray` is copied to `bytes`. |
| `list` / `tuple` | `list_value.values[]` | Recursively encodes items. Empty lists call `SetInParent()` so they do not become null. |
| `dict` | `map_value.entries[]` | Recursively encodes values. Empty dicts call `SetInParent()` so they do not become null. |
| `enum.Enum` | `enum_value` | `name` is the BAML enum FQN from `BamlTypeMap.py_type_to_baml_type(...)`; `value` is the Python enum member name. |
| `BamlPyHandle` | `handle` | Clones a handle-table key for the wire and preserves the handle type tag. |
| `BamlStream` | `handle` | Encodes as the stream's inner `BamlPyHandle`, not as a class wrapper. |
| `BamlImage` / `BamlAudio` / `BamlVideo` / `BamlPdf` | `class_value` | Sets the stdlib class FQN on `class_value.class_ty.name` and one `_data` field containing the inner handle. |
| non-class callable | `handle` with `HOST_VALUE_CALLABLE` | Registers the Python callable in the host-value registry. |
| Pydantic `BaseModel` instance | `class_value` | Sets the generated class FQN on `class_value.class_ty.name`, reified generic args on `class_value.class_ty.type_args` (via `pydantic_instance_type_args`), and recursively encodes model fields. |
| unsupported object | `TypeError` | Error message names the top-level kwarg being encoded. |

Pydantic model encoding deliberately walks `dict(value).items()` instead of
`model_dump()`. `model_dump()` would recursively flatten nested Pydantic
instances into plain dicts, losing class FQN metadata for nested generated
models. Private Pydantic attrs are not included by `dict(value)`, so the
encoder separately walks `__pydantic_private__` and emits private values that
are `BamlPyHandle`s. This is how handle-backed generated shells round-trip.

For Pydantic generics, `_base_class_for_fqn(type(value))` strips Pydantic v2's
runtime parameterized subclass (`Box[int]`) back to its origin (`Box`) before
reverse typemap lookup. The base BAML FQN goes on `class_ty.name`; the concrete
type args are also carried inbound on `class_ty.type_args` (filled via
`pydantic_instance_type_args`), and Rust reads them back into
`BexExternalValue::Instance.type_args` — generics are reified across the wire,
not only recovered engine-side.

For maps, Python can put string, int, bool, or enum keys on the inbound
`InboundMapEntry`. Rust's inbound decoder ultimately turns all map keys into
strings before constructing `BexExternalValue::Map`: int and bool keys stringify,
and enum keys become `"{enum_name}::{variant_name}"`.

Host callables have an encode-error rollback path. If encoding kwarg `a`
registers a callable and encoding a later kwarg `b` fails, `encode_call_args`
releases every callable key registered during that failed encode, because the
engine never received the payload and therefore cannot release those keys.

## Typemap Role

The generated SDK root installs a process-global typemap:

```python
from baml_bridge import BamlRuntime, set_type_map
from ._typemap import _TYPE_MAP

BamlRuntime.initialize_runtime_from_bytecode(_inlinedbaml.BYTECODE)
set_type_map(_TYPE_MAP)
```

`BamlTypeMap` maintains a reverse map from Python class identity to BAML FQN for
inbound Pydantic and enum encoding. Stdlib PyO3 re-exports such as `BamlImage`
live in `baml_bridge.baml_py`, so the typemap seeds hardcoded reverse overrides
for:

- `baml.media.Image`
- `baml.media.Audio`
- `baml.media.Video`
- `baml.media.Pdf`
- `baml.llm.Stream`

## Rust Inbound Decode

The CFFI entry point receives the serialized `CallFunctionArgs` bytes, parses
them, and calls `bridge_ctypes::value_decode::kwargs_to_bex_values(...)`.

Rust converts inbound protobuf values to `BexExternalValue`:

| Inbound proto field | Rust value |
| --- | --- |
| absent oneof | `BexExternalValue::Null` |
| `string_value` | `BexExternalValue::String` |
| `int_value` | `BexExternalValue::Int` |
| `bigint_value` | `BexExternalValue::Bigint`, parsed from strict hex with a pre-allocation length cap |
| `float_value` | `BexExternalValue::Float` |
| `bool_value` | `BexExternalValue::Bool` |
| `uint8array_value` | `BexExternalValue::Uint8Array` |
| `list_value` | `BexExternalValue::Array` with recursively decoded items |
| `map_value` | `BexExternalValue::Map` with stringified keys and recursively decoded values |
| `class_value` | `BexExternalValue::Instance { class_name, fields, type_args }` (FQN + reified generics read from `class_ty`) |
| `enum_value` | `BexExternalValue::Variant { enum_name, variant_name }` |
| `ty_value` | `proto_ty_to_external(...)` — decodable, though no Python encoder emits it |
| `handle` with `HOST_VALUE_CALLABLE` / `HOST_VALUE_OPAQUE` | `BexExternalValue::HostValue`; bypasses `HANDLE_TABLE` |
| other `handle` | drains the key from `HANDLE_TABLE` and converts the table entry |

The engine receives the decoded kwargs and the called function FQN. Any missing
required arg, extra arg, or structural type mismatch is an engine boundary
error, not a Python encoder error.

## Practical Consequences For Bridge Generics

- Generated `.pyi` annotations are static-only. Runtime arg encoding is
  structural and Python-value-driven.
- The inbound wire payload usually carries class/enum FQNs for generated
  objects, but not declared parameter types. Rust and the engine own coercion
  and BAML type validation.
- `None` and `baml.UNSET` are intentionally different: `None` is explicit null;
  `UNSET` omits the kwarg so BAML defaults can run.
- Empty lists and dicts require explicit oneof presence on the protobuf message;
  the encoder handles this with `SetInParent()`.
- Host callables and host exceptions are handle-backed values with registry
  lifetime rules, not ordinary class/function serialization.

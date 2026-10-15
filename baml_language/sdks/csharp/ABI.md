# C# bridge ABI and wire contract

The managed runtime imports one native symbol: `baml_get_api_v1`. It validates
the returned table's ABI version, required prefix size, and every required
function pointer before use. New optional functions are appended to
`BamlApiV1` and gated by `struct_size`; existing fields and offsets are never
reordered. Buffers and opaque handles cross the boundary with explicit clone,
release, and free operations. Tests cover truncated prefixes, missing entries,
callback and cancellation lifetimes, buffer ownership, media ownership, and
exact handle release.

The unified `call_function(encoded_args, length, callback_id)` signature is function-table ABI revision 2. Revision-1 hosts and runtimes reject revision-2 counterparts before reading the changed function-pointer slot, preventing mixed-revision calls from invoking incompatible layouts.

The canonical protobuf schemas live in
`crates/bridge_ctypes/types/baml_bridge/cffi/v1`. Field numbers are stable and
a new field is optional on the wire. The inbound value contract below is the
exception: it fixes which nodes carry `value_type`, and malformed or missing
metadata fails closed. The C# runtime consumes these schemas directly with
build-time `Grpc.Tools`; it does not maintain a private schema copy.

## Inbound value contract

An inbound value is the serialization of a concretely typed value graph. The
engine builds exactly what the wire says, converts nothing, and checks the
built value once for membership in the slot it fills: a parameter, a host
callback's return or throw, or a sys-op result. `InboundValue.value_type` is
the node's own type, and whether a node carries one is fixed by its arm:

| `InboundValue` arm | `value_type` | The node's type |
|---|---|---|
| `int_value`, `float_value`, `bigint_value`, `string_value`, `bool_value`, `uint8array_value`, absent (`null`) | forbidden | implied by the arm |
| `enum_value` | forbidden | the enum and variant the node names |
| `media_value`, `prompt_ast_value`, `ty_value`, `ty_def_value` | forbidden | implied by the value |
| `handle` to a resource or an opaque host value | forbidden | the handle's own type, checked live |
| `list_value` | required | `E[]` |
| `map_value` | required | `map<K, V>` |
| `class_value` | required | the class with every type argument |
| `handle` to a host callable | required | the callable's function type |
| `js_number_value` | refused | |

A statement is a realized type. The decoder refuses, before the engine sees the
value, a statement that mentions a type variable, one that lacks a part no type
can do without (an unspecified primitive, a literal without its value, an empty
type), one of another shape than its node, a missing statement, and a statement
on a node whose type is implied. Only a recursive alias exists as a declaration
at run time, so a statement spells a non-recursive alias by its body. A host
states a callback's open effect parameter as `unknown`.

The engine then hydrates the stated graph. Every child is checked against the
structure its parent states (each element of a stated `(int | string)[]` must
inhabit `int | string`), a class's fields against its declaration with the
stated type arguments substituted, and the root against its slot. A generic
function's type arguments are inferred from the landed values' concrete types
and from explicit `type_args`; a call that leaves one unbound is refused. A
refusal names the parameter and the path:

```
argument `items` of `Total`: at `.items[0].x`: Value of type 'int' does not match the declared type `float`
```

A callback return or throw that does not inhabit the declared type is a
`baml.panics.HostContractViolation`.

This contract is not additive. A list, map, class or callable node without a
`value_type`, a `value_type` on a self-evident node, and `js_number_value` are
refused rather than interpreted, where earlier runtimes filled the gap from the
slot. A host must ship with a bridge of the same revision; every SDK in this
repository encodes against the declared types, through the shared walker in
`crates/bridge_encode` or its generated code, and ships lockstep with the
runtime.

Generated C# types carry canonical serialized `BamlTy` metadata through names,
union cases, descriptors, registry factories, and codecs. The C# generator
normalizes unions once at its boundary so all those layers observe the same
ordering. Maps accept only BAML `string` keys.

Generated enum values use the stable `baml-csharp-enum-discriminant-v1`
identity grammar. Each field is encoded as a one-byte tag, a big-endian
unsigned 32-bit UTF-8 byte length, and the UTF-8 bytes. Counts use a one-byte
tag followed by a big-endian unsigned 32-bit count. The ordered input is:

1. identity field `0x00` containing the grammar name;
2. package count `0x10`, followed by package field `0x11` when the package is
   not the implicit `user` package;
3. namespace count `0x20`, followed by one namespace field `0x21` per segment;
4. enum symbol field `0x30` and variant symbol field `0x31`.

The signed discriminant is the first eight SHA-256 bytes interpreted as a
big-endian unsigned integer with the high bit cleared. Generation rejects zero
and collisions within an enum.

Opaque resource values are emitted only for classes whose compiled metadata
uses the tagged heap-handle representation. Inbound handles must belong to the
active heap, still resolve, match their declared type arguments, and resolve
to an instance of a tagged resource class. User classes remain structural.

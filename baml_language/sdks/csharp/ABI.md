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
| `handle` to a resource, a function, media, a prompt, a type or an opaque host value | forbidden | the handle's own type, checked live |
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

Map keys, class field names and keyword-argument names cross the boundary as
strings only. The decoder refuses an `int_key`, `bool_key` or `enum_key`
rather than spelling it as a string, so a `map<int, V>` or `map<Color, V>`
parameter cannot be called from a host until the boundary carries typed keys;
the key arms stay on the wire for that day.

The engine then hydrates the stated graph. Every child is checked against the
structure its parent states (each element of a stated `(int | string)[]` must
inhabit `int | string`), a class's fields against its declaration with the
stated type arguments substituted, and the root against its slot. A generic
function's type arguments are inferred from the landed values' concrete types
and from explicit `type_args`. A variable that occurs under a union or optional
(`x: T | string | null`, `x: T?`) in a parameter whose value arrived, and that
the value did not pin because a concrete sibling took it (`"hi"`, `null`), is
`never`: the value could be a `T` or not, and the tie breaks to the type no
value has. A call that leaves a variable unbound otherwise (nothing arrived
for it, or only an empty container) is refused. A refusal from the engine
names the parameter and the path; a refusal from the decoder names the node:

```
argument `items` of `Total`: at `.items[0].x`: Value of type 'int' does not match the declared type `float`
```

A callback return or throw that does not inhabit the declared type is a
`baml.panics.HostContractViolation`. A result the bridge could not carry (too
deep for the wire, a handle that could not be cloned) is a
`baml.panics.SdkPanic`; an exception the callback's own code raised while the
result was read is that exception, as any the callback throws.

### How the dynamic hosts state types

Python and TypeScript have no static type to state from, so their bridges
walk each value against the declared type (`crates/bridge_encode`) and state
what the walk decides:

- A value under a realized slot states the slot. A value under `unknown` states
  the type it has: scalars by kind, a list by the one type its elements share
  or their union, a class instance by its head with type arguments solved from
  its fields.
- An empty container has no element type of its own. A sibling's type fills it
  (`[[], [1]]` is `int[][]`); under `unknown` it is `unknown[]`; under an open
  type variable (`xs: T[]`) the call is refused until the caller names `T`.
- A handle inside a container has a type only the engine can see. No sibling
  fills it: under `unknown` the container is `unknown[]`; under an open type
  variable the call is refused.
- A union member is chosen in one place, by rank: a value that knows its type
  (an instance of a class, an enum member) takes that member; a value's own
  shape (a list under a list member, a dict under a map member) comes next;
  an interface, type variable or `unknown` member takes the value as it is
  and leaves the judgment to the engine, which beats a conversion (a dict
  standing in for a class, an `int` where a `float` is declared, a string
  naming a variant). Members that take the value as the same type
  count as one; the value's own type breaks a tie among distinct ones; a value
  that fits two distinct member types in no preferred way is refused as
  ambiguous (`[]` under `string[] | int[]`). Member order is never consulted.
  A host-side wrapper naming the member is planned; until it exists such a
  value cannot be sent.
- A dynamic host reads at most 1,048,576 nodes per argument (a shared or cyclic
  value expands to a tree on the wire) and refuses a value that would nest
  deeper than the wire's 100 protobuf messages, before anything is created.

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
uses the tagged heap-handle representation. An inbound handle carries no
statement; it must belong to the active heap and still resolve, and the live
value it names is checked against its slot like any other. User classes remain
structural.

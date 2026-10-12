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
`crates/bridge_ctypes/types/baml_bridge/cffi/v1`. Schema evolution is additive:
field numbers are stable, new metadata is optional on the wire, and decoders
must accept legacy messages that omit it. Present but malformed collection or
union metadata fails closed. The C# runtime consumes these schemas directly
with build-time `Grpc.Tools`; it does not maintain a private schema copy.

Inbound values are admitted, never converted. A value inhabits the declared
type of its slot as it is, or the call (or the host callback's return) is a
type error: an `int_value` is no `float` or `bigint`, a `map_value` is no
class, a `string_value` is no enum variant, and `js_number_value` is refused.
What a host means by a value that is not a BAML value is the host's bridge to
say before the value is sent. `InboundValue.value_type` names the one exact
type of its node: it must be assignable to the slot and must determine a value
(a concrete type or a literal, never a union, an optional, `unknown`, an
interface or a type variable), and the node is judged against it. Scalars,
`null`, enum variants, media, handles and callables are self-evident and carry
no `value_type`. A list, map or class under an abstract slot (`unknown`, an
interface, a union including an optional, or a type that mentions an unbound
type variable) must carry one. Where several members of a union take a value
and every one of them lands the same value (`"draft"` under
`string | "draft"`), none has to be named; where they land differently (an
empty list under `int[] | string[]`), the host names the member.

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

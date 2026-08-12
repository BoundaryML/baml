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

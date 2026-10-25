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

## The contract at a glance

Every value, type and callable crosses in exactly one form in each direction,
and one party checks it. The sections that follow give the rules in full.

| What crosses | Engine → host | Host → engine | Checked by | Refused when |
|---|---|---|---|---|
| `int`, `float`, `bigint`, `string`, `bool`, `uint8array`, `null` | its scalar arm | the same arm, no `value_type` | decoder (arm), engine (membership in the slot) | a `value_type` is present, or the slot does not admit the type |
| enum variant | `enum_value` naming enum and variant | the same, no `value_type` | engine | the variant is not one of the enum's, or the slot does not admit it |
| list, map, class that crosses by fields | `list_value` / `map_value` / `class_value` with its type | the same with `value_type` stating `E[]`, `map<K, V>` or the class with every type argument | decoder (statement well-formed and of the node's shape), engine (every child against the structure its parent states, the root against the slot) | the statement is missing, malformed, unrealized or of another shape; a child is not a member; the class is not one that crosses by fields |
| `$rust_type` field payload | `UNTAGGED_RUST_DATA` handle | the handle it received | engine (the table row) | a host builds the field from a value of its own |
| `reflect.Package`, `reflect.Session`, `reflect.CompileArtifact`; an instance of a run-time declaration | `ADT_RUNTIME_VALUE` handle with `ty` naming the class | the handle it received | engine | a `class_value` names such a class; the handle is not a first-class value (a declaration, a raw package) |
| media, `ai.Prompt` | `media_value`, `prompt_ast_value` | the same | decoder | a slot of another type |
| BAML callable (closure, bound method, generic instantiation, `ai.FunctionSpec`) | callable handle stating its specialized function type | the handle it received, no `type_args` | engine (the stated type against the slot) | the stated type is not a member of the slot's; an argument the host passes through the handle is not a member of the specialized parameter |
| host callable | — | `HOST_VALUE_CALLABLE` key with `value_type` stating the function type, open effect as `unknown` | decoder (one kind per key per request), engine | the key arrives under two kinds, or the function type does not inhabit the slot |
| host opaque value | its own key, back as it arrived | `HOST_VALUE_OPAQUE` key, no `value_type` | decoder | the key arrives under two kinds |
| type value | `ty_value` tree; compiled heads by name, run-time heads as `live` rows, `anonymous_label` only in annotations | `ty_value` tree; `name` resolved against the program's tables, `live` followed by handle | decoder (realized, no `anonymous_label`), engine (every head resolves) | a `type_var` or projection anywhere; a name that resolves to nothing; an `anonymous_label` |
| declaration, raw package | never | never | engine | a handle to one is sent |
| generic call's type arguments | — | `type_args` by name, the rest inferred from the landed values | engine | a variable is left unbound (nothing arrived, or an empty container); a bounded parameter is not one concrete type implementing every bound at the bound's arguments; a variable a sibling absorbed is `never`, not a refusal |
| callback outcome | `HostCallCompletion` with one of `returned`, `thrown`, `raised`, `contract_violation`, `bridge_failure` | — | engine | `returned`/`thrown` outside the declared types is `HostContractViolation`; a `bridge_failure` or empty completion is `SdkPanic` |
| size | — | at most 1,048,576 nodes per call across all arguments; at most 100 nested protobuf messages | reader (per pulled node, before anything is built), decoder | the budget is exceeded: a refusal creates nothing |

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
| `media_value`, `prompt_ast_value`, `ty_value` | forbidden | implied by the value |
| `handle` to a resource, a function, media, a prompt, a type or an opaque host value | forbidden | the handle's own type, checked live |
| `list_value` | required | `E[]` |
| `map_value` | required | `map<K, V>` |
| `class_value` | required | the class with every type argument |
| `handle` to a host callable | required | the callable's function type |
| `js_number_value` | refused | |

Every type on the wire is realized: a statement, an explicit type argument, a
`ty_value` and every type the engine sends. A `type_var` or an
`associated_type_projection` determines no value, so the decoder refuses one
anywhere in a type and the engine never sends one. The decoder also refuses,
before the engine sees the value, a statement that lacks a part no type can do
without (an unspecified primitive, a literal without its value, an empty
type), one of another shape than its node, a missing statement, and a statement
on a node whose type is implied. Only a recursive alias exists as a declaration
at run time, so a statement spells a non-recursive alias by its body. A host
states a callback's open effect parameter as `unknown`. A handle's key is
nonzero: key 0 names no slot and is refused.

A handle's `handle_type` decides which of two reference tables its key names.
`HOST_VALUE_CALLABLE` and `HOST_VALUE_OPAQUE` keys are the engine's references
to objects the host registered, numbered by the host; every other key is the
host's reference to an engine object, numbered by the engine. The two spaces
may share numbers and are never compared. Each occurrence of an engine key
transfers one reference, which the engine drains or, on a refusal anywhere in
the request, releases; a host key is one registration however often it occurs,
and keeps the kind it first arrived under: a key under two host kinds in one
request, or under a kind other than the one the engine already holds for it,
is refused. A host never clones or releases a host key through the handle API.

How an instance of a class crosses is decided once per declaration. A class
crosses by its fields, a host reads and builds it as plain data, and a field of
`$rust_type` (a cipher's state, a cancel token, an HTTP body) is an
`UNTAGGED_RUST_DATA` handle to the engine's own payload, which the host passes
back as it received it. A standard-library capability whose fields are the
runtime's internals (`reflect.Package`, `reflect.Session`,
`reflect.CompileArtifact`), and an instance of any class or enum declared at
run time, cross instead as an `ADT_RUNTIME_VALUE` handle to the engine's own
instance, with `ty` naming the class: a host holds it, passes it back as the
handle it received, and never builds one from fields; a `class_value` naming
such a class is refused. A handle a host sends lands only a value: a handle to
a declaration or to a raw package is refused. Media and `ai.Prompt` cross as
the media and the prompt themselves.

A type names each declaration by its head. A declaration compiled with the
program is spelled by `name`, its qualified name, and resolved against the
program's own tables only. A declaration the runtime created (`reflect.Class.new`,
a runtime-compiled package) has no name a lookup could resolve: it is `live`,
the key of a handle the host holds to the declaration itself, which the engine
follows. Each occurrence of a `live` key transfers one reference, like any
engine key, so a host sends a fresh clone per occurrence and keeps its own. A
`ty_value` the engine returns hands the host one such reference per `live`
head, which the host releases when it lets the type go. `anonymous_label` is
display text: it labels an unnameable declaration in the annotation beside a
value that crosses by reference (a runtime instance's `ty`, a container's
element type) and creates no reference. A stated type that carries one is
refused, and so is an instance stated by fields whose class is a runtime
declaration: such an instance is passed back as the handle it arrived as.

A class with a `$rust_type` field holds native data only the runtime creates
(a media wrapper's payload, a cipher's state, a cancel token). A host passes
such an instance back as it received it, the field holding a handle to the
runtime's data; a value of the host's own in that field is refused. The
exception is `baml.errors.HostCallable`, whose payload is the host's own
handle to the exception it raised: the runtime builds that class from a
callback's `raised` outcome and only carries it back.

Map keys, class field names and keyword-argument names cross the boundary as
strings only. The decoder refuses an `int_key`, `bool_key` or `enum_key`
rather than spelling it as a string, so a `map<int, V>` or `map<Color, V>`
parameter cannot be called from a host until the boundary carries typed keys;
the key arms stay on the wire for that day.

The engine then hydrates the stated graph. Every child is checked against the
structure its parent states (each element of a stated `(int | string)[]` must
inhabit `int | string`), a class's fields against its declaration with the
stated type arguments substituted, and the root against its slot. A
`class_value` may leave out a field whose type admits `null`; it reads as
`null`, exactly as if the host had sent one, unless the value also carries a
key the class does not declare (a misspelling, an `@alias` name), which is
refused rather than letting the real field read as `null`. Reading an omitted
field as `null` is a transformation with a small cost of its own, weighed
after every other distinction when a plain object fits more than one member of
a union: the member whose plan reads the fewest omitted fields wins (`{text}`
under `Note { text, meta: json } | Tag { text }` is a `Tag`), and a tie is
still refused as ambiguous. A generic
function's type arguments are inferred from the landed values' concrete types
and from explicit `type_args`. A variable that occurs under a union or optional
(`x: T | string | null`, `x: T?`) in a parameter whose value arrived, and that
the value did not pin because a concrete sibling took it (`"hi"`, `null`), is
`never`: the value could be a `T` or not, and the tie breaks to the type no
value has. A call that leaves a variable unbound otherwise (nothing arrived
for it, or only an empty container) is refused. A type parameter with an
interface bound (`T extends baml.ops.Compare`) takes one concrete type that
implements every bound at the bound's own arguments, whether the type was
inferred (`int` and `"x"` join to `int | string`, which is not one concrete
type) or stated in `type_args` (a literal type such as `"x"` is not one
either: it is a subset of `string`, and a bound takes a concrete type, never
a value's exact literal); otherwise the call is refused. A callable
value is already specialized and takes no `type_args`. A refusal from the engine
names the parameter and the path; a refusal from the decoder names the node:

```
argument `items` of `Total`: at `.items[0].x`: Value of type 'int' does not match the declared type `float`
```

A callback has one completion channel, `complete_host_call`, whose payload is
a `HostCallCompletion` with one of five outcomes. The host encodes a `returned`
value against the callable's declared return type and a `thrown` BAML value
against its declared throws type; the engine holds each to that type, and a
value that does not inhabit it is the callable's breach of contract, a
`baml.panics.HostContractViolation`. A thrown `baml.panics.*` instance is a
panic whatever the callable declares. An exception of the host's own is
`raised` with its identity and the host's registration of the exception
object; the engine builds the `baml.errors.HostCallable` around that handle,
and holds it to the declared throws type the same way, so a host never builds
that class itself. A `contract_violation` is a value the host already found
not to fit its declared type. A `bridge_failure` (a value too deep for the
wire, a handle that could not be cloned, a payload the engine could not
decode) is the bridge's own and fatal to the call, a `baml.panics.SdkPanic`;
so is a completion that carries no outcome. The same contract holds for the
runtime's own operations: an error one throws outside its declared throws type
is an internal error, never a catchable throw of an undeclared class.

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
- A dynamic host reads at most 1,048,576 nodes per call, shared by all of the
  call's arguments (a shared or cyclic value expands to a tree on the wire), and
  refuses a value that would nest
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

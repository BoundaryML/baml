---
date: 2026-07-23
repository: baml4
mirrors: baml_language/sdks/agent-docs/bridge-ref/ref-python-inbound-encoding.md
source_paths:
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlFfi.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/internal/ProtoWriter.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/internal/WireWriter.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/TypeRegistry.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlType.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlTypes.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlHandle.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlMedia.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlUnion.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlHostCallable.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlStream.java
  - baml_language/sdks/java/sdkgen_java/src/emit.rs
  - baml_language/sdks/java/baml_bridge/src/test/java/baml_bridge/WireCodecTest.java
  - baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/baml_inbound.proto
---

# Java Inbound Argument Encoding

> **Superseded contract (2026-10).** The inbound contract this document was
> written against is gone. The engine is now exact: it builds what the wire
> states, converts nothing, and checks the value once against its slot. The
> authority is the "Inbound value contract" section of
> `baml_language/sdks/csharp/ABI.md`, and the Java implementation is
> `ProtoWriter.java` itself. In particular, these claims below no longer hold:
>
> - `value_type` is not sparse. Every list, map, class and callable node states
>   its exact type; scalars, `null`, enums, media, prompts and handles state
>   nothing.
> - The writer does not dispatch on the Java runtime shape alone. Generated
>   bindings pass each argument's declared slot (`BamlTypedValue`, including
>   slots that mention the call's type parameters), and the writer states that
>   slot or, under an open slot, the value's own type.
> - A `BigInteger` is always `bigint_value`, never `int_value`.
> - An empty container under an open type variable is refused until the caller
>   names the variable; under `unknown` it is `unknown[]`; under a recursive
>   alias such as `json` it states the alias's member (`json[]`).
> - Media is a `media_value`, not a `class_value`.
> - There is no ambiguity policy and no `js_number_value`; the engine never
>   picks a union member or converts a number.
>
> The line citations below date from 2026-07 and have drifted.

This file records how generated Java SDK calls encode Java arguments before they
cross into the BAML engine. It is the section-for-section mirror of
`ref-python-inbound-encoding.md`, written so the two can be read side by side for
a 1:1 decision review. It complements `ref-java-codegen-conventions.md` and
`ref-java-state-of-completeness.md`: those describe the generated Java type
surface, while this one describes the inbound runtime value path.

The important implementation fact is that the runtime bridge is implemented in
`baml_language/sdks/java/baml_bridge` (Maven/Gradle artifact `baml-bridge`,
package `baml_bridge`). Generated SDK packages (`baml_sdk.*`) call this runtime
directly — `baml_bridge.BamlFfi.callSync(...)` / `callAsync(...)` — and the
generated package itself does not implement protobuf encoding or runtime lookup.
The wire envelopes (`baml_bridge.cffi.v1`) are shared with the Python and
TypeScript bridges, so this document diverges from the Python one only on the
**host language ⇒ wire** encode path, never on the wire shape or the Rust decode.

> **Reading conventions.**
> - Headings, ordering, and the value-kind table mirror the Python doc so the
>   two align row-for-row.
> - `> ⚠ **Deviation from Python:**` blockquotes flag every place Java's
>   mechanism differs from Python's (naming, sentinel strategy, boxed-type
>   widening, error types, …).
> - `**NOT YET IMPLEMENTED IN JAVA**` marks kinds the Python encoder emits that
>   the Java encoder does not yet emit, with the decided/open status from
>   `thoughts/antonio/java-function-calls-decisions.md`.
> - Every Java behavior below cites `file:line`; nothing is invented.

## Call Path Overview

There is no `define_function` factory in Java. The `sdkgen_java` emitter bakes
the runtime call directly into each generated binding — free functions and class
static methods as `static` methods on a `Fns` holder / the value class, instance
methods as non-static methods that prepend the receiver (`emit.rs:29-34`):

```java
// free function  return_int(v)  →  baml_sdk.primitives.Fns   ($RET0 = BamlType.INT)
public static long return_int(long v) {
    return (java.lang.Long) baml_bridge.BamlFfi.callSync("user.return_int", new String[] {"v"}, new Object[] {v}, $RET0);
}

// instance method  Box.get_value()  →  baml_sdk.lorem.Box
public long get_value() {
    return (java.lang.Long) baml_bridge.BamlFfi.callSync("user.Box.get_value", new String[] {"self"}, new Object[] {this}, $RET0);
}
```

Each binding therefore carries two baked literals — `names[]` (the declared
parameter names, receiver first for instance methods) and `args[]` (the boxed
values, `this` first for instance methods) — plus a return-type decode
descriptor: a pooled per-holder `private static final baml_bridge.BamlType
$RET{n}` constant (a typed `BamlType`, or the literal `null` for a wire-driven
return), **not** a descriptor string (`emit.rs`, `DescriptorPool`;
`translate_ty.rs`, `descriptor_expr`).

At runtime, `BamlFfi.callSync` / `callAsync` do this on the inbound side
(`BamlFfi.java:213-233` sync, `290-338` async):

1. Mint a `call_id` via `newCallId()` → `nativeNewCallId()` (the engine's
   `sys_types::CallId::next` counter; nonzero, with an `AtomicLong` fallback)
   (`BamlFfi.java:107-108, 220, 297, 473-478`).
2. If a `BamlCallContext ctx` was passed, attach the `call_id` to it for
   cancellation, detached in a `finally` (`BamlFfi.java:221-232`).
3. `ProtoWriter.encodeCallFunctionArgs(names, args, callId, typeArgs)` serializes
   the paired `names[]`/`args[]` (and any explicit-generics `BamlTypes` bag) into
   `CallFunctionArgs` bytes (`BamlFfi.java:225, 305`; `ProtoWriter.java:110-127`).
4. `nativeCallSync(fqn, request)` returns the `BamlOutboundResult` bytes
   inline, or `nativeCallAsync(callId, fqn, request)` spawns the engine call and
   the envelope is delivered later to `completeCall` (`BamlFfi.java:226, 306`).

Sync and async share the identical encoder — the only difference is
`nativeCallSync` vs `nativeCallAsync` (`BamlFfi.java:226` vs `306`), exactly as
Python's `call_function_sync` / `call_function` split.

> ⚠ **Deviation from Python:** Python builds a kwargs *dict* at runtime by
> zipping positional args against `required_param_names` and merging keyword
> args (`_build_kwargs`). Java has no runtime name/value merge: the emitter
> emits a fixed, positionally-paired `names[]`/`args[]` pair per binding, and the
> encoder zips them by index (`ProtoWriter.java:112-119`). Unknown keyword names
> cannot enter the payload the way they can in Python — the arg set is fixed at
> codegen time.

## Argument Collection

The facts Python captures in `define_function(...)` map to Java as follows:

| Python `define_function` fact | Java equivalent | Source |
| --- | --- | --- |
| `required_param_names` (positional zipping) | the baked `names[]` literal, receiver first | `emit.rs:411-432` |
| receiver injection (`"self"` at index 0) | emitter prepends `"self"` / `this` for instance methods | `emit.rs:416-418` |
| static methods get no receiver | `static` bindings have no prepended receiver | `emit.rs:29-31, 345-347` |
| `type_params` / `class_type_params` (the `_types=` bindable set) | a `BamlTypes` bag threaded to the generic `callSync`/`callAsync` overload | `BamlTypes.java`, `BamlFfi.java:213-219` |
| `UNSET` omission sentinel | the `$Opts` touched-set (no sentinel value) | `emit.rs:592-707` |

**Instance-method receiver.** Just like Python's descriptor protocol supplies
`self` as positional arg 0, the Java emitter prepends `"self"` to `names[]` and
`this` to `args[]` so the engine sees the receiver as required param 0
(`emit.rs:411-418`). Static methods and free functions prepend nothing.

**Optional-args tri-state (the `$Opts` touched-set).** A callable with ≥1
optional argument gets a trailing AWS-SDK-v2-style configurator overload
(`Fns.optional_args_probe(1, o -> o.opt1(5))`) plus a nested `<Ident>$Opts`
options class (`emit.rs:506-528, 592-707`). The opts class records each *touched*
optional into an insertion-ordered `$values` map plus a `$touched` set
(`emit.rs:693-701`); its package-visible `$names(base)` / `$args(base)` accessors
copy the base required arrays and append the touched optionals in touch order
(`emit.rs:703-705`). This yields the three states:

- **Omitted** (setter never called): the optional is absent from `names[]`/`args[]`
  entirely, so the engine evaluates the BAML default. This is the Java analog of
  Python's `opt1=baml.UNSET`.
- **Touched with a value**: appended to `names[]`/`args[]` and encoded normally.
- **Touched with `null`** (`o.opt1(null)`): appended to `names[]` with a `null`
  in `args[]`, which encodes as an `InboundMapEntry` carrying only `string_key`
  and an absent `value` oneof — an explicit BAML `null`
  (`ProtoWriter.java:138-147`; pinned by
  `WireCodecTest.inbound_null_kwarg_encodes_as_absent_value_entry:222-241`).

> ⚠ **Deviation from Python:** Python distinguishes omit-vs-null with the
> `baml.UNSET` sentinel value passed through kwargs. Java uses the `$Opts`
> touched-set: an optional reaches the wire **iff its setter was called at
> least once**, so "omit" is "setter never invoked" and "explicit null" is
> "setter invoked with `null`". No sentinel object exists in the Java surface
> (`emit.rs:36-42`).

Type correctness is not checked against Java static types on the encode side
(generics are erased); the engine re-runs BAML signature/type validation after
decode, so a missing required arg, extra arg, or structural mismatch is an
engine-boundary error, not an encoder error.

## Inbound Proto Shape

Java arguments encode to the shared `baml_inbound.proto` (field numbers pinned in
`ProtoWriter.java:19-38`):

```proto
message CallFunctionArgs {
  repeated InboundMapEntry kwargs = 1;
  uint64 call_id = 2;                 // mandatory, must be nonzero
  repeated BamlTyArg type_args = 3;   // explicit generic bindings from a BamlTypes bag
}

message InboundMapEntry {
  oneof key {
    string string_key = 1;            // Java only ever emits string_key (see below)
    int64 int_key = 2;
    bool bool_key = 3;
    InboundEnumValue enum_key = 5;
  }
  InboundValue value = 6;             // absent ⇒ null
}

message InboundValue {
  BamlTy value_type = 1;              // this node's exact type (see the banner above)
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
    BamlTy ty_value = 13;             // NOT YET IMPLEMENTED IN JAVA (no encoder arm; no-op)
  }
  // Absent oneof = null value.
}

message InboundClassValue {
  reserved 1;                        // class identity moved to InboundValue.value_type
  repeated InboundMapEntry fields = 2;
}
```

**The `value_type` annotation (field 1).** Since `ceae8ea6c` (#4087), class
identity lives on the node-level `InboundValue.value_type`, not on the class
payload (`InboundClassValue` field 1 is `reserved`). Under the current contract
`value_type` is the node's exact type, and whether a node carries one is fixed
by its arm: a list, map, class or callable node must state it, and a scalar,
`null`, enum, media, prompt or handle node must not. A statement is a realized
type (no type variables), never a root union or optional. See the banner at the
top and `ABI.md` "Inbound value contract".

The generated binding passes each argument's declared slot to the writer
(`BamlTypedValue`), so the writer states the slot where the slot fixes the
node's type, and the value's own type where the slot is open (`unknown`, a type
variable). Rust decodes the stated graph to `BexExternalValue`, and the engine
builds it and checks it once against the slot, converting nothing.

## Java Encoding Rules

`ProtoWriter.encodeCallFunctionArgs(names, args, callId, typeArgs)` builds a
`CallFunctionArgs`, writing one `InboundMapEntry` per `names[i]`/`args[i]` pair,
then `call_id`, then any `type_args` (`ProtoWriter.java:110-127`). It throws
`IllegalArgumentException` if `names.length != args.length` (`:112-115`). Each
value is written by `encodeInboundValue(...)` (`:200-370`).

The **arm order matters** — `Boolean` is checked before the integer arms
(`ProtoWriter.java:159-161`), preserving the Python `isinstance` order:

| Java runtime value | Inbound proto field | Notes / source |
| --- | --- | --- |
| `null` | absent oneof | `encodeInboundValue(null)` returns empty bytes; the entry omits `value` (`ProtoWriter.java:143-147, 156-158`). Encodes BAML `null`. |
| `Boolean` | `bool_value` (5) | Checked before the integer arms (`:160-161`). |
| `Long` | `int_value` (3) | `:162-163`. |
| `Integer` / `Short` / `Byte` | `int_value` (3) | Widened via `Number.longValue()` (`:164-165`). |
| `BigInteger` (any value) | `bigint_value` (12) | Lowercase base-16 via `toString(16)`, sign-prefixed; matches num-bigint's `{bi:x}`. A `bigint` is never sent as an `int`, since concrete types are atomic; pinned by `WireCodecTest.inbound_bigint_in_range_uses_bigint_channel`. |
| `Double` | `float_value` (4) | `:174`. |
| `Float` | `float_value` (4) | Widened to `double` (`:175-177`). |
| `String` | `string_value` (2) | `:178`. |
| `byte[]` | `uint8array_value` (11) | `:180-181`. |
| `List<?>` | `list_value` (6) | Recursively encodes items; a `null` item still emits an entry to preserve length. Empty list still sets the arm (see presence note). (`:182-183`, `encodeList:305-312`). |
| `Map<?,?>` | `map_value` (7) | Recursively encodes values; keys stringified via `String.valueOf` (`:184-185`, `encodeMap:314-321`). |
| `Enum<?>` (registered) | `enum_value` (9) | `name` = BAML enum FQN, `value` = wire variant from the enum's serializer map (`:186-193`, `encodeEnum:282-287`, `TypeRegistry.enumWire:280-289`). |
| `Enum<?>` (unregistered) | `IllegalArgumentException` | An unregistered enum type is not a BAML value. Rejected via `unsupported()` (an `IllegalArgumentException` subclass), named to the owning argument at the top-level kwarg loop (`:189-192`; pinned by `WireCodecTest.inbound_unregistered_enum_throws`). |
| `BamlMedia` (Image/Audio/Video/Pdf) | `media_value` (15) | The portable media value read out of the wrapper's native handle (kind, MIME type, and its one source: URL, base64, or file content with its name); no `value_type` (`encodeMedia`). |
| bare `BamlHandle` | `handle` (10) | `BamlHandle{key = cloneKeyForWire, handle_type}` (`:200-210`). |
| `BamlUnion` (Union2…Union10 arm record) | *(unwrapped)* | No union envelope inbound. Under a contextual union type the encoder reads the arm index (`genericUnionArmIndex`, the `Arm{n}` record name) and encodes the bare `value()` under that arm's declared type, which a container states (see "The `value_type` annotation" below). With no contextual union it encodes `value()` under no slot. |
| nominal union wrapper record | *(unwrapped, arm type annotated)* | Same rule via `TypeRegistry.isUnionRecord` / `unionRecordArmIndex` → select the contextual arm type, encode the inner value under it (`ProtoWriter.java:212-228, 315-319`, `TypeRegistry.unionRecordInner:361-367`, `unionRecordArmIndex:370-373`). |
| `baml_bridge.BamlStream` (streaming receiver) | `handle` (10) | Delegates to the `BamlHandle` arm: a `handle_value(ADT_TAGGED_HEAP_HANDLE)` over a freshly cloned key (`:226-235`). Landed `a6e3ca99e`. |
| host callable (`Function`/`BiFunction`/`Supplier`/`Consumer`/`BiConsumer`/`Runnable`, or a generated `BamlHostCallable`, in its `BamlTypedCallable` carrier) | `handle` (10) | `HostAdapter.of` decides the Java shape; the encoder registers the carrier in the Java-side registry (`HostCalls.registerCallable`) and emits `handle{key, HOST_VALUE_CALLABLE}` with `value_type` = its function type. A request that never reaches the engine retires the registration (`EncodingScope`). The engine dispatches back through `BamlFfi.hostDispatch` on the daemon executor. |
| `BamlType` (a type value) | `ty_value` (13) | The type itself; each runtime declaration's `live` head is a fresh clone of the row the `BamlType` holds, one per occurrence. A type naming an anonymous declaration is refused. |
| registered generated class | `class_value` (8) | One `fields` entry per registry field (declaration order), value read via the public accessor method. Nominal identity — FQN on `value_type.class_ty.name`, plus any reified `value_type.class_ty.type_args` from the side-table — rides the **enclosing `InboundValue.value_type`**, never the `class_value` payload (`ProtoWriter.java:337-347`, `writeClassType:402-411`, `encodeClass:390-399`). When the class value is a selected union arm, the contextual `CLASS` arm type is used as the `value_type` directly (`:341-342`). |
| unsupported object | `IllegalArgumentException` | Names the offending *argument* and its unsupported Java type: the top-level kwarg loop rewraps to `"cannot encode argument '<name>': unsupported Java type <class>"` (`:128-144, 269-276`, `unsupported`); pinned by `WireCodecTest.inbound_unsupported_type_throws` / `inbound_unsupported_argument_names_the_argument` / `inbound_unsupported_nested_element_names_top_level_argument`. |

> ✅ **Implemented per spec (unsupported object).** Matches Python: an
> unsupported value throws an `IllegalArgumentException` (the analog of Python's
> `TypeError`) whose message names the *top-level kwarg* being encoded plus the
> unsupported Java type — `"cannot encode argument '<name>': unsupported Java
> type <class>"` (`ProtoWriter.java:140-143`; pinned by `WireCodecTest:366,381`:
> `"cannot encode argument 'tool': unsupported Java type java.util.Date"`). The
> value encoder still recurses on values (not entries), so the deep rejection is a
> private `UnsupportedInboundTypeException` (an `IllegalArgumentException`
> subclass carrying the Java type name); the top-level kwarg loop in
> `encodeCallFunctionArgs` catches it and rewraps it to prepend the argument name.
> A value nested inside argument `x` (a list element, a class field) still reports
> `x`, mirroring Python. A *direct* `encodeInboundValue` call (no owning argument)
> surfaces the bare `"unsupported Java type <class>"` message. Landed `eab6d37cc`
> (argument-naming IAE on encode reject).
>
> *History:* previously Java threw `UnsupportedOperationException`
> (`"capability not yet implemented: cannot encode argument of type <class>"`)
> naming only the value's Java class — a different exception type that could not
> reach the owning parameter name. Flipped to the spec'd behavior 2026-07-17.

> ⚠ **Deviation from Python (boxed integer/float widening).** Python has exactly
> one `int` and one `float` type. Java's encoder must fan several boxed types
> into the two numeric arms: `Long`/`Integer`/`Short`/`Byte` → `int_value`
> (`:162-165`) and `Double`/`Float` → `float_value` (`:174-177`). `BigInteger`
> splits by i64 range into `int_value` vs `bigint_value` (`:166-173`), which is
> the closest analog to Python's arbitrary-precision `int` (Python only reaches
> `bigint_value` when the value is outside i64).

> ⚠ **Deviation from Python (class encode source).** Python walks
> `dict(value).items()` on a Pydantic model (deliberately not `model_dump()`) and
> separately walks `__pydantic_private__` for handle-backed private fields. Java
> reads a **fixed, registered field list in declaration order via public
> zero-arg accessor methods** (`ClassEntry.encode:516-528`,
> `TypeRegistry.registerClass:84-108`). A handle-backed `$rust_type` shell
> (`baml.fs.File`, `baml.http.Response`) exposes its handle as a field whose
> accessor returns a `BamlHandle`, which then rides the bare-`handle` arm — the
> Java analog of Python's `__pydantic_private__` handle walk.

> ⚠ **Deviation from Python (inbound generic reification) — on parity
> (landed `861414d55`; wire location moved to `value_type` in `ceae8ea6c`).**
> Python fills a class value's `value_type.class_ty.type_args` inbound from
> `pydantic_instance_type_args` (`proto.py:306-311`). Java mirrors this on the
> **node-level `value_type`**: for a bare (non-arm) class value `writeClassType`
> takes the instance's reified tokens (`TypeRegistry.typeArgsOf(value)`, the
> weak-identity side-table populated by a reified `of(BamlType…, value)` factory
> or an outbound decode) and writes one `value_type.class_ty.type_args` entry per
> token via `BamlType.toWireTy()` (`ProtoWriter.java:344, 402-411`), so a generic
> instance's concrete args reify across the wire on the *value*. A **non-generic
> (or unbound) instance has an empty side-table**, so `writeClassType` writes the
> bare `value_type.class_ty.name` with **no `type_args`** — the generics-reification
> channel adds nothing for it (the regression non-generic callers depend on).
> Note this is *not* byte-identical to the pre-`ceae8ea6c` wire: that migration
> relocated **all** class identity from the removed `InboundClassValue.class_ty`
> onto `InboundValue.value_type.class_ty`, so a non-generic class's bytes changed
> with the relocation even though the generics feature itself contributes nothing.
> Explicit generic *bindings* can still travel via the top-level
> `CallFunctionArgs.type_args` bag (below); the two channels are independent.

> ⚠ **Deviation from Python (map keys).** Python can put string/int/bool/enum
> keys onto `InboundMapEntry` and lets Rust stringify them on decode. Java's
> `encodeMap` **always** emits `string_key`, calling `String.valueOf(key)` on the
> Java side (`ProtoWriter.java:317-318`). Generated maps are `Map<String, V>`, so
> this is normally lossless; a non-`String` Java map key is stringified in the
> JVM rather than carried as a typed key for Rust to stringify.

### The `value_type` annotation — how Java threads it

There is no ambiguity policy: the engine builds exactly the type a node states
and never picks a union member. Every list, map, class and host-callable node
states its exact type in `InboundValue.value_type`; scalars, `null`, enums,
media, prompts, type values and handles state nothing (ABI.md, "Inbound value
contract").

**Where the stated type comes from.** Generated bindings wrap each argument
whose declared type needs it (`needs_inbound_descriptor`: a list, map, union,
literal, generic-class instance, or an alias resolving to one) in
`new baml_bridge.BamlTypedValue(value, <descriptor>)`, and class fields carry
their declared descriptors through `registerClass`. The encoder threads that
slot down the recursion:

- A container under a slot that fixes its type (the slot itself, or the one
  member of its shape in a union, through optionals and recursive aliases)
  states the slot: a plain `List<Long>` under `(int | string)[]` goes out as
  `(int | string)[]`, an empty list under `json` as `json[]`.
- A container under a slot that fixes nothing (no slot, `unknown`, a type
  variable) states its own type: the union of its elements' types. A hole in
  that type, an empty container's element type or a handle's type (which only
  the engine can see), is `unknown` there, and refused under an open type
  variable until the caller names it.
- A union arm record (`Union2.Arm0` …, or a nominal union record) is unwrapped
  and its value encoded under the arm's declared type, so `[]` as `Arm1` of
  `int[] | string[]` states `string[]`.
- A class instance states its own class with every type argument (bound when
  it was decoded, else solved from its fields); a host callable states the
  function type its carrier or slot gives it, an open effect as `unknown`.

### Explicit-generics `type_args` bag

`encodeCallFunctionArgs` takes an optional `BamlTypes typeArgs` bag and, when
non-empty, writes one `BamlTyArg{type_var = 1, type_value = 2}` per binding in
the bag's insertion (De Bruijn) order (`ProtoWriter.java:121-125`,
`encodeTypeArg:130-135`; `BamlTypes.java:22-64`). Each binding's value renders
through `BamlType.toWireTy()` — a `primitive`, `class_ty`, or `enum` arm of the
shared `BamlTy` (`BamlType.java:138-162`). `of(Class)` is bimodal on the wire:
enums lower to `BamlTy.enum`, classes to `BamlTy.class_ty` (`BamlType.java:93-106`),
matching Python's `_fill_wire_ty`.

A **`null` or empty bag writes no `type_args` field**, so the output is
byte-identical to the pre-generics encoding — the regression non-generic callers
depend on (`ProtoWriter.java:121`; pinned by
`WireCodecTest.encode_call_args_without_type_args_is_byte_identical:989-996`,
with the populated cases at `:929-986`).

> ⚠ **Deviation from Python (explicit-generics call site).** Python binds via a
> `_types=` kwarg or subscript (`identity[int](5)`), with partial binding
> allowed. Java has neither kwargs nor subscript: it uses an immutable named
> `BamlTypes` bag — `BamlTypes.of("T", BamlType.INT).and("U", ...)` — passed to a
> trailing generic overload (`BamlTypes.java:9-64`). Call-site strategy DECIDED
> (D3, named bag, minimal token grammar); the readback question is resolved as the
> emitted `bamlTypeArgs()` accessor + reified `of(...)` factory.

> ⚠ **Deviation from Python (emitter surface) — LANDED (`861414d55`).** The
> generic `callSync`/`callAsync` overloads that thread the bag are now **reached
> by generated code**: every generic free function, static factory, and instance
> method emits trailing `BamlTypes` overloads that call the 6-arg
> `callSync/callAsync(fqn, names, args, returnDesc, ctx, types)` (see
> `ref-java-examples.md`, "Generics"; e.g. `identity(x, types)`,
> `GenericBox.new$(value, types)`, `GenericBox.pair_with(other, types)` — the
> instance form guarding on a reified receiver). Non-generic calls still pass the
> four-arg overload with an implicit `null` bag, byte-identical to before.

### call_id

`CallFunctionArgs.call_id` is mandatory and nonzero; the engine rejects `0`
(`ProtoWriter.java:106-108`). Java mints it from the engine counter via
`BamlFfi.nativeNewCallId()` with an `AtomicLong` fallback (`BamlFfi.java:71-72,
473-478`) — the analog of Python's `new_function_call()`. The same `call_id` is
embedded in the encoded args and (for async) passed explicitly to
`nativeCallAsync` so the completion is routed even if the args fail to decode
(`BamlFfi.java:95-105, 297-306`).

### List / map presence (the `SetInParent()` analog)

Python calls `SetInParent()` so empty lists/dicts do not decode as null. Java's
equivalent is structural: `WireWriter.writeMessage` **always** emits the tag +
length prefix (even for a zero-length payload), which marks the oneof arm set
(`WireWriter.java`, `writeMessage`). So `list_value`/`map_value` over an empty
collection still round-trips as an empty container, not null
(`ProtoWriter.java:254-273`).

**Empty container inside a union arm.** Presence alone keeps an empty container
from decoding as null, but it does **not** identify *which* union arm an empty
`[]` selects. When an empty container is a **selected union arm**, the encoder
additionally attaches its exact `value_type` (the arm's declared list/map type)
so the engine binds the host-selected arm rather than the first-declared one — see
"The `value_type` annotation" above. This is the empty-`int[]`-vs-empty-`string[]`
arm-fidelity contract, exercised end to end by the `TestUnions`
`test_round_trip_str_or_int_list` round trip (`Arm0` empty → `Arm0`, `Arm1` empty
→ `Arm1`).

### Host callables and type values

- **Host callables — `handle` with `HOST_VALUE_CALLABLE` (15).** Encode: the
  generated binding wraps the callable in a `BamlTypedCallable` carrying its
  declared function type; the encoder checks its Java shape (`HostAdapter`),
  registers the carrier (`HostCalls.registerCallable`) and emits
  `handle{key, HOST_VALUE_CALLABLE}` stating that type. The registry is one
  `ConcurrentHashMap<Long,Object>` keyspace (callables + raised throwables) in
  `HostCalls`; `BamlFfi` holds the daemon dispatch `ExecutorService` and the
  `hostDispatch` / `hostRelease` / `nativeCompleteHostCall` trampolines. A
  dispatch binds the arguments against the declared parameters
  (`HostCalls.bind`) and completes with one `HostCallCompletion` outcome
  (`HostCalls.returnedCompletion` / `failedCompletion`), per ABI.md's
  completion contract.
- **Type values — `ty_value` (13).** A `BamlType` the engine returned holds one
  reference per runtime declaration it names (`BamlType.adoptWireTy`), and
  sending it back clones each per occurrence (`BamlType.toWireTy`).

### Handle ownership: clone-for-wire + drain contract

Both handle-bearing arms (media `class_value` and the bare `handle`) mint a
**fresh** wire key via `BamlHandle.cloneKeyForWire()` →
`nativeHandleClone(key)` (`ProtoWriter.java:199, 208-209, 258-264`;
`BamlHandle.java:115-122`). The contract (mirrors `bridge_python`'s
`_clone_key_for_wire`): the engine `drain`s its copy of the cloned key on decode,
while the Java `BamlHandle` keeps its own key, so the original media/handle value
stays valid after the call — never sharing a key avoids a double-release
(`BamlHandle.java:16-33`). Release is `Cleaner`-driven (or eager via `close()`),
guarded by a per-instance atomic latch, and skips the `HOST_VALUE_*` keyspaces
(`BamlHandle.java:35-103`).

> **Encode-failure rollback.** Every reference a request takes while it is
> encoded (a handle cloned for the wire, a host callable registered for it) is
> recorded in the active `EncodingScope`; a request that never reaches the
> engine (a later argument refused, the wire's depth exceeded) takes them all
> back. A host callback's result or thrown value is encoded in a scope of its
> own, taken back unless the completion that carries it is delivered.

## Typemap Role

Java's analog of Python's process-global typemap is the static `TypeRegistry`,
populated by the generated `baml_sdk.Baml` anchor's static initializer (before
`initFromBytecode`), one call per user class/enum/union
(`TypeRegistry.java:14-51`):

```java
TypeRegistry.registerClass("user.lorem.Resume", "baml_sdk.lorem.Resume",
                           new String[] {"name", "age"});
TypeRegistry.registerEnum("user.ipsum.Sentiment", "baml_sdk.ipsum.Sentiment",
                          new String[] {"Positive", "new$"},   // Java constants
                          new String[] {"Positive", "new"});   // wire variants
```

For **inbound encode**, `TypeRegistry` maintains a **reverse index keyed by the
registered Java binary class name** (not by a loaded `Class`), so encode resolves
a host object's type without ever forcing `Class.forName`
(`TypeRegistry.java:56-61, 247-289`):

- `classWire(obj)` → FQN + field names + accessor-read field values, or `null`
  when the object's class is not a registered generated class
  (`:271-274, 516-528`).
- `enumWire(constant)` → FQN + wire variant name via the per-enum
  constant→wire serializer map, keyed on `getDeclaringClass()` so a constant with
  a body still resolves (`:280-289, 551-606`).
- `isUnionRecord(obj)` / `unionRecordInner(obj)` → detect and unwrap a generated
  union wrapper record to its bare inner value (`:249-265`).
- `classFqnForJavaClass` / `enumFqnForJavaClass` → the FQN for a `BamlType.of(Class)`
  token lookup, without loading anything (`:300-313`).

An unregistered type returns `null`, letting the value encoder reject it.

> ⚠ **Deviation from Python (stdlib media detection).** Python seeds hardcoded
> reverse typemap overrides for `baml.media.{Image,Audio,Video,Pdf}` and
> `ai.stream.Stream`. Java does **not** register media in `TypeRegistry`; instead
> the media wrapper classes implement the `BamlMedia` marker interface
> (`bamlHandle()` + `bamlFqn()`), and the encoder detects them by
> `instanceof BamlMedia` (`ProtoWriter.java:220-225`, `BamlMedia.java`).
> Handle-backed shells are similarly detected by `instanceof BamlHandle`
> (`ProtoWriter.java:236-246`). `BamlStream` **is** encodable inbound now
> (landed `a6e3ca99e`): an `instanceof baml_bridge.BamlStream` arm
> (`ProtoWriter.java:226-235`) delegates to the `BamlHandle` arm, so a stream
> receiver rides a cloned `handle_value(ADT_TAGGED_HEAP_HANDLE)`.

> ⚠ **Deviation from Python (registration idempotency + laziness).** Registration
> is idempotent (first registration of an FQN wins) and all maps are
> `ConcurrentHashMap`s; `Class` objects and enum constants are resolved lazily via
> `Class.forName` on first *decode* use only, so encode never forces class
> loading (`TypeRegistry.java:44-51, 84-127`).

## Rust Inbound Decode

The CFFI entry point receives the serialized `CallFunctionArgs` bytes, parses
them, and calls `bridge_ctypes::value_decode::kwargs_to_bex_values(...)`. **This
stage is shared with the Python and TypeScript bridges** — the wire is identical,
so the decode table matches the Python doc exactly. The rows Java currently
exercises are marked:

| Inbound proto field | Rust value | Java emits it? |
| --- | --- | --- |
| absent oneof | `BexExternalValue::Null` | yes (null / omitted-value entry) |
| `string_value` | `BexExternalValue::String` | yes |
| `int_value` | `BexExternalValue::Int` | yes |
| `bigint_value` | `BexExternalValue::Bigint`, strict hex with a pre-alloc length cap | yes (out-of-i64 `BigInteger`) |
| `float_value` | `BexExternalValue::Float` | yes |
| `bool_value` | `BexExternalValue::Bool` | yes |
| `uint8array_value` | `BexExternalValue::Uint8Array` | yes (`byte[]`) |
| `list_value` | `BexExternalValue::Array` (recursive) | yes |
| `map_value` | `BexExternalValue::Map` (stringified keys, recursive) | yes |
| `class_value` | `BexExternalValue::Instance { class_name, fields, type_args }` | yes — the `class_value` payload carries only `fields`; the FQN + reified `type_args` ride the enclosing `InboundValue.value_type.class_ty` (`value_decode.rs` carries the annotation via `BexExternalValue::typed`; see deviation above) |
| `enum_value` | `BexExternalValue::Variant { enum_name, variant_name }` | yes |
| `ty_value` | the type, each `live` head followed by handle | yes (`BamlType`) |
| `handle` with `HOST_VALUE_CALLABLE` (15) | `BexExternalValue::HostCallable` with its stated function type; bypasses `HANDLE_TABLE` | yes (host callable — `HostCalls.registerCallable`) |
| `handle` with `HOST_VALUE_OPAQUE` (16) | `BexExternalValue::HostValue`; bypasses `HANDLE_TABLE` | no — a raised exception's registration travels as `HostException.handle` in the `HostCallCompletion`, and the engine builds the `baml.errors.HostCallable` around it |
| other `handle` | drains the key from `HANDLE_TABLE` and converts the entry | yes (media `_data`, bare handle, stream receiver) |

The engine receives the decoded kwargs and the called function FQN. Any missing
required arg, extra arg, or structural type mismatch is an engine-boundary
error, surfaced to Java as a `BamlError` / `BamlPanic` after decode — not an
encoder error.

## Practical Consequences For Bridge Generics

- Generated Java static types are compile-only. Runtime arg encoding is
  structural and Java-value-driven (`ProtoWriter.encodeInboundValue` dispatches
  on runtime shape, `:200-370`).
- The inbound wire payload carries generated objects' identities (not declared
  parameter types), but on **different channels by kind**: a **class** FQN — plus,
  for a reified generic instance, its concrete `type_args` from the side-table —
  rides the node-level `InboundValue.value_type.class_ty` (class identity moved off
  the `class_value` payload in `ceae8ea6c`; `ProtoWriter.java:344, 402-411`), while
  an **enum**'s FQN + variant ride `InboundEnumValue.name`/`value` (**not**
  `value_type`; `encodeEnum`). A non-generic/unbound class instance writes no
  `type_args` — only the bare `value_type.class_ty.name` (the identity itself
  relocated to `value_type` in `ceae8ea6c`, so this is not byte-identical to the
  older `InboundClassValue.class_ty` wire). Selected union arms additionally carry
  the arm's exact `value_type` (empty containers, overlapping arms, literals). The
  engine converts nothing: it checks each built value once against its slot.
- Omit-vs-explicit-null is preserved by the `$Opts` touched-set, not a sentinel:
  a setter never called ⇒ omitted (engine default); a setter called with `null`
  ⇒ an entry with an absent `value` oneof (explicit BAML `null`)
  (`emit.rs:592-707`; `WireCodecTest:222-241`).
- Empty lists and maps require explicit oneof presence; Java gets this for free
  because `WireWriter.writeMessage` always emits the tag + length
  (`WireWriter.java:77-90`).
- Handle-backed values are wire keys with clone-for-wire + drain lifetime rules,
  not ordinary class serialization (`BamlHandle.java:16-33, 115-122`): media
  (`_data`), bare `$rust_type` shells, and the `BamlStream` receiver clone their
  key on the way out; **host callables** register in the Java-side registry and
  ride `handle{HOST_VALUE_CALLABLE}` (no clone — the registry owns them, released
  via `hostRelease`), and an opaque host throwable rides `HOST_VALUE_OPAQUE` on the
  throw-back path.
- **Java explicitly rejects everything it cannot encode** — there is no silent
  drop on the inbound value path; a value the encoder cannot map raises
  `IllegalArgumentException` naming the argument (`ProtoWriter.java:269-276`). Host
  callables and `BamlStream` receivers are **now encoded**, not rejected. The only
  intentionally "silent" behaviors are the designed ones: `null` → absent oneof,
  and an untouched optional → omitted kwarg.

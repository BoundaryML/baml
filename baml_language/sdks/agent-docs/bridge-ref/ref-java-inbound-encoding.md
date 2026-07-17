---
date: 2026-07-17
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
  - baml_language/sdks/java/sdkgen_java/src/emit.rs
  - baml_language/sdks/java/baml_bridge/src/test/java/baml_bridge/WireCodecTest.java
  - baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/baml_inbound.proto
---

# Java Inbound Argument Encoding

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
// free function  return_int(v)  →  baml_sdk.primitives.Fns
public static long return_int(long v) {
    return (java.lang.Long) baml_bridge.BamlFfi.callSync(
        "user.return_int", new String[] {"v"}, new Object[] {v}, "int");
}

// instance method  Box.get_value()  →  baml_sdk.lorem.Box
public long get_value() {
    return (java.lang.Long) baml_bridge.BamlFfi.callSync(
        "user.Box.get_value", new String[] {"self"}, new Object[] {this}, "int");
}
```

Each binding therefore carries two baked literals — `names[]` (the declared
parameter names, receiver first for instance methods) and `args[]` (the boxed
values, `this` first for instance methods) — plus a return-type descriptor
string (`emit.rs:411-432`, `render_method_pair` `emit.rs:575-585`).

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
```

As in Python, `InboundClassValue` has no flat `name` field (field 1 is reserved);
the class FQN lives on `class_ty.name`, with any reified generics on
`class_ty.type_args` (`ProtoWriter.java:32-36`).

Inbound values do not carry declared BAML parameter types. **The Java encoder
dispatches on the Java runtime shape of each argument — never on the declared
BAML parameter type** (`ProtoWriter.java:11-17, 154-231`), exactly like Python's
`_set_inbound_value`. Rust decodes to `BexExternalValue` and the engine re-runs
BAML validation after deserialization.

## Java Encoding Rules

`ProtoWriter.encodeCallFunctionArgs(names, args, callId, typeArgs)` builds a
`CallFunctionArgs`, writing one `InboundMapEntry` per `names[i]`/`args[i]` pair,
then `call_id`, then any `type_args` (`ProtoWriter.java:110-127`). It throws
`IllegalArgumentException` if `names.length != args.length` (`:112-115`). Each
value is written by `encodeInboundValue(...)` (`:154-231`).

The **arm order matters** — `Boolean` is checked before the integer arms
(`ProtoWriter.java:159-161`), preserving the Python `isinstance` order:

| Java runtime value | Inbound proto field | Notes / source |
| --- | --- | --- |
| `null` | absent oneof | `encodeInboundValue(null)` returns empty bytes; the entry omits `value` (`ProtoWriter.java:143-147, 156-158`). Encodes BAML `null`. |
| `Boolean` | `bool_value` (5) | Checked before the integer arms (`:160-161`). |
| `Long` | `int_value` (3) | `:162-163`. |
| `Integer` / `Short` / `Byte` | `int_value` (3) | Widened via `Number.longValue()` (`:164-165`). |
| `BigInteger` within signed i64 | `int_value` (3) | `:166-170`; pinned by `WireCodecTest.inbound_bigint_in_range_uses_int_channel:244-249`. |
| `BigInteger` outside signed i64 | `bigint_value` (12) | Lowercase base-16 via `toString(16)`, sign-prefixed; matches num-bigint's `{bi:x}` (`:171-173`). |
| `Double` | `float_value` (4) | `:174`. |
| `Float` | `float_value` (4) | Widened to `double` (`:175-177`). |
| `String` | `string_value` (2) | `:178`. |
| `byte[]` | `uint8array_value` (11) | `:180-181`. |
| `List<?>` | `list_value` (6) | Recursively encodes items; a `null` item still emits an entry to preserve length. Empty list still sets the arm (see presence note). (`:182-183`, `encodeList:305-312`). |
| `Map<?,?>` | `map_value` (7) | Recursively encodes values; keys stringified via `String.valueOf` (`:184-185`, `encodeMap:314-321`). |
| `Enum<?>` (registered) | `enum_value` (9) | `name` = BAML enum FQN, `value` = wire variant from the enum's serializer map (`:186-193`, `encodeEnum:282-287`, `TypeRegistry.enumWire:280-289`). |
| `Enum<?>` (unregistered) | `IllegalArgumentException` | An unregistered enum type is not a BAML value. Rejected via `unsupported()` (an `IllegalArgumentException` subclass), named to the owning argument at the top-level kwarg loop (`:189-192`; pinned by `WireCodecTest.inbound_unregistered_enum_throws`). |
| `BamlMedia` (Image/Audio/Video/Pdf) | `class_value` (8) | Single `_data` field = `handle{cloneKeyForWire, handleType}`, stdlib FQN on `class_ty.name` (`:194-199`, `encodeMediaClass:258-279`). |
| bare `BamlHandle` | `handle` (10) | `BamlHandle{key = cloneKeyForWire, handle_type}` (`:200-210`). |
| `BamlUnion` (Union2…Union10 arm record) | *(unwrapped)* | Unwrapped to its `value()` component and re-encoded bare — no union envelope inbound (`:211-214`, `unwrapGenericUnion:296-303`). |
| nominal union wrapper record | *(unwrapped)* | `TypeRegistry.isUnionRecord` → unwrap to inner value, encode bare (`:215-219`, `TypeRegistry.unionRecordInner:259-265`). |
| registered generated class | `class_value` (8) | One `fields` entry per registry field (declaration order), value read via the public accessor method; FQN on `class_ty.name` (`:220-228`, `encodeClass:239-248`, `ClassEntry.encode:516-528`). |
| non-class callable (`Function`/lambda/…) | **NOT YET IMPLEMENTED IN JAVA** | Python emits `handle` with `HOST_VALUE_CALLABLE`. Java has no callable arm — such an arg falls through to the class lookup, resolves `null`, and is rejected (`:220-228`). See "Not yet encoded" below. |
| unsupported object | `IllegalArgumentException` | Names the offending *argument* and its unsupported Java type, e.g. `"argument 'tool' has unsupported Java type java.util.Date"` (`:220-229`, `unsupported`); pinned by `WireCodecTest.inbound_unsupported_type_throws` / `inbound_unsupported_argument_names_the_argument` / `inbound_unsupported_nested_element_names_top_level_argument`. |

> ✅ **Implemented per spec (unsupported object).** Matches Python: an
> unsupported value throws an `IllegalArgumentException` (the analog of Python's
> `TypeError`) whose message names the *top-level kwarg* being encoded plus the
> unsupported Java type — `"argument '<name>' has unsupported Java type
> <class>"`. The value encoder still recurses on values (not entries), so the
> deep rejection is a private `UnsupportedInboundTypeException` (an
> `IllegalArgumentException` subclass carrying the Java type name); the top-level
> kwarg loop in `encodeCallFunctionArgs` catches it and rewraps it to prepend the
> argument name. A value nested inside argument `x` (a list element, a class
> field) still reports `x`, mirroring Python. A *direct* `encodeInboundValue`
> call (no owning argument) surfaces the bare `"unsupported Java type <class>"`
> message.
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

> ⚠ **Deviation from Python (inbound generic reification).** Python fills a class
> value's `class_ty.type_args` inbound from `pydantic_instance_type_args`, so a
> generic instance's concrete args are reified across the wire on the *value*.
> Java's `encodeClass` **omits** `type_args` on inbound class values — "they
> reify later" (`ProtoWriter.java:236-237`), because the generated instance has
> no field to hold them yet (the reified-args side-table is decode-only,
> `TypeRegistry.bindTypeArgs/typeArgsOf:315-362`). Inbound, generic bindings
> travel **only** via the top-level `CallFunctionArgs.type_args` bag (below),
> never per class value.

> ⚠ **Deviation from Python (map keys).** Python can put string/int/bool/enum
> keys onto `InboundMapEntry` and lets Rust stringify them on decode. Java's
> `encodeMap` **always** emits `string_key`, calling `String.valueOf(key)` on the
> Java side (`ProtoWriter.java:317-318`). Generated maps are `Map<String, V>`, so
> this is normally lossless; a non-`String` Java map key is stringified in the
> JVM rather than carried as a typed key for Rust to stringify.

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
> (D3, named bag, minimal token grammar); readback naming and the
> trailing-overload matrix are **STILL OPEN**
> (`java-function-calls-decisions.md:25`).

> ⚠ **Deviation from Python (emitter surface not wired).** The generic
> `callSync`/`callAsync` overloads that thread the bag are **package-private and
> reached by no generated code yet** — the explicit-generics emitter surface is
> deferred (`BamlFfi.java:204-219, 282-296`). The runtime substrate (encode +
> decode side-table) landed; codegen has not. Non-generic calls pass the four-arg
> overload with an implicit `null` bag.

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
(`WireWriter.java:77-90`). So `list_value`/`map_value` over an empty
collection still round-trips as an empty container, not null
(`ProtoWriter.java:182-185`).

### Not yet encoded (host callables, `ty_value`)

- **Host callables — `handle` with `HOST_VALUE_CALLABLE` (15):**
  **NOT YET IMPLEMENTED IN JAVA.** Python registers a non-class callable in the
  host-value registry and emits `Handle{key, HOST_VALUE_CALLABLE}`. Java's
  encoder has no callable branch — a `Function`/lambda arg falls into the "registered
  class?" lookup, resolves `null`, and is rejected with
  `IllegalArgumentException` naming the argument (`ProtoWriter.java:220-229`). **Status: design
  scouted, decisions recommended, not built** — the whole host-callable slice
  (Rust JNI dispatch/release trampolines, a Java-side
  `ConcurrentHashMap<Long,Object>` registry + `AtomicLong`, the `ProtoWriter`
  callable branch with encode-failure rollback, `ProtoReader.BamlToHostCall`
  decode, the invoke ladder, and codegen for optional-args callable interfaces)
  is enumerated in `java-function-calls-decisions.md:57-360` (slice 4). The
  wire constant is already defined: `BamlHandle.HOST_VALUE_CALLABLE = 15`,
  `HOST_VALUE_OPAQUE = 16` (`BamlHandle.java:48-49`), and the release path
  already skips these keyspaces (`BamlHandle.java:78-81`).
- **`ty_value` (13):** **NOT YET IMPLEMENTED IN JAVA** — and a **no-op parity**
  with Python: no Python encoder branch emits it either (`ProtoWriter.java:29`;
  Python doc row for `ty_value`). Rust can decode it, but neither host encoder
  produces it. No decision pending.

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

> ⚠ **Deviation from Python (encode-failure rollback).** Python's
> `encode_call_args` has a rollback path: if encoding a later kwarg fails after
> an earlier kwarg registered a host callable, it releases every callable key
> registered during that failed encode (the engine never received the payload).
> Java has **no encode-rollback path yet** (it lands with the host-callable slice
> 4c, `java-function-calls-decisions.md:342-347`). Note a related untested edge:
> the media/bare-handle arms mint their cloned wire key eagerly
> (`ProtoWriter.java:199, 208-209, 261-264`), so if a *later* kwarg's encode
> throws, those cloned engine rows are orphaned (never sent, so never drained) —
> there is no rollback for them today.

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
> `baml.llm.Stream`. Java does **not** register media in `TypeRegistry`; instead
> the media wrapper classes implement the `BamlMedia` marker interface
> (`bamlHandle()` + `bamlFqn()`), and the encoder detects them by
> `instanceof BamlMedia` (`ProtoWriter.java:194-199`, `BamlMedia.java:1-23`).
> Handle-backed shells are similarly detected by `instanceof BamlHandle`
> (`ProtoWriter.java:200-210`). `BamlStream` is **not** encodable inbound today
> (see below).

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
| `class_value` | `BexExternalValue::Instance { class_name, fields, type_args }` | yes (FQN only; no per-value `type_args` — see deviation above) |
| `enum_value` | `BexExternalValue::Variant { enum_name, variant_name }` | yes |
| `ty_value` | `proto_ty_to_external(...)` — decodable | **no** (no Java encoder arm) |
| `handle` with `HOST_VALUE_CALLABLE`/`HOST_VALUE_OPAQUE` | `BexExternalValue::HostValue`; bypasses `HANDLE_TABLE` | **no** (host-callable slice deferred) |
| other `handle` | drains the key from `HANDLE_TABLE` and converts the entry | yes (media `_data`, bare handle) |

The engine receives the decoded kwargs and the called function FQN. Any missing
required arg, extra arg, or structural type mismatch is an engine-boundary
error, surfaced to Java as a `BamlError` / `BamlPanic` after decode — not an
encoder error.

## Practical Consequences For Bridge Generics

- Generated Java static types are compile-only. Runtime arg encoding is
  structural and Java-value-driven (`ProtoWriter.encodeInboundValue` dispatches
  on runtime shape, `:154-231`).
- The inbound wire payload carries class/enum FQNs for generated objects, but not
  declared parameter types, and — unlike Python — **not** per-class-value reified
  generics (those are omitted inbound and reified only on decode;
  `ProtoWriter.java:236-237`). Rust and the engine own coercion and BAML type
  validation.
- Omit-vs-explicit-null is preserved by the `$Opts` touched-set, not a sentinel:
  a setter never called ⇒ omitted (engine default); a setter called with `null`
  ⇒ an entry with an absent `value` oneof (explicit BAML `null`)
  (`emit.rs:592-707`; `WireCodecTest:222-241`).
- Empty lists and maps require explicit oneof presence; Java gets this for free
  because `WireWriter.writeMessage` always emits the tag + length
  (`WireWriter.java:77-90`).
- Handle-backed values (media today; host callables/opaques later) are wire keys
  with clone-for-wire + drain lifetime rules, not ordinary class serialization
  (`BamlHandle.java:16-33, 115-122`).
- **Java explicitly rejects everything it cannot encode** — there is no silent
  drop on the inbound value path. Host callables and inbound `BamlStream` args
  both raise `IllegalArgumentException` (naming the argument) rather than being
  silently omitted (`ProtoWriter.java:220-229`). The only intentionally "silent" behaviors are the
  designed ones: `null` → absent oneof, and an untouched optional → omitted kwarg.

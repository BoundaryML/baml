---
date: 2026-07-23
repository: baml4
source_paths:
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/internal/ProtoReader.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/internal/WireReader.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/internal/BamlTraceback.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlFfi.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlError.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlPanic.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlCancelledError.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlCallContext.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlType.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlHandle.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/TypeRegistry.java
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_sdk/baml/media/Image.java
  - baml_language/sdks/java/baml_bridge/src/test/java/baml_bridge/WireCodecTest.java
  - baml_language/sdks/java/baml_bridge/src/test/java/baml_bridge/ErrorMappingTest.java
  - baml_language/sdks/java/baml_bridge/src/test/java/baml_bridge/BamlCancelledErrorTest.java
  - baml_language/sdks/java/baml_bridge/src/test/java/baml_bridge/BamlCallContextTest.java
mirrors: baml_language/sdks/agent-docs/bridge-ref/ref-python-outbound-decoding.md
---

# Java Outbound Return Decoding

This file records how the runtime bridge decodes values returned from the BAML
engine back into generated Java SDK objects. It is the **1:1 Java mirror of**
`ref-python-outbound-decoding.md` and keeps the same section structure, order,
and heading names (Python-specific names are adapted to their Java analogs:
`decode_call_result` → `ProtoReader.decodeOutboundResult`, `BamlTypeMap` →
`TypeRegistry`, `BamlPyHandle` → `BamlHandle`, and so on).

**Conventions used in this doc:**

- `> ⚠ **Deviation from Python:** …` flags every point where the Java bridge
  behaves differently from the Python reference. These are the load-bearing
  cells for the side-by-side decision review.
- `**NOT YET IMPLEMENTED IN JAVA**` marks a capability the Python bridge decodes
  today but the Java bridge does not yet, with the decided/open status from
  `thoughts/antonio/java-function-calls-decisions.md`.
- Every behavior is cited to Java `file:line`. Nothing here is invented — it is
  drawn from the decoder source and the offline codec/error tests that pin it
  (`WireCodecTest`, `ErrorMappingTest`, `BamlCancelledErrorTest`,
  `BamlCallContextTest`).

The important implementation fact is that the runtime bridge is implemented in
`baml_language/sdks/java/baml_bridge` (the distributed Maven coordinate is
`com.boundaryml:baml-bridge`). Generated SDK packages call functions through
`baml_bridge.BamlFfi.callSync(...)` / `callAsync(...)`; the generated package
itself does not implement outbound result decoding or error-envelope handling.

> ⚠ **Deviation from Python:** the entire wire codec is hand-rolled Java
> (`ProtoReader` / `WireReader`), not a generated protobuf runtime. Python
> decodes through the compiled protobuf module (`holder.WhichOneof("value")`);
> Java walks raw tags with `WireReader.readTag()` / `fieldOf()` / `wireOf()` and
> a last-wins oneof loop (`ProtoReader.java:337-374`). Unknown fields are
> skipped (`r.skipField(wire)`) rather than surfaced.

## Return Path Overview

After a generated callable sends encoded args to the JNI/Rust runtime
(`bridge_java`, the JVM analog of `bridge_python`'s PyO3 module):

1. Rust executes the BAML function.
2. Rust converts the resulting `BexExternalValue` to `BamlOutboundValue` with
   `bridge_ctypes::value_encode::external_to_outbound(...)`.
3. The top-level value, thrown error, or panic is wrapped in a
   `BamlOutboundResult` envelope.
4. Java receives the serialized envelope bytes — for a sync call as the return
   of `nativeCallSync` (`BamlFfi.java:93`), for an async call delivered later to
   `completeCall(callId, bytes)` on an engine thread (`BamlFfi.java:452-457`).
5. `ProtoReader.decodeOutboundResult(bytes, returnDesc)` decodes the envelope to
   a Java value or throws a Java exception (`ProtoReader.java:188-221`).

Sync and async generated functions share the same outbound decoder:
`BamlFfi.decodeResult(response, returnDesc)` funnels both through
`ProtoReader.decodeOutboundResult`, "factored so the sync and async paths cannot
diverge in how they interpret an identical envelope" (`BamlFfi.java:459-471`).

> ⚠ **Deviation from Python:** Java threads a **type-directed return
> descriptor** (`returnDesc`, a typed `baml_bridge.BamlType`) alongside the
> envelope bytes. The generated binding passes a `BamlType` data structure for
> its declared return type (built with the public builders — `BamlType.union(…)`,
> `BamlType.list(…)`, `BamlType.classByFqn("…")`, …); a `null` descriptor is the
> pre-descriptor, wire-driven behavior. Python has no such channel — "the
> caller's Python return annotation does not drive runtime decoding." This
> descriptor is what lands a union result on the generic `Union{k}.Arm{i}` family
> and reifies nested class/list/map fields against the declared shape. See
> **Decoder Implementation Details**.

## Outbound Result Envelope

Top-level function returns use `baml_outbound.proto` (field numbers pinned in
`ProtoReader.java:49-60`):

```proto
message BamlOutboundResult {
  oneof result {
    BamlOutboundValue ok = 1;
    BamlOutboundError error = 2;
    BamlOutboundPanic panic = 3;
  }
}

message BamlOutboundError {
  BamlOutboundValue value = 1;
  repeated string trace = 2;
}

message BamlOutboundPanic {
  BamlOutboundValue value = 1;
  repeated string trace = 2;
  bool is_exit_panic = 3;
  int64 exit_code = 4;
}
```

`decodeOutboundResult(data, returnDesc)` parses that envelope
(`ProtoReader.java:188-221`); the arm is resolved last-wins, then dispatched:

| Envelope arm | Java behavior |
| --- | --- |
| `ok` (field 1) | Decodes with `decodeWithDesc(okBytes, returnDesc, lenient=false)` — `returnDesc` is a typed `BamlType`, no parse step — and returns it. |
| absent oneof | Returns `null` (an all-default envelope is a null `ok`; `ProtoReader.java:219`). Pinned by `WireCodecTest.decode_ok_absent_oneof_is_null`. |
| `error` (`baml.errors.TypeMismatch`) | Throws a native `IllegalArgumentException` (message from the decoded value, BAML trace spliced in), **not** a `BamlError` (`ProtoReader.java:248-251`). |
| `error` (other) | Decodes `error.value` and throws `BamlError(value, trace, className)`, with BAML frames spliced onto the stack (`ProtoReader.java:252`). |
| `panic` with `is_exit_panic` | run registered flush hooks (`BamlFfi.runExitFlushHooks`), then `Runtime.getRuntime().halt(exit_code)` — hard process termination, bypassing JVM shutdown hooks (`ProtoReader.java:313-319`). |
| other `panic` | Decodes `panic.value` and throws `BamlPanic(value, trace, className)`, frames spliced (`ProtoReader.java:321-323`). |

> ⚠ **Deviation from Python:** `baml.errors.TypeMismatch` remaps to
> **`IllegalArgumentException`** where Python raises native **`TypeError`**
> (`ProtoReader.java:248-251`, constant `TYPE_MISMATCH_CLASS` at `:256`). The
> remap message is the value's `message` field, resolved by a ladder that
> mirrors Python's `getattr(decoded, "message", None)` → dict lookup →
> `str(decoded)`:
>
> ```java
> if (TYPE_MISMATCH_CLASS.equals(className)) {
>     return BamlTraceback.splice(
>             new IllegalArgumentException(typeMismatchMessage(value)), trace);
> }
> return BamlTraceback.splice(new BamlError(value, trace, className), trace);
> ```
>
> `typeMismatchMessage` (`ProtoReader.java:266-293`) reads a `Map`'s `"message"`
> entry (unregistered-FQN fallback) or reflectively invokes a zero-arg
> `message()` accessor (registered generated class) — the runtime library can't
> statically reference the generated `baml.errors.TypeMismatch` class. Pinned by
> `ErrorMappingTest.type_mismatch_unregistered_remaps_to_iae_with_map_message`
> and `…_registered_instance_message_via_accessor`.

> ⚠ **Deviation from Python:** `BamlPanic extends Error` (`BamlPanic.java:20`),
> the JVM analog of Python's `BamlPanic` subclassing **`BaseException`** rather
> than `Exception`. A bare `catch (Exception)` (Java's `except Exception`) does
> **not** swallow a panic; callers that want to intercept one catch `BamlPanic`
> or `Throwable` explicitly. `decodePanic` therefore returns `Error`
> (`ProtoReader.java:295,323`). Pinned by
> `ErrorMappingTest.baml_panic_is_error_not_exception`. `BamlError` itself stays
> a `RuntimeException` (`BamlError.java:19`), matching Python's `Exception`
> subclass.

> ⚠ **Deviation from Python:** the BAML trace is **synthesized into real
> `StackTraceElement`s** and prepended to the exception's own stack via
> `BamlTraceback.splice(exc, trace)` (`BamlTraceback.java:56-83`), so
> `printStackTrace()` renders `.baml` source frames inline as ordinary
> `at ns.fn(src.baml:N)` lines. Python instead splices a synthetic **frame-object
> traceback** onto `__traceback__`. Both are best-effort (a parse failure leaves
> the native stack untouched) and both keep the raw wire lines reachable via
> `baml_trace()`. A dotted BAML function name splits into declaring-class =
> namespace / method = leaf; a bare name uses the `<baml>` sentinel
> (`BamlTraceback.java:110-120`). Wire order is most-recent-call-last, so frames
> are reversed to put the throwing function on top (`:71`). Pinned across
> `ErrorMappingTest.splice_*`.

Cancellation (async only) is handled one layer up in
`BamlFfi.callAsync`'s `whenComplete` hook via `mapAsyncFailure(t)`
(`BamlFfi.java:322-336, 428-439`), **not** inside
`ProtoReader.decodeOutboundResult`: an engine `baml.panics.Cancelled` panic is
remapped from `BamlPanic` to `BamlCancelledError` (a `CancellationException`
subclass, `BamlCancelledError.java:26`) so the future reads as cancelled; a
caller-side `future.cancel(true)` fires `nativeCancelFunctionCall(callId)` then
completes with a raw `CancellationException` (`BamlFfi.java:365-369`). Sync calls
have no async-remap path — a sync cancellation keeps the raw `BamlPanic`
carrying a `baml.panics.Cancelled` value (`BamlFfi.java:191-197`). See **Return
Path Overview → Cancellation** detail under **Decoder Implementation Details**.

> ⚠ **Deviation from Python — LANDED (`202883518`):** the same-host **exception
> rehydration path** for `baml.errors.HostCallable` (`_try_rehydrate_host_value`
> in Python, which re-raises the *original* native exception object by looking up
> its `_handle` in the host-value registry) is now implemented in Java. On the
> `error` arm, when the decoded value's class FQN is `baml.errors.HostCallable`
> (`HOST_CALLABLE_CLASS`, `ProtoReader.java:119`), `decodeError` calls
> `rehydrateHostThrowable(value)` (`ProtoReader.java:361-368`): it reads the
> hidden `_handle` (a `HOST_VALUE_OPAQUE` `BamlHandle`; `hostOpaqueHandle`
> reflectively invokes the registered class's `_handle()` accessor, or a `Map`
> fallback, `:377-390`), looks the key up in the Java-side registry via
> `BamlFfi.lookupHostValue(key)` (`:366`), and — on a same-runtime hit —
> re-throws the **original** `Throwable` *unwrapped* through `sneakyThrow`
> (`:265, 400-403`) so `assertSame` holds for any `Throwable` kind (checked,
> unchecked, `Error`). A foreign/released key (`lookupHostValue` returns `null`
> or a non-`Throwable`) falls through to the metadata `BamlError`. The mirror is
> `bridge_python`'s `_try_rehydrate_host_value` (proto.py). The `outboundClassFqn`
> peel (below) still gates the `class_name`. **Forced divergence (recorded):** the
> engine requires the `HostCallable` traceback field **present**, so the Java
> inbound encoder always synthesizes one (Python's is conditionally-always via
> `__traceback__`).

Java already mirrors Python's `_unwrap_union_variant` peel for the `class_name`
gating: `outboundClassFqn(valueBytes)` (`ProtoReader.java:1501-1517`) unwraps a
`union_variant_value` wrapper before reading the class FQN, so a union-typed
`throws` still surfaces a `class_name` on `BamlError` / `BamlPanic`.

## Outbound Proto Shape

Java decodes `BamlOutboundValue` messages; the oneof field numbers are pinned in
`ProtoReader.java:62-79`:

```proto
message BamlOutboundValue {
  oneof value {
    BamlValueNull null_value = 2;
    string string_value = 3;
    int64 int_value = 4;
    double float_value = 5;
    bool bool_value = 6;
    BamlValueClass class_value = 7;
    BamlValueEnum enum_value = 8;
    BamlLiteralValue literal_value = 9;
    BamlValueList list_value = 11;
    BamlValueMap map_value = 12;
    BamlValueUnionVariant union_variant_value = 13;
    BamlOutboundHandle handle_value = 16;
    BamlValueMedia media_value = 17;   // no decode arm → rejected (see below)
    BamlValuePromptAst prompt_ast_value = 18; // no decode arm → rejected
    bytes uint8array_value = 19;
    string bigint_value = 20;
    BamlTy ty_value = 21;              // no decode arm → rejected
  }
}
```

Unlike inbound arguments, outbound values are type-rich: class and enum values
carry BAML FQNs, handles carry discriminator tags, lists/maps carry type
metadata, and class/instance values carry concrete `type_args` for reified
generics. The nested `BamlTy` / union / handle sub-message field numbers Java
reads are pinned in `ProtoReader.java:88-161`.

## Java Value Decoding Rules

`decodeValue(WireReader r, boolean lenient)` decodes a `BamlOutboundValue`
(`ProtoReader.java:337-374`). The `decodeWithDesc(...)` wrapper
(`ProtoReader.java:936-962`) drives the same arms against a return/field
descriptor when one is present, and otherwise delegates straight to
`decodeValue`.

| Outbound proto field | Java value |
| --- | --- |
| absent oneof / `null_value` (2) | `null` (`:344-347`) |
| `string_value` (3) | `String` (`:348`) |
| `int_value` (4) | `Long` (varint; `:349`) |
| `bigint_value` (20) | `java.math.BigInteger`, parsed from hex, radix 16 (`:357`) |
| `float_value` (5) | `Double` (`:350`) |
| `bool_value` (6) | `Boolean` (`:351`) |
| `uint8array_value` (19) | `byte[]` (`:356`) |
| `literal_value` (9) | Inner Java literal (`String`, `Long`, `Boolean`, `Double`, or `BigInteger`); literal wrapper discarded (`:352`, `decodeLiteral` `:376-392`) |
| `list_value.items[]` (11) | `java.util.ArrayList<Object>` with recursively decoded items (`:353`, `decodeList` `:394-407`) |
| `map_value.entries[]` (12) | `java.util.LinkedHashMap<String,Object>` with recursively decoded values (`:354`, `decodeMap` `:409-438`) |
| `class_value` (7) | Generated value-class instance, runtime media wrapper, or field `Map` fallback (`:358`, `decodeClass` `:789-820`) |
| `enum_value` (8) | Generated enum constant, or raw variant `String` fallback (`:359`, `decodeEnum` `:852-867`) |
| `union_variant_value` (13) | Generated union wrapper record (arm chosen by canonical `selected_option_index`, else structurally), generic `Union{k}.Arm{i}`, or bare decoded inner value (`:355`, `decodeUnionVariant` `:605-660`) |
| `handle_value` (16) | Media wrapper or bare `BamlHandle`, by `handle_type` (`:360`, `decodeHandle` `:878-899`) |
| `media_value` (17) / `prompt_ast_value` (18) / `ty_value` (21) | On the `ok` path (`lenient=false`): throws `UnsupportedOperationException`. On the `error`/`panic` path (`lenient=true`): degrades to `null` (`:361-369`). |

> ⚠ **Deviation from Python — primitive box types:** Java scalars decode to the
> boxed reference types `Long` / `Double` / `Boolean` (`int_value` is always a
> `Long`, never an `int`), where Python yields `int` / `float` / `bool`. The
> value table in `ref-java-state-of-completeness.md` records the `long/Long`,
> `double/Double`, `boolean/Boolean` mapping.

> ⚠ **Deviation from Python — the `lenient` split:** Java's decoder carries a
> `lenient` flag with no Python analog (`ProtoReader.java:337`). On the `ok`
> path it is `false`, so an unhandled capability (`media_value`,
> `prompt_ast_value`, `ty_value`) throws `UnsupportedOperationException`
> (`unsupported(kindName(field))`, `:1549-1561`). On the `error`/`panic` path it
> is `true` (`decodeError`/`decodePanic` call `decodeValue(..., true)` at
> `:238,322`), so those same arms degrade to `null` — a thrown `baml.errors.*`
> value that happens to embed one still surfaces rather than masking the real
> error. Python has no split: `decode_value` unconditionally raises `BamlError`
> for `media_value` / `prompt_ast_value` in every context.

> ⚠ **Deviation from Python — `media_value` (17) / `prompt_ast_value` (18):**
> where Python raises `BamlError` ("the Python FFI path expects these to arrive
> through `handle_value`"), Java throws **`UnsupportedOperationException`** on
> the `ok` path (different exception type) and returns `null` on the error path.
> **NOT YET IMPLEMENTED IN JAVA** as first-class values — but this is treated as
> bridge drift on both sides: media is expected to arrive via `handle_value`
> (see **Handles**), so inline `media_value` should not normally occur.
> **Status:** inline `media_value` (field 17) is out of the current value slice;
> `type_shapes` exercises media exclusively through `handle_value`, which is
> green 9/9.

> ⚠ **Deviation from Python — `ty_value` (21):** Python has "no `decode_value`
> arm → `None`" (falls through to its default `return None`); Java routes
> `ty_value` into the same reject arm as media/prompt-ast, so it **throws
> `UnsupportedOperationException` on the `ok` path** and returns `null` only on
> the error path (`ProtoReader.java:361-369`). So an `ok` value that is a bare
> BAML type-reference is an error in Java but a silent `None` in Python.

> ✔ **Deviation from Python — CLOSED (bigint pre-allocation length cap):**
> Python parses `bigint_value` "from strict hex with a pre-allocation length
> cap" (`_parse_hex_bigint`, `_MAX_BIGINT_HEX_LEN = (1 << 28) // 4 + 2`). Java
> now mirrors this exactly via `ProtoReader.parseHexBigInt`
> (`ProtoReader.java:1574`), gated on `MAX_BIGINT_HEX_LEN = (1 << 28) / 4 + 2`
> (`ProtoReader.java:1562`) — byte-for-byte the Rust `MAX_BIGINT_HEX_LEN`
> (`bridge_ctypes/src/value_decode.rs`) and the Python/TypeScript caps. All
> three wire read sites route through it: the value channel
> (`ProtoReader.java:357`), the literal channel (`:386`), and the
> union-arm token channel (`:757`). An over-cap hex blob is rejected *before*
> the `BigInteger` is built; strict-hex validation (single leading `-` only —
> no `0x`, `+`, underscores, or whitespace) matches the encoders and the other
> bridges. **Reject path deviation (intentional):** where Python raises
> `ValueError` and Rust `CtypesError::InvalidBigint`, Java throws
> `IllegalStateException` — the malformed-wire failure mode the rest of this
> codec uses (`WireReader`: "truncated varint", "malformed varint", …), rather
> than a bare `NumberFormatException`. Covered offline by `WireCodecTest`
> (at-cap passes, over-cap rejects, malformed hex rejects, both read sites).

The caller's Java return type does not, by itself, drive runtime decoding: the
generated binding passes an explicit descriptor (a typed `baml_bridge.BamlType`).
Decoding is driven by the outbound wire payload plus that descriptor plus the
installed `TypeRegistry`.

## Decoder Implementation Details

`decodeValue` (`ProtoReader.java:337-374`) is a direct last-wins oneof
dispatcher. Like Python's `decode_value`, the wire-driven form receives no
expected return type — only the outbound value and the `lenient` flag:

```java
public static Object decodeValue(WireReader r, boolean lenient) {
    Object result = null;
    while (r.hasRemaining()) {
        int tag = r.readTag();
        int field = WireReader.fieldOf(tag);
        int wire = WireReader.wireOf(tag);
        switch (field) {
            case OV_NULL -> { r.skipField(wire); result = null; }
            case OV_STRING -> result = r.readString();
            case OV_INT -> result = r.readVarint();
            // … float / bool / literal / list / map / union / bytes / bigint …
            case OV_CLASS -> result = decodeClass(r.readMessage(), lenient);
            case OV_ENUM -> result = decodeEnum(r.readMessage());
            case OV_HANDLE -> result = decodeHandle(r.readMessage());
            case OV_MEDIA, OV_PROMPT_AST, OV_TY -> { /* lenient→null else throw */ }
            default -> r.skipField(wire);
        }
    }
    return result;
}
```

### Type-directed decode (the Java-specific overlay)

> ⚠ **Deviation from Python:** Java has an entire descriptor-driven decode path
> with no Python counterpart. A generated binding passes a typed
> `baml_bridge.BamlType` for its declared return type — a **data structure, not a
> parsed string** (there is no `Desc`/`parseDesc`; the descriptor is the
> `BamlType` itself); `decodeWithDesc` dispatches on `desc.kind()`:
>
> - `LIST` → `decodeListWithDesc` (recurses element decode through `desc.listItem()`)
> - `MAP` → `decodeMapWithDesc` (recurses value decode through `desc.mapValue()`)
> - `CLASS` / `ENUM` (a named type by FQN) → `decodeFqnWithDesc`
> - `UNION` → `decodeUnionWithDesc` (matches the wire value against the arms
>   structurally via `armMatchesValue`, in declaration order)
> - a primitive / literal / `TYPEVAR` / `UNKNOWN` descriptor → falls straight back
>   to the wire-driven `decodeValue`
>
> A `null` descriptor is exactly the pre-descriptor wire-driven behavior, so the
> three-arg `callSync` / one-arg `decodeOutboundResult` overloads keep Python's
> shape. Pinned by `WireCodecTest.decode_desc_*` and the regressions
> `decode_null_desc_keeps_wire_driven_registered_record` /
> `decode_wildcard_desc_falls_back_to_wire_driven`.

`decodeFqnWithDesc` (`ProtoReader.java:1026-1060`) is where the descriptor
decides the kind — this is the piece the task calls out as **type-directed
`decodeFqnWithDesc` vs Python's registry-only** lookup:

- `TypeRegistry.isClass(fqn)` → `decodeClassWithDesc`, reifying each field
  through its **per-field descriptor** (`registerClass(..., BamlType[] fieldDescs)`).
  Pinned by `WireCodecTest.decode_desc_class_field_uses_field_descs`.
- `TypeRegistry.isUnionKey(fqn)` (a **named recursive alias**) → unwrap any
  union wrapper, then reify onto the registered nominal record via
  `constructUnionForFqn`, recursing the arm's inner value through the matched
  arm's own `BamlType` as a descriptor.
- an enum, or an unresolved FQN → wire-driven `decodeValue` (`:1058-1059`).

> ⚠ **Deviation from Python:** Python's outbound decode is **registry-only** —
> it resolves `class_value.name` / `enum_value.name` through the typemap and
> validates a decoded field dict with Pydantic; it never consults a declared
> return descriptor. Java's `decodeFqnWithDesc` is **type-directed**: the
> descriptor picks class-vs-recursive-alias-vs-enum routing and supplies
> per-field descriptors before the registry is even consulted, so unions and
> recursive aliases reify onto nominal Java types the Python bridge would have
> returned as bare inner values.

### Class decoding

`decodeClass` (`ProtoReader.java:789-820`) gathers `name` (field 1), `fields`
(field 2, repeated `BamlOutboundMapEntry`) and `type_args` (field 3, repeated
`BamlTy`), then:

1. **media special-case:** if `isMediaFqn(fqn)` and the field map contains
   `_data`, return `fields.get("_data")` — the nested handle decode already built
   the media wrapper (`:811-813`; `isMediaFqn` `:909-914`). Mirrors Python's
   `_decode_class` media short-circuit.
2. `instance = TypeRegistry.constructClass(fqn, fields)`; if `null` (unregistered
   FQN) return the field `Map` (`:814-817`).
3. `bindReifiedTypeArgs(instance, typeArgBytes)` (`:818`).

```java
Object instance = TypeRegistry.constructClass(fqn, fields);
if (instance == null) {
    return fields;               // unregistered FQN → lenient field Map
}
bindReifiedTypeArgs(instance, typeArgBytes);
return instance;
```

> ⚠ **Deviation from Python — construction mechanism:** Java builds the instance
> **positionally** through the generated class's canonical all-args constructor,
> marshalling wire fields into declaration order recorded at registration
> (`ClassEntry.instantiate`, `TypeRegistry.java:474-489`). Python calls
> `cls.model_validate(field_dict)` (keyword, Pydantic-validated) and injects
> handle-backed private fields (`_handle`/`_data`/`_body`) into
> `__pydantic_private__`. Java generated classes are immutable value classes
> with no private-field bag, so there is **no private-field injection step**; a
> handle-backed field is just an ordinary constructor argument (or the whole
> media class is unwrapped in step 1). A field absent from the wire is passed as
> `null` (`TypeRegistry.java:477-482`).

> ⚠ **Deviation from Python — unregistered FQN fallback:** both bridges degrade
> an unresolved class FQN to a plain field map (Java `LinkedHashMap`, Python
> `dict`), preserving thrown stdlib/user error payloads
> (`ProtoReader.java:815-816`; pinned by
> `WireCodecTest.decode_class_value_unknown_fqn_falls_back_to_map`). The
> behavior matches; the container type differs (`LinkedHashMap` vs `dict`).

### Reified generics → weak-identity side-table

> ⚠ **Deviation from Python — generics land in a side-table, not on the
> instance:** `bindReifiedTypeArgs` (`ProtoReader.java:831-844`) converts each
> wire `type_arg` (`BamlTy`) into a `BamlType` token via `BamlType.fromWireTy`
> and retains the list in a **weak-identity side-table**
> (`TypeRegistry.bindTypeArgs` / `typeArgsOf`, `TypeRegistry.java:315-404`),
> keyed by the decoded instance's identity. Python instead **parameterizes the
> class symbol** (`_parameterize_tys` → `Wrapper[int]`) *before*
> `model_validate`, so the reified args live on the Pydantic type of the
> returned object. Java's generated value class has **no instance field** for
> them (Java generics are erased); the tokens live beside the instance in the
> side-table and are read back through the emitted `bamlTypeArgs()` accessor
> (landed `861414d55`):
>
> ```java
> private static void bindReifiedTypeArgs(Object instance, List<byte[]> typeArgBytes) {
>     if (typeArgBytes == null || typeArgBytes.isEmpty()) return;
>     List<BamlType> tokens = new ArrayList<>(typeArgBytes.size());
>     for (byte[] tyBytes : typeArgBytes) {
>         BamlType token = BamlType.fromWireTy(tyBytes);
>         if (token == null) return;   // an unrepresentable arg poisons the whole binding
>         tokens.add(token);
>     }
>     TypeRegistry.bindTypeArgs(instance, tokens);
> }
> ```
>
> Two consequences:
>
> - **All-or-nothing binding.** If any arg falls outside `BamlType`'s minimal
>   grammar (int/string/bool/float primitives, `of(Class)` for a registered
>   class/enum, and reified `of(Class, …)` generics — `BamlType.java:36-43,
>   172-233`), `fromWireTy` returns `null` and the *entire* binding is skipped, to
>   keep De Bruijn positions aligned. Pinned by
>   `WireCodecTest.decode_class_value_out_of_grammar_type_arg_skips_binding` (a
>   `list<int>` arg drops the whole list).
> - **Weak identity.** The side-table holds the instance only weakly and keys on
>   `System.identityHashCode` (generated value classes may be records with value
>   equality, so two distinct-but-equal instances must not collide);
>   `WeakIdentityKey` expunges cleared entries via a `ReferenceQueue`
>   (`TypeRegistry.java:329-404`). `typeArgsOf` returns `List.of()` for an
>   unbound instance (pinned by `type_args_of_unbound_instance_is_empty`).
>
> The emitted **`bamlTypeArgs()`** accessor delegates to `typeArgsOf`
> (`TypeRegistry.java:315-322`); it is generated on every generic value class
> alongside a reified `of(BamlType …, T value)` factory (**landed `861414d55`**
> on the `3991c4fd4` runtime substrate — see `ref-java-examples.md`, "Generics").
> Pinned by `decode_class_value_binds_reified_type_args` /
> `…_nested_reified_type_arg` / `…_without_type_args_has_empty_side_table`.
> **Status:** the readback-naming question is resolved (`bamlTypeArgs()`); the
> token grammar stays minimal by design (an out-of-grammar arg degrades
> gracefully rather than erroring).

### Enum decoding

`decodeEnum` (`ProtoReader.java:852-867`) reads `name` (1) and `value` (2),
skips `is_dynamic` (3), then `TypeRegistry.resolveEnum(fqn, variant)`; a `null`
result (unregistered FQN or unknown variant) falls back to the **raw wire
variant `String`** (`:865-866`).

> ⚠ **Deviation from Python — enum failure mode:** Python resolves the FQN
> through the typemap and constructs `cls(variant)`, and "if the variant is not a
> member of that generated enum class, decoding raises `BamlError`" (and an
> unregistered FQN raises `BamlError` from `get_class`). Java **never raises** on
> a bad enum: an unregistered FQN *or* an unknown variant both degrade to the raw
> variant string (`TypeRegistry.resolveEnum` returns `null` →
> `ProtoReader.java:866`). Pinned by
> `WireCodecTest.decode_enum_value_unknown_fqn_falls_back_to_string`. The
> keyword-escaped mapping (BAML `new` ↔ Java constant `new$`) is handled by the
> parallel `javaConstants` / `wireNames` arrays (`TypeRegistry.java:116-127`;
> pinned by `decode_enum_value_maps_wire_variant_to_constant`).

### Union-variant decoding — canonical `selected_option_index`, then structural; `value_option_name` never trusted

`decodeUnionVariant` (`ProtoReader.java:605-660`) reads `self_type` (field 4),
`value` (field 6), and — since `ceae8ea6c` (#4087) — the canonical
`selected_option_index` (field 8). It **deliberately ignores `value_option_name`
(field 5)**, which is display-only. The resolution is:

1. inner null (absent `value`, or a `null_value` arm) → `null` (pinned by
   `decode_union_null_inner_is_null`).
2. read the `self_type` `BamlTy` into its arm `BamlType`s — `wireArmType` per
   option, dropping `null` / unrepresentable arms. A **resolved union** yields
   the arm set (keyed structurally, a sorted+distinct `List<BamlType>`); a
   recursive-alias **node** yields its FQN (`selfTypeArms` / `selfTypeFqn`).
3. **canonical index when present.** If `selected_option_index` is set, the
   selected arm's raw type is resolved by position against `self_type`
   (`selfTypeOptionAt:1498`, which preserves `null` holes), decoded under that
   exact type, and reified via `TypeRegistry.constructUnionForArmsSelected` (or
   `constructUnionForFqnAtIndex` for a named alias) — the host-selected arm,
   independent of payload shape (`ProtoReader.java:632-653`). This is what lets an
   empty `int[]`-vs-`string[]` arm round-trip back onto the arm the engine
   selected rather than the first structural match.
4. **structural fallback.** When `selected_option_index` is **absent**, the arm is
   picked structurally from the inner value's own shape
   (`TypeRegistry.constructUnionForArms(arms, valueBytes, inner)` /
   `constructUnionForFqn`, `armMatchesValue`, not the wire position).
5. **bare fallback (load-bearing):** the bare decoded inner value, when the arm
   set / FQN is unregistered or no arm matches.

The descriptor-driven path (`decodeUnionWithDesc:1201`) applies the same rule, but
resolves the arm **by type, not by raw wire index**: `extractUnionSelectedType:1465`
reads `selected_option_index` off the wire and resolves it to the selected *type*
against `self_type` (`selfTypeOptionAt`), then the decoder locates that type in the
descriptor's arms by value — `int selectedArm = arms.indexOf(selectedType)`
(`:1219`) — before `wrapArm`. That match is **unambiguous** because canonical union
members are structurally distinct: `baml_type::normalize`'s `canonicalize_union`
sort+dedups members (`normalize.rs:1601`, `flat.sort(); flat.dedup();`), so a union
can never carry two value-equal arms (e.g. `string | string` collapses to
`string`). Resolving by value (rather than trusting the raw index into the
descriptor) is what makes this robust to any order difference between the wire
`self_type` and the descriptor's declaration-order arms. Only when
`selected_option_index` is **absent** does it fall back to structural
`armMatchesValue` in declaration order.

> ⚠ **Deviation from Python — precise contrast on union metadata.** The Python
> doc says the `union_variant_value` arm returns the "recursively decoded inner
> value; union metadata is discarded" — Python **erases the union wrapper
> entirely and returns the bare selected value**, reading neither
> `value_option_name` nor `selected_option_index` (a duck-typed host needs no
> wrapper). Java **never trusts `value_option_name`** (field 5) either, but rather
> than discarding the metadata it **reconstructs a typed wrapper**: it honors the
> canonical `selected_option_index` (field 8) when present — resolving the arm by
> position against `self_type` — and otherwise matches `self_type` (field 4)
> *structurally* against the inner value's own shape, yielding a registered
> nominal record via `constructUnionForArmsSelected` / `constructUnionForArms`
> (or `constructUnionForFqn{AtIndex}`), or (on the descriptor path) a generic
> `Union{k}.Arm{i}`.
> So the two bridges agree on distrusting `value_option_name`, but diverge on the
> result: Python returns the bare inner; Java preserves the union as a generated
> Java type when it can, and only *falls back* to the bare inner (Python's
> always-behavior) for erased/unregistered unions. Pinned by
> `WireCodecTest.decode_union_int_arm_constructs_record` (arm chosen by inner
> shape), `…_string_arm_constructs_record`,
> `…_unknown_signature_falls_back_to_bare_inner`, and
> `…_literal_arms_fall_back_to_bare_inner` (a literal-over-one-base union is
> erased in codegen, never registered → bare inner).

On the **descriptor** path, `decodeUnionWithDesc` (`ProtoReader.java:1201`) reads
the wire union's `selected_option_index` first (`extractUnionSelectedType:1465`)
and, when present, wraps that exact arm; only when the index is **absent** does it
fall back to matching the (unwrapped) wire value against the **declared arms in
order** structurally (`armMatchesValue`), wrapping the first match in
`baml_bridge.Union{k}.Arm{i}` (`wrapArm` reflectively constructs the record). No
arm match throws `BamlError` (pinned by `decode_desc_union_no_arm_match_throws`).
Pinned by `decode_desc_union_bare_int_arm0`, `…_bare_string_arm1`,
`…_variant_wrapped_int_arm0`, `…_class_arm_via_fqn`.

## Classes, Enums, Generics, And Typemap

The generated SDK root installs a process-global `TypeRegistry` (the Java analog
of Python's `BamlTypeMap` + `set_type_map`). Registration happens in the static
initializer of the generated `baml_sdk.Baml` anchor, **before**
`initFromBytecode`, one call per user class/enum/union
(`TypeRegistry.java:14-51`):

```java
TypeRegistry.registerClass("user.lorem.Resume", "baml_sdk.lorem.Resume",
                           new String[] {"name", "age"});
TypeRegistry.registerEnum("user.ipsum.Sentiment", "baml_sdk.ipsum.Sentiment",
                          new String[] {"Positive", "new$"},   // Java constants
                          new String[] {"Positive", "new"});   // wire variants
```

`TypeRegistry` lazily maps BAML FQNs to generated Java classes, enums, and union
records, resolving the `Class` object via `Class.forName` on first decode use and
caching it (`TypeRegistry.java:439-549`). It also maintains reverse indexes
(generated Java binary name → entry) for inbound encode, but outbound decoding
uses the forward FQN → symbol lookup. All maps are `ConcurrentHashMap`s and
registration is idempotent (first registration of an FQN wins,
`:105-107,124-126,149`).

> ⚠ **Deviation from Python — no reflection over the return type; explicit
> field order at registration.** Python's typemap seeds hardcoded reverse
> overrides for the stdlib PyO3 media re-exports and relies on Pydantic to
> validate structurally. Java carries the class's **declaration-order field
> names** (`fieldOrder`) and optional **per-field descriptors** (`fieldDescs`) in
> the registration call, because Java has no Pydantic to validate a field dict —
> the decoder marshals fields positionally into the canonical constructor
> (`TypeRegistry.java:439-549`) and reifies each field through its descriptor
> when present.

The runtime-owned media stdlib classes (`baml.media.Image` / `Audio` / `Video`
/ `Pdf`) are **never registered** in `TypeRegistry`; they are matched by FQN
constant instead (`isMediaFqn`, `ProtoReader.java:909-914`, using `Image.FQN`
etc.). This is the Java analog of Python's `_MEDIA_PYO3_TYPES` reverse overrides.

For outbound class values, `decodeClass` first decodes all fields into a
`LinkedHashMap`, then resolves `class_value.name` (a bare FQN string) through the
registry:

- **Unresolved FQN** → the decoded field `Map` (preserves thrown stdlib/user
  error payloads; `ProtoReader.java:815-816`).
- **Media stdlib FQN** with a `_data` field → the already-built media wrapper
  (`:811-813`).
- **Otherwise** → `constructClass` reifies the generated value class, and
  `bindReifiedTypeArgs` retains any wire `type_args` in the side-table
  (`:814-818`).

Outbound generic args use `BamlTy` metadata. `BamlType.fromWireTy`
(`BamlType.java:172-233`) is the runtime mirror of the Java codegen's type
lowering; it recognizes only the minimal grammar (primitive int/string/bool/float,
`class_ty`, `enum`) and returns `null` for anything else — the all-or-nothing
gate described above. There is no `cls[args...]` subscript step (Java generics are
erased); the tokens are simply retained for the emitted `bamlTypeArgs()`
accessor (which reads them back from the side-table).

## Handles

Inbound and outbound share the `BamlHandleType` enum. Java handle decoding uses
the outbound `handle_type` discriminator; `decodeHandle`
reads `key` (field 1), `handle_type` (field 2), and the root class FQN from
`ty.class_ty.name` (field 3). It builds a
`BamlHandle(key, handleType, classFqn)`, then dispatches:

| Handle type (wire) | Java decode |
| --- | --- |
| `ADT_MEDIA_IMAGE` (6) | `Image.fromHandle(handle)` |
| `ADT_MEDIA_AUDIO` (7) | `Audio.fromHandle(handle)` |
| `ADT_MEDIA_VIDEO` (8) | `Video.fromHandle(handle)` |
| `ADT_MEDIA_PDF` (9) | `Pdf.fromHandle(handle)` |
| `ADT_TAGGED_HEAP_HANDLE` (14) | `baml_bridge.BamlStream.fromHandle(handle)`. The wrapper requires and retains the concrete class FQN carried by `ty`. |
| `HANDLE_UNSPECIFIED` (0) | bare `BamlHandle` (`default` arm). |
| all other handle types | bare `BamlHandle` (`default` arm). |

Pinned by `WireCodecTest.decode_media_handle_constructs_image`,
`decode_media_class_wrapper_unwraps_to_media`, and
`decode_unknown_handle_type_falls_back_to_bare_handle`, plus
`decode_stream_handle_retains_carried_class_fqn` and
`decode_stream_handle_rejects_missing_class_fqn`.

**`ADT_TAGGED_HEAP_HANDLE` (14), including `ai.stream.Stream`:** Java uses the
handle-type tag to select the runtime-owned `BamlStream` wrapper, but it does
not erase the nominal receiver identity. It retains `handle.ty.class_ty.name`
and derives method calls as `<carried-FQN>.next` and `<carried-FQN>.final`.
`TPartial`/`TFinal` generic arguments remain host-erased, as in Python. A tagged
stream handle without a class FQN is rejected rather than falling back to a
hardcoded namespace. Stream *partials* (`next()` results) still decode as
ordinary registered `$stream` companion classes on the wire-driven path.

> ⚠ **Deviation from Python — `HANDLE_UNSPECIFIED`:** Python raises `BamlError`
> for a `HANDLE_UNSPECIFIED` handle; Java degrades it to a bare `BamlHandle`
> (the same `default` arm as any unrecognized handle type,
> `ProtoReader.java:897`). No reject path in Java.

> ⚠ **Deviation from Python — `ADT_MEDIA_GENERIC` (10):** not in Java's
> dispatch; it falls to the bare-`BamlHandle` default (`ProtoReader.java:874-897`
> Javadoc calls this out explicitly).

**The engine-drains-cloned-key contract.** A decoded handle **owns** the
engine-minted `key` — `new BamlHandle(key, handleType, classFqn)` takes ownership
(ordinary handles use the two-argument overload) and a
`Cleaner` calls `baml_handle_release(key)` exactly once when the wrapper becomes
unreachable (`BamlHandle.java:35-103`, per-instance atomic latch at `:61-91`).
On the *inbound* (encode) direction, `cloneKeyForWire()` mints a **fresh** owned
key via `baml_handle_clone` so "the engine can `drain` its copy on decode while
this object keeps its own — never sharing a key would double-release"
(`BamlHandle.java:19-27, 120-122`). Host-owned handles
(`HOST_VALUE_CALLABLE` = 15, `HOST_VALUE_OPAQUE` = 16) are **not** tracked in
`HANDLE_TABLE`, so the release path skips them (`BamlHandle.java:30-34, 78-81`) —
the same guard as Python's `BamlPyHandle`.

`BamlStream` is a runtime-owned Java class wrapping a `BamlHandle` (the JVM
analog of Python's `baml_bridge/_stream.py`); the outbound decode **now
rehydrates** a tagged-heap-handle into it via
`BamlStream.fromHandle(handle)` (the `ADT_TAGGED_HEAP_HANDLE` arm above,
retaining the handle's concrete class identity). `next()` / `get_final()` (and
`_async`) then re-enter the engine on `<carried-FQN>.next` / `.final` with
`this` as the `self` receiver and a `null` (wire-driven) descriptor. For the
canonical function-result stream the carried FQN is `ai.stream.Stream`.

### Cancellation detail (async only)

The async remap lives in `BamlFfi`, not `ProtoReader`. `callAsync`'s
`whenComplete` hook runs the shared `decodeResult`, and on failure applies
`mapAsyncFailure` (`BamlFfi.java:322-336`):

```java
private static Throwable mapAsyncFailure(Throwable t) {
    if (t instanceof BamlPanic panic && CANCELLED_PANIC_CLASS.equals(panic.class_name())) {
        return BamlTraceback.splice(
                new BamlCancelledError(panic.value(), panic.baml_trace(), panic.class_name()),
                panic.baml_trace());
    }
    return t;
}
```

`CANCELLED_PANIC_CLASS = "baml.panics.Cancelled"` (`BamlFfi.java:64`), matched by
string so the runtime library never references a generated class.

> ⚠ **Deviation from Python — cancellation type + surfacing:** Python wraps the
> engine `baml.panics.Cancelled` as a `BamlCancelledError` (a `BamlError`
> subclass) and re-raises it as `asyncio.CancelledError`, reachable via
> `CancelledError.reason`. Java's **`BamlCancelledError extends
> CancellationException`** (`BamlCancelledError.java:26`), *not* `BamlError`
> (Design B in `java-function-calls-decisions.md` D1). Because it **is** a
> `CancellationException`, a future completed with it reports
> `isCancelled() == true` and `join()`/`get()` surface it **directly**
> (unwrapped), rather than re-wrapped in a `CompletionException` /
> `ExecutionException`. Pinned by
> `BamlCancelledErrorTest.futureCompletedWithItReadsAsCancelled` /
> `isCancellationExceptionNotBamlError`.

> ⚠ **Deviation from Python — the JDK-19+ `reportJoin` re-wrap workaround.**
> The caller-visible future is a `CancellableCall` (`BamlFfi.java:358-418`). On
> JDK 19+ the base `CompletableFuture` re-wraps a stored `CancellationException`
> in a *fresh* one at report time (JDK ≤17 threw it as-is), which would defeat
> the "throw it directly, unwrapped" contract. So `join()`/`get()`/`getNow`
> override to recover the original:
>
> ```java
> private static CancellationException unwrapCancellation(CancellationException wrapped) {
>     return wrapped.getCause() instanceof BamlCancelledError cancelled ? cancelled : wrapped;
> }
> ```
>
> A host `future.cancel(true)` (whose stored value is a *plain*
> `CancellationException`, no `BamlCancelledError` cause) still surfaces as a
> plain `CancellationException`. `cancel(true)` also fires
> `nativeCancelFunctionCall(callId)` **before** the standard bookkeeping so the
> engine call is actually stopped, and the engine's late completion envelope
> no-ops against the already-cancelled future (`BamlFfi.java:365-369, 452-457`).
> Python has no such report-time-rewrap workaround because `asyncio` does not
> re-wrap `CancelledError`.

> ⚠ **Deviation from Python — sync cancellation keeps the panic.** A sync call
> bound to a `BamlCallContext` whose `abort()` fired surfaces the engine
> cancellation as a raw **`BamlPanic`** carrying a `baml.panics.Cancelled` value
> — `mapAsyncFailure` is never called on the sync path (`BamlFfi.java:191-197,
> 199-233`). This matches Python (sync → `BamlPanic(Cancelled)`, no
> cancellation remap). The `BamlCallContext` cancellation surface itself
> (`abort()`, `attach`/`detach`, abort-before-start latch) is an **invented Java
> surface** (`BamlCallContext.java`) with no Python-visible analog beyond
> `bridge_python`'s Rust `BamlCallContext`; pinned by `BamlCallContextTest`.

### OS-exit

`is_exit_panic` runs the registered telemetry-flush hooks and then terminates the
process via `Runtime.getRuntime().halt` (`ProtoReader.java:313-319`):

```java
if (isExit) {
    // Clean baml.sys.exit: run the best-effort telemetry-flush hooks (the
    // spec'd flush step — exceptions swallowed, nothing may prevent the
    // halt), then hard-terminate the process, bypassing JVM shutdown hooks
    // (the analog of Python's os._exit, which flushes then _exits).
    baml_bridge.BamlFfi.runExitFlushHooks();
    Runtime.getRuntime().halt((int) exitCode);
    return new AssertionError("halt returned");   // unreachable
}
```

> ✅ **Implemented per spec — telemetry-flush hook wired.** Python "flushes
> telemetry and calls `os._exit(exit_code)`." Java now mirrors that:
> `decodePanic` calls `BamlFfi.runExitFlushHooks()` (which runs every hook
> registered via `BamlFfi.registerExitFlushHooks(Runnable)`, best-effort —
> exceptions are swallowed so nothing may prevent or delay the halt) **before**
> `Runtime.getRuntime().halt(exitCode)`, which bypasses JVM shutdown hooks (the
> correct `os._exit` analog). The hooks are a socket: no telemetry ships in this
> slice, so the registry is empty by default and the halt behavior is unchanged.
> The design intent (`java-function-calls-decisions.md` §5, and the
> state-of-completeness row) is "flush telemetry then `halt`" — the flush step is
> now present. The hook drain is factored into `runExitFlushHooks()` so its
> mechanics are unit-tested (`BamlFfiSmokeTest.exit_flush_hooks_run_best_effort_and_swallow_exceptions`)
> without halting; the halt itself is verified via a `ProcessBuilder` subprocess
> in `TestErrors` (per the decisions doc). `halt` never returns, so the returned
> `AssertionError` is unreachable and exists only to satisfy the `Error`-returning
> signature (`BamlPanic` is now an `Error`).

## Concrete Type-Shape Examples

These mirror the Python doc's three examples over the shared `type_shapes`
SDK-test fixture. The BAML source and returned `BexExternalValue` / outbound
proto are identical across bridges (the same engine encodes them); only the Java
*decode* differs. Where the Python doc shows a Pydantic result, the Java result
is a generated immutable value class, with reified generics in the side-table
rather than on the type.

### 1. Simple: `Wrapper<int>`

Fixture BAML (shared):

```baml
class Wrapper<T> { value T }
function round_trip_wrapper_int(w: Wrapper<int>) -> Wrapper<int> { w }
```

The returned engine value carries reified `type_args` (`RuntimeTy::Int`), and
`external_to_outbound` produces:

```text
BamlOutboundValue {
  class_value {
    name: "user.generics.Wrapper"
    type_args: [ { int_type {} } ]
    fields: [ { key: "value", value: { int_value: 5 } } ]
  }
}
```

Java decoding (`decodeClass`, `ProtoReader.java:789-820`):

1. `decodeValue` sees `class_value` (field 7).
2. `decodeClass` recursively decodes fields → `{"value": 5L}` (a `Long`).
3. `TypeRegistry.constructClass("user.generics.Wrapper", fields)` reifies the
   generated `baml_sdk.generics.Wrapper` via its canonical constructor
   (`TypeRegistry.java:474-489`).
4. `bindReifiedTypeArgs(instance, [int])` retains `List.of(BamlType.INT)` in the
   weak-identity side-table (`ProtoReader.java:831-844`).

The host value is a `baml_sdk.generics.Wrapper` instance holding `value = 5L`;
`TypeRegistry.typeArgsOf(instance)` returns `[int]`.

> ⚠ **Deviation from Python:** Python returns a *parameterized* `Wrapper[int]`
> Pydantic object — the `int` is on the object's type. Java returns a bare
> `Wrapper` value class; the `int` lives beside it in the side-table
> (`typeArgsOf`), read back through the emitted `bamlTypeArgs()` accessor, because
> Java generics are erased and the args cannot ride an instance field. This exact
> wire shape is pinned by
> `WireCodecTest.decode_class_value_binds_reified_type_args`. Even with empty
> `type_args`, the FQN-plus-constructor path still reconstructs the object
> (`…_without_type_args_has_empty_side_table`).

### 2. Medium: `NestedGenerics`

Fixture BAML (shared):

```baml
class GenericLinkedList<T> { value T; next GenericLinkedList<T>? }
class NestedGenerics {
  ww Wrapper<Wrapper<int>>
  wl Wrapper<int[]>
  wr Wrapper<GenericLinkedList<int>>
}
function round_trip_nested_generics(n: NestedGenerics) -> NestedGenerics { n }
```

The outbound proto keeps the class graph, list `item_type` metadata, and reified
per-instance `type_args` (as in the Python doc's expanded tree). Java decodes
recursively from the leaves upward (`ProtoReader.java:337-374, 789-820,
394-407`):

1. Innermost `int_value` nodes → `1L`, `2L`, `9L`.
2. The empty `next` value has no oneof set → `null` (`:344-347, 219`).
3. The `list_value` → `ArrayList[1L, 2L]`. Java does **not** consult `item_type`
   for list decoding; it only walks `items` (`decodeList` skips `item_type`,
   `:400-404`) — same as Python.
4. Each `class_value` resolves by FQN through `TypeRegistry` and constructs the
   generated value class positionally.
5. The outer `NestedGenerics` is constructed from the decoded child values in
   declaration order (`ww`, `wl`, `wr`).

The final host value is a `baml_sdk.generics.NestedGenerics` with nested
generated value objects. Each nested generic instance also gets its own
side-table `type_args` binding (nested reified class args round-trip through the
side-table — pinned by
`WireCodecTest.decode_class_value_binds_nested_reified_type_arg`).

> ⚠ **Deviation from Python:** Python validates each decoded child dict against
> the generated Pydantic field annotations (`Wrapper[Wrapper[int]]`,
> `Wrapper[List[int]]`, `Wrapper[GenericLinkedList[int]]`). Java has no
> structural validation step — it marshals fields **positionally** into each
> value class's constructor by the registered field order. The graph
> round-trips because every object node carries a concrete class FQN; reified
> `type_args` ride the side-table rather than the type. As in Python, even empty
> `type_args` would still reconstruct via FQN + constructor.

### 3. High Complexity: `ComplexProfile`

Fixture BAML (shared — enum, literal-string union, nested classes, class arrays,
`map<string,string>`, optional fields, and unions of classes and of primitives):

```baml
enum AccountTier { Free, Pro, Enterprise }
class Invoice {
  id string
  status "draft" | "sent" | "paid"
  items LineItem[]
  payment CardPayment | WirePayment | null
  notes string?
}
class ComplexProfile {
  id string
  tier AccountTier
  owner ProfileOwner
  addresses PostalAddress[]
  invoices Invoice[]
  audit_trail AuditEvent[]
  metadata map<string, string>
  featured Invoice | PostalAddress | string | null
  flags (int | string | bool)[]
}
```

Java decoding proceeds mechanically over the same outbound tree:

1. **Scalars** decode directly: `"profile-001"` (String), `19.5` (Double),
   `2L` (Long), `true` (Boolean), and `null` for absent optionals
   (`ProtoReader.java:348-351, 344-347`).
2. **`enum_value`** resolves `"user.complex_models.AccountTier"` +
   `"Enterprise"` to the generated `AccountTier.Enterprise` constant via
   `TypeRegistry.resolveEnum` (`:852-867`); an unknown FQN/variant would degrade
   to the raw string (Python raises — see the enum deviation above).
3. **`map_value`** entries → `LinkedHashMap<String,Object>`; `key_type` /
   `value_type` are skipped (`decodeMap`, `:409-438`) — same as Python.
4. **`list_value`** entries → `ArrayList` by recursively decoding `items`;
   `item_type` skipped (`:394-407`).
5. Each nested **`class_value`** becomes a generated value class through
   `decodeClass`.
6. **Unions** are where the Java decode diverges most, and how they decode
   depends on whether the caller supplied a return descriptor:
   - **Wire-driven** (no descriptor): `decodeUnionVariant` reads `self_type` into
     its arm `BamlType`s (structural registry key), matches the arm from the inner
     value's shape (`armMatchesValue`), and constructs the registered nominal
     wrapper record — or falls back to the bare inner. `Invoice.payment`
     (`CardPayment | WirePayment
     | null`) reifies to a registered union record whose arm is chosen by the
     inner class FQN; the `null` case decodes to bare `null`. `featured`
     (`Invoice | PostalAddress | string | null`) likewise. `flags`
     (`(int|string|bool)[]`) decodes each element's `union_variant_value` to a
     registered `int|string|bool` record picked by the primitive discriminator.
     `Invoice.status` (`"draft"|"sent"|"paid"`) is a **literal-over-one-base
     union**, erased in codegen and never registered → the bare `String`
     (`"sent"`), exactly like Python's bare selected value (pinned by
     `decode_union_literal_arms_fall_back_to_bare_inner`).
   - **Type-directed** (the generated binding passes a `union[...]` descriptor):
     `decodeUnionWithDesc` matches the wire value against the *declared* arms in
     order and wraps in `baml_bridge.Union{k}.Arm{i}`
     (`ProtoReader.java:1141-1170`; e.g. `decode_desc_union_class_arm_via_fqn`).
7. Finally the top-level `ComplexProfile` is constructed positionally from the
   complete decoded object graph.

> ⚠ **Deviation from Python:** Python discards every union wrapper and returns
> the bare selected value (`"sent"`, a `CardPayment`, `7`, etc.), then relies on
> `ComplexProfile.model_validate` to enforce the final field shape. Java
> **preserves** each non-erased union as a generated Java union type — a
> registered nominal record (wire-driven) or a generic `Union{k}.Arm{i}`
> (descriptor-driven) — and only erased literal-over-one-base unions collapse to
> the bare value the way Python's always do. There is no final structural
> validation pass; the generated class constructors enforce shape by arity/type.

This high-complexity example shows the main runtime split, same as Python: class
and enum nodes are `TypeRegistry`-driven, and list/map type metadata is ignored
by the decoder. The Java-specific twist is unions — type-directed, with the wire
`value_option_name` distrusted on both the wire-driven and descriptor paths.

## Practical Consequences For Bridge Generics

- Generated Java return types are static-only. Runtime return decoding is driven
  by the outbound wire shape **plus the type-directed return descriptor** the
  generated binding passes.
- Outbound decoding is type-rich and `TypeRegistry`-driven. Generic return
  values materialize as generated value classes; the reified
  `class_value.type_args` are retained in the **weak-identity side-table**
  (`TypeRegistry.typeArgsOf`), read back through the emitted `bamlTypeArgs()`
  accessor — **not** parameterized onto the class the way Python subscripts a
  Pydantic symbol.
- `BexExternalValue::Instance` carries `type_args`, and `external_to_outbound`
  encodes them via `runtime_ty_to_proto_ty`, so reified generics survive on the
  wire. Java retains them all-or-nothing (an out-of-grammar arg poisons the whole
  binding, to keep De Bruijn positions aligned). Even with empty `type_args`,
  normal class returns reconstruct via the concrete FQN plus the canonical
  constructor.
- Union wrappers **do** survive into Java values (as registered nominal records
  or generic `Union{k}.Arm{i}`), except for erased literal-over-one-base unions,
  which collapse to the bare value. This is the opposite of Python, where union
  wrappers never survive.
- Media and prompt AST arrive via `handle_value` on the Java FFI path; inline
  `media_value` / `prompt_ast_value` / `ty_value` are treated as bridge drift and
  throw `UnsupportedOperationException` on the `ok` path (degrading to `null` on
  the error path). Python raises `BamlError` for `media_value`/`prompt_ast_value`
  and returns `None` for `ty_value`.
- **Host-callable error rehydration** (returning the original `Throwable` by
  identity, `202883518`) and **tagged-heap-handle → `BamlStream`** rehydration
  (`a6e3ca99e`) are both **LANDED**; see the flags above.

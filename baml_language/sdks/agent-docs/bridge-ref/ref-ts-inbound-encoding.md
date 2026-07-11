---
date: 2026-07-08
repository: baml4
runtime_package: "@boundaryml/baml-bridge"
implementation_root: baml_language/sdks/nodejs
source_files:
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/define_function.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/host_value_registry.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/wire_ty.ts
  - baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/baml_inbound.proto
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/typemap.ts
  - baml_language/sdks/nodejs/bridge_nodejs/src/runtime.rs
  - baml_language/sdks/nodejs/bridge_nodejs/src/host_value.rs
  - baml_language/crates/bridge_ctypes/src/value_decode.rs
  - baml_language/crates/bex_engine/src/conversion.rs
  - baml_language/sdks/nodejs/sdkgen_typescript_node/src/leaf.rs
  - baml_language/sdks/nodejs/sdkgen_typescript_node/src/emit/typemap_file.rs
example_sources:
  - baml_language/sdk_tests/fixtures/type_shapes/baml_src
  - baml_language/sdk_tests/crates/typescript_node/type_shapes/generated
---

# TypeScript Inbound Encoding

This file records how the current Node TypeScript SDK encodes host values into
runtime arguments. "Inbound" means TypeScript/JavaScript to BAML runtime. The
generated `baml_sdk` package provides the typed surface, but the actual value
codec lives in `@boundaryml/baml-bridge`, implemented in
`baml_language/sdks/nodejs/bridge_nodejs`.

The short version:

- generated functions are thin `defineFunction(...)` / `defineInstanceFunction(...)`
  wrappers (generic callables also carry a `typeParams` / `classTypeParams`
  descriptor)
- wrapper calls build a kwargs object from positional arguments and optional
  `$opts`, after pulling out the reserved `$ctx` / `$types` option keys
- `encodeCallArgs` serializes those kwargs into a `CallFunctionArgs` protobuf,
  stamping a mandatory nonzero `call_id` and any explicit generic `type_args`
- Node's native Rust layer decodes that protobuf into `BexArgs`
- the engine coerces inbound values against the declared BAML parameter types

For the reverse direction, see
[00a-prior-art-ts-outbound-encoding.md](00a-prior-art-ts-outbound-encoding.md).

## Generated Binding Shape

Codegen emits typed declarations, but those declarations are TypeScript casts
over untyped runtime factories. A typical free function becomes:

```ts
export const round_trip_primitives =
  defineFunction("user.primitives.round_trip_primitives", "sync", ["p"])
    as (p: Primitives) => Primitives;

export const round_trip_primitives_async =
  defineFunction("user.primitives.round_trip_primitives", "async", ["p"])
    as (p: Primitives) => Promise<Primitives>;
```

Defaulted BAML parameters are grouped into a trailing `$opts` object. The runtime
factory receives two parameter-name lists (required positional names and optional
names) and an optional 5th `generics?: GenericParams` argument
(`{ typeParams?, classTypeParams? }`) for callables on or returning a generic
type.

```ts
export const optional_args_probe =
  defineFunction("user.optional_args_probe", "sync", ["arg0"], ["opt1", "opt2"])
    as (
      arg0: number,
      $opts?: { opt1?: number | null | undefined; opt2?: number | null | undefined } | undefined
    ) => (number | null)[];
```

Instance methods use the same path, except generated class constructors bind the
receiver into a synthetic `"self"` parameter (and a method on a generic class
passes `{ classTypeParams: [...] }` as the 5th argument, with the optional-names
slot filled by `undefined`):

```ts
greet = defineInstanceFunction(
  "user.methods_on_classes.Greeter.greet",
  "sync",
  ["self", "greeting"],
).bind(this) as (greeting: string) => string;
```

`buildArgs` in `define_function.ts` is the first runtime step:

- the reserved option keys `$ctx` (call context) and `$types` (generic bindings)
  are pulled out of the options object before kwargs are built
- required positional arguments are zipped against `requiredParamNames`
- too many positional arguments throw a `TypeError`
- optional parameters must be supplied as one trailing object
- unknown optional keys throw a `TypeError`
- optional values of `undefined` or `UNSET` are skipped, which lets the engine
  apply the BAML default
- required values of `UNSET` are skipped, but ordinary `undefined` is encoded as
  BAML null

After args are built, a `call_id` is minted with `newFunctionCall()` and any
`$types` bindings are lowered to wire `type_args` (`buildTypeArgs`). Sync calls
then run:

```ts
encodeCallArgs(built.kwargs, { syncMode: true, callId, typeArgs })
rt.callFunctionSync(bamlFqn, argsProto, null, null)
decodeCallResult(resultBytes)
```

Async calls run the same sequence without `syncMode` and await
`rt.callFunction(...)`. A call-context binding is attached around the runtime
call and detached afterward.

## Runtime Initialization And Typemap

Generated `baml_sdk/index.ts` imports bytecode, initializes the process-global
runtime, imports `_TYPE_MAP`, and installs it:

```ts
initializeRuntimeFromBytecode(_inlinedbaml.BYTECODE);
setTypeMap(_TYPE_MAP);
```

The generated `_typemap.ts` contains lazy thunks keyed by BAML FQN:

```ts
const _CLASS_ENTRIES = {
  "user.primitives.Primitives": () =>
    (__leaf_31 as Record<string, unknown>)["Primitives"],
};

export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({
  classes: _CLASS_ENTRIES,
  enums: _ENUM_ENTRIES,
  typeAliases: _ALIAS_ENTRIES,
});
```

Normal user class argument encoding intentionally does not require the typemap.
The inbound path uses it for two cases via the reverse `jsTypeToBamlType`
lookup: handle-backed class instances (recovering their BAML FQN before
re-entering the runtime) and generic class instances (tagging them with an FQN +
`type_args`). (`jsTypeToBamlType` now returns `""` rather than `null` on a miss.)

## CallFunctionArgs Encoding

`encodeCallArgs(kwargs, options)` — where `options: EncodeCallArgsOptions` is
`{ callId: bigint; syncMode?: boolean; typeArgs? }` — creates a
`CallFunctionArgs { kwargs, callId, typeArgs }` protobuf. It rejects
`callId === 0n`. Each kwarg value is encoded by `setInboundValue`.

The wire schema in `baml_inbound.proto`:

```proto
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
    BamlTy ty_value = 13;  // decoded by Rust; the TS encoder has no branch that sets it
  }
  // Absent oneof = null value.
}

message CallFunctionArgs {
  repeated InboundMapEntry kwargs = 1;
  uint64 call_id = 2;                // mandatory, must be nonzero
  repeated BamlTyArg type_args = 3;  // explicit generic bindings from `$types`
}
```

`InboundClassValue` no longer has a flat `name` field (field 1 is `reserved`);
the FQN and any reified generics live on `class_ty` (`class_ty.name` +
`class_ty.type_args`).

The TypeScript implementation mirrors that schema directly. Abridged from
`proto.ts`, the core branching is:

```ts
function setInboundValue(iv: IInboundValue, value: unknown, ctx: EncodeCtx): void {
  if (value === null || value === undefined) {
    return; // Leave oneof unset -> null
  }
  if (typeof value === "boolean") {
    iv.boolValue = value;
  } else if (typeof value === "number") {
    if (Number.isInteger(value)) iv.intValue = value;
    else iv.floatValue = value;
  } else if (typeof value === "bigint") {
    iv.bigintValue = value.toString(16);
  } else if (typeof value === "string") {
    iv.stringValue = value;
  } else if (value instanceof Uint8Array) {
    iv.uint8arrayValue = value;
  } else if (value instanceof BamlHandle) {
    if (ctx.syncMode && value.handleType === BamlHandleType.HOST_VALUE_CALLABLE) {
      throw new HostCallableSyncError("...");
    }
    iv.handle = { key: value._cloneKeyForWire(), handleType: value.handleType };
  } else if (value instanceof BamlStream) {
    const h = value._toHandle();
    iv.handle = { key: h.key, handleType: h.handleType };
  } else if (
    value instanceof BamlImage ||
    value instanceof BamlAudio ||
    value instanceof BamlVideo ||
    value instanceof BamlPdf
  ) {
    const h = value._toHandle();
    iv.handle = { key: h.key, handleType: h.handleType };
  } else if (typeof value === "function") {
    if (ctx.syncMode) throw new HostCallableSyncError("...");
    const key = registerHostCallable(makeHostCallableDispatch(value));
    ctx.registered.push(key);
    iv.handle = { key, handleType: BamlHandleType.HOST_VALUE_CALLABLE };
  } else if (Array.isArray(value)) {
    const listVal: IInboundValue[] = [];
    for (const item of value) {
      const child: IInboundValue = {};
      setInboundValue(child, item, ctx);
      listVal.push(child);
    }
    iv.listValue = { values: listVal };
  } else if (value !== null && typeof value === "object") {
    const proto = Object.getPrototypeOf(value);
    const isClassInstance = proto !== Object.prototype && proto !== null;

    if (isClassInstance && Object.values(value).some(v => v instanceof BamlHandle)) {
      const fqn = getTypeMap().jsTypeToBamlType((value as object).constructor);
      if (fqn) {
        const classFields: IInboundMapEntry[] = [];
        for (const [k, v] of Object.entries(value)) {
          if (typeof v === "function") continue;
          const childVal: IInboundValue = {};
          setInboundValue(childVal, v, ctx);
          classFields.push({ stringKey: k, value: childVal });
        }
        iv.classValue = { classTy: { name: fqn }, fields: classFields };
        return;
      }
    }

    // Generic class instance: the constructor carries `static $generic`.
    const genericParams = (value as object).constructor?.["$generic"];
    if (isClassInstance && Array.isArray(genericParams)) {
      const fqn = getTypeMap().jsTypeToBamlType((value as object).constructor);
      if (fqn) {
        // Lower the instance's `$types` bindings to wire type args; an unbound
        // parameter lowers to `{ unknown: {} }`.
        const typeArgs = genericParams.map((p) => lowerType(value["$types"]?.[p]));
        const classFields: IInboundMapEntry[] = [];
        for (const [k, v] of Object.entries(value)) {
          if (typeof v === "function" || k === "$types") continue;
          const childVal: IInboundValue = {};
          setInboundValue(childVal, v, ctx);
          classFields.push({ stringKey: k, value: childVal });
        }
        iv.classValue = { classTy: { name: fqn, typeArgs }, fields: classFields };
        return;
      }
    }

    const entries: IInboundMapEntry[] = [];
    for (const [k, v] of Object.entries(value)) {
      if (isClassInstance && typeof v === "function") continue;
      const childVal: IInboundValue = {};
      setInboundValue(childVal, v, ctx);
      entries.push({ stringKey: k, value: childVal });
    }
    iv.mapValue = { entries };
  } else {
    throw new TypeError("...");
  }
}
```

The real object branch has these details that matter:

- it computes `isClassInstance` with `Object.getPrototypeOf(value) !== Object.prototype`
- for class instances, it skips own function-valued fields so generated method
  bindings do not encode as object state
- if a class instance has a `BamlHandle` field and the typemap can recover a
  BAML FQN for its constructor, it emits `classValue { classTy: { name }, fields }`
  instead of a bare `mapValue`
- if a class instance is generic (its constructor carries `static $generic`), it
  emits `classValue { classTy: { name, typeArgs }, fields }`, where `typeArgs`
  are lowered from the instance's `$types` field (an unbound parameter lowers to
  `{ unknown: {} }`) and the `$types` field itself is dropped from `fields`. The
  engine rejects coercing a bare map into a generic class slot, so the FQN tag is
  required here.

The top-level `encodeCallArgs` wraps each kwarg in an `InboundMapEntry`, creates
the protobuf, and rolls back host-callable registrations if a later field fails:

```ts
export function encodeCallArgs(
  kwargs: Record<string, unknown>,
  options: EncodeCallArgsOptions, // { callId: bigint; syncMode?: boolean; typeArgs? }
): Buffer {
  if (options.callId === 0n) throw new Error("call_id must be nonzero");
  const ctx: EncodeCtx = { syncMode: options.syncMode ?? false, registered: [] };
  try {
    const entries: IInboundMapEntry[] = [];
    for (const [key, value] of Object.entries(kwargs)) {
      const iv: IInboundValue = {};
      setInboundValue(iv, value, ctx);
      entries.push({ stringKey: key, value: iv });
    }
    const msg = CallFunctionArgs.fromObject({
      kwargs: entries,
      callId: options.callId,
      typeArgs: options.typeArgs,
    });
    return Buffer.from(CallFunctionArgs.encode(msg).finish());
  } catch (err) {
    for (const k of ctx.registered) releaseHostCallable(k);
    throw err;
  }
}
```

Note that the snippets above shorten protobuf type names and elide the exact
error text, but preserve the implementation shape in
`bridge_nodejs/typescript_src/proto.ts`.

### Scalar Values

Current JS-to-protobuf mapping:

| JavaScript value | InboundValue field | Notes |
| --- | --- | --- |
| `null` | unset oneof | Rust decodes unset as `BexExternalValue::Null` |
| `undefined` | unset oneof | same as `null`, except omitted optional `$opts` keys are skipped before this point |
| `boolean` | `boolValue` | |
| integer `number` | `intValue` | selected by `Number.isInteger(value)` |
| non-integer `number` | `floatValue` | no finite/NaN guard in TS before protobuf encode |
| `bigint` | `bigintValue` | base-16 string, e.g. `-2a` |
| `string` | `stringValue` | |
| `Uint8Array` | `uint8arrayValue` | |

Rust decodes these in `bridge_ctypes::inbound_to_external`. Bigints are parsed
as strict base-16 with a large pre-allocation cap before becoming
`BexExternalValue::Bigint`.

### Handles, Media, Streams

Some JS runtime values are wrappers around native handle-table entries:

- `BamlHandle`
- `BamlStream`
- `BamlImage`
- `BamlAudio`
- `BamlVideo`
- `BamlPdf`

These encode as `InboundValue.handle`, not as maps. Media wrappers and streams
first expose or clone their backing `BamlHandle`; Rust drains or resolves the
handle-table entry into the corresponding `BexExternalValue`.

For host-callable handles, sync mode is rejected. A callable dispatch needs the
Node event loop, but sync calls block the main thread while Rust waits, so the
runtime fails early instead of hanging.

### Host Callables

If a plain JS `function` appears in an async call's kwargs, the encoder:

1. wraps it in a dispatch function
2. registers that wrapper with native `registerHostCallable`
3. gets back a `HandleKey`
4. emits `InboundValue.handle { key, handleType: HOST_VALUE_CALLABLE }`

When BAML invokes the host value, Rust schedules the registered
`ThreadsafeFunction` on the Node event loop with `(callId, argsBytes)`.
The TS dispatch wrapper:

1. decodes `argsBytes` as a bare `BamlOutboundValue` list
2. calls the user function
3. awaits promise-like results
4. encodes the returned value as an `InboundValue`
5. calls `completeHostCall(callId, isError, bytes)`

If the user function throws, the bridge sends an error `InboundValue`. A thrown
`BamlError` with a non-null `.value` attempts to encode that value directly as
the thrown BAML value. Other JS exceptions become a `baml.errors.HostCallable`
instance with metadata plus a handle to the original JS error. If that error
propagates back to the same Node process, outbound decoding can rehydrate and
rethrow the same JS object by identity.

The encoder tracks host-callable registrations made during a single encode. If
a later kwarg cannot be encoded, it releases earlier registrations so the engine
does not leak a host-value table entry it never received.

### Arrays

`Array.isArray(value)` encodes recursively as `listValue.values`.

The inbound list carries no precise element type from TypeScript. Rust initially
decodes it with a broad default scalar-union element type; later engine coercion
and validation use the declared BAML parameter type.

### Objects And Generated Classes

Objects are the most important rule for bridge-generics work.

Most objects, including generated class instances, encode as an untagged
`mapValue`:

```ts
new Resume({ name: "Ada" })  // InboundValue.mapValue { name: "Ada" }
```

The encoder uses `Object.entries(value)`, so generated classes work because
their constructor does `Object.assign(this, init)`.

Class instances also carry generated method bindings as own enumerable fields,
for example `greet = defineInstanceFunction(...).bind(this)`. Those are
behavior, not state. For non-plain class instances, function-valued own fields
are skipped during encode. Plain objects do not get this filtering, so nested
host callables in plain objects still encode as callables.

On the Rust side, a host `mapValue` passed to a BAML class parameter is promoted
to an `Instance` with the declared class name. For a union containing a class,
the engine routes a map/object to the first class arm. This means ordinary
*non-generic* class arguments do not carry TS constructor identity over the
wire; the declared parameter type supplies that identity.

There are two exceptions where the encoder emits an FQN-tagged `classValue`
instead of a bare `mapValue`:

- **Handle-backed instances.** If a non-plain class instance has a `BamlHandle`
  field, the encoder asks `getTypeMap().jsTypeToBamlType(value.constructor)` for
  a FQN and emits `classValue { classTy: { name: "<BAML FQN>" }, fields }`. This
  is used for handle-backed stdlib/generated values such as filesystem or HTTP
  objects, so the engine can re-bind the embedded native handle and preserve
  cursor/connection/body state across calls.
- **Generic instances.** If the constructor carries `static $generic`, the
  encoder emits `classValue { classTy: { name, typeArgs }, fields }`, lowering
  the instance's `$types` bindings to `type_args`. So a generic class *does*
  transmit its FQN and concrete type args inbound — `Wrapper<number>` and
  `Wrapper<string>` differ on the wire when `$types` is supplied. The engine
  rejects coercing a bare map into a generic class slot, which is why the tag is
  mandatory here.

### Enums

Generated TypeScript enums are string-valued. The generic inbound encoder has no
special enum branch, so an enum member normally encodes as a string. Any enum
argument support therefore comes from the BAML side accepting/coercing the
declared argument shape, not from a TypeScript enum-object tag in the protobuf.

Outbound enum return values are different: the engine emits an FQN-tagged
`enumValue`, and the outbound decoder uses the typemap to return the generated
enum member.

## Worked Type-Shape Examples

These examples come from:

- `baml_language/sdk_tests/fixtures/type_shapes/baml_src`
- `baml_language/sdk_tests/crates/typescript_node/type_shapes/generated`

They show three different host values and what the inbound path produces. The
important distinction is:

- TypeScript encoding produces a `baml_inbound.proto` `CallFunctionArgs`
- `bridge_ctypes::kwargs_to_bex_values` decodes those protobuf values into
  `BexExternalValue`
- `bex_engine::coerce_arg_to_declared_type` may then rewrite the top-level
  argument to match the function parameter type, most commonly by promoting a
  `Map` to an `Instance`

### Simple: `Primitives`

Fixture BAML:

```baml
class Primitives {
  int_field int
  float_field float
  string_field string
  bool_field bool
  null_field null
  uint8array_field uint8array
}

function round_trip_primitives(p: Primitives) -> Primitives {
  p
}
```

Generated SDK shape:

```ts
export class Primitives {
  int_field!: number;
  float_field!: number;
  string_field!: string;
  bool_field!: boolean;
  null_field!: null;
  uint8array_field!: Uint8Array;
  constructor(init: { ... }) {
    Object.assign(this, init);
  }
}

export const round_trip_primitives =
  defineFunction("user.primitives.round_trip_primitives", "sync", ["p"])
    as (p: Primitives) => Primitives;
```

Host value from `roundtrip_primitives.test.ts`:

```ts
const p = new Primitives({
  int_field: 1,
  float_field: 1.5,
  string_field: "s",
  bool_field: true,
  null_field: null,
  uint8array_field: new Uint8Array([97, 98]),
});
round_trip_primitives(p);
```

`defineFunction` first builds:

```ts
{ p }
```

`encodeCallArgs` then emits this logical protobuf shape:

```text
CallFunctionArgs {
  kwargs: [
    InboundMapEntry {
      string_key: "p"
      value: InboundValue {
        map_value: InboundMapValue {
          entries: [
            { string_key: "int_field",        value: { int_value: 1 } },
            { string_key: "float_field",      value: { float_value: 1.5 } },
            { string_key: "string_field",     value: { string_value: "s" } },
            { string_key: "bool_field",       value: { bool_value: true } },
            { string_key: "null_field",       value: { /* absent oneof = null */ } },
            { string_key: "uint8array_field", value: { uint8array_value: [97, 98] } },
          ]
        }
      }
    }
  ]
}
```

Even though `p` is a generated class instance, it is not handle-backed, so it
uses `map_value`, not `class_value`. The constructor assigned the fields as own
enumerable properties, and `Object.entries(p)` sees exactly those fields.

Rust direct decode produces:

```text
kwargs["p"] =
BexExternalValue::Map {
  key_type: string,
  value_type: int | float | string | bool | uint8array | null,
  entries: {
    "int_field":        Int(1),
    "float_field":      Float(1.5),
    "string_field":     String("s"),
    "bool_field":       Bool(true),
    "null_field":       Null,
    "uint8array_field": Uint8Array([97, 98]),
  }
}
```

Then declared-argument coercion sees the parameter type
`user.primitives.Primitives` and promotes only the top-level value:

```text
BexExternalValue::Instance {
  class_name: "user.primitives.Primitives",
  fields: { ...same entries... }
}
```

### Medium: `NestedGenerics`

Fixture BAML:

```baml
class Wrapper<T> {
  value T
}

class GenericLinkedList<T> {
  value T
  next GenericLinkedList<T>?
}

class NestedGenerics {
  ww Wrapper<Wrapper<int>>
  wl Wrapper<int[]>
  wr Wrapper<GenericLinkedList<int>>
}

function round_trip_nested_generics(n: NestedGenerics) -> NestedGenerics {
  n
}
```

Host value from `roundtrip_generics.test.ts`:

```ts
const n = new NestedGenerics({
  ww: new Wrapper({ value: new Wrapper({ value: 1 }) }),
  wl: new Wrapper({ value: [1, 2] }),
  wr: new Wrapper({
    value: new GenericLinkedList({ value: 9, next: null }),
  }),
});
round_trip_nested_generics(n);
```

`NestedGenerics` is a non-generic class, so it encodes as a bare `map_value`.
But `Wrapper` and `GenericLinkedList` are generic (their constructors carry
`static $generic`), so each encodes as an FQN-tagged `class_value` with
`class_ty.type_args`. The test does not bind `$types`, so each type arg lowers
to `{ unknown: {} }` — but the FQN and `class_value` envelope are present:

```text
CallFunctionArgs {
  call_id: <nonzero>
  kwargs: [
    {
      string_key: "n"
      value: {
        map_value: {
          entries: [
            {
              string_key: "ww"
              value: { class_value: {
                class_ty: { name: "user.generics.Wrapper", type_args: [{ unknown: {} }] }
                fields: [
                  { string_key: "value", value: { class_value: {
                    class_ty: { name: "user.generics.Wrapper", type_args: [{ unknown: {} }] }
                    fields: [{ string_key: "value", value: { int_value: 1 } }]
                  } } }
                ]
              } }
            },
            {
              string_key: "wl"
              value: { class_value: {
                class_ty: { name: "user.generics.Wrapper", type_args: [{ unknown: {} }] }
                fields: [
                  { string_key: "value", value: { list_value: {
                    values: [{ int_value: 1 }, { int_value: 2 }]
                  } } }
                ]
              } }
            },
            {
              string_key: "wr"
              value: { class_value: {
                class_ty: { name: "user.generics.Wrapper", type_args: [{ unknown: {} }] }
                fields: [
                  { string_key: "value", value: { class_value: {
                    class_ty: { name: "user.generics.GenericLinkedList", type_args: [{ unknown: {} }] }
                    fields: [
                      { string_key: "value", value: { int_value: 9 } },
                      { string_key: "next", value: { /* null */ } }
                    ]
                  } } }
                ]
              } }
            },
          ]
        }
      }
    }
  ]
}
```

The direct `BexExternalValue` decode: a top-level `Map` (non-generic
`NestedGenerics`), and an `Instance` for every generic `Wrapper` /
`GenericLinkedList` node, each carrying `type_args`:

```text
kwargs["n"] =
BexExternalValue::Map {
  entries: {
    "ww": Instance { class_name: "user.generics.Wrapper", type_args: [Unknown],
      fields: { "value": Instance { class_name: "user.generics.Wrapper", type_args: [Unknown],
        fields: { "value": Int(1) } } } },
    "wl": Instance { class_name: "user.generics.Wrapper", type_args: [Unknown],
      fields: { "value": Array {
        element_type: int | float | string | bool | uint8array | null,
        items: [Int(1), Int(2)]
      } } },
    "wr": Instance { class_name: "user.generics.Wrapper", type_args: [Unknown],
      fields: { "value": Instance { class_name: "user.generics.GenericLinkedList", type_args: [Unknown],
        fields: { "value": Int(9), "next": Null } } } }
  }
}
```

Declared-argument coercion promotes the top-level argument to:

```text
BexExternalValue::Instance {
  class_name: "user.generics.NestedGenerics",
  fields: {
    "ww": Instance { ... },
    "wl": Instance { ... },
    "wr": Instance { ... },
  }
}
```

The FQN and generic-slot structure now ride the inbound protobuf for generic
classes; the concrete *type argument* is only carried when the caller binds
`$types` (otherwise it lowers to `Unknown` and the declared parameter type plus
VM materialization supply the instantiation). So `Wrapper<number>` and
`Wrapper<string>` differ on the wire only when `$types` is supplied.

### High Complexity: `ComplexProfile`

Fixture BAML combines nested classes, lists, maps, optional fields, enum values,
literal unions, class unions, and mixed primitive unions:

```baml
enum AccountTier {
  Free,
  Pro,
  Enterprise,
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

function round_trip_complex_profile(profile: ComplexProfile) -> ComplexProfile {
  profile
}
```

Condensed host value from `roundtrip_complex_models.test.ts`:

```ts
const home = new PostalAddress({
  line1: "1 Compiler Way",
  line2: null,
  city: "San Francisco",
  region: "CA",
  postal_code: "94107",
  location: new GeoPoint({ lat: 37.7749, lng: -122.4194 }),
});

const invoice = new Invoice({
  id: "inv-001",
  status: "sent",
  items: [
    new LineItem({
      sku: "sdk-pro",
      quantity: 2,
      unit_price: 19.5,
      tags: ["sdk", "typescript"],
      attributes: { language: "ts", support: "priority" },
    }),
  ],
  payment: new CardPayment({
    brand: "visa",
    last4: "4242",
    billing_address: home,
  }),
  notes: "first invoice",
});

const profile = new ComplexProfile({
  id: "profile-001",
  tier: AccountTier.Enterprise,
  owner: new ProfileOwner({
    name: "Ada Lovelace",
    primary_contact: new ContactMethod({
      label: "email",
      value: "ada@example.com",
      verified: true,
    }),
    backup_contacts: [
      new ContactMethod({
        label: "phone",
        value: "+1-555-0100",
        verified: false,
      }),
    ],
  }),
  addresses: [home],
  invoices: [invoice],
  audit_trail: [
    new AuditEvent({
      actor: "system",
      action: "created",
      context: { source: "fixture" },
    }),
  ],
  metadata: { cohort: "beta", owner_kind: "internal" },
  featured: invoice,
  flags: [7, "manual-review", true],
});
round_trip_complex_profile(profile);
```

Interesting encoder decisions in this value:

- `tier: AccountTier.Enterprise` is just the string `"Enterprise"` because TS
  enums in this SDK are string-valued and there is no inbound enum branch
- `status: "sent"` and `action: "created"` are also strings; literal-union
  identity is not represented in protobuf
- `home`, `invoice`, `ProfileOwner`, `ContactMethod`, `LineItem`, `CardPayment`,
  and `AuditEvent` all encode as tagless nested `map_value`
- plain JS maps such as `metadata`, `context`, and `attributes` are also
  `map_value`; protobuf map entries have `string_key`
- `featured: invoice` is a class-union arm, but the wire shape is still just a
  `map_value` with invoice-shaped fields
- `flags` is a list whose children use their scalar oneof arms independently

The logical protobuf is large; this is the shape with repeated subtrees
collapsed:

```text
CallFunctionArgs {
  kwargs: [{
    string_key: "profile"
    value: { map_value: { entries: [
      { string_key: "id",   value: { string_value: "profile-001" } },
      { string_key: "tier", value: { string_value: "Enterprise" } },
      { string_key: "owner", value: { map_value: { entries: [
        { string_key: "name", value: { string_value: "Ada Lovelace" } },
        { string_key: "primary_contact", value: <ContactMethod-as-map> },
        { string_key: "backup_contacts", value: { list_value: {
          values: [<ContactMethod-as-map>]
        } } },
      ] } } },
      { string_key: "addresses", value: { list_value: {
        values: [<PostalAddress-as-map>]
      } } },
      { string_key: "invoices", value: { list_value: {
        values: [<Invoice-as-map>]
      } } },
      { string_key: "audit_trail", value: { list_value: {
        values: [<AuditEvent-as-map>]
      } } },
      { string_key: "metadata", value: { map_value: { entries: [
        { string_key: "cohort",     value: { string_value: "beta" } },
        { string_key: "owner_kind", value: { string_value: "internal" } },
      ] } } },
      { string_key: "featured", value: <Invoice-as-map> },
      { string_key: "flags", value: { list_value: {
        values: [
          { int_value: 7 },
          { string_value: "manual-review" },
          { bool_value: true },
        ]
      } } },
    ] } }
  }]
}
```

Expanded examples for two collapsed nodes:

```text
<PostalAddress-as-map> =
{ map_value: { entries: [
  { string_key: "line1",       value: { string_value: "1 Compiler Way" } },
  { string_key: "line2",       value: { /* null */ } },
  { string_key: "city",        value: { string_value: "San Francisco" } },
  { string_key: "region",      value: { string_value: "CA" } },
  { string_key: "postal_code", value: { string_value: "94107" } },
  { string_key: "location",    value: { map_value: { entries: [
    { string_key: "lat", value: { float_value: 37.7749 } },
    { string_key: "lng", value: { float_value: -122.4194 } },
  ] } } },
] } }

<Invoice-as-map> =
{ map_value: { entries: [
  { string_key: "id",     value: { string_value: "inv-001" } },
  { string_key: "status", value: { string_value: "sent" } },
  { string_key: "items",  value: { list_value: { values: [<LineItem-as-map>] } } },
  { string_key: "payment", value: { map_value: { entries: [
    { string_key: "brand", value: { string_value: "visa" } },
    { string_key: "last4", value: { string_value: "4242" } },
    { string_key: "billing_address", value: <PostalAddress-as-map> },
  ] } } },
  { string_key: "notes", value: { string_value: "first invoice" } },
] } }
```

The direct Rust decode is mechanically equivalent:

```text
kwargs["profile"] =
BexExternalValue::Map {
  entries: {
    "id": String("profile-001"),
    "tier": String("Enterprise"),
    "owner": Map { ... },
    "addresses": Array { items: [Map { ... }] },
    "invoices": Array { items: [Map { ... }] },
    "audit_trail": Array { items: [Map { ... }] },
    "metadata": Map {
      "cohort": String("beta"),
      "owner_kind": String("internal")
    },
    "featured": Map { ...invoice fields... },
    "flags": Array { items: [Int(7), String("manual-review"), Bool(true)] },
  }
}
```

Declared-argument coercion sees `profile: user.complex_models.ComplexProfile`
and promotes the top-level map to:

```text
BexExternalValue::Instance {
  class_name: "user.complex_models.ComplexProfile",
  fields: { ...same field values... }
}
```

Nested class identity is still not in the raw protobuf. It is recovered from
the compiled BAML parameter and field types as the engine materializes the VM
value. For unions such as `featured: Invoice | PostalAddress | string | null`,
the inbound object is just a map; the current incoming coercion rule routes a
map/object in a union to the first class arm.

## Rust Argument Decode

Node's native `BamlRuntime.callFunction*` receives already-encoded
`CallFunctionArgs` bytes. In `runtime.rs`:

1. protobuf bytes are decoded into `CallFunctionArgs`
2. `bridge_ctypes::kwargs_to_bex_values` decodes each `InboundValue`
3. the result becomes `BexArgs`
4. the engine call receives `BexArgs` plus a `FunctionCallContext`

Inbound maps and classes decode to `BexExternalValue::Map` and
`BexExternalValue::Instance` respectively. Handles are resolved through the
process-global handle table, except `HOST_VALUE_CALLABLE` and
`HOST_VALUE_OPAQUE`, which are host-owned values and are represented as
`BexExternalValue::HostValue`.

Before VM materialization, the engine runs argument coercion against the
declared BAML parameter type. The relevant current behavior:

- map to class parameter: promote to `Instance` with the declared class FQN
- instance to class parameter: rewrite the class FQN to the declared class FQN
- variant to enum parameter: rewrite the enum FQN to the declared enum FQN
- object/map to union with a class arm: route to the first class arm
- numerics/optionals/unions get additional coercion in the same path

Nested container element types are not walked by this host-boundary coercion.
The comment in `bex_engine/src/conversion.rs` states that host-side
schema-aware encoders own that shaping.

## Design Implications For Bridge Generics

The current Node inbound path is partly schema-aware and partly schema-free:

- generated TypeScript signatures know the apparent generic type
- non-generic, non-handle-backed objects encode as bare maps and transmit no
  class FQN, enum FQN, or type arguments — the declared parameter type supplies
  identity
- generic class instances (constructor with `static $generic`) and handle-backed
  instances *do* transmit an FQN via `classValue.class_ty`, and generic
  instances also carry `class_ty.type_args` lowered from the caller's `$types`
  binding

So TS argument encoding is now schema-aware for generic instances: a
`Wrapper<number>` argument and a `Wrapper<string>` argument differ on the wire
when the caller supplies `$types` (without a binding, the type arg lowers to
`Unknown` and the engine gives the value its meaning from the declared parameter
type and VM materialization path). Only non-generic, non-handle-backed classes
remain FQN-free maps.

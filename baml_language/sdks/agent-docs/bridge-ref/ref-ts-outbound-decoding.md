---
date: 2026-07-08
repository: baml4
runtime_package: "@boundaryml/baml-bridge"
implementation_root: baml_language/sdks/nodejs
source_files:
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/typemap.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/stream.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/errors.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/host_value_registry.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/wire_ty.ts
  - baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts
  - baml_language/crates/bridge_ctypes/src/value_encode.rs
  - baml_language/crates/bridge_cffi/src/baml_to_host.rs
  - baml_language/sdks/nodejs/sdkgen_typescript_node/src/emit/typemap_file.rs
---

# TypeScript Outbound Return Decoding

This file records how the current Node TypeScript SDK decodes values returned
from the BAML engine back into generated TypeScript SDK objects. "Outbound"
means BAML runtime to TypeScript/JavaScript. Rust encodes engine values into
protobuf envelopes, and `@boundaryml/baml-bridge` decodes those protobuf
bytes back into JS values.

The short version:

- `bridge_cffi::call_and_encode` calls the engine and returns a
  `BamlOutboundResult` protobuf envelope
- `bridge_ctypes::external_to_outbound` maps `BexExternalValue` into
  `BamlOutboundValue`
- `decodeCallResult` decodes ok/error/panic envelopes
- ok values become JS values through `decodeValueHolder`
- error and panic arms throw `BamlError` / `BamlPanic`
- generated classes/enums are reconstructed by a runtime `BamlTypeMap` that the
  generated SDK installs at import time

For the call-argument direction, see
[00a-prior-art-ts-inbound-encoding.md](sam-projects/bridge-ref/00a-prior-art-ts-inbound-encoding.md).

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

For outbound decoding, the typemap lets FQN-tagged outbound classes/enums become
generated class instances / enum members. If no generated SDK has installed a
typemap, the bare bridge falls back to plain objects and raw enum variant
strings.

## Runtime Return Encoding In Rust

`bridge_cffi::call_and_encode` is shared by Node, Python, and C-ABI paths. It:

1. calls `runtime.call_function(function_name, args, call_ctx)`
2. catches Rust panics around the engine call
3. converts the result/error/panic into `BamlOutboundResult`
4. returns protobuf bytes

The return envelope has three semantic arms:

- `ok`: carries a `BamlOutboundValue`
- `error`: carries a thrown BAML value plus BAML trace frames
- `panic`: carries a panic value plus trace frames and optional process-exit
  metadata

`bridge_ctypes::external_to_outbound` maps `BexExternalValue` into
`BamlOutboundValue`. Important outbound shapes:

| BexExternalValue | BamlOutboundValue field | Notes |
| --- | --- | --- |
| `Null` | unset oneof | decoded by TS as `null` |
| `Int` | `intValue` | decoded to JS `number` |
| `Bigint` | `bigintValue` | base-16 string decoded to JS `bigint` |
| `Float` | `floatValue` | decoded to JS `number` |
| `Bool` | `boolValue` | |
| `String` | `stringValue` | |
| `Uint8Array` | `uint8arrayValue` | |
| `Array` | `listValue` | includes item type metadata, but TS currently decodes just values |
| `Map` | `mapValue` | includes key/value type metadata, but TS currently decodes to a null-prototype object |
| `Instance` | `classValue` | includes BAML class FQN |
| `Variant` | `enumValue` | includes BAML enum FQN |
| `Union` | `unionVariantValue` | TS unwraps and decodes the inner value |
| media/stream/rust/host handles | `handleValue` | decoded to wrappers or `BamlHandle` |

Media and prompt AST can be serialized inline only when bridge options ask for
that. The in-process Node call path expects media/prompt AST to travel as
handles; if TS sees inline `mediaValue` or `promptAstValue`, it throws a
`BamlError` rather than silently returning `null`.

The core Rust implementation is direct structural recursion. Each
`BexExternalValue` variant picks the matching protobuf oneof arm; container
variants call `external_to_outbound` on their children and attach type metadata
from the BEX `Ty`:

```rust
pub fn external_to_outbound(
    value: &BexExternalValue,
    options: &CffiHandleTableOptions,
) -> Result<BamlOutboundValue, CtypesError> {
    let variant = match value {
        BexExternalValue::Null => None,
        BexExternalValue::Int(i) => Some(BamlValueVariant::IntValue(*i)),
        BexExternalValue::Bigint(bi) => {
            Some(BamlValueVariant::BigintValue(format!("{bi:x}")))
        }
        BexExternalValue::Float(f) => Some(BamlValueVariant::FloatValue(*f)),
        BexExternalValue::Bool(b) => Some(BamlValueVariant::BoolValue(*b)),
        BexExternalValue::String(s) => {
            Some(BamlValueVariant::StringValue(s.to_string()))
        }
        BexExternalValue::Uint8Array(bytes) => {
            Some(BamlValueVariant::Uint8arrayValue(bytes.clone()))
        }
        BexExternalValue::Array { items, element_type } => {
            let values: Result<Vec<BamlOutboundValue>, CtypesError> =
                items.iter().map(|v| external_to_outbound(v, options)).collect();
            Some(BamlValueVariant::ListValue(BamlValueList {
                item_type: Some(ty_to_field_type(element_type)),
                items: values?,
            }))
        }
        BexExternalValue::Map { entries, key_type, value_type } => {
            let mut baml_entries = Vec::new();
            for (key, val) in entries {
                baml_entries.push(BamlOutboundMapEntry {
                    key: key.clone(),
                    value: Some(external_to_outbound(val, options)?),
                });
            }
            Some(BamlValueVariant::MapValue(BamlValueMap {
                key_type: Some(ty_to_field_type(key_type)),
                value_type: Some(ty_to_field_type(value_type)),
                entries: baml_entries,
            }))
        }
        BexExternalValue::Instance { class_name, type_args, fields } => {
            let mut baml_fields = Vec::new();
            for (key, val) in fields {
                baml_fields.push(BamlOutboundMapEntry {
                    key: key.clone(),
                    value: Some(external_to_outbound(val, options)?),
                });
            }
            Some(BamlValueVariant::ClassValue(BamlValueClass {
                name: class_name.clone(),
                type_args: type_args.iter().map(crate::ty_encode::runtime_ty_to_proto_ty).collect(),
                fields: baml_fields,
            }))
        }
        BexExternalValue::Variant { enum_name, variant_name } => {
            Some(BamlValueVariant::EnumValue(BamlValueEnum {
                name: enum_name.clone(),
                value: variant_name.clone(),
                is_dynamic: false,
            }))
        }
        BexExternalValue::Union { value, metadata } => {
            let inner = external_to_outbound(value, options)?;
            Some(BamlValueVariant::UnionVariantValue(Box::new(
                BamlValueUnionVariant {
                    name: metadata.name.clone().unwrap_or_default(),
                    is_optional: metadata.is_optional,
                    is_single_pattern: metadata.is_single_pattern,
                    self_type: Some(ty_to_field_type(&metadata.union_type)),
                    value_option_name: format!("{}", metadata.selected_option),
                    value: Some(Box::new(inner)),
                },
            )))
        }
        // media, prompt AST, host values, rust data, handles elided here
    };

    Ok(BamlOutboundValue { value: variant })
}
```

The important detail for TS is that class and enum identity is encoded as a
runtime BAML FQN in the bare `name` string field (the old `BamlTyName` wrapper
with a nested `name` + `generic_args` was flattened away). Reified generics ride
on `BamlValueClass.type_args` (encoded via `runtime_ty_to_proto_ty`), and `BamlTy`
metadata rides on list, map, and union fields. The class/enum return decoder
still selects generated JS constructors by FQN, and additionally reconstructs a
`$types` field from `type_args` (see Class Decode).

## Outbound Return Decoding In TypeScript

`decodeCallResult(bytes)` decodes the `BamlOutboundResult` envelope.

For `ok`, it calls `decodeValueHolder(result.ok, getTypeMap())`; an absent or
empty ok holder decodes to `null`.

For `error`, it decodes the thrown value defensively and throws `BamlError`.
The thrown error carries:

- `.value`: fully decoded thrown BAML value, usually a generated class instance
  when mapped
- `.bamlTrace`: trace frame strings from the runtime
- `.className`: thrown BAML class FQN when known

For `panic`, it throws `BamlPanic` with the same structured detail. If the panic
is an exit panic, Node exits with the given code after telemetry flushing.

Two classes special-case a thrown/panicked `baml.panics.Cancelled` (constant
`CANCELLED_PANIC_CLASS`) on **both** the `error` and `panic` arms: it surfaces
as a `BamlAbortError` (`.name = 'AbortError'`) whose `reason` is a
`BamlCancelledError` (both defined in `errors.ts`). Unlike Python, Node does not
map `baml.errors.TypeMismatch` to a native `TypeError`.

### Value Holder Decode Rules

Current protobuf-to-JS mapping:

| BamlOutboundValue field | JavaScript result |
| --- | --- |
| unset / `nullValue` | `null` |
| `stringValue` | `string` |
| `intValue` | `number` |
| `bigintValue` | `bigint`, parsed from base-16 with a length cap |
| `floatValue` | `number` |
| `boolValue` | `boolean` |
| `uint8arrayValue` | `Uint8Array` |
| `literalValue` | inner JS primitive (`string` / `number` / `bigint` / `boolean`); the typed-literal envelope is discarded |
| `listValue` | `unknown[]` |
| `mapValue` | null-prototype plain object |
| `unionVariantValue` | decoded inner value; union metadata is dropped |
| `classValue` | generated class instance if FQN is mapped, else plain object; a generic class also gets a reconstructed `$types` field from `type_args` |
| `enumValue` | generated enum member if FQN/variant is mapped, else raw variant string |
| media handle (`ADT_MEDIA_*`) | `BamlImage` / `BamlAudio` / `BamlVideo` / `BamlPdf` |
| tagged-heap handle (`ADT_TAGGED_HEAP_HANDLE`) | typed wrapper resolved through the typemap keyed on `handleValue.ty.classTy.name` (e.g. `BamlStream`); bare `BamlHandle` only if unmapped |
| other handle | `BamlHandle` (`HANDLE_UNSPECIFIED` is rejected with a `BamlError`) |
| `tyValue` | **not decoded** — no branch, so a returned reflected type falls through to `null` |
| inline `mediaValue` / `promptAstValue` | throw `BamlError` |

The implementation is also recursive and mostly ignores type metadata once the
protobuf has decoded. The abridged `decodeValueHolder` body is:

```ts
function decodeValueHolder(
  holder: baml_bridge.cffi.v1.IBamlOutboundValue,
  typeMap: BamlTypeMap,
): unknown {
  if (holder.nullValue != null) return null;
  if (holder.stringValue != null) return holder.stringValue;
  if (holder.intValue != null) return Number(holder.intValue);
  if (holder.bigintValue != null) return parseHexBigint(holder.bigintValue);
  if (holder.floatValue != null) return holder.floatValue;
  if (holder.boolValue != null) return holder.boolValue;
  if (holder.uint8arrayValue != null) return holder.uint8arrayValue;
  if (holder.literalValue) return decodeLiteral(holder.literalValue); // string/int/bool/bigint/float oneof
  if (holder.classValue) return decodeClass(holder.classValue, typeMap);
  if (holder.enumValue) return decodeEnum(holder.enumValue, typeMap);
  if (holder.listValue) {
    return (holder.listValue.items || [])
      .map(item => decodeValueHolder(item, typeMap));
  }
  if (holder.mapValue) {
    const obj: Record<string, unknown> = Object.create(null);
    for (const entry of holder.mapValue.entries || []) {
      if (entry.key != null && entry.value) {
        obj[entry.key] = decodeValueHolder(entry.value, typeMap);
      }
    }
    return obj;
  }
  if (holder.unionVariantValue && holder.unionVariantValue.value) {
    return decodeValueHolder(holder.unionVariantValue.value, typeMap);
  }
  if (holder.handleValue) {
    const ht = holder.handleValue.handleType ?? 0;
    if (ht === BamlHandleType.HANDLE_UNSPECIFIED) {
      throw new BamlError("decoded handle has HANDLE_UNSPECIFIED handle_type");
    }
    // ADT_MEDIA_* → BamlImage/BamlAudio/BamlVideo/BamlPdf.
    // ADT_TAGGED_HEAP_HANDLE → resolve the wrapper class through the typemap
    //   keyed on holder.handleValue.ty.classTy.name and return Cls._fromHandle(handle)
    //   (this is how baml.llm.Stream auto-decodes to BamlStream).
    // everything else → bare BamlHandle.
    return decodeHandle(holder.handleValue, typeMap);
  }
  if (holder.mediaValue || holder.promptAstValue) throw new BamlError(...);
  // Note: there is no `tyValue` branch, so a returned reflected type falls here.
  return null;
}
```

This is the Node TypeScript equivalent of the Python bridge's `decode_value`
helper: it receives one outbound value holder, picks the populated oneof field,
and recursively returns the host-language value.

### Class Decode

`decodeClass` first recursively decodes all fields into a field dictionary. It
then reads `classValue.name` (a bare FQN string).

If the FQN exists in `BamlTypeMap`, the decoder resolves the generated class
constructor and runs:

```ts
new Ctor(fieldDict)
```

This is why decoded BAML classes are real generated class instances and can
host generated instance methods. The class constructor itself is intentionally
simple:

```ts
constructor(init: { ... }) {
  Object.assign(this, init);
}
```

If the resolved constructor is generic (it has a static `$generic`), the decoder
reads `classValue.type_args` and defines a non-enumerable `$types` on the
instance (mapping each `$generic` param to a `BamlType` via `outboundTyToBamlType`).
This is how a decoded generic instance carries its concrete type args back for
re-encoding. Constructor selection is still purely by FQN — `type_args` do not
pick a different constructor.

Stdlib media wrappers are special-cased: when a class decode resolves to
`BamlImage` / `BamlAudio` / `BamlVideo` / `BamlPdf` and the field dictionary
contains `_data`, the decoder returns `fieldDict._data`. The `_data` field has
already decoded from a media handle into the typed wrapper.

If the FQN is not in the typemap, the decoder returns a null-prototype plain
object with the decoded fields. This preserves bare-bridge behavior when no
generated SDK has installed a typemap.

### Enum Decode

`decodeEnum` reads the enum FQN and variant string. If the FQN resolves in the
typemap and the variant is a property on the generated enum object, the decoder
returns that enum member. Otherwise it returns the raw variant string.

Because generated TS enums are string-valued, the success case and fallback can
be observably equal for many callers, but the typemap still validates that the
variant exists on the generated enum object before indexing it.

## Type-Shape Examples

These examples come from:

- `baml_language/sdk_tests/fixtures/type_shapes/baml_src`
- `baml_language/sdk_tests/crates/typescript_node/type_shapes/generated`

They show three concrete `BexExternalValue` results, the protobuf-shaped
`baml_outbound.proto` value Rust writes, and the final host TS value. The
protobuf examples are reduced object shapes using the protobufjs field names;
they omit repeated subtrees and type metadata where those details do not change
the decode behavior being illustrated.

### Simple: `return_int`

Fixture:

```baml
function return_int() -> int {
  42
}
```

Generated TS:

```ts
export const return_int =
  defineFunction("user.primitives.return_int", "sync", []) as () => number;
```

The engine result is the scalar:

```rust
BexExternalValue::Int(42)
```

`external_to_outbound` maps it into the `ok` arm of `BamlOutboundResult`:

```ts
{
  ok: {
    intValue: 42,
  },
}
```

`decodeCallResult(bytes)` sees `result.result === "ok"` and calls
`decodeValueHolder(result.ok, getTypeMap())`. The first matching primitive arm
is:

```ts
if (holder.intValue != null) return Number(holder.intValue);
```

The host value is the JS number `42`; no typemap lookup is involved.

### Medium: `round_trip_list_container`

Fixture:

```baml
class ListContainer {
  ints int[]
  optional_strings string?[]
  union_list (int | string)[]
}

function round_trip_list_container(c: ListContainer) -> ListContainer {
  c
}
```

Test input:

```ts
const c = new ListContainer({
  ints: [1, 2],
  optional_strings: [null, "z"],
  union_list: [1, "x"],
});
expect(round_trip_list_container(c)).toEqual(c);
```

After inbound coercion and the identity function body, the runtime returns a
named BEX instance:

```rust
BexExternalValue::Instance {
    class_name: "user.lists.ListContainer".into(),
    fields: [
        ("ints", BexExternalValue::Array {
            element_type: Ty::Int,
            items: [Int(1), Int(2)],
        }),
        ("optional_strings", BexExternalValue::Array {
            element_type: Ty::Union(string | null),
            items: [Null, String("z")],
        }),
        ("union_list", BexExternalValue::Array {
            element_type: Ty::Union(int | string),
            items: [
                Union { value: Int(1), metadata: selected int },
                Union { value: String("x"), metadata: selected string },
            ],
        }),
    ],
}
```

Rust writes a class value whose fields recursively carry list values. The
`itemType` / `selfType` metadata is present on the wire, but TS uses it only as
opaque protobuf data and decodes the actual nested values:

```ts
{
  ok: {
    classValue: {
      name: "user.lists.ListContainer",
      fields: [
        {
          key: "ints",
          value: {
            listValue: {
              itemType: { intType: {} },
              items: [{ intValue: 1 }, { intValue: 2 }],
            },
          },
        },
        {
          key: "optional_strings",
          value: {
            listValue: {
              itemType: { optionalType: { value: { stringType: {} } } },
              items: [{}, { stringValue: "z" }],
            },
          },
        },
        {
          key: "union_list",
          value: {
            listValue: {
              itemType: { unionVariantType: {} },
              items: [
                {
                  unionVariantValue: {
                    valueOptionName: "int",
                    value: { intValue: 1 },
                  },
                },
                {
                  unionVariantValue: {
                    valueOptionName: "string",
                    value: { stringValue: "x" },
                  },
                },
              ],
            },
          },
        },
      ],
    },
  },
}
```

Decoding works inside-out:

1. `decodeClass` decodes `ints`, `optional_strings`, and `union_list` into a
   field dictionary.
2. `listValue.items` maps each child through `decodeValueHolder`.
3. Empty/null child holders decode to `null`.
4. `unionVariantValue` unwraps and decodes only `.value`, so the selected union
   metadata is dropped.
5. `typeMap.getClass("user.lists.ListContainer")` resolves the generated
   `ListContainer` constructor.
6. The final return is:

```ts
new ListContainer({
  ints: [1, 2],
  optional_strings: [null, "z"],
  union_list: [1, "x"],
})
```

The generated constructor is only:

```ts
constructor(init: {
  ints: number[];
  optional_strings: (string | null)[];
  union_list: (number | string)[];
}) {
  Object.assign(this, init);
}
```

So the decoded value is a real `ListContainer` instance, but its fields are
ordinary JS arrays and scalars.

### High: `round_trip_complex_profile`

Fixture excerpt:

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
```

Generated TS installs typemap entries for the involved FQNs:

```ts
"user.complex_models.ComplexProfile": () => ComplexProfile,
"user.complex_models.Invoice": () => Invoice,
"user.complex_models.CardPayment": () => CardPayment,
"user.complex_models.PostalAddress": () => PostalAddress,
"user.complex_models.GeoPoint": () => GeoPoint,
"user.complex_models.AccountTier": () => AccountTier,
```

The test builds a nested profile with:

- `tier: AccountTier.Enterprise`
- `owner.primary_contact` as a `ContactMethod`
- two `PostalAddress` values, one with a nested `GeoPoint`, one with
  `location: null`
- two `Invoice` values, one with `CardPayment`, one with `WirePayment`
- `metadata` and audit `context` maps
- `featured` as the first `Invoice`
- `flags: [7, "manual-review", true]`

The returned BEX value is a graph of named instances, enum variants, arrays,
maps, nullable fields, and union wrappers. A reduced but faithful shape is:

```rust
BexExternalValue::Instance {
    class_name: "user.complex_models.ComplexProfile".into(),
    fields: [
        ("id", String("profile-001")),
        ("tier", Variant {
            enum_name: "user.complex_models.AccountTier",
            variant_name: "Enterprise",
        }),
        ("owner", Instance {
            class_name: "user.complex_models.ProfileOwner",
            fields: [
                ("name", String("Ada Lovelace")),
                ("primary_contact", Instance {
                    class_name: "user.complex_models.ContactMethod",
                    fields: [
                        ("label", String("email")),
                        ("value", String("ada@example.com")),
                        ("verified", Bool(true)),
                    ],
                }),
                ("backup_contacts", Array {
                    element_type: Ty::Class(ContactMethod),
                    items: [Instance { class_name: ContactMethod, ... }],
                }),
            ],
        }),
        ("addresses", Array {
            element_type: Ty::Class(PostalAddress),
            items: [
                Instance {
                    class_name: "user.complex_models.PostalAddress",
                    fields: [
                        ("line1", String("1 Compiler Way")),
                        ("line2", Null),
                        ("city", String("San Francisco")),
                        ("region", String("CA")),
                        ("postal_code", String("94107")),
                        ("location", Union {
                            value: Instance {
                                class_name: "user.complex_models.GeoPoint",
                                fields: [
                                    ("lat", Float(37.7749)),
                                    ("lng", Float(-122.4194)),
                                ],
                            },
                            metadata: selected GeoPoint,
                        }),
                    ],
                },
                Instance { class_name: "user.complex_models.PostalAddress", ... },
            ],
        }),
        ("invoices", Array {
            element_type: Ty::Class(Invoice),
            items: [
                Instance {
                    class_name: "user.complex_models.Invoice",
                    fields: [
                        ("id", String("inv-001")),
                        ("status", Union {
                            value: String("sent"),
                            metadata: selected literal "sent",
                        }),
                        ("items", Array { items: [Instance { class_name: LineItem, ... }] }),
                        ("payment", Union {
                            value: Instance { class_name: "user.complex_models.CardPayment", ... },
                            metadata: selected CardPayment,
                        }),
                        ("notes", Union {
                            value: String("first invoice"),
                            metadata: selected string,
                        }),
                    ],
                },
                Instance { class_name: "user.complex_models.Invoice", ... },
            ],
        }),
        ("metadata", Map {
            key_type: Ty::String,
            value_type: Ty::String,
            entries: [
                ("cohort", String("beta")),
                ("owner_kind", String("internal")),
            ],
        }),
        ("featured", Union {
            value: Instance { class_name: "user.complex_models.Invoice", ... },
            metadata: selected Invoice,
        }),
        ("flags", Array {
            element_type: Ty::Union(int | string | bool),
            items: [
                Union { value: Int(7), metadata: selected int },
                Union { value: String("manual-review"), metadata: selected string },
                Union { value: Bool(true), metadata: selected bool },
            ],
        }),
    ],
}
```

The protobuf shape preserves the same structure. The top-level envelope starts:

```ts
{
  ok: {
    classValue: {
      name: "user.complex_models.ComplexProfile",
      fields: [
        { key: "id", value: { stringValue: "profile-001" } },
        {
          key: "tier",
          value: {
            enumValue: {
              name: "user.complex_models.AccountTier",
              value: "Enterprise",
              isDynamic: false,
            },
          },
        },
        {
          key: "owner",
          value: {
            classValue: {
              name: "user.complex_models.ProfileOwner",
              fields: [
                { key: "name", value: { stringValue: "Ada Lovelace" } },
                {
                  key: "primary_contact",
                  value: {
                    classValue: {
                      name: "user.complex_models.ContactMethod",
                      fields: [
                        { key: "label", value: { stringValue: "email" } },
                        { key: "value", value: { stringValue: "ada@example.com" } },
                        { key: "verified", value: { boolValue: true } },
                      ],
                    },
                  },
                },
                { key: "backup_contacts", value: { listValue: { items: [/* ContactMethod */] } } },
              ],
            },
          },
        },
        {
          key: "addresses",
          value: { listValue: { items: [/* PostalAddress */] } },
        },
        {
          key: "invoices",
          value: { listValue: { items: [/* Invoice */] } },
        },
        {
          key: "metadata",
          value: {
            mapValue: {
              keyType: { stringType: {} },
              valueType: { stringType: {} },
              entries: [
                { key: "cohort", value: { stringValue: "beta" } },
                { key: "owner_kind", value: { stringValue: "internal" } },
              ],
            },
          },
        },
        {
          key: "featured",
          value: {
            unionVariantValue: {
              valueOptionName: "Invoice",
              value: {
                classValue: {
                  name: "user.complex_models.Invoice",
                  fields: [/* ... */],
                },
              },
            },
          },
        },
        {
          key: "flags",
          value: {
            listValue: {
              items: [
                { unionVariantValue: { value: { intValue: 7 } } },
                { unionVariantValue: { value: { stringValue: "manual-review" } } },
                { unionVariantValue: { value: { boolValue: true } } },
              ],
            },
          },
        },
      ],
    },
  },
}
```

The host TS decode proceeds recursively:

1. `decodeClass(ComplexProfile)` builds `fieldDict`.
2. `tier` calls `decodeEnum`, looks up `user.complex_models.AccountTier`, checks
   that `"Enterprise"` is a member, and returns `AccountTier.Enterprise`.
3. `owner`, `addresses[*]`, `invoices[*]`, `payment`, and `featured` recursively
   call `decodeClass` and then instantiate the matching generated classes by
   FQN.
4. `metadata` and audit `context` decode into null-prototype objects; they still
   structurally match the generated `{ [key: string]: string }` field type.
5. Optional and union fields unwrap: `line2: null`, `location: GeoPoint | null`,
   `payment: CardPayment | WirePayment | null`, `featured`, and each `flags`
   element all return only the decoded selected value.
6. The top-level class FQN resolves to the generated constructor:

```ts
export class ComplexProfile {
  id!: string;
  tier!: AccountTier;
  owner!: ProfileOwner;
  addresses!: PostalAddress[];
  invoices!: Invoice[];
  audit_trail!: AuditEvent[];
  metadata!: { [key: string]: string };
  featured!: Invoice | PostalAddress | string | null;
  flags!: (number | string | boolean)[];
  constructor(init: { /* same fields */ }) {
    Object.assign(this, init);
  }
}
```

The final host value is:

```ts
new ComplexProfile({
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
  addresses: [
    new PostalAddress({
      line1: "1 Compiler Way",
      line2: null,
      city: "San Francisco",
      region: "CA",
      postal_code: "94107",
      location: new GeoPoint({ lat: 37.7749, lng: -122.4194 }),
    }),
    new PostalAddress({
      line1: "200 Type Lane",
      line2: "Suite 42",
      city: "Oakland",
      region: "CA",
      postal_code: "94612",
      location: null,
    }),
  ],
  invoices: [
    new Invoice({
      id: "inv-001",
      status: "sent",
      items: [
        new LineItem({
          sku: "sdk-pro",
          quantity: 2,
          unit_price: 19.5,
          tags: ["sdk", "typescript"],
          attributes: Object.assign(Object.create(null), {
            language: "ts",
            support: "priority",
          }),
        }),
      ],
      payment: new CardPayment({ /* brand, last4, billing_address */ }),
      notes: "first invoice",
    }),
    new Invoice({
      id: "inv-002",
      status: "paid",
      items: [/* LineItem */],
      payment: new WirePayment({
        bank_name: "Boundary Bank",
        routing_code: "110000000",
        reference: null,
      }),
      notes: null,
    }),
  ],
  audit_trail: [/* AuditEvent instances */],
  metadata: Object.assign(Object.create(null), {
    cohort: "beta",
    owner_kind: "internal",
  }),
  featured: new Invoice({ /* same values as inv-001 */ }),
  flags: [7, "manual-review", true],
})
```

`expect(round_trip_complex_profile(profile)).toEqual(profile)` passes because
the returned graph has the same field values. In the plain-object version of the
test, `toStrictEqual` intentionally fails: outbound decode reconstructs a fresh
`ComplexProfile` instance through the typemap, while the input was a literal
with `Object.prototype`.

## Host-Callable Outbound Decoding

Host-callable dispatch uses the outbound value decoder in a smaller shape. When
BAML invokes a JS function registered as a host callable, Rust sends its
positional arguments as a bare list-shaped `BamlOutboundValue`, not as a
`BamlOutboundResult` envelope. The TS dispatch wrapper decodes that value with
`decodeOutboundValue(argsBytes)` and expects the result to be an array.

If the host callable later throws and the resulting `baml.errors.HostCallable`
propagates back to the same Node process, `decodeCallResult` inspects its
decoded `_handle` field and rethrows the original JS error object by identity.
The rehydration is keyed off `HOST_VALUE_OPAQUE` handles and lives in
`host_value_registry.ts` (`registerHostOpaque` / `tryRehydrateHostValueByKey`).
Foreign runtimes or released handles fall back to a metadata-bearing
`BamlError`.

## Stream Wrapper Decode And Re-encode

`BamlStream<TStream, TFinal>` is a TypeScript wrapper around a handle whose
handle type is `ADT_TAGGED_HEAP_HANDLE`. Outbound tagged-heap handles now
auto-decode to their typed wrapper: `decodeHandle` reads the class FQN from
`holder.handleValue.ty.classTy.name`, resolves it through the typemap, and
returns `Cls._fromHandle(handle)` — so `baml.llm.Stream` decodes directly to a
`BamlStream` (and any self-registered tagged-heap class to its wrapper) without
a caller constructing it. Only truly unmapped handles stay bare `BamlHandle`.
The wrapper round-trips like this:

- `next()` calls `baml.llm.Stream.next` synchronously with `{ self: this }`
- `nextAsync()` calls the same function asynchronously
- `final()` and `finalAsync()` call `baml.llm.Stream.final`
- encoding `{ self: this }` uses `BamlStream._toHandle()` and sends a handle

The per-chunk result still goes through `decodeCallResult`, so stream chunk
classes and final classes decode through the installed typemap like ordinary
returns.

## Design Implications For Bridge Generics

The current Node outbound decoding path is FQN-driven:

- return decoding uses runtime FQNs from the engine and constructs generated
  classes from those FQNs
- tagged-heap handle values carry a full `BamlTy`, and the decoder resolves the
  wrapper class through the typemap keyed on `ty.classTy.name`
- the JS decoder sees FQN-tagged `classValue` / `enumValue` and instantiates
  the generated class or enum by FQN
- the JS decoder does not use `type_args` to select a distinct TypeScript
  constructor (selection is by FQN), but for a generic class it *does* read
  `type_args` and reconstruct a non-enumerable `$types` field on the instance

So a decoded generic instance carries its concrete type args back (via `$types`)
for later re-encoding, even though the constructor itself is chosen purely by
FQN. Class and enum identity still arrives as runtime FQNs, not as TypeScript
generic instantiations.

---
date: 2026-07-08
repository: baml4
source_fixtures:
  - baml_language/sdk_tests/fixtures/type_shapes/baml_src
  - baml_language/sdk_tests/fixtures/function_calls/baml_src
generated_fixtures:
  - baml_language/sdk_tests/crates/typescript_node/type_shapes/generated
  - baml_language/sdk_tests/crates/typescript_node/function_calls/generated
---

# TypeScript Codegen Examples From SDK Tests

This file records what the Node TypeScript generator actually emits in the SDK
tests, using these fixture pairs:

```text
baml_language/sdk_tests/fixtures/type_shapes/baml_src
baml_language/sdk_tests/crates/typescript_node/type_shapes/generated

baml_language/sdk_tests/fixtures/function_calls/baml_src
baml_language/sdk_tests/crates/typescript_node/function_calls/generated
```

The generated package is real TypeScript, not a runtime file plus a separate
declaration file. Each public namespace path is represented by a directory with
one `index.ts`; that file contains both runtime bindings and the typed surface.
There is no sibling `index.d.ts`.

The implementation has several consistent traits:

- root `baml_sdk/index.ts` initializes the runtime from bytecode with
  `initializeRuntimeFromBytecode(_inlinedbaml.BYTECODE)` and registers
  `_TYPE_MAP` with `setTypeMap(_TYPE_MAP)`
- imports between generated files use ESM `.js` specifiers, for example
  `export * as primitives from "./primitives/index.js"`
- child namespaces are re-exported as namespace objects; parent modules do not
  flatten child symbols
- reserved namespace names get a safe local import and renamed export; `void`
  is emitted as `import * as __ns_void from "./void/index.js"; export { __ns_void as void };`
- user classes are `export class` values with definite-assignment fields (`!`)
  and constructors that `Object.assign(this, init)`
- stream companion shapes are emitted in the same leaf module with a `$stream`
  suffix; there is no `stream_types/` tree
- free functions use `defineFunction`; static class functions are `static`
  fields using `defineFunction`; instance functions are class fields using
  `defineInstanceFunction(...).bind(this)`
- required parameters stay positional; optional BAML parameters are grouped in a
  trailing `$opts?: { ... } | undefined` object
- host-callable function parameters are rendered as TypeScript function types
  such as `(arg0: number) => string`
- generated JSDoc carries `@throws` annotations where the BAML surface exposes
  typed throw propagation

The snippets below are representative and come from the generated SDK test
outputs, not from the older Python-stub-derived sketch. Note: the committed
generated fixtures are pre-rename snapshots (they still import
`@boundaryml/baml-core-node` and predate the generics `$types` work). Where the
fixtures and the generator source (`sdkgen_typescript_node`) disagree, the
snippets below follow the generator source.

## File Layout

Type-shape fixture paths such as:

```text
fixtures/type_shapes/baml_src/root.baml
fixtures/type_shapes/baml_src/ns_primitives/types.baml
fixtures/type_shapes/baml_src/ns_symbol_collisions/ns_lorem/uses.baml
fixtures/type_shapes/baml_src/ns_generics/types.baml
```

produce TypeScript paths such as:

```text
type_shapes/generated/baml_sdk/index.ts
type_shapes/generated/baml_sdk/primitives/index.ts
type_shapes/generated/baml_sdk/symbol_collisions/lorem/index.ts
type_shapes/generated/baml_sdk/generics/index.ts
```

Function-call fixtures produce the same package shape in their own generated
directory:

```text
function_calls/generated/baml_sdk/index.ts
function_calls/generated/baml_sdk/methods_on_classes/index.ts
function_calls/generated/baml_sdk/host_callable_tests/index.ts
```

Container directories only expose child namespaces:

```ts
// type_shapes/generated/baml_sdk/symbol_collisions/index.ts
export * as a from "./a/index.js";
export * as fizz from "./fizz/index.js";
export * as foo from "./foo/index.js";
export * as lorem from "./lorem/index.js";

// type_shapes/generated/baml_sdk/symbol_collisions/fizz/index.ts
export * as buzz from "./buzz/index.js";
export * as foo from "./foo/index.js";
```

This preserves namespace boundaries. `symbol_collisions.lorem.Ipsum` is
reachable through `symbol_collisions.lorem`; it is not hoisted to
`symbol_collisions.Ipsum`.

## Root Package

From `type_shapes/generated/baml_sdk/index.ts`:

```ts
import { defineFunction, initializeRuntimeFromBytecode, setTypeMap } from "@boundaryml/baml-bridge";
import * as _inlinedbaml from "./_inlinedbaml.js";
import { _TYPE_MAP } from "./_typemap.js";

initializeRuntimeFromBytecode(_inlinedbaml.BYTECODE);
setTypeMap(_TYPE_MAP);

export * as a from "./a/index.js";
export * as aliases from "./aliases/index.js";
export * as aliases_consumer from "./aliases_consumer/index.js";
export * as baml from "./baml/index.js";
export * as class_refs from "./class_refs/index.js";
export * as complex_models from "./complex_models/index.js";
export * as enums from "./enums/index.js";
export * as forward_refs from "./forward_refs/index.js";
export * as generics from "./generics/index.js";
export * as ipsum from "./ipsum/index.js";
export * as lists from "./lists/index.js";
export * as literals from "./literals/index.js";
export * as lorem from "./lorem/index.js";
export * as maps from "./maps/index.js";
export * as media from "./media/index.js";
export * as optional from "./optional/index.js";
export * as primitives from "./primitives/index.js";
export * as recursion from "./recursion/index.js";
export * as symbol_collisions from "./symbol_collisions/index.js";
export * as unions from "./unions/index.js";
export * as vendor from "./vendor/index.js";
import * as __ns_void from "./void/index.js";
export { __ns_void as void };

export class Foo {
  v!: number;
  constructor(init: {
    v: number;
  }) {
    Object.assign(this, init);
  }
}

export class Foo$stream {
  v!: number | null;
  constructor(init: {
    v: number | null;
  }) {
    Object.assign(this, init);
  }
}

export const make_foo = defineFunction("user.make_foo", "sync", ["v"]) as (v: number) => Foo;
export const make_foo_async = defineFunction("user.make_foo", "async", ["v"]) as (v: number) => Promise<Foo>;
```

There is no generated `b` client object. The package module itself is the
client-like surface:

```ts
import "./baml_sdk/index.js"; // initializes runtime
import { make_foo } from "./baml_sdk/index.js";
import { return_int } from "./baml_sdk/primitives/index.js";

const foo = make_foo(1);
const n = return_int();
```

Users can also namespace-import the package and call through the namespace path:

```ts
import * as b from "./baml_sdk/index.js";

const foo = b.make_foo(1);
const n = b.primitives.return_int();
```

Wrong: these would require flattened child exports:

```ts
b.return_int();       // primitives.return_int hoisted to root
b.Ipsum;              // symbol_collisions.lorem.Ipsum hoisted to parent
```

## Type Map

Root initialization also imports `_typemap.ts`. That file imports each generated
leaf module and builds a `BamlTypeMap` from fully-qualified BAML names to the
runtime constructors:

```ts
import { BamlTypeMap } from "@boundaryml/baml-bridge";
import * as __leaf_0 from "./index.js";
import * as __leaf_31 from "./primitives/index.js";
import * as __leaf_37 from "./symbol_collisions/lorem/index.js";

const _CLASS_ENTRIES: Record<string, () => unknown> = {
  "user.Foo": () => (__leaf_0 as Record<string, unknown>)["Foo"],
  "user.Foo$stream": () => (__leaf_0 as Record<string, unknown>)["Foo$stream"],
  "user.primitives.Primitives": () => (__leaf_31 as Record<string, unknown>)["Primitives"],
  "user.primitives.Primitives$stream": () => (__leaf_31 as Record<string, unknown>)["Primitives$stream"],
  "user.symbol_collisions.lorem.Ipsum": () => (__leaf_37 as Record<string, unknown>)["Ipsum"],
  "user.symbol_collisions.lorem.Ipsum$stream": () => (__leaf_37 as Record<string, unknown>)["Ipsum$stream"],
};
```

The root calls `setTypeMap(_TYPE_MAP)` after runtime initialization so the
runtime can construct generated class values during decode.

## Primitive Namespace

From `type_shapes/generated/baml_sdk/primitives/index.ts`:

```ts
import { defineFunction } from "@boundaryml/baml-bridge";

export class Primitives {
  int_field!: number;
  float_field!: number;
  string_field!: string;
  bool_field!: boolean;
  null_field!: null;
  uint8array_field!: Uint8Array;
  constructor(init: {
    int_field: number;
    float_field: number;
    string_field: string;
    bool_field: boolean;
    null_field: null;
    uint8array_field: Uint8Array;
  }) {
    Object.assign(this, init);
  }
}

export class Primitives$stream {
  int_field!: number | null;
  float_field!: number | null;
  string_field!: string | null;
  bool_field!: boolean | null;
  null_field!: null;
  uint8array_field!: Uint8Array;
  constructor(init: {
    int_field: number | null;
    float_field: number | null;
    string_field: string | null;
    bool_field: boolean | null;
    null_field: null;
    uint8array_field: Uint8Array;
  }) {
    Object.assign(this, init);
  }
}

export const return_int = defineFunction("user.primitives.return_int", "sync", []) as () => number;
export const return_int_async = defineFunction("user.primitives.return_int", "async", []) as () => Promise<number>;

export const round_trip_uint8_array = defineFunction("user.primitives.round_trip_uint8_array", "sync", ["b"]) as (b: Uint8Array) => Uint8Array;
export const round_trip_primitives = defineFunction("user.primitives.round_trip_primitives", "sync", ["p"]) as (p: Primitives) => Primitives;
```

The generated tests import the root once for initialization and then import leaf
symbols directly:

```ts
import "./baml_sdk/index.js";
import { Primitives, return_int, round_trip_primitives } from "./baml_sdk/primitives/index.js";
```

## Enums And Aliases

Enums are emitted as string-valued TypeScript enums, and enum fields in stream
companions are nullable:

```ts
export class Enums$stream {
  bare_enum!: Sentiment | null;
  variant_as_type!: Sentiment | null;
  constructor(init: {
    bare_enum: Sentiment | null;
    variant_as_type: Sentiment | null;
  }) {
    Object.assign(this, init);
  }
}

export enum Sentiment {
  Positive = "Positive",
  Negative = "Negative",
}

export class Enums {
  bare_enum!: Sentiment;
  variant_as_type!: Sentiment;
  constructor(init: {
    bare_enum: Sentiment;
    variant_as_type: Sentiment;
  }) {
    Object.assign(this, init);
  }
}
```

Aliases are TypeScript `type` exports. Recursive aliases get matching stream
aliases:

```ts
export type RecList$stream = number | RecList$stream[];
export type RecList = number | RecList[];

export class AliasContainer$stream {
  list_field!: string[];
  rec_field!: number | RecList$stream[] | null;
  constructor(init: {
    list_field: string[];
    rec_field: number | RecList$stream[] | null;
  }) {
    Object.assign(this, init);
  }
}

export type StringList = string[];
export type StringList$stream = string[];
```

## Generics And Instance Methods

From `type_shapes/generated/baml_sdk/generics/index.ts`:

```ts
import { defineFunction, defineInstanceFunction, type BamlType } from "@boundaryml/baml-bridge";

export class WrapperMethods<T> {
  value!: T;
  $types?: { T?: BamlType };
  constructor(init: {
    value: T;
    $types?: { T?: BamlType };
  }) {
    Object.assign(this, init);
  }
  get_value = defineInstanceFunction("user.generics.WrapperMethods.get_value", "sync", ["self"], undefined, { classTypeParams: ["T"] }).bind(this) as () => T;
  get_value_async = defineInstanceFunction("user.generics.WrapperMethods.get_value", "async", ["self"], undefined, { classTypeParams: ["T"] }).bind(this) as () => Promise<T>;
  get_value_or_marker = defineInstanceFunction("user.generics.WrapperMethods.get_value_or_marker", "sync", ["self"], undefined, { classTypeParams: ["T"] }).bind(this) as () => T | WrapperMarker;
  get_value_or_marker_async = defineInstanceFunction("user.generics.WrapperMethods.get_value_or_marker", "async", ["self"], undefined, { classTypeParams: ["T"] }).bind(this) as () => Promise<T | WrapperMarker>;
  static readonly $generic = ["T"] as const;
}

export class WrapperMarker {
  reason!: string;
  constructor(init: {
    reason: string;
  }) {
    Object.assign(this, init);
  }
}

export const make_wrapper_methods = defineFunction("user.generics.make_wrapper_methods", "sync", ["text"]) as (text: string) => WrapperMethods<string>;

export class Box<T> {
  value!: T;
  wrapped!: Wrapper<T>;
  $types?: { T?: BamlType };
  constructor(init: {
    value: T;
    wrapped: Wrapper<T>;
    $types?: { T?: BamlType };
  }) {
    Object.assign(this, init);
  }
  static readonly $generic = ["T"] as const;
}

export const round_trip_box_int = defineFunction("user.generics.round_trip_box_int", "sync", ["b"]) as (b: Box<number>) => Box<number>;
```

Instance-method bindings are the class members themselves. There is no
module-level helper and no hand-written method that delegates to one. The
initializer binds the receiver at construction time, so the synthetic `self`
parameter appears in the runtime parameter-name list but not in the TypeScript
call signature.

Generic classes carry generic metadata in two places. A `static readonly
$generic = ["T"] as const;` declares the class's type parameters, and a
`$types?: { T?: BamlType }` field (in both the class body and the constructor
`init`) is the value-level channel through which a caller can bind a concrete
type argument. Bindings on (or returning) a generic type pass their parameters
as the 5th factory argument — `classTypeParams` for a method on a generic class
(with the optional-names slot filled by `undefined`), or `typeParams` for a
callable with its own `<...>` parameters. This is the TS analog of Python's
`_types=` / subscript binding; at call time the `$types` value resolves to the
wire `type_args`.

The type-shape test suite exercises the generic boundary directly (it is no
longer a skipped known-bug pin — the `$types` repopulation now recovers
`T=string` from the receiver, so the call arrives fully bound):

```ts
it("test_generic", () => {
  const w = make_wrapper_methods("hello");
  expect(w.get_value_or_marker()).toBe("hello");
});
```

The generated TypeScript surface represents the generic method type as
`() => T | WrapperMarker`.

## Static And Instance Class Functions

The function-call fixture `ns_methods_on_classes/types.baml` produces:

```ts
import { defineFunction, defineInstanceFunction } from "@boundaryml/baml-bridge";

export class Greeter {
  name!: string;
  constructor(init: {
    name: string;
  }) {
    Object.assign(this, init);
  }
  static create = defineFunction("user.methods_on_classes.Greeter.create", "sync", ["name"]) as (name: string) => Greeter;
  static create_async = defineFunction("user.methods_on_classes.Greeter.create", "async", ["name"]) as (name: string) => Promise<Greeter>;
  who = defineInstanceFunction("user.methods_on_classes.Greeter.who", "sync", ["self"]).bind(this) as () => string;
  who_async = defineInstanceFunction("user.methods_on_classes.Greeter.who", "async", ["self"]).bind(this) as () => Promise<string>;
  greet = defineInstanceFunction("user.methods_on_classes.Greeter.greet", "sync", ["self", "greeting"]).bind(this) as (greeting: string) => string;
  greet_async = defineInstanceFunction("user.methods_on_classes.Greeter.greet", "async", ["self", "greeting"]).bind(this) as (greeting: string) => Promise<string>;
}
```

The generated tests verify that static functions hang off the class and instance
functions hang off instances:

```ts
const g = await Greeter.create_async("ada");
g.who();
g.greet("hi");
```

## Optional Args

Optional BAML parameters are not emitted as additional positional TypeScript
parameters. They are grouped into a final `$opts` object whose keys are optional
and whose values accept `null | undefined` for nullable optional parameters.

From `function_calls/generated/baml_sdk/index.ts`:

```ts
export const optional_args_probe = defineFunction("user.optional_args_probe", "sync", ["arg0"], ["opt1", "opt2"]) as (
  arg0: number,
  $opts?: { opt1?: number | null | undefined; opt2?: number | null | undefined } | undefined,
) => (number | null)[];

export const optional_args_probe_async = defineFunction("user.optional_args_probe", "async", ["arg0"], ["opt1", "opt2"]) as (
  arg0: number,
  $opts?: { opt1?: number | null | undefined; opt2?: number | null | undefined } | undefined,
) => Promise<(number | null)[]>;

export class OptBox {
  base!: number;
  constructor(init: {
    base: number;
  }) {
    Object.assign(this, init);
  }
  static make = defineFunction("user.OptBox.make", "sync", ["base"], ["opt1"]) as (base: number, $opts?: { opt1?: number | null | undefined } | undefined) => OptBox;
  probe = defineInstanceFunction("user.OptBox.probe", "sync", ["self", "arg0"], ["opt1"]).bind(this) as (arg0: number, $opts?: { opt1?: number | null | undefined } | undefined) => (number | null)[];
}
```

The generated tests cover omission, explicit `undefined`, and explicit `null`:

```ts
optional_args_probe(1);
optional_args_probe(1, {});
optional_args_probe(1, undefined);
optional_args_probe(1, { opt1: undefined });
optional_args_probe(1, { opt1: null });
optional_args_probe(1, { opt2: 9 });
```

At runtime, unknown option keys and missing required positional arguments throw
when callers bypass TypeScript types.

## Host Callables And Throws

Host-callable parameters are normal TypeScript function types. From
`function_calls/generated/baml_sdk/host_callable_tests/index.ts`:

```ts
export const call_with_callback = defineFunction("user.host_callable_tests.call_with_callback", "sync", ["callback", "x"]) as (
  callback: (arg0: number) => string,
  x: number,
) => string;

export const call_with_two_args = defineFunction("user.host_callable_tests.call_with_two_args", "sync", ["callback", "x", "prefix"]) as (
  callback: (arg0: number, arg1: string) => string,
  x: number,
  prefix: string,
) => string;

export class Person {
  name!: string;
  age!: number;
  constructor(init: {
    name: string;
    age: number;
  }) {
    Object.assign(this, init);
  }
}

export const call_with_class_callback = defineFunction("user.host_callable_tests.call_with_class_callback", "sync", ["callback", "p"]) as (
  callback: (arg0: Person) => string,
  p: Person,
) => string;
```

Typed throw propagation is represented with JSDoc:

```ts
export class ValidationError {
  code!: number;
  message!: string;
  fields!: string[];
  constructor(init: {
    code: number;
    message: string;
    fields: string[];
  }) {
    Object.assign(this, init);
  }
}

/**
 * @throws ValidationError
 */
export const call_with_typed_throws_propagating = defineFunction("user.host_callable_tests.call_with_typed_throws_propagating", "sync", ["callback", "x"]) as (
  callback: (arg0: number) => string,
  x: number,
) => string;
```

## Cross-Namespace References

From `type_shapes/generated/baml_sdk/symbol_collisions/lorem/index.ts`:

```ts
import { defineFunction } from "@boundaryml/baml-bridge";
import type * as symbol_collisions from "../../symbol_collisions/index.js";

export class Ipsum {
  bar1!: symbol_collisions.foo.Bar;
  bar2!: symbol_collisions.fizz.foo.Bar;
  bar3!: symbol_collisions.fizz.buzz.foo.Bar;
  constructor(init: {
    bar1: symbol_collisions.foo.Bar;
    bar2: symbol_collisions.fizz.foo.Bar;
    bar3: symbol_collisions.fizz.buzz.foo.Bar;
  }) {
    Object.assign(this, init);
  }
}

export class Ipsum$stream {
  bar1!: symbol_collisions.foo.Bar$stream | null;
  bar2!: symbol_collisions.fizz.foo.Bar$stream | null;
  bar3!: symbol_collisions.fizz.buzz.foo.Bar$stream | null;
  constructor(init: {
    bar1: symbol_collisions.foo.Bar$stream | null;
    bar2: symbol_collisions.fizz.foo.Bar$stream | null;
    bar3: symbol_collisions.fizz.buzz.foo.Bar$stream | null;
  }) {
    Object.assign(this, init);
  }
}

export const make_ipsum = defineFunction("user.symbol_collisions.lorem.make_ipsum", "sync", ["bar1", "bar2", "bar3"]) as (
  bar1: symbol_collisions.foo.Bar,
  bar2: symbol_collisions.fizz.foo.Bar,
  bar3: symbol_collisions.fizz.buzz.foo.Bar,
) => Ipsum;
```

The `import type * as symbol_collisions` form lets leaf code reference sibling
and descendant namespaces without value imports and without flattening parent
exports.

## Stdlib Re-Exports

Some BAML stdlib values are runtime-owned and are re-exported from
`@boundaryml/baml-bridge` under their public generated names. In the current
implementation, media base classes are re-exported, while their stream companion
classes are generated handle wrappers:

```ts
import type { BamlHandle as _BamlHandle } from "@boundaryml/baml-bridge";

export class Image$stream {
  _data!: _BamlHandle;
  constructor(init: {
    _data: _BamlHandle;
  }) {
    Object.assign(this, init);
  }
}

import { BamlPdf as Pdf } from "@boundaryml/baml-bridge";
export { Pdf };

import { BamlAudio as Audio } from "@boundaryml/baml-bridge";
export { Audio };

import { BamlVideo as Video } from "@boundaryml/baml-bridge";
export { Video };

import { BamlImage as Image } from "@boundaryml/baml-bridge";
export { Image };
```

`baml.llm.Stream` is also re-exported from core, while `Stream$stream`,
`StreamAccumulator`, and `StreamCache` are generated classes in the same leaf:

```ts
import { BamlStream as Stream } from "@boundaryml/baml-bridge";
export { Stream };
```

This is narrower than "all stream-related types are runtime-owned": the base
`Stream` value is runtime-owned, but several adjacent `baml.llm` stream support
types are generated.

## Stream Companion Types

The Python codegen had a separate `stream_types/` package tree. TypeScript does
not. Each companion is emitted in the same leaf module using the `$stream`
suffix, which is a valid TypeScript identifier character.

Examples from `generics/index.ts`:

```ts
export class Wrapper$stream<T> {
  value!: T | null;
  $types?: { T?: BamlType };
  constructor(init: {
    value: T | null;
    $types?: { T?: BamlType };
  }) {
    Object.assign(this, init);
  }
  static readonly $generic = ["T"] as const;
}

export class GenericLinkedList$stream<T> {
  value!: T | null;
  next!: GenericLinkedList$stream<T> | null;
  $types?: { T?: BamlType };
  constructor(init: {
    value: T | null;
    next: GenericLinkedList$stream<T> | null;
    $types?: { T?: BamlType };
  }) {
    Object.assign(this, init);
  }
  static readonly $generic = ["T"] as const;
}

export class Box$stream<T> {
  value!: T | null;
  wrapped!: Wrapper$stream<T> | null;
  $types?: { T?: BamlType };
  constructor(init: {
    value: T | null;
    wrapped: Wrapper$stream<T> | null;
    $types?: { T?: BamlType };
  }) {
    Object.assign(this, init);
  }
  static readonly $generic = ["T"] as const;
}
```

Generic `$stream` companions carry the same `$types` field and `static $generic`
as their base types. Field references inside stream companions point at stream
companion types where appropriate (`Wrapper$stream<T>`,
`GenericLinkedList$stream<T>`). Stream companion classes do not emit static or
instance function bindings.

Emission order is not a semantic guarantee. Some generated files place stream
companions before base classes (`Enums$stream` before `Sentiment`/`Enums`,
several generic stream classes before their base classes); TypeScript permits
these forward type references.

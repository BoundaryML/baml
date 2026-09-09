# TypeScript interface API probes

This directory contains API models and real generated-SDK probes. The initial
commands below run models; the F50/F51 sections exercise the production bridge.

Run the strict positive and `@ts-expect-error` checks:

```sh
node /Users/aaron/projects/baml/node_modules/typescript/bin/tsc -p interface_probes/typescript/tsconfig.json --noEmit
```

Run the deliberately failing fixture (exit status 2 is expected):

```sh
node /Users/aaron/projects/baml/node_modules/typescript/bin/tsc \
  --target ES2022 --module NodeNext --moduleResolution NodeNext \
  --lib ES2022,DOM --strict --exactOptionalPropertyTypes \
  --noUncheckedIndexedAccess --noEmit interface_probes/typescript/negative.ts
```

Compile and run the runtime model:

```sh
node /Users/aaron/projects/baml/node_modules/typescript/bin/tsc \
  -p interface_probes/typescript/tsconfig.json
node interface_probes/typescript/dist/runtime.js
```

The runtime model uses an internal `WeakMap` as a stand-in for bridge-owned
projection state. Its Promise/reentrancy/cancellation checks are scheduler
simulations; they are not evidence that the current bridge implements them.

`cross-module/` models a generated package split across modules. The internal
brand symbol is exported only from a generated internal module so sibling
generated modules can share it; the public barrel does not re-export it. The
ordinary strict command above proves a provider facade imported through the
barrel satisfies `ClientInput` while a raw host does not. Run the standalone
negative fixture with:

```sh
node /Users/aaron/projects/baml/node_modules/typescript/bin/tsc \
  --target ES2022 --module NodeNext --moduleResolution NodeNext \
  --lib ES2022,DOM --strict --noEmit \
  interface_probes/typescript/cross-module/negative-consumer.ts
```

An internal symbol is a normal-consumer type boundary, not runtime authority.
Deep imports, `any`, or assertions can bypass it. Package exports should hide
the internal subpath, and the encoder must still validate bridge-owned state.

The `NoInfer` probe preserves an exact pin chosen by the annotated host. A
string-only producer is sound where a `string | number` producer is expected;
the issue is accidental joint inference changing the registered associated
type, rather than covariance itself being unsound.

`ergonomic-inputs.ts` models the synchronous `Greeter.implement(...)` factory
and lazy preparation by a generated async call. Compile and execute it with:

```sh
node /Users/aaron/projects/baml/node_modules/typescript/bin/tsc \
  -p interface_probes/typescript/tsconfig.json
node interface_probes/typescript/dist/ergonomic-inputs.js
```

The factory records explicit implementation intent but performs no runtime
registration. The first generated call registers it in that call's runtime;
later calls reuse its active identity. Existing refs are validated before the
implementation-wrapper path. Default methods go through the mock BAML
dispatcher. The model uses explicit ref `close()` for deterministic ownership;
a production `FinalizationRegistry` may only provide fallback release because
JavaScript does not guarantee when finalizers run.

`disposal.ts` uses actual TypeScript `using` and `await using` syntax, compiled
to ES2022 disposal helpers, and runs on the local Node runtime:

```sh
node /Users/aaron/projects/baml/node_modules/typescript/bin/tsc \
  -p interface_probes/typescript/tsconfig.json
node interface_probes/typescript/dist/disposal.js
node interface_probes/typescript/native-disposal-support.mjs
```

The native-support script reports the Node/V8 versions, disposal symbols, and
whether that runtime's parser accepts raw `using` syntax. It intentionally does
not assert that raw syntax remains unsupported on future Node versions.

`generated_sdk_context.ts` exercises actual generated code and the Node addon.
Use a temporary BAML project containing the shared
`sdk_tests/fixtures/interfaces/baml_src/main.baml` and
`interface_probes/baml/sdk_context.baml`. Generate `typescript/node` with
`output_dir = "generated"` and `naming_convention = "preserve-case"`, then copy
the TypeScript probe to that project's root. The project must resolve the
current built `@boundaryml/baml-bridge` package and Node type declarations.

```sh
tsc --target ES2022 --module NodeNext --moduleResolution NodeNext \
  --strict --skipLibCheck --outDir compiled generated_sdk_context.ts
node compiled/generated_sdk_context.js
```

This checks SDK initialization through the real ESM namespace/typemap imports,
root and nested generic calls after changing the default typemap, callback
record codecs, native callable pass-back, and rejection after replacing the
runtime. It uses ordinary copied records to isolate context behavior. It is not
evidence that the TypeScript generator emits callable interface refs yet.

### Generated interface and concrete callers (F50)

These probes exercise the real Node bridge and generated TypeScript, rather
than modeling the proposed API. Build the CLI and the Node bridge first. Use a
temporary project containing the shared interfaces fixture plus:

- `interface_probes/baml/sdk_context.baml`
- `interface_probes/baml/interface_inputs.baml`
- `interface_probes/baml/concrete_facades.baml`
- `interface_probes/baml/typescript_interface_names.baml`

Generate `typescript/node` with `output_dir = "generated"` and
`naming_convention = "preserve-case"`. Copy these probes into the project root:

- `generated_interfaces.ts`: interface calls, fixed associated types, media,
  inherited input views, callbacks, retained receivers, non-class iterators,
  copied records with live children and copied marker implementors.
- `generated_interfaces_negative.ts`: type-check only; each `@ts-expect-error`
  must correspond to a rejected use, including wrong pins, wrong methods,
  undeclared method-shaped objects and direct live-field mutation/construction.
- `generated_interface_inputs.ts`: distinct implementations of the same generic
  interface, rejected merged/wrong-error views, and generated-name collisions.
- `generated_concrete_methods.ts`: concrete/default/generic methods, concrete
  Self, exact generic record codecs after replacing the default map, local
  close/admitted calls, and offline ResponsesClient/Agent factories.

With the project resolving the freshly built bridge and Node type declarations:

```sh
tsc --target ES2022 --module NodeNext --moduleResolution NodeNext \
  --strict --skipLibCheck --outDir compiled \
  generated_interfaces.ts generated_interfaces_negative.ts \
  generated_interface_inputs.ts generated_concrete_methods.ts
node compiled/generated_interfaces.js
node compiled/generated_interface_inputs.js
node compiled/generated_concrete_methods.js
```

The negative source is compiled, never executed. Unused `@ts-expect-error`
markers fail compilation. No positive caller uses a cast to manufacture
implementation evidence. Assertions in the runtime probes check concrete
constructor identity as well as method results.

Check natural process exit as well as the success message. These programs need
no explicit GC, shutdown call or forced process exit. An earlier Node delivery
channel retained callbacks correctly but kept the process alive after all
assertions passed. The native `tests/callback_lifetime.test.ts` suite now runs
bounded child processes to check idle exit, pending work, callback re-entry,
retiring runtimes and background error delivery. That test verifies process
completion, not a memory bound for a long-running server.

For shared SDK execution, copy
`sdk_tests/crates/typescript/interfaces/customizable/interfaces.test.ts`
beside `generated/baml_sdk/`, type-check it, then run Vitest on that file.
The 19 declared cases use the same canonical names as their Python counterparts.
This remains a Node result: browser/worker ownership operations, general generic
evidence, host implementation registration and full callable disposal
still need implementation. The OpenAI/Agent probe makes no network request and
does not execute `Agent.run`.

### Checked interface projection and published declarations (F51)

Also add `interface_probes/baml/required_interfaces.baml` and
`interface_probes/baml/interface_projection.baml`, regenerate, and copy
`generated_interface_projection.ts` into the project. This checks a fully
specified declaring view of an unpinned interface, a generated record as an
associated result, SDK identity (including nested tokens), wrong Output/Error
rejection, and admission surviving local close. It also verifies the typed
primitive `reflect.Type.of` helper. Compile and run it like the consumers above.

Check the public declaration surface as well as source:

```sh
python3 interface_probes/typescript/check_generated_declarations.py /path/to/probe-project
```

The runner requires the four F50 consumer files and the F51 projection file.
It checks the original consumers, emits the SDK's `.d.ts` files into
`declaration_check/baml_sdk`, and checks copied consumers against those files.
All 21 marked invalid uses must still reject. The runtime package itself is
resolved through its built declarations too. This caught TypeScript erasing the
annotation on private generic fields; internal symbol-keyed evidence now keeps
the type relationship in the emitted declarations.

At F51, general method type inference, typed generic class/enum factories and
nested root-scoped conformance definitions remained open; F52 below advances the
explicit method and nominal factory paths. A raw reflected schema has
type `BamlType<unknown>`; it is not static evidence for a chosen native type.

### Typed generic method evidence (F52)

The checked-in F50 consumers now use `BamlType.from("string")` / `BamlType.from("int")`
for method `$types`. Interface choices use `CounterRef.type()`. The concrete probe
uses generated `FacadeRecordType(FacadeBoxType(...))` factories after replacing
the global typemap, plus enum evidence and a factory-name collision. Refresh
`concrete_facades.baml` and the shared fixture in the probe project before
regenerating. Copy the current consumer files as well.

The declaration runner now checks **31** expected rejection sites, including
wrong generic arguments/results, a falsely narrow string-literal result,
inconsistent native type arguments, callback result types, raw tokens and
missing generic class arguments. The same checks run against source and emitted
SDK declarations. Runtime tests separately cover JavaScript callers that bypass
static checking.

The shared TypeScript fixture has 21 canonical cases, including generic method
roundtrips and rejection of incompatible choices; matching Python declarations
exercise the same behaviors. Declaration parity does not establish browser or
other SDK execution. Full free-function inference, generic copied-record
constructor metadata and embedded conformance definitions remain open.

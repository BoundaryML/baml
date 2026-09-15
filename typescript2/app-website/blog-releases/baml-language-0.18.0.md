Thanks to everyone who contributed!

# Query your BAML execution history with SQL

BAML now records detailed execution profiles locally. You can query calls, timing, errors, arguments, outputs, and call paths with portable SQL. The new playground Telemetry tab explores the same profile store. No external telemetry service is required. ([#4548](https://github.com/BoundaryML/baml/pull/4548), [#4563](https://github.com/BoundaryML/baml/pull/4563), [#4578](https://github.com/BoundaryML/baml/pull/4578))

```shell
baml query "SELECT thread_id, status, total_errors, started_at FROM threads WHERE parent_thread_id IS NULL ORDER BY started_at DESC LIMIT 20"
baml query "SELECT fqn, sum(calls_started) AS calls, sum(self_ns) AS self_ns FROM call_path_stats GROUP BY fqn ORDER BY self_ns DESC LIMIT 10"
```

# Features

## Observe every LLM call with `on_event`

Direct calls, streams, and `ai.Agent` now accept the same `on_event` callback. The callback receives granular model, tool, usage, and lifecycle events. ([#4570](https://github.com/BoundaryML/baml/pull/4570))

```baml
let on_event = (event: ai.events.Event) -> void {
    baml.log.info(event.to_string())
};

let result = Extract("input", on_event = on_event);
let stream = Extract@stream("input", on_event = on_event);
let agent = ai.Agent.new(client = MyClient, on_event = on_event);
```

## Iterate over Python streams asynchronously

Python LLM streams now implement the asynchronous iterator protocol. Streaming parsing also processes ready deltas in batches for better throughput. ([#4604](https://github.com/BoundaryML/baml/pull/4604))

```python
stream = await StreamingExtract_stream_async("extract")
async for partial in stream:
    print(partial)
final = await stream.final_async()
```

## Use the new project-aware language server

The BAML editor extension now uses a new project-aware language server. It provides diagnostics, completion, hover, go-to-definition, references, symbols, semantic tokens, inlay hints, and code lenses. `baml ide install` materializes standard-library sources for go-to-definition and adds run code lenses for testsets. ([#4581](https://github.com/BoundaryML/baml/pull/4581))

## Migrate Python applications more easily

The Python SDK can preview prompts without provider secrets. It adds `FinishReasonError`, provider-specific response metadata, and other BAML v0 migration helpers. ([#4459](https://github.com/BoundaryML/baml/pull/4459))

## Reflect over runtime classes and generic functions

`reflect.AnyClass` provides checked, read-only access to static and runtime-created class values. Reflection can list generic functions, inspect their parameters, and specialize them before invocation. ([#4491](https://github.com/BoundaryML/baml/pull/4491), [#4493](https://github.com/BoundaryML/baml/pull/4493), [#4519](https://github.com/BoundaryML/baml/pull/4519))

## Use runtime types in more places

`unreflect(value)` can now appear in any type position where the runtime type does not escape its valid scope. `reflect.call_any<R>` also checks that the dynamic return value matches `R`. ([#4574](https://github.com/BoundaryML/baml/pull/4574), [#4600](https://github.com/BoundaryML/baml/pull/4600))

## Program against common I/O interfaces

`baml.io.Read` and `baml.io.Write` provide shared interfaces for files, network streams, subprocesses, byte arrays, and other standard I/O values. ([#4606](https://github.com/BoundaryML/baml/pull/4606))

## Serialize values typed as `unknown`

`baml.json.to_string(value)` and `baml.json.to_json(value)` inspect the runtime value, so they now work when its static type is `unknown`. ([#4601](https://github.com/BoundaryML/baml/pull/4601))

## Use truthy values in conditions

Conditions now accept truthy and falsy values such as numbers, strings, and collections. Branches narrow those values based on the condition. Strings also provide `is_empty()`. ([#4498](https://github.com/BoundaryML/baml/pull/4498))

```baml
if (items) {
    // items is non-empty here
}
```

## Project interface members

BAML now supports `(Type as Interface).member` and `Interface.method(instance)`. ([#4500](https://github.com/BoundaryML/baml/pull/4500))

## Infer stored lambda parameter types

Stored lambdas can infer omitted parameter types from a concrete later use. ([#4599](https://github.com/BoundaryML/baml/pull/4599))

## Preserve context when wrapping unknown errors

`baml.errors.UnknownError` wraps arbitrary failures while retaining the original cause and stack trace. ([#4441](https://github.com/BoundaryML/baml/pull/4441))

## Bound lazy iterators

Lazy iterators now support `take`, `skip`, `take_while`, and `skip_while`. The adapters can safely bound infinite iterators. ([#4510](https://github.com/BoundaryML/baml/pull/4510))

```baml
let first_five = baml.iter.Repeat.new(0).take(5).collect();
let page = baml.iter.Range.new(0, 100).skip(20).take(10).collect();
```

## Make primitive randomness reproducible

`int.random`, `float.random`, `bool.random`, and `bigint.random` accept an optional `rng` value. ([#4135](https://github.com/BoundaryML/baml/pull/4135))

## Control CLI language logs

`baml run` and `baml test` now respect `BAML_LOG` and `--log <LEVEL>`. The CLI also explains when shutdown is waiting for active futures. ([#4408](https://github.com/BoundaryML/baml/pull/4408), [#4409](https://github.com/BoundaryML/baml/pull/4409))

## Compile runtime packages faster

Runtime package compilation starts faster because the standard-library prefix is precompiled. ([#4453](https://github.com/BoundaryML/baml/pull/4453))

## Run compiler inference faster

Compiler inference reuses canonical facts and alias caches across more queries. ([#4458](https://github.com/BoundaryML/baml/pull/4458), [#4461](https://github.com/BoundaryML/baml/pull/4461), [#4463](https://github.com/BoundaryML/baml/pull/4463))

## Produce cleaner formatted code

`baml fmt` removes redundant parentheses in binary expressions, call arguments, and method receivers. ([#4489](https://github.com/BoundaryML/baml/pull/4489), [#4541](https://github.com/BoundaryML/baml/pull/4541))

## Detect incompatible generated artifacts

Generated programs, packed binaries, and mounted package interfaces now carry version and checksum metadata. Corrupt or mismatched artifacts fail early with guidance to regenerate the SDK and bridge from the same build. ([#4568](https://github.com/BoundaryML/baml/pull/4568))

# Breaking changes

## Move reflection APIs to `reflect.*`

Reflection is now a root package. Type views are also no longer accepted where an API requires a concrete `reflect.Type`. ([#4543](https://github.com/BoundaryML/baml/pull/4543), [#4580](https://github.com/BoundaryML/baml/pull/4580))

Update code like this:

```baml
let t = type.of<User>();
let package: baml.reflect.Package = baml.reflect.Package.compile(source);
```

To use the root package:

```baml
let t = reflect.Type.of<User>();
let package: reflect.Package = reflect.Package.compile(source);
```

Host code that matches reflected error class names must also change `baml.reflect.errors.*` strings to `reflect.errors.*`.

## Rename reflected field reads

Update every `reflect.class.Field.read<T>()` call to `value<T>()`. ([#4493](https://github.com/BoundaryML/baml/pull/4493))

```baml
// Before
let name = field.read<string>();

// After
let name = field.value<string>();
```

## Simplify JSON serialization

`baml.json.encode` has been removed. `baml.json.to_string` and `baml.json.to_json` no longer take explicit generic arguments. ([#4601](https://github.com/BoundaryML/baml/pull/4601))

```baml
// Before
let text = baml.json.encode(value);
let json_value = baml.json.to_json<MyType>(value);

// After
let text = baml.json.to_string(value);
let json_value = baml.json.to_json(value);
```

## Replace hash-delimited templates

Hash-delimited Jinja string literals are no longer supported. Use BAML template literals. ([#4565](https://github.com/BoundaryML/baml/pull/4565))

```baml
// Before
#"Hello {{ name }}"#

// After
`Hello ${name}`
```

## Rewrite legacy test declarations

BAML v0 test declarations have been removed. Tests must use BAML v1 expression bodies. ([#4602](https://github.com/BoundaryML/baml/pull/4602))

```baml
// Before
test ClassifyMessage {
    functions [Classify1, Classify2]
    args { input "hello" }
}

// After
test "classifies a message" {
    let result = Classify1("hello");
    assert.equal(result, Expected)
}
```

## Call `ctx.output_format`

`ctx.output_format` is now a method. The method also restores controls for prefixes, enum formatting, class hoisting, map style, and null rendering. ([#4567](https://github.com/BoundaryML/baml/pull/4567))

```baml
// Before
`${ctx.output_format}`

// After
`${ctx.output_format()}`
```

## Update generated SDK paths

The default `baml_sdk` output directory is now beside `baml.toml`, not in its parent directory. Update imports or configure an explicit output directory if the old layout is required. ([#4522](https://github.com/BoundaryML/baml/pull/4522))

## Rename the generated C# namespace

Generated C# clients now use `baml_sdk` instead of `baml_client`. Update C# namespace imports and references. ([#4535](https://github.com/BoundaryML/baml/pull/4535))

## Remove output generics from agents and runners

`ai.Agent<Out>` and `ai.Runner<Out>` become `ai.Agent` and `ai.Runner`. The output type is inferred by `run<Out>`. Low-level `TurnStream.next()` consumers must also handle `string[] | Done` instead of `string | Done`. ([#4570](https://github.com/BoundaryML/baml/pull/4570), [#4604](https://github.com/BoundaryML/baml/pull/4604))

```baml
// Before
let runner: ai.Runner<Result> = make_runner();

// After
let runner: ai.Runner = make_runner();
let result: ai.RunResult<Result> = runner.run(spec);
```

## Specialize generic functions before storing them

Stored function values are monomorphic. A bare generic function can no longer be assigned to a local and called with different inferred types. Direct calls remain generic. ([#4621](https://github.com/BoundaryML/baml/pull/4621))

```baml
// Before
let copy = identity;

// After: explicit specialization
let copy = identity<string>;

// Or provide a concrete function type
let copy: (string) -> string throws never = identity;
```

## Make `throws unknown` precise

`throws unknown` is rejected when a function throws nothing or only known, narrower types. Remove the declaration and use inferred throws, or declare the precise error type. Genuine unknown error boundaries can still use `throws unknown`. ([#4593](https://github.com/BoundaryML/baml/pull/4593))

```baml
// Before
function parse() -> int throws unknown {
    throw "invalid"
}

// After
function parse() -> int throws string {
    throw "invalid"
}
```

# Bug fixes

- Fixed: unsupported runtime-dependent checks now receive a compiler diagnostic instead of panicking or being silently omitted in release builds. ([#4460](https://github.com/BoundaryML/baml/pull/4460))
- Fixed: member access and calls on `unknown` now report a compiler error instead of reaching an internal VM error. ([#4466](https://github.com/BoundaryML/baml/pull/4466))
- Fixed: literal patterns use type-membership semantics instead of runtime equality. ([#4478](https://github.com/BoundaryML/baml/pull/4478))
- Fixed: object mutations performed by callees inside loops persist on the original value. ([#4467](https://github.com/BoundaryML/baml/pull/4467))
- Fixed: non-data LLM output schemas produce catchable diagnostics instead of crashes or silent schema omission. ([#4470](https://github.com/BoundaryML/baml/pull/4470))
- Fixed: invalid dynamic use of unspecialized reflected generics produces a targeted error. ([#4473](https://github.com/BoundaryML/baml/pull/4473))
- Fixed: `for` loops over joined collection arms and `Iterable`-bounded generics no longer abort the compiler. ([#4490](https://github.com/BoundaryML/baml/pull/4490))
- Fixed: optional method calls preserve explicit and inferred type arguments. ([#4495](https://github.com/BoundaryML/baml/pull/4495))
- Fixed: runtime-created and compiled-package types retain their identity, definitions, and source names through interface dispatch. ([#4501](https://github.com/BoundaryML/baml/pull/4501), [#4516](https://github.com/BoundaryML/baml/pull/4516), [#4536](https://github.com/BoundaryML/baml/pull/4536))
- Fixed: invalid inline `unreflect` types receive actionable diagnostics when they would escape through return or thrown types. ([#4518](https://github.com/BoundaryML/baml/pull/4518), [#4530](https://github.com/BoundaryML/baml/pull/4530))
- Fixed: method calls work on global bindings, and session assignments preserve each binding's type. ([#4529](https://github.com/BoundaryML/baml/pull/4529), [#4531](https://github.com/BoundaryML/baml/pull/4531))
- Fixed: unsupported `naming_convention = "language"` SDK generation returns a normal error instead of panicking. ([#4526](https://github.com/BoundaryML/baml/pull/4526))
- Fixed: reassigned short-circuit locals and branch-carried locals keep correct values after optimization. ([#4508](https://github.com/BoundaryML/baml/pull/4508), [#4544](https://github.com/BoundaryML/baml/pull/4544))
- Fixed: match exhaustiveness preserves array type ascriptions. ([#4547](https://github.com/BoundaryML/baml/pull/4547))
- Fixed: runtime-created types survive reflection and host boundaries without name collisions or lost identity. ([#4560](https://github.com/BoundaryML/baml/pull/4560), [#4577](https://github.com/BoundaryML/baml/pull/4577))
- Fixed: type diagnostics point to the correct signature or default expression and no longer crash rendering. ([#4566](https://github.com/BoundaryML/baml/pull/4566))
- Fixed: ambiguous empty arrays and maps receive a compiler diagnostic. ([#4573](https://github.com/BoundaryML/baml/pull/4573))
- Fixed: rejected reflection compile artifacts no longer poison sessions, and invalid artifact reuse fails clearly. ([#4571](https://github.com/BoundaryML/baml/pull/4571))
- Fixed: `eval()` preserves original runtime diagnostics and reflected schemas. ([#4583](https://github.com/BoundaryML/baml/pull/4583))
- Fixed: invalid compiler types no longer degrade into `unknown`, and incompatible numeric compound assignments are rejected. ([#4603](https://github.com/BoundaryML/baml/pull/4603))
- Fixed: HTTP operations work again in `baml run` and the playground. ([#4609](https://github.com/BoundaryML/baml/pull/4609))
- Fixed: optional nested class values survive LLM response parsing when omitted fields are valid. ([#4612](https://github.com/BoundaryML/baml/pull/4612))
- Fixed: class constructors that omit required fields receive a diagnostic listing the missing fields. ([#4619](https://github.com/BoundaryML/baml/pull/4619))

# Step 2a: user effects by PR

## [#4453](https://github.com/BoundaryML/baml/pull/4453)

- Runtime calls to `reflect.Package.compile` start faster because the standard-library compiler prefix is precompiled.

## [#4458](https://github.com/BoundaryML/baml/pull/4458)

- Repeated compiler queries avoid recomputing canonical body-inference facts.
- Compilation is faster without changing valid program behavior.

## [#4460](https://github.com/BoundaryML/baml/pull/4460)

- Unsupported runtime-checked arguments on indirect calls now receive an `E0010` diagnostic.
- Release builds no longer silently omit those runtime checks.
- Runtime-created type names no longer leak through mounted-package diagnostics.

## [#4461](https://github.com/BoundaryML/baml/pull/4461)

- Compiler inference caches are reused across more queries.
- Compilation is faster without changing valid program behavior.

## [#4463](https://github.com/BoundaryML/baml/pull/4463)

- Implementation scans reuse alias inference results.
- Compilation is faster for code with aliases and implementations.

## [#4408](https://github.com/BoundaryML/baml/pull/4408)

- The CLI logs when shutdown is waiting for spawned work to finish.
- Users can identify background tasks that delay process exit.

## [#4409](https://github.com/BoundaryML/baml/pull/4409)

- `baml run` and `baml test` surface language log events.
- `BAML_LOG` and `--log <LEVEL>` control those events.

## [#4466](https://github.com/BoundaryML/baml/pull/4466)

- Member access and method calls on `unknown` produce a compiler diagnostic.
- Invalid calls no longer reach an internal VM error.

## [#4478](https://github.com/BoundaryML/baml/pull/4478)

- Literal patterns now test whether a value belongs to the literal type.
- They no longer behave like a runtime `==` comparison.

## [#4489](https://github.com/BoundaryML/baml/pull/4489)

- `baml fmt` removes redundant parentheses in binary chains and call arguments.

## [#4467](https://github.com/BoundaryML/baml/pull/4467)

- Mutations to maps, arrays, and class values persist when a called function mutates them inside a loop.
- The compiler no longer reallocates the pre-loop value on every iteration.

## [#4470](https://github.com/BoundaryML/baml/pull/4470)

- Non-data types such as function values and raw type values cannot be used as LLM output schemas.
- Invalid schemas produce catchable diagnostics instead of aborting or silently omitting the schema.

## [#4473](https://github.com/BoundaryML/baml/pull/4473)

- Extracting or dynamically invoking an unspecialized generic function produces a targeted reflection diagnostic.
- Generated generic companions remain discoverable.

## [#4491](https://github.com/BoundaryML/baml/pull/4491)

- `reflect.AnyClass` can hold either a static class value or a runtime-created class value.
- Users can inspect fields and read their values through a checked reflection API.

## [#4493](https://github.com/BoundaryML/baml/pull/4493)

- `reflect.class.Field.read<T>()` is renamed to `value<T>()`.
- Invalid construction of built-in companion classes produces a compiler diagnostic.
- Infinite `while (true)` loops without reachable breaks count as diverging control flow.

## [#4490](https://github.com/BoundaryML/baml/pull/4490)

- `for` loops work over collection values joined from conditional arms.
- `for` loops work over values bounded by `baml.iter.Iterable`.
- These cases no longer abort the compiler.

## [#4495](https://github.com/BoundaryML/baml/pull/4495)

- Optional-chained method calls preserve explicit and inferred type arguments.
- Calls such as `value?.method<T>()` no longer fail with an internal frame error.

## [#4501](https://github.com/BoundaryML/baml/pull/4501)

- Runtime-defined types retain their definitions through interface dispatch.
- Nested type views and pending reflection fields keep the correct metadata.

## [#4498](https://github.com/BoundaryML/baml/pull/4498)

- Conditions accept truthy and falsy values such as numbers, strings, and collections.
- `if` branches narrow types based on truthiness.
- Strings provide an `is_empty()` helper.

## [#4518](https://github.com/BoundaryML/baml/pull/4518)

- Inline `unreflect(value)` type arguments that escape into a published result receive `E0168`.
- The diagnostic suggests binding the runtime type to a named type first.

## [#4516](https://github.com/BoundaryML/baml/pull/4516)

- Runtime-created class and enum types preserve identity through interface calls.
- Identity-keyed type registries no longer miss values inside implementations.

## [#4519](https://github.com/BoundaryML/baml/pull/4519)

- `reflect.Package.functions()` lists generic functions.
- Reflection exposes generic parameter inspection and explicit specialization.
- Specialized reflected functions can be invoked with runtime-created type arguments.

## [#4529](https://github.com/BoundaryML/baml/pull/4529)

- Method calls work on top-level bindings such as declared clients.
- Session `let` bindings widen fresh literal values to their ordinary base types.

## [#4531](https://github.com/BoundaryML/baml/pull/4531)

- Assignments inside a reflection session are checked against the binding's type.
- Invalid assignments fail during compilation instead of corrupting the session value.

## [#4530](https://github.com/BoundaryML/baml/pull/4530)

- The `unreflect` escape diagnostic also covers thrown types and optional-chained return types.
- Naming the runtime type before use remains the supported migration.

## [#4441](https://github.com/BoundaryML/baml/pull/4441)

- `baml.errors.UnknownError` wraps arbitrary errors while preserving their cause and stack trace.
- Users can normalize unknown failures without discarding debugging context.

## [#4535](https://github.com/BoundaryML/baml/pull/4535)

- Generated C# code uses the `baml_sdk` namespace instead of `baml_client`.

## [#4459](https://github.com/BoundaryML/baml/pull/4459)

- Python can preview prompts without requiring provider secrets.
- Python exposes `FinishReasonError` and provider-specific response metadata.
- Python gains migration helpers for BAML v0 applications.

## [#4526](https://github.com/BoundaryML/baml/pull/4526)

- SDK generation with unsupported `naming_convention = "language"` returns a normal error.
- The generator no longer panics.

## [#4510](https://github.com/BoundaryML/baml/pull/4510)

- Lazy iterators support `take`, `skip`, `take_while`, and `skip_while`.
- These adapters can bound infinite iterators without eagerly consuming them.

## [#4536](https://github.com/BoundaryML/baml/pull/4536)

- Types from a compiled package keep their package-specific identity through interface dispatch.
- Runtime diagnostics and JSON conversion display source names instead of internal names.

## [#4500](https://github.com/BoundaryML/baml/pull/4500)

- Users can access interface members through `(Type as Interface).member`.
- Users can call instance methods through `Interface.method(instance)`.

## [#4508](https://github.com/BoundaryML/baml/pull/4508)

- Reassigned locals used by short-circuit expressions retain their values.
- Valid code no longer reads an uninitialized or stale local after optimization.

## [#4544](https://github.com/BoundaryML/baml/pull/4544)

- Locals carried across branches and loops are initialized using all predecessor paths.
- Valid control-flow code no longer fails because of an incorrect stack-local proof.

## [#4522](https://github.com/BoundaryML/baml/pull/4522)

- Generated SDKs are written beside `baml.toml` by default.
- Projects that expected `baml_sdk` in the parent directory must update imports or generator output settings.

## [#4547](https://github.com/BoundaryML/baml/pull/4547)

- Match exhaustiveness preserves authored array type ascriptions.
- Array arms are no longer treated as the wrong shape during coverage analysis.

## [#4548](https://github.com/BoundaryML/baml/pull/4548)

- BAML records local execution profiles in a segmented store.
- Profiles retain call-tree and exact execution evidence for later queries.

## [#4541](https://github.com/BoundaryML/baml/pull/4541)

- `baml fmt` removes redundant parentheses around method-call receivers.
- Valid `unreflect` expressions no longer trigger a MIR compiler crash.

## [#4543](https://github.com/BoundaryML/baml/pull/4543)

- Reflection moves from `baml.reflect.*` to the root `reflect.*` package.
- `type.of<T>()` becomes `reflect.Type.of<T>()`.
- Reflection error class names exposed to host SDKs also move to `reflect.errors.*`.

## [#4560](https://github.com/BoundaryML/baml/pull/4560)

- Runtime-created type declarations have stable identity even when names collide.
- Runtime-defined class and enum values can safely cross reflection and host boundaries.
- Recursive type aliases work in more runtime reflection paths.

## [#4565](https://github.com/BoundaryML/baml/pull/4565)

- Hash-delimited Jinja string literals such as `#"..."#` are rejected.
- LLM prompts must use BAML template literals.

## [#4566](https://github.com/BoundaryML/baml/pull/4566)

- Type errors in function signatures and parameter defaults point to the correct source range.
- Unresolved types no longer panic diagnostic rendering.

## [#4135](https://github.com/BoundaryML/baml/pull/4135)

- `int.random`, `float.random`, `bool.random`, and `bigint.random` accept an optional `rng` argument.
- Users can make primitive random generation reproducible.

## [#4580](https://github.com/BoundaryML/baml/pull/4580)

- Reflection type views are no longer accepted where a concrete `reflect.Type` is required.
- Users must resolve or construct a concrete reflected type before passing it to those APIs.

## [#4570](https://github.com/BoundaryML/baml/pull/4570)

- Direct LLM calls, streams, and `ai.Agent` accept the same `on_event` callback.
- The callback exposes granular model, tool, usage, and lifecycle events.
- `ai.Agent` no longer takes an output type parameter.

## [#4563](https://github.com/BoundaryML/baml/pull/4563)

- `baml query` executes SQL against local execution profiles.
- Users can inspect runs without exporting telemetry to another service.

## [#4578](https://github.com/BoundaryML/baml/pull/4578)

- The playground has a Telemetry tab for exploring locally stored profiles.
- The tab reads the same canonical profile store as `baml query`.

## [#4573](https://github.com/BoundaryML/baml/pull/4573)

- Empty arrays and maps require enough context to infer their element types.
- Ambiguous empty containers produce a compiler diagnostic instead of an invalid runtime value.

## [#4567](https://github.com/BoundaryML/baml/pull/4567)

- `ctx.output_format` becomes the callable `ctx.output_format()` API.
- Output-format controls such as prefixes, enum formatting, class hoisting, map style, and null rendering are restored.

## [#4581](https://github.com/BoundaryML/baml/pull/4581)

- The BAML extension uses a new project-aware language server.
- Editor features include diagnostics, completion, hover, go-to-definition, references, symbols, semantic tokens, inlay hints, and code lenses.
- `baml ide install` materializes standard-library sources so go-to-definition can open them.
- Testset blocks receive run code lenses.

## [#4568](https://github.com/BoundaryML/baml/pull/4568)

- Generated programs, packed binaries, and mounted package interfaces carry version and integrity metadata.
- Corrupt or incompatible artifacts fail early with instructions to regenerate matching SDK and bridge artifacts.

## [#4571](https://github.com/BoundaryML/baml/pull/4571)

- Reflection compile artifacts cannot be consumed twice or as the wrong artifact kind.
- Sessions remain usable after a rejected compile artifact.
- Session-generated names no longer collide with user bindings.

## [#4577](https://github.com/BoundaryML/baml/pull/4577)

- Runtime type identity survives additional reflection and dispatch boundaries.
- Reflected schemas and values continue to refer to the same runtime declaration.

## [#4574](https://github.com/BoundaryML/baml/pull/4574)

- `unreflect(value)` can appear in any type position when its runtime scope is valid.

## [#4583](https://github.com/BoundaryML/baml/pull/4583)

- Errors thrown by evaluated code retain their original runtime diagnostics.
- Reflected schemas survive evaluation and package boundaries.

## [#4601](https://github.com/BoundaryML/baml/pull/4601)

- `baml.json.to_string(value)` and `baml.json.to_json(value)` serialize values typed as `unknown`.
- The functions no longer take explicit generic arguments.
- `baml.json.encode` is removed.

## [#4600](https://github.com/BoundaryML/baml/pull/4600)

- `reflect.call_any<R>` checks the dynamic function result against `R`.
- Mismatches raise a reflection error instead of returning an invalid typed value.

## [#4603](https://github.com/BoundaryML/baml/pull/4603)

- Invalid or unresolved compiler types no longer silently become the valid top type `unknown`.
- Invalid numeric compound assignments are rejected during compilation.

## [#4602](https://github.com/BoundaryML/baml/pull/4602)

- BAML v0 `test Name { functions [...] }` declarations are removed.
- Tests must use BAML v1 expression bodies.

## [#4604](https://github.com/BoundaryML/baml/pull/4604)

- Python LLM streams support `async for`.
- Streaming parsing consumes ready delta batches for better throughput.
- `ai.Runner<Out>` becomes `ai.Runner`, with `Out` inferred by `run<Out>`.
- `TurnStream.next()` returns `string[] | Done` instead of `string | Done`.

## [#4606](https://github.com/BoundaryML/baml/pull/4606)

- `baml.io.Read` and `baml.io.Write` define common I/O interfaces.
- Files, network streams, subprocesses, byte arrays, and other standard I/O values implement the shared interfaces where appropriate.

## [#4599](https://github.com/BoundaryML/baml/pull/4599)

- A stored lambda can infer omitted parameter types from later uses.
- Users need fewer redundant lambda annotations when a concrete call or assignment supplies the type.

## [#4609](https://github.com/BoundaryML/baml/pull/4609)

- HTTP operations work again in `baml run` and the playground.

## [#4612](https://github.com/BoundaryML/baml/pull/4612)

- LLM response parsing preserves an optional nested class when omitted fields are valid.
- The parser no longer collapses that class to `null` solely because optional fields are absent.

## [#4619](https://github.com/BoundaryML/baml/pull/4619)

- Class constructors that omit non-nullable fields receive a diagnostic listing every missing field.
- Nullable fields remain omittable.
- `unknown` fields require an explicit value and are not implicitly omitted.

## [#4621](https://github.com/BoundaryML/baml/pull/4621)

- A generic function cannot be stored before its type parameters are specialized.
- The diagnostic suggests an explicit specialization or a concrete function-type annotation.
- Direct generic calls continue to infer fresh type arguments.

## [#4593](https://github.com/BoundaryML/baml/pull/4593)

- `throws unknown` is rejected when the function throws nothing or only a narrower known type.
- A genuine unknown error boundary can still declare `throws unknown`.
- The diagnostic suggests inference or a precise explicit throws type.

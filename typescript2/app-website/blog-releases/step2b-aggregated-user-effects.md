# Step 2b and Step 3: aggregated user effects with classifications

Each entry has exactly one classification. A PR may appear in more than one entry when it has separate user effects that belong in different changelog sections.

## Local execution telemetry and SQL queries

- Classification: `HEADLINE_CHANGE`
- PRs: [#4548](https://github.com/BoundaryML/baml/pull/4548), [#4563](https://github.com/BoundaryML/baml/pull/4563), [#4578](https://github.com/BoundaryML/baml/pull/4578)
- BAML now stores local execution profiles, queries them with SQL through `baml query`, and explores them in the playground Telemetry tab.

## Unified LLM event callbacks

- Classification: `FEATURE`
- PRs: [#4570](https://github.com/BoundaryML/baml/pull/4570)
- Direct LLM calls, streams, and agents now accept the same `on_event` callback for granular model, tool, usage, and lifecycle events.

## Async-iterable Python streams

- Classification: `FEATURE`
- PRs: [#4604](https://github.com/BoundaryML/baml/pull/4604)
- Python users can consume LLM streams with `async for`, while batched delta processing improves streaming throughput.

## New project-aware language server

- Classification: `FEATURE`
- PRs: [#4581](https://github.com/BoundaryML/baml/pull/4581)
- The editor extension gains a new project-aware language server, standard-library go-to-definition, and run code lenses for testsets.

## Python SDK migration support

- Classification: `FEATURE`
- PRs: [#4459](https://github.com/BoundaryML/baml/pull/4459)
- Python can preview prompts without provider secrets and exposes finish-reason failures, provider metadata, and BAML v0 migration helpers.

## Runtime class and generic function reflection

- Classification: `FEATURE`
- PRs: [#4491](https://github.com/BoundaryML/baml/pull/4491), [#4493](https://github.com/BoundaryML/baml/pull/4493), [#4519](https://github.com/BoundaryML/baml/pull/4519)
- Reflection can inspect `AnyClass` values, list generic functions, and explicitly specialize reflected generics.

## More flexible dynamic types and calls

- Classification: `FEATURE`
- PRs: [#4574](https://github.com/BoundaryML/baml/pull/4574), [#4600](https://github.com/BoundaryML/baml/pull/4600)
- `unreflect` works in any valid type position, and `reflect.call_any<R>` verifies that dynamic results match `R`.

## Common read and write interfaces

- Classification: `FEATURE`
- PRs: [#4606](https://github.com/BoundaryML/baml/pull/4606)
- `baml.io.Read` and `baml.io.Write` provide shared interfaces across standard I/O values.

## JSON serialization through `unknown`

- Classification: `FEATURE`
- PRs: [#4601](https://github.com/BoundaryML/baml/pull/4601)
- `baml.json.to_string` and `baml.json.to_json` can serialize values whose static type is `unknown`.

## Truthy conditions

- Classification: `FEATURE`
- PRs: [#4498](https://github.com/BoundaryML/baml/pull/4498)
- Condition positions accept truthy and falsy values and narrow branch types accordingly. Strings also provide `is_empty()`.

## Interface member projections

- Classification: `FEATURE`
- PRs: [#4500](https://github.com/BoundaryML/baml/pull/4500)
- Users can access `(Type as Interface).member` and call `Interface.method(instance)`.

## Lambda parameter inference from later use

- Classification: `FEATURE`
- PRs: [#4599](https://github.com/BoundaryML/baml/pull/4599)
- Stored lambdas can infer omitted parameter types from concrete later uses.

## Error wrapping that preserves context

- Classification: `FEATURE`
- PRs: [#4441](https://github.com/BoundaryML/baml/pull/4441)
- `baml.errors.UnknownError` wraps arbitrary errors without losing the original cause or stack trace.

## Lazy iterator bounds

- Classification: `FEATURE`
- PRs: [#4510](https://github.com/BoundaryML/baml/pull/4510)
- Iterators support `take`, `skip`, `take_while`, and `skip_while`.

## Reproducible primitive random values

- Classification: `FEATURE`
- PRs: [#4135](https://github.com/BoundaryML/baml/pull/4135)
- Primitive random methods accept an optional RNG instance.

## CLI language logs

- Classification: `FEATURE`
- PRs: [#4408](https://github.com/BoundaryML/baml/pull/4408), [#4409](https://github.com/BoundaryML/baml/pull/4409)
- `baml run` and `baml test` respect log-level controls and explain shutdown waits.

## Faster runtime package compilation

- Classification: `FEATURE`
- PRs: [#4453](https://github.com/BoundaryML/baml/pull/4453)
- Precompiled standard-library state reduces the startup cost of `reflect.Package.compile`.

## Faster compiler inference

- Classification: `FEATURE`
- PRs: [#4458](https://github.com/BoundaryML/baml/pull/4458), [#4461](https://github.com/BoundaryML/baml/pull/4461), [#4463](https://github.com/BoundaryML/baml/pull/4463)
- Reused inference facts and alias caches reduce repeated compiler work.

## Cleaner formatter output

- Classification: `FEATURE`
- PRs: [#4489](https://github.com/BoundaryML/baml/pull/4489), [#4541](https://github.com/BoundaryML/baml/pull/4541)
- `baml fmt` removes redundant parentheses in binary expressions, call arguments, and method receivers.

## Self-validating compiler artifacts

- Classification: `FEATURE`
- PRs: [#4568](https://github.com/BoundaryML/baml/pull/4568)
- Generated programs, packed binaries, and package interfaces carry version and checksum metadata so incompatible artifacts fail with regeneration guidance.

## Reflection package and type migration

- Classification: `BREAKING_CHANGE`
- PRs: [#4543](https://github.com/BoundaryML/baml/pull/4543), [#4580](https://github.com/BoundaryML/baml/pull/4580)
- Reflection APIs move from `baml.reflect.*` to `reflect.*`, and type views can no longer stand in for concrete `reflect.Type` values.

## Reflection field accessor rename

- Classification: `BREAKING_CHANGE`
- PRs: [#4493](https://github.com/BoundaryML/baml/pull/4493)
- `reflect.class.Field.read<T>()` is renamed to `value<T>()`.

## JSON API simplification

- Classification: `BREAKING_CHANGE`
- PRs: [#4601](https://github.com/BoundaryML/baml/pull/4601)
- Remove `baml.json.encode` and remove explicit generic arguments from `baml.json.to_string` and `baml.json.to_json`.

## Hash-delimited templates removed

- Classification: `BREAKING_CHANGE`
- PRs: [#4565](https://github.com/BoundaryML/baml/pull/4565)
- Hash-delimited Jinja strings are rejected; prompts must use BAML template literals.

## Legacy test declarations removed

- Classification: `BREAKING_CHANGE`
- PRs: [#4602](https://github.com/BoundaryML/baml/pull/4602)
- BAML v0 test declarations must be rewritten as BAML v1 expression-body tests.

## Callable output-format context

- Classification: `BREAKING_CHANGE`
- PRs: [#4567](https://github.com/BoundaryML/baml/pull/4567)
- `ctx.output_format` becomes `ctx.output_format()`, with the v0 formatting options restored as arguments.

## Generated SDK location

- Classification: `BREAKING_CHANGE`
- PRs: [#4522](https://github.com/BoundaryML/baml/pull/4522)
- `baml_sdk` is generated beside `baml.toml` instead of in its parent directory.

## Generated C# namespace

- Classification: `BREAKING_CHANGE`
- PRs: [#4535](https://github.com/BoundaryML/baml/pull/4535)
- Generated C# code uses `baml_sdk` instead of `baml_client`.

## Agent, runner, and low-level stream generics

- Classification: `BREAKING_CHANGE`
- PRs: [#4570](https://github.com/BoundaryML/baml/pull/4570), [#4604](https://github.com/BoundaryML/baml/pull/4604)
- Remove the output generic from `ai.Agent` and `ai.Runner`, infer it on `run`, and return batches from `TurnStream.next()`.

## Stored generic functions require specialization

- Classification: `BREAKING_CHANGE`
- PRs: [#4621](https://github.com/BoundaryML/baml/pull/4621)
- A generic function stored as a value must have explicit type arguments or a concrete function-type context.

## Precise `throws unknown` contracts

- Classification: `BREAKING_CHANGE`
- PRs: [#4593](https://github.com/BoundaryML/baml/pull/4593)
- `throws unknown` is rejected unless an unknown-typed failure can actually escape.

## Runtime compilation boundary diagnostics

- Classification: `BUGFIX`
- PRs: [#4460](https://github.com/BoundaryML/baml/pull/4460)
- Unsupported runtime-dependent checks fail at compile time instead of panicking or being omitted in release builds.

## Diagnostics for member access on `unknown`

- Classification: `BUGFIX`
- PRs: [#4466](https://github.com/BoundaryML/baml/pull/4466)
- Invalid member access on `unknown` reports a compiler error instead of reaching the VM.

## Literal pattern semantics

- Classification: `BUGFIX`
- PRs: [#4478](https://github.com/BoundaryML/baml/pull/4478)
- Literal patterns use type-membership semantics rather than runtime equality.

## Mutable identity across loop calls

- Classification: `BUGFIX`
- PRs: [#4467](https://github.com/BoundaryML/baml/pull/4467)
- Mutations performed by callees inside loops persist on the original object.

## Invalid LLM output schemas

- Classification: `BUGFIX`
- PRs: [#4470](https://github.com/BoundaryML/baml/pull/4470)
- Non-data LLM schemas produce catchable diagnostics instead of crashes or silent schema omission.

## Unspecialized reflected generic diagnostics

- Classification: `BUGFIX`
- PRs: [#4473](https://github.com/BoundaryML/baml/pull/4473)
- Invalid dynamic use of unspecialized reflected generics fails with a targeted error.

## Generic and joined-collection loops

- Classification: `BUGFIX`
- PRs: [#4490](https://github.com/BoundaryML/baml/pull/4490)
- Loops over joined collection arms and `Iterable`-bounded generics no longer abort compilation.

## Optional-chained generic calls

- Classification: `BUGFIX`
- PRs: [#4495](https://github.com/BoundaryML/baml/pull/4495)
- Optional method calls preserve explicit and inferred type arguments.

## Runtime type identity through dispatch

- Classification: `BUGFIX`
- PRs: [#4501](https://github.com/BoundaryML/baml/pull/4501), [#4516](https://github.com/BoundaryML/baml/pull/4516), [#4536](https://github.com/BoundaryML/baml/pull/4536)
- Runtime-created and compiled-package types retain identity, definitions, and source names through interface dispatch.

## Safe inline `unreflect`

- Classification: `BUGFIX`
- PRs: [#4518](https://github.com/BoundaryML/baml/pull/4518), [#4530](https://github.com/BoundaryML/baml/pull/4530)
- Runtime types that would escape through return or thrown types receive an actionable diagnostic instead of reaching invalid lowering.

## Global and session bindings

- Classification: `BUGFIX`
- PRs: [#4529](https://github.com/BoundaryML/baml/pull/4529), [#4531](https://github.com/BoundaryML/baml/pull/4531)
- Method calls work on global bindings, and session assignments preserve the binding's declared type.

## SDK naming errors

- Classification: `BUGFIX`
- PRs: [#4526](https://github.com/BoundaryML/baml/pull/4526)
- Unsupported language-native SDK naming returns a normal error instead of panicking.

## Correct stack-carried locals

- Classification: `BUGFIX`
- PRs: [#4508](https://github.com/BoundaryML/baml/pull/4508), [#4544](https://github.com/BoundaryML/baml/pull/4544)
- Reassigned short-circuit locals and branch-carried locals retain correct values after optimization.

## Array match coverage

- Classification: `BUGFIX`
- PRs: [#4547](https://github.com/BoundaryML/baml/pull/4547)
- Match exhaustiveness preserves array type ascriptions.

## Stable runtime type identity

- Classification: `BUGFIX`
- PRs: [#4560](https://github.com/BoundaryML/baml/pull/4560), [#4577](https://github.com/BoundaryML/baml/pull/4577)
- Runtime-created types survive reflection and host boundaries without name collisions or lost identity.

## Correct diagnostic source locations

- Classification: `BUGFIX`
- PRs: [#4566](https://github.com/BoundaryML/baml/pull/4566)
- Type errors point to their owning signature or default expression and no longer crash rendering.

## Ambiguous empty containers

- Classification: `BUGFIX`
- PRs: [#4573](https://github.com/BoundaryML/baml/pull/4573)
- Empty arrays and maps without enough type context receive a diagnostic.

## Recoverable reflection sessions

- Classification: `BUGFIX`
- PRs: [#4571](https://github.com/BoundaryML/baml/pull/4571)
- Rejected compile artifacts no longer poison sessions, and invalid artifact reuse fails clearly.

## Preserved evaluated errors and schemas

- Classification: `BUGFIX`
- PRs: [#4583](https://github.com/BoundaryML/baml/pull/4583)
- `eval()` preserves original runtime diagnostics and reflected schema values.

## Invalid compiler types

- Classification: `BUGFIX`
- PRs: [#4603](https://github.com/BoundaryML/baml/pull/4603)
- Invalid types no longer degrade into `unknown`, and incompatible numeric compound assignments are rejected.

## HTTP in CLI and playground

- Classification: `BUGFIX`
- PRs: [#4609](https://github.com/BoundaryML/baml/pull/4609)
- HTTP operations work again in `baml run` and the playground.

## Optional nested classes in LLM output

- Classification: `BUGFIX`
- PRs: [#4612](https://github.com/BoundaryML/baml/pull/4612)
- Optional nested class values survive response parsing when their omitted fields are valid.

## Missing required class fields

- Classification: `BUGFIX`
- PRs: [#4619](https://github.com/BoundaryML/baml/pull/4619)
- Class constructors that omit required fields receive one diagnostic listing the missing fields.

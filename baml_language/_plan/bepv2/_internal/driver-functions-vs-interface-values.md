# Driver Functions vs Nominal Driver Values

> **Status:** Comparative design experiment. This is not a normative BEPv2
> decision. The function-driven reference remains in
> `crates/baml_tests/baml_src_temp`; the interface-driven comparison lives in
> `crates/baml_tests/baml_src_temp2`.

## Question

BEPv2 currently represents a driver as an ordinary generic function:

```baml
let response = ai.drivers.generate_with_meta(task)
let run = ai.drivers.run_agent(task, options)
```

The competing design represents a driver as a nominal value implementing one
protocol, with `Task<T>.drive(...)` as the unary convenience:

```baml
let response = task.drive(ai.driver.generate_with_meta<Resolution>())
let run = task.drive(ai.driver.agent<Resolution>(options))
```

This note records how to compare the designs across the entire BEPv2 lifecycle
catalog rather than deciding from the attractive unary example alone.

## Evaluation criteria

The primary audience is an application developer encountering the API for the
first time. Compare each design on:

1. **Legibility:** can the call be understood without knowing BEPv2 internals?
2. **Progressive disclosure:** is the direct typed call still the easy path?
3. **Type preservation:** does `T`, the provider capability, and the exact error
   channel survive without casts or erased registries?
4. **Discoverability:** can completion show both standard and third-party
   lifecycles at the point of use?
5. **Extensibility:** can a library add a lifecycle without compiler or SDK
   code generation?
6. **Composability:** can configuration, wrappers, stateful resources, and
   higher-order code be combined without duplicate abstractions?
7. **Ownership honesty:** does the receiver identify the value that actually
   owns the operation?
8. **Ceremony:** what must an SDE1 read or construct for the common case?

## Shared model

The experiment does not change the BEPv2 ownership boundary:

```text
LLM declaration -> Task<T> -> execution policy -> provider capability -> result/resource
```

It changes only how execution policy is represented and invoked. Providers,
tasks, resources, transcripts, tools, hooks, and errors remain the same in both
packages. The interface façade delegates to the existing executable driver
functions so provider behavior cannot accidentally bias the comparison.

## Nominal protocol

The prototype uses an input-parameterized interface with associated output and
error types:

```baml
interface Driver<Input> {
  type Output
  type Error

  function drive(self, input: Input) -> Self.Output throws Self.Error
}
```

A generation driver binds all three pieces:

```baml
class GenerateWithMeta<T> {}

implements<T> Driver<Task<T>> for GenerateWithMeta<T> {
  type Output = Response<T>
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, task: Task<T>) -> Response<T>
      throws baml.errors.CallError | baml.errors.UnknownError {
    ai.drivers.generate_with_meta<T>(task)
  }
}
```

`Task<T>` provides the unary convenience. The explicit `Output` and `Error`
bindings in this signature are intentional: they make the projection concrete
at the method boundary in the executable branch.

```baml
function drive<
  Output,
  Error,
  D extends Driver<Task<T>, Output = Output, Error = Error>,
>(self, driver: D) -> Output throws Error {
  driver.drive(self)
}
```

The shorter `D extends Driver<Task<T>> -> D.Output throws D.Error` is the
desired equivalent spelling. The comparison should record whether improving
that projection and outside-in inference materially changes the user-facing
result.

## Lifecycle shapes

One invocation spelling should not obscure that BEPv2 has several ownership
and arity shapes.

| Shape | Function-driven | Interface-driven experiment |
| --- | --- | --- |
| Unary task | `ai.drivers.stream(task)` | `task.drive(ai.driver.stream<T>())` |
| Configured unary task | `ai.drivers.run_agent(task, options)` | `task.drive(ai.driver.agent<T>(options))` |
| Task plus caller resource | `ai.drivers.open_live(task, channel)` | `task.drive(ai.driver.live(channel))` |
| Plural task operation | `ai.drivers.submit_batch(provider, tasks, options)` | `ai.driver.batch<T>(provider, options).drive(tasks)` |
| Provider-only construction | `ai.drivers.open_session(provider, options)` | `ai.driver.open_session(options).drive(provider)` |
| Existing state owner | `session.run(task)` | `session.run(task)` |
| Specialized media input | `ai.drivers.transcribe(provider, audio, options)` | driver value over a typed media input or a captured provider/options value |

The interface design is allowed to use `driver.drive(input)` where `Task` is
not the natural receiver. Requiring every lifecycle to look like
`task.drive(...)` would make batching, provider-only construction, and
resource-owned continuation less honest.

## Default calls and desugaring

This experiment does **not** add compiler lowering. Both packages continue to
use hand-written `ResolveTicket_task(...)` helpers. The normal LLM call remains
conceptually:

```baml
ResolveTicket(ticket)
// default execution selected by the provider
```

Do not add `task.drive()` in the comparison yet. It duplicates the direct call,
needs an overload or default driver value, and makes it harder to distinguish a
realtime-only provider whose task is valid but whose direct call is not.

## Generic inference

The result type of a stateless driver is a function of the task's `T`:

```text
GenerateWithMeta + Task<T> -> Response<T>
Stream           + Task<T> -> Stream<T$stream, T>
Agent            + Task<T> -> AgentRun<T>
Background       + Task<T> -> Job<T>
```

The prototype therefore gives these driver classes a generic `T`. Today the
call normally writes it explicitly:

```baml
task.drive(ai.driver.generate_with_meta<Resolution>())
```

The ideal call omits it:

```baml
task.drive(ai.driver.generate_with_meta())
```

That requires outside-in inference from `Task<T>.drive` into the zero-argument
factory result. Reusable polymorphic driver values are a separate question:

```baml
let stream = ai.driver.stream()
task_a.drive(stream)
task_b.drive(stream)
```

If `task_a` and `task_b` have different result types, this value needs
higher-rank polymorphism or a generic associated output family. The experiment
must not hide that issue through `unknown` or runtime casts.

## Batch and variance

V1 batch remains homogeneous:

```baml
Task<T>[] -> Batch<T>
```

A mutable `Task<T>[]` queue is invariant. Widening `Task<A>` and `Task<B>` to
`Task<A | B>` also loses the correlation between each submitted item and its
result. The interface experiment should therefore use either:

```baml
ai.driver.batch<Resolution>(provider, options).drive(tasks)
```

or a separate stateful homogeneous queue:

```baml
let queue = ai.BatchQueue<Resolution>.new(provider, options)
queue.add(task)
let batch = queue.submit()
```

A heterogeneous queue needs typed item handles or tuple-shaped results and is a
separate design. `task.drive(ai.driver.batch(queue))` is not used: adding work
to a queue is not the same event as starting that work.

## Discoverability

Nominality helps navigation and lets documentation state “implement
`Driver<Input>`.” It does not by itself make an editor enumerate every class
implementing the interface at a call site. Both alternatives still need a
curated namespace:

```text
ai.drivers.*  function-driven operations
ai.driver.*   interface-driver factories
acme.driver.* third-party factories
```

The comparison should test completion quality, required imports, hover
signatures, go-to-implementation, and whether incompatible provider
capabilities can be filtered. A future LSP action such as “drive this task with
a compatible lifecycle” is useful to either representation.

Do not introduce a heterogeneous runtime driver registry for discoverability.
Such a registry erases associated input/output types, just as a heterogeneous
function registry erases individual function signatures. Discovery is an
editor/package concern; execution remains statically typed.

## Relationship to existing frameworks

- React's move from class components to function components is evidence against
  requiring object ceremony for stateless behavior, not evidence against
  objects that genuinely own state.
- Tower's `Service<Request>` is the closest nominal analogue: one request type,
  associated response/error types, and composable wrappers. Its mapping is
  fixed for a concrete service value, which is exactly the generic-inference
  question this experiment must expose.
- LangChain's `Runnable` demonstrates both the discovery benefit and the risk
  of accumulating invoke, batch, stream, async, retry, fallback, schema, and
  composition methods on one universal object.
- Pydantic AI and Vercel AI SDK put run/stream methods on configured agent
  objects. BEPv2 already has provider, agent, harness, and resource objects; the
  question is whether immutable `Task<T>` should also become an active
  execution receiver.

## Prototype rules

The comparison package follows these rules:

1. No compiler edits and no desugaring work.
2. Do not change provider wire behavior or test fixtures to favor either API.
3. `ai.driver.*` values are immutable configuration.
4. Mutable registries, streams, sessions, jobs, batches, caches, and observers
   remain explicit stateful resources.
5. Preserve the exact output and error types through associated types.
6. Preserve every offline scenario and live integration test from the baseline.
7. Keep resource methods once the resource exists; do not route them back
   through a driver merely for visual uniformity.
8. Record every place requiring explicit generic arguments, input bundling,
   duplicated names, or special inference.

## Decision gate

After both packages compile and their equivalent scenarios pass, review them
side by side. Prefer the interface surface only if it materially improves the
beginner call site and third-party discovery without:

- adding a second confusing execution object beside the provider;
- requiring explicit result generics in most calls;
- forcing plural or provider-owned operations into fake task receivers;
- erasing output/error types for registries or wrappers; or
- growing a universal object with one method per lifecycle.

If those conditions are not met, retain generic functions as the canonical
surface and consider a general language-level pipe/UFCS feature for postfix
ergonomics. That would benefit ordinary and third-party driver functions
without introducing driver-specific compiler behavior.

## Prototype findings — 2026-07-20

The first complete migration establishes that the interface design is viable
with BAML's existing generic interfaces and associated types. It does not need
driver-specific desugaring:

- 110 source files compile in the comparison package;
- all 68 non-live tests pass;
- test discovery lists 126 selectors in about six seconds: the baseline 122
  plus three offline interface seed tests and one live seed test;
- every explicit lifecycle call in copied scenarios now uses `ai.driver.*`;
  remaining `ai.drivers.*` calls are only the hand-written direct-call lowering
  helpers and the README's baseline example; and
- the same live testsets remain selectable because provider behavior and
  fixtures were not changed.

The experiment exposed several concrete usability differences.

### What reads well

Unary execution is compact and puts the intent first:

```baml
let response = task.drive(ai.driver.generate_with_meta<Resolution>())
let run = task.drive(ai.driver.agent<Resolution>(options))
let job = task.drive(ai.driver.background<Resolution>(options))
let live = voice_task.drive(ai.driver.live(channel))
```

Realtime is especially clean because `Live` implements exactly
`Driver<Task<null>>`; no result generic is needed. A provider with only
realtime capability cannot accidentally drive an arbitrary `Task<T>`.

The non-unary cases also found honest receivers instead of fake task methods:

```baml
let batch = ai.driver.batch<Resolution>(provider, options).drive(tasks)
let session = ai.driver.open_session(options).drive(provider)
let cache = ai.driver.create_cache(messages, options).drive(provider)
let transcript = ai.driver.transcribe_with_meta(provider, options).drive(audio)
```

Once a resource exists, its operations remain ordinary resource methods.

Third-party extensibility stays fully typed. The direct-provider-access
scenario defines a package-local driver class, associated output/error types,
an implementation, and a factory without touching `Task`, a registry, or the
compiler.

### What remains awkward

Factories currently repeat the task result type:

```baml
task.drive(ai.driver.generate<Resolution>())
task.drive(ai.driver.stream<Resolution$stream, Resolution>())
```

Outside-in inference could remove this repetition. Until then the function
surface is shorter for generic drivers.

Configured drivers are readable when configuration has a local name, but
inline construction can become deeply nested:

```baml
let options = ai.AgentOptions.new(...)
let run = task.drive(ai.driver.agent<Resolution>(options))
```

This is clearer than placing the complete `AgentOptions.new(...)` expression
inside `agent(...)` inside `drive(...)`.

The `baml-cli` compiler path currently needs explicit associated bindings on
the generic `Task.drive` method. The shorter, semantically equivalent method:

```baml
function drive<D extends Driver<Task<T>>>(self, driver: D)
    -> D.Output throws D.Error
```

projects the input as `Task<!error>` in that path. The executable workaround
binds `Output` and `Error` in the constraint. A second normalization issue can
surface when `Task.drive(...)` is called inside a function with an explicit
`throws` declaration; calling the concrete `driver.drive(task)` preserves the
same exact types and is used at those few comparison sites. These are compiler
projection bugs around a supported language feature, not a reason to erase the
associated types.

Provider-first facades currently accept erased `Provider` and retain a runtime
`Unsupported` branch because the underlying compatibility drivers do. A final
interface design should make those driver implementations generic over the
precise provider capability so unsupported combinations fail statically.

The interface authoring cost is real: a custom lifecycle needs a class,
associated bindings, an `implements` block, and usually a factory. A generic
function expresses the same behavior in one declaration. The usability review
must decide whether nominal discovery and value composition repay that cost.

The live seed test is declared and selectable as
`integ-test-interface-driver-e2e::`. Its attempted credentialed run in this
session stopped before provider I/O because the local Infisical CLI had no
active login; this does not affect compilation or the 68 deterministic tests.

## Files to compare

- Function baseline: `crates/baml_tests/baml_src_temp`
- Interface experiment: `crates/baml_tests/baml_src_temp2`
- Seed interface: `baml_src_temp2/ns_ai/core/driver.baml`
- Task convenience: `baml_src_temp2/ns_ai/core/task.baml`
- Seed driver family: `baml_src_temp2/ns_ai/ns_driver/generation.baml`
- Seed scenarios:
  `baml_src_temp2/ns_ai_scenarios/01_tasks_and_providers/00_interface_driver_e2e.baml`
